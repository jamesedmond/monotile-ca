//! Library-generated patches as terrain: the ground truth the atlas is
//! validated against, and the substrate the observed flights replay on.
//! Edge lists are recovered per cell from the neighbour transducer on
//! the cell's eventually-periodic address (FINDINGS §10.6 method) and
//! cross-checked against the CSR graph wherever the whole neighbourhood
//! is in-patch.

use std::cell::RefCell;
use std::collections::HashMap;

use ca_engine::{CaRule, Engine};
use substitution_tiling_transducers::address::{EPTileAddress, TileAddrSymbol, TileAddress};
use substitution_tiling_transducers::common::{DisplayViaSystem, System, TileIndex};
use tiling_core::results::ResultRecord;
use tiling_core::{Geometry, Neighbourhood, Patch};

use crate::ball::{EdgeCtx, build_ball};
use crate::ctx::Ctx;
use crate::situation::{Situation, encode};

const OUT: u32 = u32::MAX;

/// Per-cell base type and transducer-recovered edge list.
type CellEntry = (TileIndex, Vec<(u32, u8)>);

pub struct PatchCtx<'a> {
    pub ctx: &'a Ctx<'a>,
    pub patch_e: Patch,
    pub patch_v: Patch,
    pub geom: Geometry,
    pub radius: u32,
    addr_index: HashMap<String, u32>,
    cache: RefCell<HashMap<u32, CellEntry>>,
    pub centroids: Vec<[f64; 2]>,
    /// Prototile reference polygons (float), for placement recovery.
    protos: HashMap<TileIndex, Vec<[f64; 2]>>,
}

impl<'a> PatchCtx<'a> {
    pub fn new(ctx: &'a Ctx<'a>, root: &str, radius: u32) -> Self {
        Self::try_new(ctx, root, radius).expect("root has seed artifacts")
    }

    /// As [`new`](Self::new), but `None` if the root has seed artifacts
    /// (a degenerate, non-interior periodic chain) at this radius.
    pub fn try_new(ctx: &'a Ctx<'a>, root: &str, radius: u32) -> Option<Self> {
        let (patch_v, geom) = ctx
            .tiling
            .generate_patch_with_geometry(root, radius, Neighbourhood::Vertex)
            .expect("vertex patch");
        let patch_e = ctx.tiling.generate(root, radius).expect("edge patch");
        if !patch_v.seed_artifact_cells.is_empty() {
            return None;
        }
        assert_eq!(patch_e.cells, patch_v.cells, "edge/vertex numbering must agree");
        let n = patch_e.cells.len();
        let centroids: Vec<[f64; 2]> = (0..n)
            .map(|c| {
                let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
                let m = (hi - lo) as f64;
                let (sx, sy) = geom.xy[lo..hi]
                    .iter()
                    .fold((0.0, 0.0), |(a, b), &[x, y]| (a + x, b + y));
                [sx / m, sy / m]
            })
            .collect();
        let addr_index = patch_e
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| (c.address.clone(), i as u32))
            .collect();
        let protos = ctx
            .base_types
            .iter()
            .map(|&t| {
                let p = ctx.od.place_default(t);
                let verts: Vec<[f64; 2]> = ctx
                    .od
                    .tile_vertices(&p)
                    .map(|v| v.to_float().unwrap())
                    .collect();
                (t, verts)
            })
            .collect();
        Some(Self {
            ctx,
            patch_e,
            patch_v,
            geom,
            radius,
            addr_index,
            cache: RefCell::new(HashMap::new()),
            centroids,
            protos,
        })
    }

    pub fn polygon(&self, c: u32) -> &[[f64; 2]] {
        let (lo, hi) = (
            self.geom.offsets[c as usize] as usize,
            self.geom.offsets[c as usize + 1] as usize,
        );
        &self.geom.xy[lo..hi]
    }

    pub fn cells(&self) -> u32 {
        self.patch_e.graph.cells()
    }

    pub fn distance(&self, c: u32) -> u32 {
        self.patch_e.cells[c as usize].distance
    }

    fn entry(&self, c: u32) -> CellEntry {
        if let Some(e) = self.cache.borrow().get(&c) {
            return e.clone();
        }
        let sys = self.ctx.sys;
        let cs = self.ctx.cs;
        let addr = EPTileAddress::parse(&self.patch_e.cells[c as usize].address, &sys.tmap)
            .expect("cell address parses");
        let base = addr.base_type().unwrap();
        let nedges = cs.lookup_tile_nedges(base);
        let mut edges: Vec<(u32, u8)> = Vec::with_capacity(nedges);
        for e in 0..nedges {
            let (naddr, back) = addr.neighbour(e, &sys.tr).unwrap();
            let idx = self
                .addr_index
                .get(&format!("{}", naddr.display(cs)))
                .copied()
                .unwrap_or(OUT);
            edges.push((idx, back as u8));
        }
        if edges.iter().all(|&(n, _)| n != OUT) {
            let mut mine: Vec<u32> = edges.iter().map(|&(n, _)| n).collect();
            mine.sort_unstable();
            mine.dedup();
            let mut graph = self.patch_e.graph.neighbours(c).to_vec();
            graph.sort_unstable();
            assert_eq!(mine, graph, "transducer edges disagree with patch graph at cell {c}");
        }
        let entry = (base, edges);
        self.cache.borrow_mut().insert(c, entry.clone());
        entry
    }

    pub fn base_type(&self, c: u32) -> TileIndex {
        self.entry(c).0
    }
}

