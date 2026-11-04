//! Balls: the canonical, location-free encoding of the tiling within
//! edge radius R of a centre tile — tile types, classes, intrinsic edge
//! gluings — plus the vertex adjacency derived from exact geometry. One
//! encoder serves every source of terrain (a transducer exploration
//! configuration, a library-generated patch, a finite-address patch),
//! so atlas balls and patch balls are byte-comparable.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

/// Read access to a piece of tiling, by cell id. `edge` returns the
/// neighbour across intrinsic edge `e` and the back-edge index, or
/// `None` if unknown / outside the available terrain.
pub trait EdgeCtx {
    fn ty(&self, c: u32) -> usize;
    fn class(&self, c: u32) -> u16;
    fn nedges(&self, c: u32) -> usize;
    fn edge(&self, c: u32, e: usize) -> Option<(u32, u8)>;
    fn centroid(&self, c: u32) -> [f64; 2];
    /// Tiles sharing at least one vertex with `c` (the vertex
    /// neighbourhood); only meaningful where the fan is complete.
    fn vertex_neighbours(&self, c: u32) -> Vec<u32>;
    /// Rigid placement of the tile: rotation as a multiple of 30°
    /// (index 0..12) and translation, relative to the prototile's
    /// reference position.
    fn placement(&self, c: u32) -> (u8, [f64; 2]);
}

/// Rotation index of a unit complex number: nearest multiple of 30°.
pub fn rotation_index(unit: [f64; 2]) -> u8 {
    let deg = unit[1].atan2(unit[0]).to_degrees().rem_euclid(360.0);
    let idx = (deg / 30.0).round();
    let err = (deg - idx * 30.0).abs();
    assert!(err < 1e-6 || (err - 360.0).abs() < 1e-6, "rotation {deg}° is not a multiple of 30°");
    (idx as u8) % 12
}

