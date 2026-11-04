//! Situations: a pattern (non-quiescent cells with their states)
//! together with the plain k-collar of its support — classes and
//! intrinsic gluings of every cell within edge distance k — canonically
//! encoded by edge-ordered BFS from an anchor with lexicographic
//! minimisation over support anchors (FINDINGS §10.6's key, without
//! vertex degrees: the closure computes the step on a full atlas ball
//! where degrees are known, so the key only has to name the state and
//! carry causal memory).

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::ball::{Ball, EdgeCtx, OUTSIDE};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SitCell {
    pub state: u8,
    pub class: u16,
    pub edges: Vec<Option<(u16, u8)>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Situation {
    pub k: u8,
    /// Canonical order; index 0 is the anchor (a support cell). Not
    /// serialised: it is a decoding of `key` (see [`Situation::restore`]).
    #[serde(skip)]
    pub cells: Vec<SitCell>,
    /// Canonical key bytes, serialised as a hex string.
    #[serde(with = "hex_bytes")]
    pub key: Vec<u8>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(v.len() * 2);
        for b in v {
            out.push_str(&format!("{b:02x}"));
        }
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}

/// Collar members (cell → depth) within edge distance `k` of `support`.
pub fn collar<C: EdgeCtx>(ctx: &C, support: &[u32], k: u8) -> HashMap<u32, u8> {
    let mut depth: HashMap<u32, u8> = HashMap::new();
    let mut queue = VecDeque::new();
    for &c in support {
        depth.insert(c, 0);
        queue.push_back(c);
    }
    while let Some(c) = queue.pop_front() {
        let d = depth[&c];
        if d >= k {
            continue;
        }
        for e in 0..ctx.nedges(c) {
            if let Some((n, _)) = ctx.edge(c, e)
                && !depth.contains_key(&n)
            {
                depth.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }
    depth
}

fn encode_from<C: EdgeCtx>(
    ctx: &C,
    anchor: u32,
    members: &HashMap<u32, u8>,
    state: &dyn Fn(u32) -> u8,
) -> Option<(Vec<u8>, Vec<u32>)> {
    let mut order = vec![anchor];
    let mut index: HashMap<u32, u16> = HashMap::new();
    index.insert(anchor, 0);
    let mut bytes = Vec::with_capacity(members.len() * 48);
    let mut i = 0;
    while i < order.len() {
        let c = order[i];
        i += 1;
        bytes.push(state(c));
        bytes.extend(ctx.class(c).to_be_bytes());
        let ne = ctx.nedges(c);
        bytes.push(ne as u8);
        for e in 0..ne {
            match ctx.edge(c, e) {
                Some((n, back)) if members.contains_key(&n) => {
                    let idx = match index.get(&n) {
                        Some(&j) => j,
                        None => {
                            let j = order.len() as u16;
                            index.insert(n, j);
                            order.push(n);
                            j
                        }
                    };
                    bytes.extend(idx.to_be_bytes());
                    bytes.push(back);
                }
                _ => bytes.extend(OUTSIDE),
            }
        }
    }
    (order.len() == members.len()).then_some((bytes, order))
}

/// Canonical situation of `support` (cells with non-zero state) at
/// collar radius `k`. Returns the situation and the canonical cell order
/// (context cell ids). `None` if the collar is not edge-connected from
/// some anchor (the key would silently drop cells).
pub fn encode<C: EdgeCtx>(
    ctx: &C,
    support: &[u32],
    state: &dyn Fn(u32) -> u8,
    k: u8,
) -> Option<(Situation, Vec<u32>)> {
    assert!(!support.is_empty());
    let members = collar(ctx, support, k);
    let mut best: Option<(Vec<u8>, Vec<u32>)> = None;
    for &a in support {
        let cand = encode_from(ctx, a, &members, state)?;
        if best.as_ref().is_none_or(|b| cand.0 < b.0) {
            best = Some(cand);
        }
    }
    let (key, order) = best?;
    let cells = decode_cells(&key);
    Some((Situation { k, cells, key }, order))
}

fn decode_cells(key: &[u8]) -> Vec<SitCell> {
    let mut cells = Vec::new();
    let mut p = 0;
    while p < key.len() {
        let state = key[p];
        let class = u16::from_be_bytes([key[p + 1], key[p + 2]]);
        let ne = key[p + 3] as usize;
        p += 4;
        let mut edges = Vec::with_capacity(ne);
        for _ in 0..ne {
            let b = [key[p], key[p + 1], key[p + 2]];
            p += 3;
            edges.push(if b == OUTSIDE {
                None
            } else {
                Some((u16::from_be_bytes([b[0], b[1]]), b[2]))
            });
        }
        cells.push(SitCell {
            state,
            class,
            edges,
        });
    }
    cells
}

/// A situation's own collar graph as terrain, for re-encoding it at a
/// smaller collar radius (projection).
struct SitGraph<'a>(&'a [SitCell]);

impl EdgeCtx for SitGraph<'_> {
    fn ty(&self, _c: u32) -> usize {
        0
    }
    fn class(&self, c: u32) -> u16 {
        self.0[c as usize].class
    }
    fn nedges(&self, c: u32) -> usize {
        self.0[c as usize].edges.len()
    }
    fn edge(&self, c: u32, e: usize) -> Option<(u32, u8)> {
        self.0[c as usize].edges[e].map(|(i, b)| (u32::from(i), b))
    }
    fn centroid(&self, _c: u32) -> [f64; 2] {
        [0.0, 0.0]
    }
    fn vertex_neighbours(&self, _c: u32) -> Vec<u32> {
        Vec::new()
    }
    fn placement(&self, _c: u32) -> (u8, [f64; 2]) {
        (0, [0.0, 0.0])
    }
}

impl Situation {
    /// Rebuild `cells` from `key` after deserialisation.
    pub fn restore(&mut self) {
        if self.cells.is_empty() && !self.key.is_empty() {
            self.cells = decode_cells(&self.key);
        }
    }

    /// The same situation at a smaller collar radius `k2 <= k`: the
    /// canonical key of the pattern with cells within `k2` of the support.
    pub fn project(&self, k2: u8) -> Vec<u8> {
        assert!(k2 <= self.k);
        let g = SitGraph(&self.cells);
        let support: Vec<u32> = self.support().into_iter().map(|i| i as u32).collect();
        let state_of = |c: u32| self.cells[c as usize].state;
        encode(&g, &support, &state_of, k2)
            .map(|(s, _)| s.key)
            .expect("projection of a connected situation is connected")
    }

    /// The key of this situation with its states renamed by `map` (`map[s]` is the new state of
    /// a cell in state `s`; 0 deletes the cell; states beyond the map are kept), re-encoded at
    /// the same collar radius. `None` if no cell survives or the survivors are disconnected.
    pub fn relabel(&self, map: &[u8]) -> Option<Vec<u8>> {
        let m = |s: u8| map.get(s as usize).copied().unwrap_or(s);
        let g = SitGraph(&self.cells);
        let support: Vec<u32> = (0..self.cells.len()).filter(|&i| m(self.cells[i].state) != 0).map(|i| i as u32).collect();
        if support.is_empty() {
            return None;
        }
        let state_of = |c: u32| m(self.cells[c as usize].state);
        encode(&g, &support, &state_of, self.k).map(|(s, _)| s.key)
    }

    pub fn support(&self) -> Vec<usize> {
        (0..self.cells.len())
            .filter(|&i| self.cells[i].state != 0)
            .collect()
    }

    pub fn population(&self) -> usize {
        self.cells.iter().filter(|c| c.state != 0).count()
    }

    /// All-pairs BFS distances within the situation graph from `from`.
    fn dists_from(&self, from: usize) -> Vec<u32> {
        let mut d = vec![u32::MAX; self.cells.len()];
        d[from] = 0;
        let mut q = VecDeque::from([from]);
        while let Some(c) = q.pop_front() {
            for e in &self.cells[c].edges {
                if let Some((n, _)) = e
                    && d[*n as usize] == u32::MAX
                {
                    d[*n as usize] = d[c] + 1;
                    q.push_back(*n as usize);
                }
            }
        }
        d
    }

    /// The support cell minimising the max distance to other support
    /// cells (ties → lowest index), and that radius ρ.
    pub fn centre(&self) -> (usize, u32) {
        let support = self.support();
        let mut best = (usize::MAX, u32::MAX);
        for &c in &support {
            let d = self.dists_from(c);
            let rho = support.iter().map(|&s| d[s]).max().unwrap();
            if rho < best.1 {
                best = (c, rho);
            }
        }
        best
    }

    /// Embed the situation into `ball` with `centre` mapped to the
    /// ball's root. Returns the map situation index → ball index, or
    /// `None` if the ball does not extend this situation.
    pub fn embed(&self, centre: usize, ball: &Ball) -> Option<Vec<u16>> {
        let n = self.cells.len();
        let mut map: Vec<Option<u16>> = vec![None; n];
        if ball.tiles[0].class != self.cells[centre].class
            || ball.tiles[0].slots.len() != self.cells[centre].edges.len()
        {
            return None;
        }
        map[centre] = Some(0);
        let mut queue = VecDeque::from([centre]);
        while let Some(i) = queue.pop_front() {
            let b = map[i].unwrap() as usize;
            for (e, edge) in self.cells[i].edges.iter().enumerate() {
                if let Some((j, back)) = *edge {
                    let (b2, back2) = ball.tiles[b].slots[e]?;
                    if back2 != back {
                        return None;
                    }
                    match map[j as usize] {
                        Some(m) => {
                            if m != b2 {
                                return None;
                            }
                        }
                        None => {
                            let t = &ball.tiles[b2 as usize];
                            if t.class != self.cells[j as usize].class
                                || t.slots.len() != self.cells[j as usize].edges.len()
                            {
                                return None;
                            }
                            map[j as usize] = Some(b2);
                            queue.push_back(j as usize);
                        }
                    }
                }
            }
        }
        let map: Vec<u16> = map.into_iter().map(|m| m.expect("situation connected")).collect();
        // Injective, and "outside" edges really leave the image.
        let mut image: HashMap<u16, usize> = HashMap::new();
        for (i, &b) in map.iter().enumerate() {
            if image.insert(b, i).is_some() {
                return None;
            }
        }
        for (i, cell) in self.cells.iter().enumerate() {
            let b = map[i] as usize;
            for (e, edge) in cell.edges.iter().enumerate() {
                if edge.is_none()
                    && let Some((b2, _)) = ball.tiles[b].slots[e]
                    && image.contains_key(&b2)
                {
                    return None;
                }
            }
        }
        Some(map)
    }
}
