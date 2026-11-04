//! The certificate walk from a reference situation on a patch (the
//! per-context body of the Lemma B enumerator, FINDINGS 10.11.19, made
//! reusable): embed the reference situation with its centre at a cell,
//! run the rule until the reference key recurs, and record the loop, the
//! landing cell and the *terrain* — the union of (R+1)-balls around every
//! generation's centre, which is all the tiling the walk ever read.
use std::collections::{HashMap, HashSet, VecDeque};

use ca_engine::CaRule;
use tiling_core::results::ResultRecord;

use crate::ball::EdgeCtx;
use crate::closure::Closure;
use crate::patchctx::PatchCtx;
use crate::situation::{Situation, encode};

#[derive(Clone, Debug)]
pub enum WalkOutcome {
    /// Returned to the reference key after `len` generations; the centre
    /// moved by `disp` (patch frame) and landed on cell `landing`.
    Loop { len: u64, disp: [f64; 2], landing: u32 },
    OffCertificate { g: u64 },
    NoReturn { gens: u64 },
    Escaped { g: u64 },
}

pub struct Walker {
    pub sit: Situation,
    pub sit_centre: usize,
    pub key_index: HashMap<Vec<u8>, usize>,
    pub target_key: Vec<u8>,
    pub k: u8,
    pub rball: u32,
    pub horizon: u64,
    pub radius: u32,
}

impl Walker {
    pub fn new(cert: &Closure, state: usize, horizon: u64, radius: u32) -> Self {
        let mut sit: Situation = cert.states[state].sit.clone();
        sit.restore();
        let (sit_centre, _) = sit.centre();
        let key_index: HashMap<Vec<u8>, usize> =
            cert.states.iter().enumerate().map(|(i, s)| (s.sit.key.clone(), i)).collect();
        let target_key = cert.states[state].sit.key.clone();
        Self { sit, sit_centre, key_index, target_key, k: cert.k, rball: cert.r + 1, horizon, radius }
    }

    /// Embed the reference situation with its centre at `cell` by BFS over
    /// its edge structure (classes, edge counts and back-edges must match).
    /// Returns the cell of the patch for every cell of the situation.
    pub fn embed(&self, pctx: &PatchCtx, cell: u32) -> Option<Vec<u32>> {
        let sit = &self.sit;
        let n = sit.cells.len();
        let mut map: Vec<Option<u32>> = vec![None; n];
        if pctx.class(cell) != sit.cells[self.sit_centre].class || pctx.nedges(cell) != sit.cells[self.sit_centre].edges.len() {
            return None;
        }
        map[self.sit_centre] = Some(cell);
        let mut queue = VecDeque::from([self.sit_centre]);
        while let Some(i) = queue.pop_front() {
            let b = map[i].unwrap();
            for (e, edge) in sit.cells[i].edges.iter().enumerate() {
                if let Some((j, back)) = *edge {
                    match pctx.edge(b, e) {
                        Some((b2, back2)) if back2 == back => match map[j as usize] {
                            Some(m) => {
                                if m != b2 {
                                    return None;
                                }
                            }
                            None => {
                                if pctx.class(b2) != sit.cells[j as usize].class
                                    || pctx.nedges(b2) != sit.cells[j as usize].edges.len()
                                {
                                    return None;
                                }
                                map[j as usize] = Some(b2);
                                queue.push_back(j as usize);
                            }
                        },
                        _ => return None,
                    }
                }
            }
        }
        map.into_iter().collect()
    }

    /// Walk from the situation embedded by `map` (from [`embed`](Self::embed)).
    pub fn walk(&self, record: &ResultRecord, pctx: &PatchCtx, map: &[u32]) -> (WalkOutcome, HashSet<u32>) {
        let ncells = pctx.cells() as usize;
        let mut init_state = vec![0u8; ncells];
        for (i, c) in self.sit.cells.iter().enumerate() {
            if c.state != 0 {
                init_state[map[i] as usize] = c.state;
            }
        }
        let (strata, rule) = record.replay_setup(&pctx.patch_v);
        let mut engine = ca_engine::Engine::with_strata(pctx.patch_v.graph.clone(), strata);
        engine.load_state(&init_state);
        let mut terrain: HashSet<u32> = HashSet::new();
        let rball = self.rball;
        let ball_around = |c: u32, terrain: &mut HashSet<u32>| {
            let mut d: HashMap<u32, u32> = HashMap::new();
            let mut q = VecDeque::from([c]);
            d.insert(c, 0);
            while let Some(x) = q.pop_front() {
                let dx = d[&x];
                terrain.insert(x);
                if dx == rball {
                    continue;
                }
                for e in 0..pctx.nedges(x) {
                    if let Some((y, _)) = pctx.edge(x, e)
                        && let std::collections::hash_map::Entry::Vacant(v) = d.entry(y)
                    {
                        v.insert(dx + 1);
                        q.push_back(y);
                    }
                }
            }
        };
        let start = map[self.sit_centre];
        ball_around(start, &mut terrain);
        let start_off = pctx.placement(start).1;
        for g in 1..=self.horizon {
            rule.step(&mut engine);
            let state = engine.state();
            let support: Vec<u32> = (0..pctx.cells()).filter(|&c| state[c as usize] != 0).collect();
            if support.is_empty() {
                return (WalkOutcome::OffCertificate { g }, terrain);
            }
            let maxd = support.iter().map(|&c| pctx.distance(c)).max().unwrap();
            if maxd + u32::from(self.k) + rball + 2 > self.radius {
                return (WalkOutcome::Escaped { g }, terrain);
            }
            let state_of = |c: u32| state[c as usize];
            let Some((s, order)) = encode(pctx, &support, &state_of, self.k) else {
                return (WalkOutcome::OffCertificate { g }, terrain);
            };
            if !self.key_index.contains_key(&s.key) {
                return (WalkOutcome::OffCertificate { g }, terrain);
            }
            let centre = order[s.centre().0];
            ball_around(centre, &mut terrain);
            if s.key == self.target_key {
                let off = pctx.placement(centre).1;
                return (WalkOutcome::Loop { len: g, disp: [off[0] - start_off[0], off[1] - start_off[1]], landing: centre }, terrain);
            }
        }
        (WalkOutcome::NoReturn { gens: self.horizon }, terrain)
    }

    /// The cell whose placement offset is `start + v` (exact to 1e-6), if any.
    pub fn cell_at(pctx: &PatchCtx, start: u32, v: [f64; 2]) -> Option<u32> {
        let s = pctx.placement(start).1;
        let q = [s[0] + v[0], s[1] + v[1]];
        (0..pctx.cells()).find(|&c| {
            let off = pctx.placement(c).1;
            (off[0] - q[0]).abs() < 1e-6 && (off[1] - q[1]).abs() < 1e-6
        })
    }
}
