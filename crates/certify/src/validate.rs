//! Two checks on the atlas, in opposite directions.
//!
//! * Completeness on data: every R-ball occurring deep inside a large
//!   library-generated patch must be in the atlas (and carry the same
//!   vertex adjacency).
//! * Soundness of each atlas entry, by an independent driver: regenerate
//!   the ball from its witness prefix with the library's own
//!   finite-address neighbour computation and re-encode it; the key must
//!   match. This checks this crate's DFA driver against the library's.

use std::collections::{HashMap, HashSet};

use substitution_tiling_transducers::address::{FiniteTileAddress, TileAddrSymbol, TileAddress};
use substitution_tiling_transducers::common::TileIndex;
use substitution_tiling_transducers::geometric::TilePlacement;
use substitution_tiling_transducers::polyring::RingElement;

use crate::ball::{Atlas, EdgeCtx, build_ball};
use crate::ctx::Ctx;
use crate::patchctx::PatchCtx;

#[derive(Default, Debug)]
pub struct PatchReport {
    pub checked: usize,
    pub found: usize,
    pub missing: Vec<u32>,
    pub vadj_mismatch: Vec<u32>,
    pub distinct_in_patch: usize,
}

/// Check every cell of the patch whose R-ball (plus fan guard) is fully
/// inside it, stepping by `stride` through the cell list.
pub fn check_patch(atlas: &Atlas, pctx: &PatchCtx, stride: usize) -> PatchReport {
    let r = atlas.radius;
    let index = atlas.index();
    let mut rep = PatchReport::default();
    let mut distinct: HashSet<Vec<u8>> = HashSet::new();
    for c in (0..pctx.cells()).step_by(stride.max(1)) {
        if pctx.distance(c) + r + 3 > pctx.radius {
            continue;
        }
        let ball = build_ball(pctx, c, r, Vec::new()).expect("patch ball determined");
        rep.checked += 1;
        distinct.insert(ball.key.clone());
        match index.get(ball.key.as_slice()) {
            Some(&i) => {
                rep.found += 1;
                if atlas.balls[i].vadj != ball.vadj {
                    rep.vadj_mismatch.push(c);
                }
            }
            None => rep.missing.push(c),
        }
    }
    rep.distinct_in_patch = distinct.len();
    rep
}

/// Terrain generated from one finite address with the library's own
/// transducer driver (`neighbour_partial`): tiles identified by
/// placement as soon as a query emits their type, each tile's address
/// being the longest prefix any query has produced for it, iterated to a
/// fixed point so that accepted queries propagate copied symbols exactly
/// as the atlas exploration's copy links do.
struct FiniteCtx<'a> {
    ctx: &'a Ctx<'a>,
    tiles: Vec<(FiniteTileAddress, TilePlacement, TileIndex)>,
    edges: Vec<Vec<Option<(u32, u8)>>>,
    vadj: Vec<Vec<u32>>,
    centroids: Vec<[f64; 2]>,
}