impl EdgeCtx for PatchCtx<'_> {
    fn ty(&self, c: u32) -> usize {
        self.entry(c).0.0
    }
    fn class(&self, c: u32) -> u16 {
        self.patch_e.cells[c as usize].class
    }
    fn nedges(&self, c: u32) -> usize {
        self.entry(c).1.len()
    }
    fn edge(&self, c: u32, e: usize) -> Option<(u32, u8)> {
        let (n, back) = self.entry(c).1[e];
        (n != OUT).then_some((n, back))
    }
    fn centroid(&self, c: u32) -> [f64; 2] {
        self.centroids[c as usize]
    }
    fn vertex_neighbours(&self, c: u32) -> Vec<u32> {
        self.patch_v.graph.neighbours(c).to_vec()
    }
    fn placement(&self, c: u32) -> (u8, [f64; 2]) {
        // The placed polygon is rotation·proto + offset; recover the
        // rotation from the first edge direction, then the offset.
        let poly = self.polygon(c);
        let proto = &self.protos[&self.base_type(c)];
        let e = [poly[1][0] - poly[0][0], poly[1][1] - poly[0][1]];
        let p = [proto[1][0] - proto[0][0], proto[1][1] - proto[0][1]];
        let ang = e[1].atan2(e[0]) - p[1].atan2(p[0]);
        let unit = [ang.cos(), ang.sin()];
        let rot = crate::ball::rotation_index(unit);
        let rp0 = crate::ball::rotate(proto[0], rot);
        (rot, [poly[0][0] - rp0[0], poly[0][1] - rp0[1]])
    }
}

/// Which live component of the support to follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    /// The whole support (single-object records).
    All,
    /// The vertex-connected component farthest from the root at the
    /// first sampled generation, then tracked by nearest centroid.
    Farthest,
    /// The n-th component at the first sampled generation, components
    /// sorted by the polar angle of their centroid about the patch root,
    /// then tracked by nearest centroid.
    Index(usize),
}

/// One observed generation of a replayed flight.
pub struct GenObs {
    pub generation: u64,
    pub support: Vec<u32>,
    pub sit: Situation,
    /// Canonical order: situation index → patch cell.
    pub order: Vec<u32>,
    pub centre_cell: u32,
    pub rho: u32,
    /// Key of the patch's R-ball around the centre cell (None if the
    /// ball would touch the patch boundary).
    pub ball_key: Option<Vec<u8>>,
    pub components: usize,
}

pub struct FlightObs {
    pub gens: Vec<GenObs>,
    pub stop_reason: String,
    /// Sampled generations skipped because another live object lay
    /// inside the tracked component's collar.
    pub skipped_not_isolated: u64,
}

