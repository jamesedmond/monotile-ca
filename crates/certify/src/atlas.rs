//! The legal ball atlas by transducer exploration.
//!
//! A *configuration* reads the centre tile's address one supertile
//! symbol at a time. Every stored tile owns one DFA run per edge (its
//! neighbour query). A run consumes its tile's address symbols as they
//! become known and emits its neighbour's address symbols, possibly
//! before accepting: the first emitted symbol names the neighbour's
//! type and back-edge, which places it exactly and identifies it (by
//! placement); later emitted symbols are delivered to the neighbour's
//! own runs. Acceptance means the two tiles share every higher
//! supertile, so from then on every symbol the source receives is
//! copied to the target (a *copy link*) and the run is dropped. Symbols
//! are consumed the moment they are emitted, so a configuration is
//! exactly: the stored tiles (types, classes, resolved gluings), the DFA
//! state of every live run, and the copy links — plus the top (layer,
//! type) that constrains the next symbol. Two configurations with the
//! same key have the same future, since every future emission is a
//! function of DFA states and the shared symbols still to come; keys are
//! bounded (finite region, finite DFA), so the exploration terminates.
//!
//! A configuration is a *leaf* once every tile within edge radius R has
//! a known class and no unresolved edge: the R-ball is then exact and
//! further symbols cannot change it. The leaves are exactly the legal
//! R-balls.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

use substitution_tiling_transducers::address::EdgeAddrSymbol;
use substitution_tiling_transducers::combinatorial::Transducer;
use substitution_tiling_transducers::common::{LayerIndex, System, TileIndex};
use substitution_tiling_transducers::geometric::TilePlacement;
use substitution_tiling_transducers::polyring::RingElement;

use crate::ball::{Ball, EdgeCtx, build_ball};
use crate::ctx::Ctx;

/// Memo-key layout (shared with the faithfulness checker, which
/// re-serialises its independently recomputed configuration in exactly
/// this format): `[layer u16][top type u16]`, then per stored tile in
/// canonical BFS order over forward resolved edges from the root:
/// `[type u16][class u16 or FFFF][slot count u8]` and per slot
/// `0` (outside) | `1 state u16` (pending) | `2 idx u16 back u8`
/// (resolved, query accepted) | `3 idx u16 back u8 state u16`
/// (resolved, query live), then `[count u8]` and the sorted canonical
/// indices of the tile's copy children.
pub const KEY_FORMAT_VERSION: u8 = 1;

#[derive(Clone, Debug)]
pub struct Run {
    pub state: usize,
    /// Output symbols emitted so far (the target's known address length
    /// contributed by this run).
    pub emitted: usize,
}

impl Run {
    fn start(tr: &Transducer, ty: TileIndex, edge: usize) -> (Self, Vec<EdgeAddrSymbol>) {
        let mut run = Self {
            state: tr.start,
            emitted: 0,
        };
        let out = run.feed(
            tr,
            EdgeAddrSymbol::Initial {
                base_type: ty,
                edge,
            },
        );
        (run, out)
    }

    fn feed(&mut self, tr: &Transducer, sym: EdgeAddrSymbol) -> Vec<EdgeAddrSymbol> {
        let (next, out) = tr.trans[&self.state]
            .get(&sym)
            .unwrap_or_else(|| panic!("illegal symbol {sym:?} in DFA state {}", self.state));
        self.state = *next;
        out.clone()
    }

    fn accepted(&self, tr: &Transducer) -> bool {
        tr.accept.contains(&self.state)
    }
}

#[derive(Clone, Debug)]
pub enum Slot {
    /// Query live, neighbour not yet identified.
    Pending(Run),
    /// Neighbour identified; `run` is `Some` while the query has not
    /// accepted (it still feeds the neighbour's address).
    Resolved {
        target: u32,
        back: u8,
        run: Option<Run>,
    },
    /// The neighbour lies outside the region: dropped.
    Outside,
}

#[derive(Clone)]
pub struct Tile {
    pub ty: TileIndex,
    /// `u16::MAX` until the level-1 symbol is known.
    pub class: u16,
    pub placement: TilePlacement,
    pub centroid: [f64; 2],
    pub slots: Vec<Slot>,
    /// Provisional edge distance from the centre over resolved edges.
    pub dist: u32,
    /// Address symbols known so far (level 0 = the type).
    pub known: usize,
    /// Tiles that receive a copy of every symbol this tile receives.
    pub copy_children: Vec<u32>,
    /// Debug (verify mode): the known address symbols.
    pub prefix: Vec<EdgeAddrSymbol>,
}