impl<'a> FiniteCtx<'a> {
    /// Terrain within the same geometric region the atlas exploration
    /// used (tiles whose centroid lies within `region_radius` of the
    /// root's), every stored tile queried along every edge.
    fn new(ctx: &'a Ctx<'a>, root: FiniteTileAddress, region_radius: f64) -> Self {
        let tr = ctx.tr;
        let ty0 = root.base_type().expect("root has a type");
        let root_placement = ctx.od.place_default(ty0);
        let centre = ctx.centroid(&root_placement);
        let inside = |c: [f64; 2]| (c[0] - centre[0]).hypot(c[1] - centre[1]) <= region_radius;
        let mut tiles = vec![(root, root_placement, ty0)];
        let mut by_place: HashMap<TilePlacement, u32> = HashMap::new();
        by_place.insert(tiles[0].1.clone(), 0);
        let mut edges: Vec<Vec<Option<(u32, u8)>>> = vec![Vec::new()];
        loop {
            let mut changed = false;
            let n = tiles.len();
            for i in 0..n {
                let (addr, placement, ty) = tiles[i].clone();
                let mut row = vec![None; ctx.nedges(ty)];
                for (e, slot) in row.iter_mut().enumerate() {
                    let (out, back) = addr.neighbour_partial(e, tr);
                    let Ok(back) = back else { continue };
                    let nty = out.base_type().unwrap();
                    let np = ctx.od.place_next_to_tile(nty, back, &placement, e);
                    if !by_place.contains_key(&np) && !inside(ctx.centroid(&np)) {
                        continue;
                    }
                    let j = match by_place.get(&np) {
                        Some(&j) => {
                            // Keep the longest known prefix; prefixes must agree.
                            let have = &tiles[j as usize].0;
                            let (short, long) = if have.len() < out.len() { (have, &out) } else { (&out, have) };
                            for k in 0..short.len() {
                                assert_eq!(
                                    short.get(k).unwrap(),
                                    long.get(k).unwrap(),
                                    "library outputs disagree on a tile's address"
                                );
                            }
                            if out.len() > have.len() {
                                tiles[j as usize].0 = out.clone();
                                changed = true;
                            }
                            j
                        }
                        None => {
                            let j = tiles.len() as u32;
                            by_place.insert(np.clone(), j);
                            tiles.push((out.clone(), np, nty));
                            edges.push(Vec::new());
                            changed = true;
                            j
                        }
                    };
                    *slot = Some((j, back as u8));
                }
                // Merge with what earlier rounds (and symmetric completion)
                // established; never lose an edge.
                let mut merged = std::mem::take(&mut edges[i]);
                if merged.len() != row.len() {
                    merged = vec![None; row.len()];
                }
                for (m, r) in merged.iter_mut().zip(&row) {
                    if let Some(x) = r {
                        if let Some(y) = m {
                            assert_eq!(x, y, "library edge disagrees with itself");
                        } else {
                            *m = Some(*x);
                            changed = true;
                        }
                    }
                }
                edges[i] = merged;
                // Symmetric completion: v.e -> (w, back) implies w.back -> (v, e).
                let ne = ctx.nedges(ty);
                for e in 0..ne {
                    if let Some((j, back)) = edges[i][e] {
                        let jty = tiles[j as usize].2;
                        if edges[j as usize].is_empty() {
                            edges[j as usize] = vec![None; ctx.nedges(jty)];
                        }
                        let rev = &mut edges[j as usize][back as usize];
                        match rev {
                            Some((t, b)) => assert!(
                                *t == i as u32 && usize::from(*b) == e,
                                "gluing asymmetry in library replay"
                            ),
                            None => {
                                *rev = Some((i as u32, e as u8));
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut buckets: HashMap<RingElement, Vec<u32>> = HashMap::new();
        for (i, (_, p, _)) in tiles.iter().enumerate() {
            for v in ctx.od.tile_vertices(p) {
                let cells = buckets.entry(v).or_default();
                if cells.last() != Some(&(i as u32)) {
                    cells.push(i as u32);
                }
            }
        }
        let mut vadj: Vec<Vec<u32>> = vec![Vec::new(); tiles.len()];
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
        for (i, row) in edges.iter_mut().enumerate() {
            if row.is_empty() {
                *row = vec![None; ctx.nedges(tiles[i].2)];
            }
        }
        let centroids = tiles.iter().map(|(_, p, _)| ctx.centroid(p)).collect();
        Self {
            ctx,
            tiles,
            edges,
            vadj,
            centroids,
        }
    }
}

impl EdgeCtx for FiniteCtx<'_> {
    fn ty(&self, c: u32) -> usize {
        self.tiles[c as usize].2.0
    }
    fn class(&self, c: u32) -> u16 {
        match self.tiles[c as usize].0.get(1) {
            Ok(TileAddrSymbol::Supertile {
                parent_type,
                subtile,
            }) => self.ctx.class_of[&(parent_type, subtile)],
            _ => u16::MAX,
        }
    }
    fn nedges(&self, c: u32) -> usize {
        self.edges[c as usize].len()
    }
    fn edge(&self, c: u32, e: usize) -> Option<(u32, u8)> {
        self.edges[c as usize][e]
    }
    fn centroid(&self, c: u32) -> [f64; 2] {
        self.centroids[c as usize]
    }
    fn vertex_neighbours(&self, c: u32) -> Vec<u32> {
        self.vadj[c as usize].clone()
    }
    fn placement(&self, c: u32) -> (u8, [f64; 2]) {
        crate::atlas::placement_of(&self.tiles[c as usize].1)
    }
}

// ------------------------------------------------------------------
// Faithfulness of the exploration graph: every node re-derived from
// its address path alone, with none of the exploration's incremental
// bookkeeping (no delivery queue, no copy links, no run objects).
// ------------------------------------------------------------------

use substitution_tiling_transducers::address::EdgeAddrSymbol;
use substitution_tiling_transducers::common::{LayerIndex, System};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RSlot {
    Outside,
    Pending(usize),
    Resolved { target: u32, back: u8, live: Option<usize> },
}

/// Run the transducer DFA over a tile's known prefix for one edge:
/// (final state, symbols emitted, accepted).
fn dfa_over_prefix(ctx: &Ctx, addr: &FiniteTileAddress, edge: usize) -> (usize, usize, bool) {
    let tr = ctx.tr;
    let ty = addr.base_type().unwrap();
    let mut state = tr.start;
    let mut emitted = 0usize;
    let mut feed = |state: &mut usize, sym: EdgeAddrSymbol| {
        let (next, out) = &tr.trans[state][&sym];
        *state = *next;
        emitted += out.len();
    };
    feed(&mut state, EdgeAddrSymbol::Initial { base_type: ty, edge });
    for i in 1..addr.len() {
        let TileAddrSymbol::Supertile { parent_type, subtile } = addr.get(i).unwrap() else {
            panic!("non-initial symbol is not a supertile symbol");
        };
        feed(&mut state, EdgeAddrSymbol::Supertile { parent_type, subtile });
    }
    (state, emitted, tr.accept.contains(&state))
}

/// Recompute the memo key of the configuration reached by `path` from a
/// root of type `root_ty`, using only the library's finite-address
/// neighbour computation and its transition table.
pub fn recompute_key(ctx: &Ctx, root_ty: TileIndex, path: &[(usize, usize)], r: u32, region_factor: f64) -> Vec<u8> {
    recompute(ctx, root_ty, path, r, region_factor).0
}

/// The recomputed configuration's key, and whether it is a leaf at
/// radius `r`: every tile within edge distance `r` (over resolved
/// gluings) has a known class and no pending edge.
pub fn recompute(ctx: &Ctx, root_ty: TileIndex, path: &[(usize, usize)], r: u32, region_factor: f64) -> (Vec<u8>, bool) {
    let region_radius = region_factor * (f64::from(r) + 1.0) * ctx.d_nb;
    let mut syms = vec![TileAddrSymbol::Initial { base_type: root_ty }];
    syms.extend(path.iter().map(|&(p, s)| TileAddrSymbol::Supertile {
        parent_type: TileIndex(p),
        subtile: s,
    }));
    let root = FiniteTileAddress::from(syms);
    let root_placement = ctx.od.place_default(root_ty);
    let centre = ctx.centroid(&root_placement);
    let dist_to = |c: [f64; 2]| (c[0] - centre[0]).hypot(c[1] - centre[1]);

    // Fixpoint over the region: every stored tile queried on every edge
    // not geometrically certain to leave the region; tiles keep the
    // longest prefix any query produced for them; identified targets
    // outside the region are remembered as such.
    let mut tiles: Vec<(FiniteTileAddress, TilePlacement, TileIndex)> = vec![(root, root_placement, root_ty)];
    let mut by_place: HashMap<TilePlacement, u32> = HashMap::new();
    by_place.insert(tiles[0].1.clone(), 0);
    let mut targets: Vec<Vec<Option<(u32, u8)>>> = vec![vec![None; ctx.nedges(root_ty)]];
    let mut outside_target: Vec<Vec<bool>> = vec![vec![false; ctx.nedges(root_ty)]];
    let mut certainly: Vec<Vec<bool>> = vec![(0..ctx.nedges(root_ty))
        .map(|e| dist_to(ctx.edge_midpoint(&tiles[0].1, e)) > region_radius + ctx.d_tile)
        .collect()];
    loop {
        let mut changed = false;
        let n = tiles.len();
        for i in 0..n {
            let (addr, placement, ty) = tiles[i].clone();
            for e in 0..ctx.nedges(ty) {
                if certainly[i][e] || targets[i][e].is_some() && outside_target[i][e] {
                    continue;
                }
                let (out, back) = addr.neighbour_partial(e, ctx.tr);
                let Ok(back) = back else { continue };
                let nty = out.base_type().unwrap();
                let np = ctx.od.place_next_to_tile(nty, back, &placement, e);
                let j = match by_place.get(&np) {
                    Some(&j) => {
                        let have = &tiles[j as usize].0;
                        let (short, long) = if have.len() < out.len() { (have, &out) } else { (&out, have) };
                        for k in 0..short.len() {
                            assert_eq!(short.get(k).unwrap(), long.get(k).unwrap(), "library outputs disagree");
                        }
                        if out.len() > have.len() {
                            tiles[j as usize].0 = out.clone();
                            changed = true;
                        }
                        j
                    }
                    None => {
                        if dist_to(ctx.centroid(&np)) > region_radius {
                            if !outside_target[i][e] {
                                outside_target[i][e] = true;
                                changed = true;
                            }
                            continue;
                        }
                        let j = tiles.len() as u32;
                        by_place.insert(np.clone(), j);
                        let ne = ctx.nedges(nty);
                        certainly.push(
                            (0..ne)
                                .map(|e2| dist_to(ctx.edge_midpoint(&np, e2)) > region_radius + ctx.d_tile)
                                .collect(),
                        );
                        tiles.push((out.clone(), np, nty));
                        targets.push(vec![None; ne]);
                        outside_target.push(vec![false; ne]);
                        changed = true;
                        j
                    }
                };
                if targets[i][e].is_none() {
                    targets[i][e] = Some((j, back as u8));
                    changed = true;
                }
                // Symmetric completion.
                let rev = &mut targets[j as usize][back];
                if rev.is_none() {
                    *rev = Some((i as u32, e as u8));
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    // Slot classification and copy children.
    let n = tiles.len();
    let mut slots: Vec<Vec<RSlot>> = Vec::with_capacity(n);
    let mut copy_children: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n {
        let (addr, _, ty) = &tiles[i];
        let mut row = Vec::with_capacity(ctx.nedges(*ty));
        for e in 0..ctx.nedges(*ty) {
            if certainly[i][e] || outside_target[i][e] {
                row.push(RSlot::Outside);
                continue;
            }
            let (state, _emitted, accepted) = dfa_over_prefix(ctx, addr, e);
            match targets[i][e] {
                Some((t, back)) => {
                    if accepted {
                        if t != i as u32 && !copy_children[i].contains(&t) {
                            copy_children[i].push(t);
                        }
                        row.push(RSlot::Resolved { target: t, back, live: None });
                    } else {
                        row.push(RSlot::Resolved { target: t, back, live: Some(state) });
                    }
                }
                None => {
                    assert!(!accepted, "accepted query without an identified target");
                    row.push(RSlot::Pending(state));
                }
            }
        }
        slots.push(row);
    }
    // Classes and the top state.
    let class_of = |addr: &FiniteTileAddress| -> u16 {
        match addr.get(1) {
            Ok(TileAddrSymbol::Supertile { parent_type, subtile }) => ctx.class_of[&(parent_type, subtile)],
            _ => u16::MAX,
        }
    };
    let mut layer: LayerIndex = ctx.cs.base_layer;
    let mut top = root_ty;
    for &(p, _) in path {
        layer = ctx.cs.lookup_layer(layer).parent;
        top = TileIndex(p);
    }
    // Canonical order: BFS over forward resolved slots in edge order.
    let mut order = vec![0u32];
    let mut index: Vec<u16> = vec![u16::MAX; n];
    index[0] = 0;
    let mut i = 0;
    while i < order.len() {
        let c = order[i] as usize;
        i += 1;
        for s in &slots[c] {
            if let RSlot::Resolved { target, .. } = s
                && index[*target as usize] == u16::MAX
            {
                index[*target as usize] = order.len() as u16;
                order.push(*target);
            }
        }
    }
    assert_eq!(order.len(), n, "recomputed tile unreachable via forward resolved edges");
    // Leaf test: distances over resolved gluings (both directions).
    let mut dist = vec![u32::MAX; n];
    dist[0] = 0;
    let mut q = std::collections::VecDeque::from([0usize]);
    while let Some(c) = q.pop_front() {
        for s in &slots[c] {
            if let RSlot::Resolved { target, .. } = s {
                let t = *target as usize;
                if dist[t] == u32::MAX {
                    dist[t] = dist[c] + 1;
                    q.push_back(t);
                }
            }
        }
    }
    let leaf = (0..n).all(|c| {
        dist[c] > r
            || (class_of(&tiles[c].0) != u16::MAX && !slots[c].iter().any(|s| matches!(s, RSlot::Pending(_))))
    });
    let mut key = Vec::with_capacity(4 + n * 64);
    key.extend((layer.0 as u16).to_be_bytes());
    key.extend((top.0 as u16).to_be_bytes());
    for &c in &order {
        let (addr, _, ty) = &tiles[c as usize];
        key.extend((ty.0 as u16).to_be_bytes());
        key.extend(class_of(addr).to_be_bytes());
        key.push(slots[c as usize].len() as u8);
        for s in &slots[c as usize] {
            match s {
                RSlot::Outside => key.push(0),
                RSlot::Pending(state) => {
                    key.push(1);
                    key.extend(ctx.state_id(*state).to_be_bytes());
                }
                RSlot::Resolved { target, back, live } => {
                    key.push(if live.is_some() { 3 } else { 2 });
                    key.extend(index[*target as usize].to_be_bytes());
                    key.push(*back);
                    if let Some(state) = live {
                        key.extend(ctx.state_id(*state).to_be_bytes());
                    }
                }
            }
        }
        let mut children: Vec<u16> = copy_children[c as usize].iter().map(|&t| index[t as usize]).collect();
        children.sort_unstable();
        key.push(children.len() as u8);
        for ch in children {
            key.extend(ch.to_be_bytes());
        }
    }
    (key, leaf)
}

#[derive(Default, Debug)]
pub struct GraphReport {
    pub nodes: usize,
    pub node_mismatches: Vec<usize>,
    pub edges: usize,
    pub edge_mismatches: Vec<(usize, usize)>,
    pub missing_children: Vec<usize>,
    pub leaf_errors: Vec<usize>,
    pub ball_mismatches: Vec<usize>,
}

/// Verify an exploration graph: every node's key from its path, every
/// non-leaf's children = the legal parent symbols with correctly keyed
/// targets, every leaf a leaf with the recorded ball.
pub fn check_graph(ctx: &Ctx, graph: &crate::atlas::ExplorationGraph, check_edges: bool, log: bool) -> GraphReport {
    let r = graph.radius;
    let f = graph.region_factor;
    let root_ty = TileIndex(graph.root_ty);
    let unhex = |s: &str| -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    };
    let mut rep = GraphReport { nodes: graph.nodes.len(), ..Default::default() };
    for (id, node) in graph.nodes.iter().enumerate() {
        if log && id.is_multiple_of(500) {
            eprintln!("  graph check: node {id}/{}", graph.nodes.len());
        }
        let (key, determined) = recompute(ctx, root_ty, &node.path, r, f);
        if key != unhex(&node.key) {
            rep.node_mismatches.push(id);
            continue;
        }
        // Leaf condition from the recomputed configuration.
        if node.leaf != determined {
            rep.leaf_errors.push(id);
            continue;
        }
        if node.leaf {
            // The leaf's ball, rebuilt with the library driver.
            let region_radius = f * (f64::from(r) + 1.0) * ctx.d_nb;
            let mut syms = vec![TileAddrSymbol::Initial { base_type: root_ty }];
            syms.extend(node.path.iter().map(|&(p, s)| TileAddrSymbol::Supertile {
                parent_type: TileIndex(p),
                subtile: s,
            }));
            let fctx = FiniteCtx::new(ctx, FiniteTileAddress::from(syms), region_radius);
            let ball = build_ball(&fctx, 0, r, node.path.clone());
            let ok = matches!((&ball, &node.ball_key), (Ok(b), Some(k)) if hex_eq(&b.key, k));
            if !ok {
                rep.ball_mismatches.push(id);
            }
            continue;
        }
        // Children: exactly the legal parent symbols.
        let mut layer = ctx.cs.base_layer;
        let mut top = root_ty;
        for &(p, _) in &node.path {
            layer = ctx.cs.lookup_layer(layer).parent;
            top = TileIndex(p);
        }
        let mut legal: Vec<(usize, usize)> = ctx.legal_parents(layer, top).iter().map(|&(p, s)| (p.0, s)).collect();
        legal.sort_unstable();
        let mut have: Vec<(usize, usize)> = node.children.iter().map(|&(p, s, _)| (p, s)).collect();
        have.sort_unstable();
        if legal != have {
            rep.missing_children.push(id);
        }
        if check_edges {
            for &(p, s, cid) in &node.children {
                rep.edges += 1;
                let mut path = node.path.clone();
                path.push((p, s));
                let ck = recompute_key(ctx, root_ty, &path, r, f);
                if ck != unhex(&graph.nodes[cid].key) {
                    rep.edge_mismatches.push((id, cid));
                }
            }
        }
    }
    rep
}

fn hex_eq(bytes: &[u8], hexs: &str) -> bool {
    hexs.len() == bytes.len() * 2
        && bytes.iter().zip((0..hexs.len()).step_by(2)).all(|(b, i)| u8::from_str_radix(&hexs[i..i + 2], 16).ok() == Some(*b))
}

#[derive(Default, Debug)]
pub struct WitnessReport {
    pub checked: usize,
    pub ok: usize,
    pub failures: Vec<(usize, String)>,
}

pub fn check_witnesses(atlas: &Atlas, ctx: &Ctx) -> WitnessReport {
    let mut rep = WitnessReport::default();
    for (i, ball) in atlas.balls.iter().enumerate() {
        rep.checked += 1;
        let mut syms = vec![TileAddrSymbol::Initial {
            base_type: TileIndex(ball.root_ty),
        }];
        syms.extend(ball.witness.iter().map(|&(p, s)| TileAddrSymbol::Supertile {
            parent_type: TileIndex(p),
            subtile: s,
        }));
        let region_radius = atlas.region_factor * (f64::from(atlas.radius) + 1.0) * ctx.d_nb;
        let fctx = FiniteCtx::new(ctx, FiniteTileAddress::from(syms), region_radius);
        match build_ball(&fctx, 0, atlas.radius, ball.witness.clone()) {
            Ok(b) if b.key == ball.key
                && b.vadj == ball.vadj
                && b.tiles.iter().zip(&ball.tiles).all(|(x, y)| {
                    x.rot == y.rot
                        && (x.offset[0] - y.offset[0]).abs() < 1e-9
                        && (x.offset[1] - y.offset[1]).abs() < 1e-9
                }) =>
            {
                rep.ok += 1
            }
            Ok(b) => rep.failures.push((
                i,
                format!(
                    "key mismatch (library ball {} tiles vs atlas {}; vadj equal: {})",
                    b.tiles.len(),
                    ball.tiles.len(),
                    b.vadj == ball.vadj
                ),
            )),
            Err(e) => rep.failures.push((i, e)),
        }
    }
    let _ = &rep.failures;
    rep
}
