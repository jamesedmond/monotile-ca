//! The situation closure: from seed situations, follow every legal
//! extension (every atlas ball that extends the situation around its
//! centre cell), step once on the ball, canonicalise the successor, and
//! repeat until no new situation appears. A closed set with no death
//! transition certifies that any evolution entering the set never dies.

use std::collections::HashMap;

use ca_engine::{AnyRule, MAX_COUNTED_STATES, Rule, TableRule};
use serde::{Deserialize, Serialize};

use crate::ball::{Atlas, Ball, FAN_MARGIN};
use crate::situation::{Situation, encode};

/// A rule the closure can step: uniform Generations or a priority table.
pub enum StepRule {
    Table(TableRule),
    Generations(Rule),
    StratifiedTable(Vec<TableRule>),
}

impl StepRule {
    pub fn from_any(rule: &AnyRule) -> Result<Self, String> {
        match rule {
            AnyRule::Table(t) => Ok(Self::Table(t.clone())),
            AnyRule::Generations(s) if s.strata() == 1 => Ok(Self::Generations(s.tables[0])),
            AnyRule::Generations(_) => Err("stratified Generations rules unsupported".into()),
            AnyRule::StratifiedTable(s) => Ok(Self::StratifiedTable(s.tables.clone())),
        }
    }

    pub fn next(&self, stratum: u8, own: u8, counts: &[u8; MAX_COUNTED_STATES]) -> u8 {
        match self {
            Self::Table(t) => t.next_state(own, counts),
            Self::Generations(r) => r.next_state(own, u32::from(counts[1])),
            Self::StratifiedTable(ts) => ts[stratum as usize].next_state(own, counts),
        }
    }

    pub fn vacuum_ignites(&self) -> bool {
        match self {
            Self::Table(t) => t.vacuum_ignites(),
            Self::Generations(r) => r.birth & 1 != 0,
            Self::StratifiedTable(ts) => ts.iter().any(TableRule::vacuum_ignites),
        }
    }