pub struct Region {
    pub centre: [f64; 2],
    pub radius: f64,
    pub d_tile: f64,
    /// Corridor variant (FINDINGS 10.11.22): the region is the set of
    /// points within `radius` of the segment `centre`–`end` instead of
    /// the disc around `centre`.
    pub end: Option<[f64; 2]>,
}

impl Region {
    pub fn dist(&self, c: [f64; 2]) -> f64 {
        match self.end {
            None => (c[0] - self.centre[0]).hypot(c[1] - self.centre[1]),
            Some(e) => {
                let d = [e[0] - self.centre[0], e[1] - self.centre[1]];
                let l2 = d[0] * d[0] + d[1] * d[1];
                let t = if l2 == 0.0 { 0.0 } else { (((c[0] - self.centre[0]) * d[0] + (c[1] - self.centre[1]) * d[1]) / l2).clamp(0.0, 1.0) };
                let q = [self.centre[0] + t * d[0], self.centre[1] + t * d[1]];
                (c[0] - q[0]).hypot(c[1] - q[1])
            }
        }
    }
    pub fn inside(&self, c: [f64; 2]) -> bool {
        self.dist(c) <= self.radius
    }
    /// A tile containing this edge midpoint has its centroid within
    /// `d_tile` of it, so it is certainly outside if the midpoint is
    /// farther than `radius + d_tile`.
    fn certainly_outside(&self, mid: [f64; 2]) -> bool {
        self.dist(mid) > self.radius + self.d_tile
    }
}

type Delivery = (u32, usize, EdgeAddrSymbol);

#[derive(Clone)]
pub struct Config {
    pub level: usize,
    pub layer: LayerIndex,
    pub top: TileIndex,
    pub tiles: Vec<Tile>,
    pub by_place: HashMap<TilePlacement, u32>,
    pub path: Vec<(TileIndex, usize)>,
    pub verify: bool,
}

impl Config {
    pub fn root(ctx: &Ctx, ty: TileIndex, region: &Region, verify: bool) -> Self {
        let placement = ctx.od.place_default(ty);
        let centroid = ctx.centroid(&placement);
        let mut by_place = HashMap::new();
        by_place.insert(placement.clone(), 0u32);
        let mut cfg = Self {
            level: 0,
            layer: ctx.cs.base_layer,
            top: ty,
            tiles: vec![Tile {
                ty,
                class: u16::MAX,
                placement,
                centroid,
                slots: (0..ctx.nedges(ty)).map(|_| Slot::Outside).collect(),
                dist: 0,
                known: 1,
                copy_children: Vec::new(),
                prefix: vec![EdgeAddrSymbol::Initial {
                    base_type: ty,
                    edge: 0,
                }],
            }],
            by_place,
            path: Vec::new(),
            verify,
        };
        let mut queue = VecDeque::new();
        cfg.spawn_runs(ctx, region, 0, &mut queue);
        cfg.deliver(ctx, region, queue);
        cfg.recompute_dists();
        cfg.check_consistency();
        cfg
    }

    /// Start the queries of a freshly stored tile: every edge not already
    /// marked, minus those whose neighbour is geometrically certain to
    /// lie outside the region.
    fn spawn_runs(&mut self, ctx: &Ctx, region: &Region, v: u32, queue: &mut VecDeque<Delivery>) {
        let ty = self.tiles[v as usize].ty;
        let placement = self.tiles[v as usize].placement.clone();
        for e in 0..ctx.nedges(ty) {
            let pre_resolved = matches!(self.tiles[v as usize].slots[e], Slot::Resolved { .. });
            if !pre_resolved && region.certainly_outside(ctx.edge_midpoint(&placement, e)) {
                self.tiles[v as usize].slots[e] = Slot::Outside;
                continue;
            }
            let (run, out) = Run::start(ctx.tr, ty, e);
            match &mut self.tiles[v as usize].slots[e] {
                Slot::Resolved { run: r, .. } => *r = Some(run),
                slot => *slot = Slot::Pending(run),
            }
            self.after_feed(ctx, region, v, e, out, queue);
        }
    }