/// Run `initial` under `rule` for `gen` generations and report the live
/// support size and its number of vertex-connected components (None if
/// it died or reached the patch margin).
pub fn probe_at(
    pctx: &PatchCtx,
    rule: &ca_engine::AnyRule,
    strata: Vec<u8>,
    initial: &[u8],
    generations: u64,
    margin: u32,
) -> Option<(usize, usize)> {
    let n = pctx.cells();
    let mut engine = Engine::with_strata(pctx.patch_v.graph.clone(), strata);
    engine.load_state(initial);
    for _ in 0..generations {
        rule.step(&mut engine);
    }
    let state = engine.state();
    let support: Vec<u32> = (0..n).filter(|&c| state[c as usize] != 0).collect();
    if support.is_empty() {
        return None;
    }
    let maxd = support.iter().map(|&c| pctx.distance(c)).max().unwrap();
    if maxd + margin > pctx.radius {
        return None;
    }
    let comps = components_of(pctx, state, &support);
    Some((support.len(), comps.len()))
}

/// One settle-time component of a launch and where it went `delta`
/// generations later (followed one generation at a time by nearest
/// centroid, as the tracker does). `heading` is NaN if it died or merged.
pub struct Fragment {
    pub index: usize,
    pub pop: usize,
    pub heading: f64,
    pub disp: f64,
    pub end_pop: usize,
}