    /// Number of strata the rule distinguishes (1 unless stratified).
    pub fn strata(&self) -> usize {
        match self {
            Self::StratifiedTable(ts) => ts.len(),
            _ => 1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StateRec {
    pub sit: Situation,
    pub centre: usize,
    pub rho: u32,
    pub population: usize,
    pub origin: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Transition {
    pub from: usize,
    pub ball: usize,
    pub to: usize,
    /// Rigid motion from the source state's frame (its centre tile at
    /// the reference placement) to the successor's frame: the successor
    /// centre's placement in the ball — rotation index (× 30°) and
    /// translation.
    #[serde(default)]
    pub rot: u8,
    #[serde(default)]
    pub offset: [f64; 2],
}

/// A ball the closure used, by atlas index and by its regenerating
/// witness (root type + finite address path), so the certificate stands
/// without the atlas file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BallRef {
    pub index: usize,
    pub root_ty: usize,
    pub witness: Vec<(usize, usize)>,
}

/// The bridge from a committed launch to the closed set: which record,
/// replayed how, enters which state at which generation.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Entry {
    pub record: String,
    pub index: usize,
    pub root: String,
    pub radius: u32,
    pub start_gen: u64,
    pub component: String,
    pub entry_generation: u64,
    pub entry_state: usize,
    /// Last observed generation that was keyed (all in the closed set).
    pub last_generation: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Closure {
    pub k: u8,
    pub r: u32,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub rule_note: String,
    #[serde(default)]
    pub entry: Option<Entry>,
    pub states: Vec<StateRec>,
    pub transitions: Vec<Transition>,
    /// Index of the death state (empty pattern), if ever reached.
    pub dead: Option<usize>,
    pub errors: Vec<String>,
    #[serde(default)]
    pub balls: Vec<BallRef>,
    /// Transition ids along the observed flight, in generation order
    /// (consecutive keyed generations only).
    #[serde(default)]
    pub observed: Vec<usize>,
    #[serde(skip)]
    pub index: HashMap<Vec<u8>, usize>,
}

impl Closure {
    pub fn successors(&self, s: usize) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .transitions
            .iter()
            .filter(|t| t.from == s)
            .map(|t| t.to)
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// One CA step of `sit` embedded into `ball` via `map`. Returns the next
/// state of every ball tile.
pub fn step_on_ball(
    ball: &Ball,
    rule: &StepRule,
    stratum_of_class: &[u8],
    map: &[u16],
    sit: &Situation,
) -> Result<Vec<u8>, String> {
    let r = ball.radius;
    let n = ball.tiles.len();
    let mut st = vec![0u8; n];
    for (i, cell) in sit.cells.iter().enumerate() {
        st[map[i] as usize] = cell.state;
    }
    // Every cell that can change is within edge distance FAN_MARGIN of
    // the support, and needs its own fan: support must sit at distance
    // <= r - 2*FAN_MARGIN from the centre.
    for (t, &s) in st.iter().enumerate() {
        if s != 0 && ball.tiles[t].dist + 2 * FAN_MARGIN > r {
            return Err(format!(
                "support cell at ball distance {} too close to the rim (r = {r})",
                ball.tiles[t].dist
            ));
        }
    }
    let mut next = vec![0u8; n];
    for t in 0..n {
        if ball.tiles[t].dist + FAN_MARGIN > r {
            continue; // fan incomplete; far from the support, stays quiescent
        }
        let mut counts = [0u8; MAX_COUNTED_STATES];
        for &v in &ball.vadj[t] {
            counts[st[v as usize] as usize] += 1;
        }
        next[t] = rule.next(stratum_of_class[ball.tiles[t].class as usize], st[t], &counts);
    }
    Ok(next)
}

pub fn run_closure(
    atlas: &Atlas,
    base_of_class: &[usize],
    stratum_of_class: &[u8],
    rule: &StepRule,
    seeds: &[(Situation, String)],
    k: u8,
    log: bool,
) -> Closure {
    assert!(!rule.vacuum_ignites(), "rule ignites the vacuum; closure unsound");
    assert!(
        stratum_of_class.iter().all(|&s| (s as usize) < rule.strata()),
        "a class maps to a stratum the rule has no table for"
    );
    let r = atlas.radius;
    let mut cl = Closure {
        k,
        r,
        family: atlas.family.clone(),
        rule_note: String::new(),
        entry: None,
        states: Vec::new(),
        transitions: Vec::new(),
        dead: None,
        errors: Vec::new(),
        balls: Vec::new(),
        observed: Vec::new(),
        index: HashMap::new(),
    };
    let mut worklist: Vec<usize> = Vec::new();
    for (sit, origin) in seeds {
        if let std::collections::hash_map::Entry::Vacant(e) = cl.index.entry(sit.key.clone()) {
            let (centre, rho) = sit.centre();
            let id = cl.states.len();
            e.insert(id);
            cl.states.push(StateRec {
                population: sit.population(),
                sit: sit.clone(),
                centre,
                rho,
                origin: origin.clone(),
            });
            worklist.push(id);
        }
    }
    let mut processed = 0usize;
    while let Some(sid) = worklist.pop() {
        processed += 1;
        let sit = cl.states[sid].sit.clone();
        let centre = cl.states[sid].centre;
        let ty = base_of_class[sit.cells[centre].class as usize];
        let mut n_balls = 0usize;
        for (bi, ball) in atlas.by_type(ty) {
            let Some(map) = sit.embed(centre, ball) else {
                continue;
            };
            n_balls += 1;
            let next = match step_on_ball(ball, rule, stratum_of_class, &map, &sit) {
                Ok(n) => n,
                Err(e) => {
                    cl.errors.push(format!("state {sid} ball {bi}: {e}"));
                    continue;
                }
            };
            let support: Vec<u32> = (0..next.len() as u32)
                .filter(|&c| next[c as usize] != 0)
                .collect();
            let mut motion = (0u8, [0.0f64; 2]);
            let to = if support.is_empty() {
                *cl.dead.get_or_insert_with(|| {
                    let id = cl.states.len();
                    cl.states.push(StateRec {
                        sit: Situation {
                            k,
                            cells: Vec::new(),
                            key: Vec::new(),
                        },
                        centre: 0,
                        rho: 0,
                        population: 0,
                        origin: format!("death from state {sid} on ball {bi}"),
                    });
                    cl.index.insert(Vec::new(), id);
                    id
                })
            } else {
                let next_of = |c: u32| next[c as usize];
                let Some((succ, order)) = encode(ball, &support, &next_of, k) else {
                    cl.errors.push(format!(
                        "state {sid} ball {bi}: successor collar not edge-connected"
                    ));
                    continue;
                };
                if let Some(&c) = order.iter().find(|&&c| ball.tiles[c as usize].dist + 1 > r) {
                    cl.errors.push(format!(
                        "state {sid} ball {bi}: successor collar reaches ball rim (cell dist {})",
                        ball.tiles[c as usize].dist
                    ));
                    continue;
                }
                let (c2, rho2) = succ.centre();
                let t2 = order[c2] as usize;
                motion = (ball.tiles[t2].rot, ball.tiles[t2].offset);
                match cl.index.get(&succ.key) {
                    Some(&id) => id,
                    None => {
                        let id = cl.states.len();
                        cl.index.insert(succ.key.clone(), id);
                        cl.states.push(StateRec {
                            population: succ.population(),
                            sit: succ,
                            centre: c2,
                            rho: rho2,
                            origin: format!("from state {sid} on ball {bi}"),
                        });
                        worklist.push(id);
                        id
                    }
                }
            };
            cl.transitions.push(Transition {
                from: sid,
                ball: bi,
                to,
                rot: motion.0,
                offset: motion.1,
            });
        }
        if n_balls == 0 {
            cl.errors.push(format!(
                "state {sid}: no atlas ball extends it (atlas incomplete or R too small)"
            ));
        }
        if log && processed.is_multiple_of(25) {
            eprintln!(
                "  closure: {processed} states processed, {} known, {} transitions, worklist {}",
                cl.states.len(),
                cl.transitions.len(),
                worklist.len()
            );
        }
    }
    let mut used: Vec<usize> = cl.transitions.iter().map(|t| t.ball).collect();
    used.sort_unstable();
    used.dedup();
    cl.balls = used
        .into_iter()
        .map(|i| BallRef {
            index: i,
            root_ty: atlas.balls[i].root_ty,
            witness: atlas.balls[i].witness.clone(),
        })
        .collect();
    cl
}