    /// Read the next address symbol `(parent type, slot)` of the centre.
    pub fn feed(&mut self, ctx: &Ctx, region: &Region, sym: (TileIndex, usize)) {
        let (p, s) = sym;
        let layer = ctx.cs.lookup_layer(self.layer);
        assert_eq!(layer.subtiles[&p][s], self.top, "illegal parent symbol");
        self.level += 1;
        self.path.push(sym);
        self.layer = layer.parent;
        self.top = p;
        let esym = EdgeAddrSymbol::Supertile {
            parent_type: p,
            subtile: s,
        };
        let queue = VecDeque::from([(0u32, self.level, esym)]);
        self.deliver(ctx, region, queue);
        self.recompute_dists();
        self.check_consistency();
    }

    /// Deliver address symbols to tiles, feeding their runs and
    /// propagating emissions and copies until quiescent.
    fn deliver(&mut self, ctx: &Ctx, region: &Region, mut queue: VecDeque<Delivery>) {
        while let Some((v, level, sym)) = queue.pop_front() {
            let known = self.tiles[v as usize].known;
            if level < known {
                if self.verify && self.tiles[v as usize].prefix[level] != sym {
                    let t = &self.tiles[v as usize];
                    panic!(
                        "inconsistent address delivery to tile {v} (type {}, dist {}, known {}) at level {level}: had {:?}, got {sym:?}; config level {}, path {:?}; prefix {:?}",
                        t.ty.0, t.dist, t.known, t.prefix[level], self.level, self.path, t.prefix
                    );
                }
                continue;
            }
            assert_eq!(level, known, "address symbol delivered out of order to tile {v}");
            self.tiles[v as usize].known += 1;
            if self.verify {
                self.tiles[v as usize].prefix.push(sym);
            }
            if level == 1 {
                let EdgeAddrSymbol::Supertile {
                    parent_type,
                    subtile,
                } = sym
                else {
                    panic!("level-1 symbol is not a supertile symbol");
                };
                self.tiles[v as usize].class = ctx.class_of[&(parent_type, subtile)];
            }
            // Copy links established during this delivery (a run accepting
            // on this very symbol) must not carry this symbol: the run has
            // already emitted the target's own symbol for this level.
            let children = self.tiles[v as usize].copy_children.clone();
            let nslots = self.tiles[v as usize].slots.len();
            for e in 0..nslots {
                let out = {
                    let run = match &mut self.tiles[v as usize].slots[e] {
                        Slot::Pending(run) => run,
                        Slot::Resolved { run: Some(run), .. } => run,
                        _ => continue,
                    };
                    run.feed(ctx.tr, sym)
                };
                self.after_feed(ctx, region, v, e, out, &mut queue);
            }
            for c in children {
                queue.push_back((c, level, sym));
            }
        }
    }

    /// Process a run's fresh emissions (identify the neighbour on its
    /// first symbol, deliver the rest), then handle acceptance.
    fn after_feed(
        &mut self,
        ctx: &Ctx,
        region: &Region,
        v: u32,
        e: usize,
        out: Vec<EdgeAddrSymbol>,
        queue: &mut VecDeque<Delivery>,
    ) {
        for osym in out {
            let p = match &self.tiles[v as usize].slots[e] {
                Slot::Pending(run) | Slot::Resolved { run: Some(run), .. } => run.emitted,
                Slot::Outside => return, // run dropped (target outside)
                Slot::Resolved { run: None, .. } => unreachable!("accepted run still emitting"),
            };
            if p == 0 {
                let EdgeAddrSymbol::Initial {
                    base_type: wty,
                    edge: back,
                } = osym
                else {
                    panic!("first emitted symbol is not Initial");
                };
                if !self.identify(ctx, region, v, e, wty, back, queue) {
                    return;
                }
            } else {
                let target = match &self.tiles[v as usize].slots[e] {
                    Slot::Resolved { target, .. } => *target,
                    _ => unreachable!("emission before identification"),
                };
                queue.push_back((target, p, osym));
            }
            match &mut self.tiles[v as usize].slots[e] {
                Slot::Pending(run) | Slot::Resolved { run: Some(run), .. } => run.emitted += 1,
                _ => unreachable!(),
            }
        }
        // Acceptance: copy link, drop the run.
        let accepted = match &self.tiles[v as usize].slots[e] {
            Slot::Pending(run) | Slot::Resolved { run: Some(run), .. } => run.accepted(ctx.tr),
            _ => false,
        };
        if accepted {
            let (target, emitted) = match &self.tiles[v as usize].slots[e] {
                Slot::Resolved {
                    target,
                    run: Some(run),
                    ..
                } => (*target, run.emitted),
                Slot::Pending(_) => panic!("run accepted without identifying its neighbour"),
                _ => unreachable!(),
            };
            assert_eq!(
                emitted, self.tiles[v as usize].known,
                "accepted run's output has not caught up with its input"
            );
            if target != v && !self.tiles[v as usize].copy_children.contains(&target) {
                self.tiles[v as usize].copy_children.push(target);
            }
            if let Slot::Resolved { run, .. } = &mut self.tiles[v as usize].slots[e] {
                *run = None;
            }
        }
    }