/// Fragment screen for showers (FINDINGS 10.11.9): run to `settle`,
/// label the vertex-connected components — indexed as
/// [`Component::Index`] does, by polar angle of centroid about the root
/// — then follow each for `delta` generations and report its heading.
/// None if the support died or reached the margin.
pub fn screen_components(
    pctx: &PatchCtx,
    rule: &ca_engine::AnyRule,
    strata: Vec<u8>,
    initial: &[u8],
    settle: u64,
    delta: u64,
    margin: u32,
) -> Option<Vec<Fragment>> {
    let n = pctx.cells();
    let mut engine = Engine::with_strata(pctx.patch_v.graph.clone(), strata);
    engine.load_state(initial);
    for _ in 0..settle {
        rule.step(&mut engine);
    }
    let live = |state: &[u8]| -> Vec<u32> { (0..n).filter(|&c| state[c as usize] != 0).collect() };
    let state0 = engine.state().to_vec();
    let support0 = live(&state0);
    if support0.is_empty() || support0.iter().map(|&c| pctx.distance(c)).max().unwrap() + margin > pctx.radius {
        return None;
    }
    let comps0 = components_of(pctx, &state0, &support0);
    let root = pctx.centroids[0];
    let cents0: Vec<[f64; 2]> = comps0.iter().map(|c| centroid_of(pctx, c)).collect();
    let angle = |c: [f64; 2]| (c[1] - root[1]).atan2(c[0] - root[0]).rem_euclid(std::f64::consts::TAU);
    let mut order: Vec<usize> = (0..comps0.len()).collect();
    order.sort_by(|&a, &b| angle(cents0[a]).partial_cmp(&angle(cents0[b])).unwrap());
    // Follow every component one generation at a time.
    let mut pos: Vec<Option<[f64; 2]>> = cents0.iter().map(|&c| Some(c)).collect();
    let mut end_pop: Vec<usize> = comps0.iter().map(|c| c.len()).collect();
    for _ in 0..delta {
        rule.step(&mut engine);
        let state = engine.state();
        let support = live(state);
        if support.iter().map(|&c| pctx.distance(c)).max().unwrap_or(0) + margin > pctx.radius {
            return None;
        }
        let comps = components_of(pctx, state, &support);
        let cents: Vec<[f64; 2]> = comps.iter().map(|c| centroid_of(pctx, c)).collect();
        for (i, p) in pos.iter_mut().enumerate() {
            let Some(q) = *p else { continue };
            let best = cents
                .iter()
                .enumerate()
                .map(|(j, c)| ((c[0] - q[0]).hypot(c[1] - q[1]), j))
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            match best {
                // A small object's centroid can jump a few units when an
                // edge cell flips; gliders move ≤ ~2.5 units per generation.
                Some((d, j)) if d <= 9.0 => {
                    *p = Some(cents[j]);
                    end_pop[i] = comps[j].len();
                }
                _ => {
                    *p = None;
                    end_pop[i] = 0;
                }
            }
        }
    }
    Some(
        order
            .iter()
            .enumerate()
            .map(|(index, &i)| {
                let a = cents0[i];
                match pos[i] {
                    Some(b) => Fragment {
                        index,
                        pop: comps0[i].len(),
                        heading: (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees().rem_euclid(360.0),
                        disp: (b[0] - a[0]).hypot(b[1] - a[1]),
                        end_pop: end_pop[i],
                    },
                    None => Fragment { index, pop: comps0[i].len(), heading: f64::NAN, disp: 0.0, end_pop: 0 },
                }
            })
            .collect(),
    )
}

pub fn components_of(pctx: &PatchCtx, state: &[u8], support: &[u32]) -> Vec<Vec<u32>> {
    let mut seen: HashMap<u32, bool> = HashMap::new();
    let mut comps = Vec::new();
    for &s in support {
        if seen.contains_key(&s) {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![s];
        seen.insert(s, true);
        while let Some(c) = stack.pop() {
            comp.push(c);
            for &m in pctx.patch_v.graph.neighbours(c) {
                if state[m as usize] != 0 && !seen.contains_key(&m) {
                    seen.insert(m, true);
                    stack.push(m);
                }
            }
        }
        comp.sort_unstable();
        comps.push(comp);
    }
    comps
}

fn centroid_of(pctx: &PatchCtx, cells: &[u32]) -> [f64; 2] {
    let m = cells.len() as f64;
    let (sx, sy) = cells.iter().fold((0.0, 0.0), |(a, b), &c| {
        (a + pctx.centroids[c as usize][0], b + pctx.centroids[c as usize][1])
    });
    [sx / m, sy / m]
}

/// Replay `record` on the patch and encode every generation from
/// `start_gen` as a situation at collar radius `k`, with the R-ball of
/// its centre cell. With a component selection, only that object is
/// keyed, and only at generations where no other live cell lies within
/// its collar (a situation is a lone object in its terrain).
pub fn replay_flight(
    pctx: &PatchCtx,
    record: &ResultRecord,
    start_gen: u64,
    k: u8,
    r: u32,
    component: Component,
) -> FlightObs {
    assert_eq!(record.neighbourhood, Neighbourhood::Vertex);
    let n = pctx.cells();
    let (strata, rule) = record.replay_setup(&pctx.patch_v);
    let initial = record.initial_state(n).unwrap();
    replay_flight_from(pctx, &rule, strata, &initial, start_gen, k, r, component, u64::MAX)
}

/// Replay from an explicit initial state (one byte per cell) under
/// `rule`, keying from `start_gen` for at most `max_keyed` generations.
#[allow(clippy::too_many_arguments)]
pub fn replay_flight_from(
    pctx: &PatchCtx,
    rule: &ca_engine::AnyRule,
    strata: Vec<u8>,
    initial: &[u8],
    start_gen: u64,
    k: u8,
    r: u32,
    component: Component,
    max_keyed: u64,
) -> FlightObs {
    let n = pctx.cells();
    let mut engine = Engine::with_strata(pctx.patch_v.graph.clone(), strata);
    engine.load_state(initial);
    let mut gens = Vec::new();
    let stop_reason;
    let mut generation = 0u64;
    let mut prev_centroid: Option<[f64; 2]> = None;
    let mut skipped = 0u64;
    let root_centroid = pctx.centroids[0];
    loop {
        let state = engine.state();
        let support: Vec<u32> = (0..n).filter(|&c| state[c as usize] != 0).collect();
        if support.is_empty() {
            stop_reason = format!("population died at generation {generation}");
            break;
        }
        // Choose the tracked support (selection starts at `start_gen`;
        // before that the whole support is followed for the margin check).
        let comps = components_of(pctx, state, &support);
        let tracked: Vec<u32> = match component {
            Component::All => support.clone(),
            _ if generation < start_gen => support.clone(),
            _ => {
                let chosen = match prev_centroid {
                    Some(pc) => comps
                        .iter()
                        .min_by(|a, b| {
                            let d = |comp: &[u32]| {
                                let c = centroid_of(pctx, comp);
                                (c[0] - pc[0]).hypot(c[1] - pc[1])
                            };
                            d(a).partial_cmp(&d(b)).unwrap()
                        })
                        .unwrap(),
                    None => match component {
                        Component::Farthest => comps
                            .iter()
                            .max_by_key(|comp| comp.iter().map(|&c| pctx.distance(c)).max().unwrap())
                            .unwrap(),
                        Component::Index(i) => {
                            let mut order: Vec<usize> = (0..comps.len()).collect();
                            let angle = |comp: &[u32]| {
                                let c = centroid_of(pctx, comp);
                                (c[1] - root_centroid[1])
                                    .atan2(c[0] - root_centroid[0])
                                    .rem_euclid(std::f64::consts::TAU)
                            };
                            order.sort_by(|&a, &b| {
                                angle(&comps[a]).partial_cmp(&angle(&comps[b])).unwrap()
                            });
                            &comps[*order.get(i).unwrap_or_else(|| {
                                panic!(
                                    "component index {i} out of range: {} components at generation {generation}",
                                    comps.len()
                                )
                            })]
                        }
                        Component::All => unreachable!(),
                    },
                };
                chosen.clone()
            }
        };
        let maxd = tracked.iter().map(|&c| pctx.distance(c)).max().unwrap();
        if maxd + u32::from(k) + r + 4 > pctx.radius {
            stop_reason = format!(
                "terrain margin reached at generation {generation} (support distance {maxd})"
            );
            break;
        }
        if generation >= start_gen.saturating_add(max_keyed) {
            stop_reason = format!("keyed-generation budget reached at generation {generation}");
            break;
        }
        if generation >= start_gen {
            if component != Component::All {
                prev_centroid = Some(centroid_of(pctx, &tracked));
            }
            let state_of = |c: u32| state[c as usize];
            // Isolation: no other live cell within the tracked collar.
            let collar = crate::situation::collar(pctx, &tracked, k);
            let isolated = collar
                .keys()
                .all(|&c| state[c as usize] == 0 || tracked.binary_search(&c).is_ok());
            let encoded = if isolated { encode(pctx, &tracked, &state_of, k) } else { None };
            if let Some((sit, order)) = encoded {
                let (centre, rho) = sit.centre();
                let centre_cell = order[centre];
                let ball_key = (pctx.distance(centre_cell) + r + 3 <= pctx.radius)
                    .then(|| build_ball(pctx, centre_cell, r, Vec::new()).expect("patch ball").key);
                gens.push(GenObs {
                    generation,
                    support: tracked.clone(),
                    sit,
                    order,
                    centre_cell,
                    rho,
                    ball_key,
                    components: comps.len(),
                });
            } else {
                // Not isolated, or the collar is not edge-connected (the
                // tracked cells are too spread out to be one situation).
                skipped += 1;
            }
        }
        rule.step(&mut engine);
        generation += 1;
    }
    FlightObs {
        gens,
        stop_reason,
        skipped_not_isolated: skipped,
    }
}

/// The canonical patch of a context: the first eventually-interior
/// completion among the self-slotted parents of the top type, else a
/// parent's self-slot (FINDINGS 10.11.19), at the given radius, with the
/// context's tile at cell 0.
pub fn canonical_patch<'a>(
    ctx: &'a Ctx<'a>,
    subtiles: &dyn Fn(TileIndex) -> Vec<TileIndex>,
    display: &dyn Fn(EPTileAddress) -> String,
    parents_of: &dyn Fn(TileIndex) -> Vec<(TileIndex, usize)>,
    context: &[TileAddrSymbol],
    radius: u32,
) -> Option<PatchCtx<'a>> {
    let top = match context.last().unwrap() {
        TileAddrSymbol::Supertile { parent_type, .. } => *parent_type,
        _ => unreachable!(),
    };
    // Candidate completions: the self-slots of the top type, then of its parents, grandparents, …
    // (breadth-first over ancestor chains, up to four levels), the first eventually-interior one wins.
    let mut chains: Vec<(Vec<TileAddrSymbol>, TileIndex)> = vec![(context.to_vec(), top)];
    for _depth in 0..4 {
        let mut next: Vec<(Vec<TileAddrSymbol>, TileIndex)> = Vec::new();
        for (init, t) in &chains {
            for (s, c) in subtiles(*t).into_iter().enumerate() {
                if c == *t {
                    let r = display(EPTileAddress::from_vecs(init.clone(), vec![TileAddrSymbol::Supertile { parent_type: *t, subtile: s }]));
                    if let Some(pc) = PatchCtx::try_new(ctx, &r, radius) {
                        return Some(pc);
                    }
                }
            }
            for (pp, s) in parents_of(*t) {
                let mut i2 = init.clone();
                i2.push(TileAddrSymbol::Supertile { parent_type: pp, subtile: s });
                next.push((i2, pp));
            }
        }
        chains = next;
    }
    None
}