/// Rotate a vector by `index` × 30°.
pub fn rotate(v: [f64; 2], index: u8) -> [f64; 2] {
    let a = f64::from(index) * 30.0f64.to_radians();
    let (s, c) = a.sin_cos();
    [c * v[0] - s * v[1], s * v[0] + c * v[1]]
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BallTile {
    pub ty: usize,
    pub class: u16,
    pub dist: u32,
    /// Per intrinsic edge: (ball index, back-edge) or `None` when the
    /// neighbour lies outside the ball.
    pub slots: Vec<Option<(u16, u8)>>,
    pub centroid: [f64; 2],
    /// Placement relative to the ball root's frame (root: rot 0, offset 0).
    #[serde(default)]
    pub rot: u8,
    #[serde(default)]
    pub offset: [f64; 2],
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Ball {
    pub root_ty: usize,
    pub radius: u32,
    /// Canonical key: a function of types, classes and gluings only.
    pub key: Vec<u8>,
    /// Tiles in canonical order (index 0 = centre).
    pub tiles: Vec<BallTile>,
    /// Vertex adjacency (sorted ball indices), complete for tiles with
    /// `dist <= radius - 2`; empty beyond.
    pub vadj: Vec<Vec<u16>>,
    /// Address path of the centre from its base type: (parent type,
    /// slot) per level — a legal finite address realising this ball.
    pub witness: Vec<(usize, usize)>,
}

/// Fan margin: vertex neighbours lie within this edge distance (measured
/// 2 on hat/spectre; asserted while building).
pub const FAN_MARGIN: u32 = 2;

pub const OUTSIDE: [u8; 3] = [0xFF, 0xFF, 0xFF];

/// Canonical BFS numbering of `members` from `root` in intrinsic edge
/// order over `ctx.edge`, restricted to `members` (any cell with
/// `members[c] == Some(dist)`). Returns the order; every member must be
/// reachable.
pub fn canonical_order<C: EdgeCtx>(
    ctx: &C,
    root: u32,
    is_member: &dyn Fn(u32) -> bool,
) -> (Vec<u32>, HashMap<u32, u16>) {
    let mut order = vec![root];
    let mut index: HashMap<u32, u16> = HashMap::new();
    index.insert(root, 0);
    let mut i = 0;
    while i < order.len() {
        let c = order[i];
        i += 1;
        for e in 0..ctx.nedges(c) {
            if let Some((n, _)) = ctx.edge(c, e)
                && is_member(n)
                && !index.contains_key(&n)
            {
                index.insert(n, order.len() as u16);
                order.push(n);
            }
        }
    }
    (order, index)
}

/// Build the radius-`r` ball around `root`. Requires every tile within
/// distance `r - 1` to have all edges known (else the ball is not
/// determined — returns `Err` naming the offending cell).
pub fn build_ball<C: EdgeCtx>(
    ctx: &C,
    root: u32,
    r: u32,
    witness: Vec<(usize, usize)>,
) -> Result<Ball, String> {
    // Multi-source-free BFS to depth r.
    let mut dist: HashMap<u32, u32> = HashMap::new();
    dist.insert(root, 0);
    let mut queue = VecDeque::from([root]);
    while let Some(c) = queue.pop_front() {
        let d = dist[&c];
        if d >= r {
            continue;
        }
        for e in 0..ctx.nedges(c) {
            match ctx.edge(c, e) {
                Some((n, _)) => {
                    if let std::collections::hash_map::Entry::Vacant(v) = dist.entry(n) {
                        v.insert(d + 1);
                        queue.push_back(n);
                    }
                }
                None => {
                    return Err(format!(
                        "cell {c} at distance {d} has undetermined edge {e}"
                    ));
                }
            }
        }
    }
    let is_member = |c: u32| dist.contains_key(&c);
    let (order, index) = canonical_order(ctx, root, &is_member);
    assert_eq!(order.len(), dist.len(), "ball not connected from root");

    let mut tiles = Vec::with_capacity(order.len());
    for &c in &order {
        let ne = ctx.nedges(c);
        let mut slots = Vec::with_capacity(ne);
        for e in 0..ne {
            slots.push(match ctx.edge(c, e) {
                Some((n, back)) => index.get(&n).map(|&i| (i, back)),
                None => None,
            });
        }
        let (rot, offset) = ctx.placement(c);
        tiles.push(BallTile {
            ty: ctx.ty(c),
            class: ctx.class(c),
            dist: dist[&c],
            slots,
            centroid: ctx.centroid(c),
            rot,
            offset,
        });
    }
    // Vertex adjacency for tiles whose fan is complete inside the ball.
    let mut vadj: Vec<Vec<u16>> = vec![Vec::new(); order.len()];
    for (i, &c) in order.iter().enumerate() {
        let d = tiles[i].dist;
        if d + FAN_MARGIN > r {
            continue;
        }
        let mut vs: Vec<u16> = Vec::new();
        for n in ctx.vertex_neighbours(c) {
            let &j = index
                .get(&n)
                .unwrap_or_else(|| panic!("vertex neighbour of ring-{d} cell outside ball"));
            let dj = tiles[j as usize].dist;
            assert!(
                dj <= d + FAN_MARGIN,
                "fan margin violated: ring {d} tile has vertex neighbour at ring {dj}"
            );
            vs.push(j);
        }
        vs.sort_unstable();
        vs.dedup();
        vadj[i] = vs;
    }
    let key = ball_key(&tiles);
    Ok(Ball {
        root_ty: tiles[0].ty,
        radius: r,
        key,
        tiles,
        vadj,
        witness,
    })
}

pub fn ball_key(tiles: &[BallTile]) -> Vec<u8> {
    let mut key = Vec::with_capacity(tiles.len() * 48);
    for t in tiles {
        key.extend((t.ty as u16).to_be_bytes());
        key.extend(t.class.to_be_bytes());
        key.push(t.slots.len() as u8);
        for s in &t.slots {
            match s {
                Some((i, b)) => {
                    key.extend(i.to_be_bytes());
                    key.push(*b);
                }
                None => key.extend(OUTSIDE),
            }
        }
    }
    key
}

impl EdgeCtx for Ball {
    fn ty(&self, c: u32) -> usize {
        self.tiles[c as usize].ty
    }
    fn class(&self, c: u32) -> u16 {
        self.tiles[c as usize].class
    }
    fn nedges(&self, c: u32) -> usize {
        self.tiles[c as usize].slots.len()
    }
    fn edge(&self, c: u32, e: usize) -> Option<(u32, u8)> {
        self.tiles[c as usize].slots[e].map(|(i, b)| (u32::from(i), b))
    }
    fn centroid(&self, c: u32) -> [f64; 2] {
        self.tiles[c as usize].centroid
    }
    fn vertex_neighbours(&self, c: u32) -> Vec<u32> {
        self.vadj[c as usize].iter().map(|&i| u32::from(i)).collect()
    }
    fn placement(&self, c: u32) -> (u8, [f64; 2]) {
        (self.tiles[c as usize].rot, self.tiles[c as usize].offset)
    }
}

/// The atlas: balls keyed by canonical key, per root type.
#[derive(Serialize, Deserialize, Default)]
pub struct Atlas {
    pub family: String,
    pub radius: u32,
    /// Geometric region factor the exploration used (region radius =
    /// factor × (R + 1) × max neighbour-centroid distance).
    #[serde(default = "default_factor")]
    pub region_factor: f64,
    pub balls: Vec<Ball>,
}

fn default_factor() -> f64 {
    1.0
}

impl Atlas {
    pub fn index(&self) -> HashMap<&[u8], usize> {
        self.balls
            .iter()
            .enumerate()
            .map(|(i, b)| (b.key.as_slice(), i))
            .collect()
    }

    pub fn by_type(&self, ty: usize) -> impl Iterator<Item = (usize, &Ball)> {
        self.balls
            .iter()
            .enumerate()
            .filter(move |(_, b)| b.root_ty == ty)
    }
}