    /// The run `(v, e)` has emitted its neighbour's type and back-edge.
    /// Returns whether the neighbour is stored (else the run is dropped).
    #[allow(clippy::too_many_arguments)]
    fn identify(
        &mut self,
        ctx: &Ctx,
        region: &Region,
        v: u32,
        e: usize,
        wty: TileIndex,
        back: usize,
        queue: &mut VecDeque<Delivery>,
    ) -> bool {
        let placement = ctx
            .od
            .place_next_to_tile(wty, back, &self.tiles[v as usize].placement, e);
        if let Slot::Resolved { target, back: b, .. } = &self.tiles[v as usize].slots[e] {
            let t = &self.tiles[*target as usize];
            assert!(
                t.ty == wty && usize::from(*b) == back && t.placement == placement,
                "pre-resolved slot contradicts the run's emission"
            );
            return true;
        }
        let Slot::Pending(run) = self.tiles[v as usize].slots[e].clone() else {
            unreachable!("identify on a non-pending slot");
        };
        match self.by_place.get(&placement).copied() {
            Some(w) => {
                assert_eq!(self.tiles[w as usize].ty, wty, "loop closure: type mismatch");
                self.tiles[v as usize].slots[e] = Slot::Resolved {
                    target: w,
                    back: back as u8,
                    run: Some(run),
                };
                self.link_reverse(w, back, v, e);
                true
            }
            None => {
                let centroid = ctx.centroid(&placement);
                if !region.inside(centroid) {
                    self.tiles[v as usize].slots[e] = Slot::Outside;
                    return false;
                }
                let w = self.tiles.len() as u32;
                let nedges = ctx.nedges(wty);
                let mut slots: Vec<Slot> = (0..nedges).map(|_| Slot::Outside).collect();
                slots[back] = Slot::Resolved {
                    target: v,
                    back: e as u8,
                    run: None,
                };
                self.tiles.push(Tile {
                    ty: wty,
                    class: u16::MAX,
                    placement: placement.clone(),
                    centroid,
                    slots,
                    dist: self.tiles[v as usize].dist + 1,
                    known: 1,
                    copy_children: Vec::new(),
                    prefix: vec![EdgeAddrSymbol::Initial {
                        base_type: wty,
                        edge: back,
                    }],
                });
                self.by_place.insert(placement, w);
                self.tiles[v as usize].slots[e] = Slot::Resolved {
                    target: w,
                    back: back as u8,
                    run: Some(run),
                };
                self.spawn_runs(ctx, region, w, queue);
                true
            }
        }
    }

    /// Record that `w`'s edge `back` leads to `v` (edge `e`), keeping any
    /// live run of that slot.
    fn link_reverse(&mut self, w: u32, back: usize, v: u32, e: usize) {
        match &mut self.tiles[w as usize].slots[back] {
            Slot::Pending(run) => {
                let run = run.clone();
                self.tiles[w as usize].slots[back] = Slot::Resolved {
                    target: v,
                    back: e as u8,
                    run: Some(run),
                };
            }
            Slot::Resolved { target, back: b, .. } => {
                assert!(
                    *target == v && usize::from(*b) == e,
                    "gluing asymmetry: {w}.{back} already resolved elsewhere"
                );
            }
            Slot::Outside => {
                // The reverse query was dropped as certainly outside, yet
                // its target is stored: the midpoint test is symmetric, so
                // this cannot happen.
                panic!("reverse slot marked outside for a stored neighbour");
            }
        }
    }

    fn recompute_dists(&mut self) {
        let n = self.tiles.len();
        let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (i, t) in self.tiles.iter().enumerate() {
            for s in &t.slots {
                if let Slot::Resolved { target, .. } = s {
                    adj[i].push(*target);
                    adj[*target as usize].push(i as u32);
                }
            }
        }
        let mut dist = vec![u32::MAX; n];
        dist[0] = 0;
        let mut q = VecDeque::from([0u32]);
        while let Some(c) = q.pop_front() {
            for &m in &adj[c as usize] {
                if dist[m as usize] == u32::MAX {
                    dist[m as usize] = dist[c as usize] + 1;
                    q.push_back(m);
                }
            }
        }
        for (t, d) in self.tiles.iter_mut().zip(dist) {
            assert!(d != u32::MAX, "stored tile unreachable from the centre");
            t.dist = d;
        }
    }

    fn check_consistency(&self) {
        for (u, t) in self.tiles.iter().enumerate() {
            for (e, s) in t.slots.iter().enumerate() {
                if let Slot::Resolved { target, back, .. } = s {
                    match &self.tiles[*target as usize].slots[*back as usize] {
                        Slot::Resolved { target: t2, back: b2, .. } => assert!(
                            *t2 == u as u32 && usize::from(*b2) == e,
                            "resolved edge {u}.{e} -> {target}.{back} has a different reverse"
                        ),
                        other => panic!("reverse of {u}.{e} is {other:?}"),
                    }
                }
            }
        }
    }

    pub fn is_leaf(&self, r: u32) -> bool {
        self.tiles.iter().all(|t| {
            t.dist > r
                || (t.class != u16::MAX
                    && t.slots
                        .iter()
                        .all(|s| !matches!(s, Slot::Pending(_))))
        })
    }

    /// Leaf test for a geometric region (FINDINGS 10.11.22): every stored
    /// tile whose centroid lies inside the region has a known class and
    /// no unresolved edge, so the region's pattern is exact and further
    /// symbols cannot change it.
    pub fn is_leaf_region(&self, region: &Region) -> bool {
        self.tiles.iter().all(|t| {
            !region.inside(t.centroid)
                || (t.class != u16::MAX && t.slots.iter().all(|s| !matches!(s, Slot::Pending(_))))
        })
    }

    pub fn live_runs(&self) -> usize {
        self.tiles
            .iter()
            .flat_map(|t| t.slots.iter())
            .filter(|s| matches!(s, Slot::Pending(_) | Slot::Resolved { run: Some(_), .. }))
            .count()
    }

    /// Canonical memo key (DFA states by their canonical ids).
    pub fn key(&self, ctx: &Ctx) -> Vec<u8> {
        let n = self.tiles.len();
        let mut order = vec![0u32];
        let mut index: Vec<u16> = vec![u16::MAX; n];
        index[0] = 0;
        let mut i = 0;
        while i < order.len() {
            let c = order[i] as usize;
            i += 1;
            for s in &self.tiles[c].slots {
                if let Slot::Resolved { target, .. } = s
                    && index[*target as usize] == u16::MAX
                {
                    index[*target as usize] = order.len() as u16;
                    order.push(*target);
                }
            }
        }
        assert_eq!(order.len(), n, "stored tile not reachable via forward resolved edges");
        let mut key = Vec::with_capacity(4 + n * 64);
        key.extend((self.layer.0 as u16).to_be_bytes());
        key.extend((self.top.0 as u16).to_be_bytes());
        for &c in &order {
            let t = &self.tiles[c as usize];
            key.extend((t.ty.0 as u16).to_be_bytes());
            key.extend(t.class.to_be_bytes());
            key.push(t.slots.len() as u8);
            for s in &t.slots {
                match s {
                    Slot::Outside => key.push(0),
                    Slot::Pending(run) => {
                        key.push(1);
                        key.extend(ctx.state_id(run.state).to_be_bytes());
                    }
                    Slot::Resolved { target, back, run } => {
                        key.push(if run.is_some() { 3 } else { 2 });
                        key.extend(index[*target as usize].to_be_bytes());
                        key.push(*back);
                        if let Some(run) = run {
                            key.extend(ctx.state_id(run.state).to_be_bytes());
                        }
                    }
                }
            }
            let mut children: Vec<u16> = t
                .copy_children
                .iter()
                .map(|&c| index[c as usize])
                .collect();
            children.sort_unstable();
            key.push(children.len() as u8);
            for c in children {
                key.extend(c.to_be_bytes());
            }
        }
        key
    }

    /// Extract the exact R-ball of a leaf configuration.
    pub fn ball(&self, ctx: &Ctx, r: u32) -> Result<Ball, String> {
        let mut buckets: HashMap<RingElement, Vec<u32>> = HashMap::new();
        for (i, t) in self.tiles.iter().enumerate() {
            for v in ctx.od.tile_vertices(&t.placement) {
                let cells = buckets.entry(v).or_default();
                if cells.last() != Some(&(i as u32)) {
                    cells.push(i as u32);
                }
            }
        }
        let mut vadj: Vec<Vec<u32>> = vec![Vec::new(); self.tiles.len()];
        for cells in buckets.values() {
            for &a in cells {
                for &b in cells {
                    if a != b {
                        vadj[a as usize].push(b);
                    }
                }
            }
        }
        for v in vadj.iter_mut() {
            v.sort_unstable();
            v.dedup();
        }
        let view = ConfigView { cfg: self, vadj };
        let witness: Vec<(usize, usize)> = self.path.iter().map(|&(p, s)| (p.0, s)).collect();
        build_ball(&view, 0, r, witness.clone())
            .map_err(|e| format!("{e} (region too small? level {}, path {:?})", self.level, witness))
    }
}

struct ConfigView<'a> {
    cfg: &'a Config,
    vadj: Vec<Vec<u32>>,
}

impl EdgeCtx for ConfigView<'_> {
    fn ty(&self, c: u32) -> usize {
        self.cfg.tiles[c as usize].ty.0
    }
    fn class(&self, c: u32) -> u16 {
        self.cfg.tiles[c as usize].class
    }
    fn nedges(&self, c: u32) -> usize {
        self.cfg.tiles[c as usize].slots.len()
    }
    fn edge(&self, c: u32, e: usize) -> Option<(u32, u8)> {
        match &self.cfg.tiles[c as usize].slots[e] {
            Slot::Resolved { target, back, .. } => Some((*target, *back)),
            _ => None,
        }
    }
    fn centroid(&self, c: u32) -> [f64; 2] {
        self.cfg.tiles[c as usize].centroid
    }
    fn vertex_neighbours(&self, c: u32) -> Vec<u32> {
        self.vadj[c as usize].clone()
    }
    fn placement(&self, c: u32) -> (u8, [f64; 2]) {
        placement_of(&self.cfg.tiles[c as usize].placement)
    }
}

/// Rotation index and translation of an exact placement.
pub fn placement_of(p: &TilePlacement) -> (u8, [f64; 2]) {
    let rot = crate::ball::rotation_index(p.rotation.to_float().expect("unit rotation"));
    let offset = p.offset.to_float().expect("offset converts");
    (rot, offset)
}

pub fn key_hash(key: &[u8]) -> u128 {
    let mut h1 = DefaultHasher::new();
    key.hash(&mut h1);
    let mut h2 = DefaultHasher::new();
    (0x5A17u16, key).hash(&mut h2);
    (u128::from(h1.finish()) << 64) | u128::from(h2.finish())
}

#[derive(Default, Debug)]
pub struct ExploreStats {
    pub nodes: u64,
    pub leaves: u64,
    pub balls: usize,
    pub max_level: usize,
    pub max_tiles: usize,
    pub max_live_runs: usize,
    pub max_stack: usize,
}

pub struct ExploreOpts {
    pub region_factor: f64,
    pub verify: bool,
    pub full_keys: bool,
    pub log_every: u64,
    /// Abort if more than this many nodes are expanded (0 = unlimited).
    pub max_nodes: u64,
    /// Record the configuration graph (requires `full_keys`).
    pub dump: bool,
}

/// One configuration of the exploration graph: its address path, its
/// memo key (hex), and its children by symbol — the object the
/// faithfulness checker re-derives node by node.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct GraphNode {
    pub path: Vec<(usize, usize)>,
    pub key: String,
    pub leaf: bool,
    /// Ball key (hex) at a leaf.
    pub ball_key: Option<String>,
    /// (parent type, slot, child node id).
    pub children: Vec<(usize, usize, usize)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ExplorationGraph {
    pub family: String,
    pub root_ty: usize,
    pub radius: u32,
    pub region_factor: f64,
    pub nodes: Vec<GraphNode>,
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Enumerate every legal R-ball around a tile of type `ty`.
pub fn explore(
    ctx: &Ctx,
    ty: TileIndex,
    r: u32,
    opts: &ExploreOpts,
) -> Result<(Vec<Ball>, ExploreStats, Option<ExplorationGraph>), String> {
    assert!(!opts.dump || opts.full_keys, "graph dump needs full keys");
    let root_placement = ctx.od.place_default(ty);
    let region = Region {
        centre: ctx.centroid(&root_placement),
        radius: opts.region_factor * (f64::from(r) + 1.0) * ctx.d_nb,
        d_tile: ctx.d_tile,
        end: None,
    };
    let root = Config::root(ctx, ty, &region, opts.verify);
    // Visited set: full keys (mapped to node ids) or 128-bit hashes.
    let mut visited_full: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut visited_hash: HashSet<u128> = HashSet::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut visit = |key: Vec<u8>, path: &[(TileIndex, usize)], nodes: &mut Vec<GraphNode>| -> (bool, usize) {
        if opts.full_keys {
            let next = visited_full.len();
            match visited_full.entry(key) {
                std::collections::hash_map::Entry::Occupied(e) => (false, *e.get()),
                std::collections::hash_map::Entry::Vacant(e) => {
                    if opts.dump {
                        nodes.push(GraphNode {
                            path: path.iter().map(|&(p, s)| (p.0, s)).collect(),
                            key: hex(e.key()),
                            leaf: false,
                            ball_key: None,
                            children: Vec::new(),
                        });
                    }
                    e.insert(next);
                    (true, next)
                }
            }
        } else {
            (visited_hash.insert(key_hash(&key)), 0)
        }
    };
    let (_, root_id) = visit(root.key(ctx), &root.path, &mut nodes);
    let mut stack = vec![(root, root_id)];
    let mut balls: HashMap<Vec<u8>, Ball> = HashMap::new();
    let mut stats = ExploreStats::default();
    while let Some((cfg, id)) = stack.pop() {
        stats.nodes += 1;
        if opts.max_nodes > 0 && stats.nodes > opts.max_nodes {
            return Err(format!("node budget {} exceeded", opts.max_nodes));
        }
        stats.max_level = stats.max_level.max(cfg.level);
        stats.max_tiles = stats.max_tiles.max(cfg.tiles.len());
        stats.max_live_runs = stats.max_live_runs.max(cfg.live_runs());
        if opts.log_every > 0 && stats.nodes.is_multiple_of(opts.log_every) {
            eprintln!(
                "  {} nodes, {} leaves, {} balls, stack {}, level {}, tiles {}, live runs {}",
                stats.nodes,
                stats.leaves,
                balls.len(),
                stack.len(),
                cfg.level,
                cfg.tiles.len(),
                cfg.live_runs()
            );
        }
        if cfg.is_leaf(r) {
            stats.leaves += 1;
            let ball = cfg.ball(ctx, r)?;
            if opts.dump {
                nodes[id].leaf = true;
                nodes[id].ball_key = Some(hex(&ball.key));
            }
            balls.entry(ball.key.clone()).or_insert(ball);
            continue;
        }
        let parents = ctx.legal_parents(cfg.layer, cfg.top);
        assert!(!parents.is_empty(), "no legal parent for the top type");
        for &(p, s) in parents {
            let mut child = cfg.clone();
            child.feed(ctx, &region, (p, s));
            let (new, cid) = visit(child.key(ctx), &child.path, &mut nodes);
            if opts.dump {
                nodes[id].children.push((p.0, s, cid));
            }
            if new {
                stack.push((child, cid));
            }
        }
        stats.max_stack = stats.max_stack.max(stack.len());
    }
    stats.balls = balls.len();
    let mut out: Vec<Ball> = balls.into_values().collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    let graph = opts.dump.then(|| ExplorationGraph {
        family: format!("{:?}", ctx.family).to_lowercase(),
        root_ty: ty.0,
        radius: r,
        region_factor: opts.region_factor,
        nodes,
    });
    Ok((out, stats, graph))
}

/// Name of a type, for reports.
pub fn type_name(ctx: &Ctx, ty: usize) -> String {
    ctx.cs.lookup_tile_name(TileIndex(ty)).to_string()
}
