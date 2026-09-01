//! Patch generation for hat/spectre tilings.
//!
//! Wraps Tatham's `substitution-tiling-transducers` library
//! (arXiv:2512.16595). A patch is identified by (family, root tile
//! address, radius) and regenerated deterministically: starting from the
//! root address, a breadth-first walk over the neighbour transducer
//! enumerates every tile within `radius` steps, producing a CSR adjacency
//! graph for `ca-engine` plus per-cell metadata. No geometry is involved
//! anywhere in this process.

pub mod results;
pub mod slide;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fmt::{self, Display};

use ca_engine::Graph;
use substitution_tiling_transducers::address::{
    EPTileAddress, TileAddrSymbol, TileAddress,
};
use substitution_tiling_transducers::builtin::{
    BuiltinSystem, HatTilingType, hat_htpf, p2_whole, p3_whole, spectre,
};
use substitution_tiling_transducers::combinatorial::{CombSystem, Layer};
use substitution_tiling_transducers::common::{
    DisplayViaSystem, LayerIndex, Subedge, System, TileIndex,
};
use substitution_tiling_transducers::geometric::TilePlacement;

/// The supported tiling families: the two aperiodic monotiles (the
/// project's subjects) and the two Penrose tilings (whole kites/darts
/// and whole rhombs), added as the FINDINGS §9 control substrate — the
/// one published aperiodic-tiling glider lives on P3.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TilingFamily {
    Hat,
    Spectre,
    /// Penrose P2 (kites and darts), whole tiles.
    PenroseP2,
    /// Penrose P3 (thin and thick rhombs), whole tiles.
    PenroseP3,
}

/// Which adjacency relation the patch graph encodes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Neighbourhood {
    /// Tiles sharing a transducer edge — the default, used by every
    /// pre-§9 result.
    #[default]
    Edge,
    /// Tiles sharing at least one boundary vertex (the Owens–Stepney
    /// "generalised Moore" neighbourhood; Goucher's Penrose glider is
    /// defined on it). A strict superset of [`Neighbourhood::Edge`].
    /// Computed from the exact-arithmetic tile placements inside
    /// tiling-core; the engine still receives a pure CSR graph.
    ///
    /// The patch is still the *edge*-metric ball and
    /// [`CellMeta::distance`] is edge-BFS distance. Tiles around a
    /// shared vertex form an edge-connected fan, so a cell's vertex
    /// neighbourhood is only complete a few edge-rings inside the
    /// boundary — pass `boundary_distance ≈ radius − 3` to
    /// classification instead of `radius` (exact fan bound is
    /// per-family; ≤ 3 for the Penrose tilings, asserted in tests).
    Vertex,
}

impl Neighbourhood {
    /// For serde `skip_serializing_if`: the default (edge) is omitted,
    /// keeping pre-§9 records byte-identical.
    pub fn is_edge(&self) -> bool {
        *self == Neighbourhood::Edge
    }
}

/// Error from address parsing/validation or patch generation.
#[derive(Clone, Debug)]
pub struct PatchError(pub String);

impl Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PatchError {}

/// A tile class usable for rule stratification: which kind of base tile
/// this is, distinguished by its position in its level-1 supertile. The
/// class table is derived from the substitution system alone, so class
/// indices are stable across patches of any root and radius.
///
/// For the hat system the base tile name already encodes chirality
/// (`hat` / `antihat`); for the spectre system the parent metatile
/// distinguishes plain spectres from the two members of a Mystic pair.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TileClass {
    /// `"<base>@<parent>.<subtile>"`, e.g. `"hat@H0.2"`.
    pub name: String,
    /// Base tile type name (`hat`, `antihat`, `spectre`).
    pub base: String,
    /// Level-1 supertile (metatile) type name.
    pub parent: String,
    /// Which subtile of the parent this base tile is.
    pub subtile: usize,
}

/// Per-cell metadata, index-aligned with the patch graph.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CellMeta {
    /// Canonical Tatham address of this tile, parseable back via
    /// [`Tiling::generate`] as a root.
    pub address: String,
    /// Index into the tiling's [`TileClass`] table.
    pub class: u16,
    /// Graph distance from the root cell.
    pub distance: u32,
}

/// A generated patch: pure adjacency graph plus per-cell metadata.
/// Geometry, when needed for rendering, is produced separately.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Patch {
    pub graph: Graph,
    pub cells: Vec<CellMeta>,
    pub classes: Vec<TileClass>,
    /// Canonical form of the root address (cell 0).
    pub root: String,
    pub radius: u32,
    /// Which adjacency relation [`Self::graph`] encodes. Cell indices,
    /// metadata and distances are identical across neighbourhoods (the
    /// BFS is always over edge adjacency); only the graph differs.
    pub neighbourhood: Neighbourhood,
    /// Cells whose neighbour computation either crossed an
    /// infinite-order supertile boundary (transducer accept point
    /// `None`) or returned the cell *itself* as a neighbour. Both signal
    /// a degenerate eventually-periodic root: if the repeating cycle's
    /// supertile slots touch their parents' boundaries, the
    /// infinite-supertile union covers a cone (or glued sector) rather
    /// than the plane, and adjacency near the seam is not that of any
    /// legal tiling. Default roots use eventually-interior cycles and
    /// produce an empty list; patches with a non-empty list must not be
    /// used to verify search results. (Conservative: a deliberate
    /// multi-infinite-supertile root would also be flagged.)
    pub seed_artifact_cells: Vec<u32>,
}

/// How to assign cells to rule strata for a [`StratifiedRule`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Stratification {
    /// One stratum — a uniform rule.
    Uniform,
    /// Two strata by the natural tile two-colouring: hat→0 / antihat→1;
    /// spectre plain→0 / Gamma-pair (Mystic)→1; Penrose kite/thick→0,
    /// dart/thin→1. This is the brief's "free expressive power".
    Chirality,
    /// One stratum per tile class (17 hat / 10 spectre): the finest
    /// stratification, for evolutionary rule search. The stratum is the
    /// cell's class index, so a [`StratifiedRule`] needs one table per
    /// entry of [`Patch::classes`].
    PerClass,
}

impl Patch {
    /// Per-cell stratum index under `scheme` (length = cell count) and
    /// the number of strata. The byte array is ready for
    /// [`Engine::with_strata`](ca_engine::Engine::with_strata); the count
    /// sizes a [`StratifiedRule`](ca_engine::StratifiedRule)'s tables.
    pub fn strata(&self, scheme: Stratification) -> (Vec<u8>, usize) {
        match scheme {
            Stratification::Uniform => (vec![0u8; self.cells.len()], 1),
            Stratification::Chirality => {
                let strata = self
                    .cells
                    .iter()
                    .map(|c| {
                        let class = &self.classes[c.class as usize];
                        // antihat (hat family), Gamma-pair (spectre
                        // Mystic), dart (P2) or thin rhomb (P3) →
                        // stratum 1; everything else → 0.
                        u8::from(
                            class.base == "antihat"
                                || class.parent == "Gamma"
                                || class.base == "dart"
                                || class.base == "thin",
                        )
                    })
                    .collect();
                (strata, 2)
            }
            Stratification::PerClass => {
                assert!(
                    self.classes.len() <= 256,
                    "PerClass needs class count <= 256 to fit a u8 stratum"
                );
                let strata = self.cells.iter().map(|c| c.class as u8).collect();
                (strata, self.classes.len())
            }
        }
    }
}

/// A tiling family with its transducer machinery built and ready to
/// generate patches. Construction is moderately expensive (it builds the
/// adjacency recogniser and transducer), so build once and reuse.
pub struct Tiling {
    family: TilingFamily,
    sys: BuiltinSystem,
    classes: Vec<TileClass>,
    /// (level-1 parent type, subtile index) → class table index.
    class_index: HashMap<(TileIndex, usize), u16>,
}

impl Tiling {
    pub fn new(family: TilingFamily) -> Self {
        let sys = match family {
            TilingFamily::Hat => hat_htpf(HatTilingType::Hats),
            TilingFamily::Spectre => spectre(),
            TilingFamily::PenroseP2 => p2_whole(),
            TilingFamily::PenroseP3 => p3_whole(),
        };
        let cs = &sys.cs_fine;

        // Enumerate every (parent metatile, subtile slot) of the base
        // layer, in deterministic order, as the class table.
        let base_layer = cs.lookup_layer(cs.base_layer);
        let mut parents: Vec<TileIndex> =
            base_layer.subtiles.keys().copied().collect();
        parents.sort();
        let mut classes = Vec::new();
        let mut class_index = HashMap::new();
        for p in parents {
            for (i, &child) in base_layer.subtiles[&p].iter().enumerate() {
                class_index.insert((p, i), classes.len() as u16);
                classes.push(TileClass {
                    name: format!(
                        "{}@{}.{}",
                        cs.lookup_tile_name(child),
                        cs.lookup_tile_name(p),
                        i
                    ),
                    base: cs.lookup_tile_name(child).to_string(),
                    parent: cs.lookup_tile_name(p).to_string(),
                    subtile: i,
                });
            }
        }

        Self {
            family,
            sys,
            classes,
            class_index,
        }
    }

    pub fn family(&self) -> TilingFamily {
        self.family
    }

    pub fn classes(&self) -> &[TileClass] {
        &self.classes
    }

    /// Direct access to the underlying library system, for callers that
    /// need more than patch generation (e.g. geometry output).
    pub fn system(&self) -> &BuiltinSystem {
        &self.sys
    }

    /// A canonical default root address for this family: the
    /// lowest-numbered base tile type, embedded via the canonical
    /// shortest supertile path into the lexicographically-least
    /// *eventually-interior* repeating cycle (see
    /// [`canonical_interior_cycle`]). Such a seed's nested supertiles
    /// eventually contain their predecessors strictly, so the
    /// eventually-periodic address describes a tile of a genuine planar
    /// tiling rather than a cone-glued sector (FINDINGS.md §2).
    pub fn default_root(&self) -> String {
        let cs = &self.sys.cs_fine;
        let base_type = *cs
            .lookup_layer(cs.base_layer)
            .tiles
            .iter()
            .min()
            .expect("base layer has tiles");
        let (cycle_layer, cycle) = canonical_interior_cycle(cs);
        let cycle_entry =
            cs.lookup_layer(cycle_layer).subtiles[&cycle[0].0][cycle[0].1];
        let path = canonical_path(
            cs,
            (base_type, cs.base_layer),
            (cycle_entry, cycle_layer),
        )
        .expect("cycle entry type is reachable from the base tile");

        let supertile = |&(parent_type, subtile): &Step| {
            TileAddrSymbol::Supertile {
                parent_type,
                subtile,
            }
        };
        let mut initial = vec![TileAddrSymbol::Initial { base_type }];
        initial.extend(path.iter().map(supertile));
        let repeating = cycle.iter().map(supertile).collect();
        let addr = EPTileAddress::from_vecs(initial, repeating);
        format!("{}", addr.display(cs))
    }

    /// Check that an address is a legal trajectory through the
    /// substitution hierarchy: the base type lives in the base layer, and
    /// each symbol's tile really is the stated subtile of its successor,
    /// in the layer reached at that point of the address.
    fn validate(&self, addr: &EPTileAddress) -> Result<(), PatchError> {
        let cs = &self.sys.cs_fine;
        let bad = |msg: String| Err(PatchError(msg));
        let (initial, repeating) = (&addr.initial, &addr.repeating);
        if initial.is_empty() || repeating.is_empty() {
            return bad("address must have initial and repeating parts".into());
        }
        let TileAddrSymbol::Initial { base_type } = initial[0] else {
            return bad("address must start with a base tile symbol".into());
        };
        if !cs.lookup_layer(cs.base_layer).tiles.contains(&base_type) {
            return bad(format!(
                "'{}' is not a base-layer tile",
                cs.lookup_tile_name(base_type)
            ));
        }
        // Walk enough repeats that the (layer, phase) validation state is
        // guaranteed to have revisited itself; passing that prefix proves
        // the infinite address valid.
        let periods = cs.layers.len() + 1;
        let mut prev = base_type;
        let mut layer = cs.base_layer;
        let tail = repeating.iter().cycle().take(repeating.len() * periods);
        for sym in initial[1..].iter().chain(tail) {
            let TileAddrSymbol::Supertile {
                parent_type,
                subtile,
            } = *sym
            else {
                return bad("non-leading base tile symbol in address".into());
            };
            let l = cs.lookup_layer(layer);
            match l.subtiles.get(&parent_type).and_then(|s| s.get(subtile)) {
                Some(&c) if c == prev => {}
                _ => {
                    return bad(format!(
                        "'{}' is not subtile {} of '{}' here",
                        cs.lookup_tile_name(prev),
                        subtile,
                        cs.lookup_tile_name(parent_type)
                    ));
                }
            }
            prev = parent_type;
            layer = l.parent;
        }
        Ok(())
    }

    /// Generate the edge-adjacency patch of all tiles within `radius`
    /// transducer steps of `root`. Deterministic: cell indices follow
    /// breadth-first discovery order and neighbour lists follow sorted
    /// edge order.
    pub fn generate(&self, root: &str, radius: u32) -> Result<Patch, PatchError> {
        self.generate_patch(root, radius, Neighbourhood::Edge)
    }

    /// Like [`generate`](Self::generate), also producing render-only
    /// polygon geometry. Each cell is placed once, via the breadth-first
    /// tree edge it was discovered through; every *non-tree* graph edge
    /// therefore independently checks that two placements derived along
    /// different paths still abut (loop closure).
    pub fn generate_with_geometry(
        &self,
        root: &str,
        radius: u32,
    ) -> Result<(Patch, Geometry), PatchError> {
        self.generate_patch_with_geometry(root, radius, Neighbourhood::Edge)
    }

    /// [`generate`](Self::generate) with an explicit adjacency relation.
    /// The BFS (cell indices, metadata, distances) is identical for
    /// every neighbourhood; [`Neighbourhood::Vertex`] additionally links
    /// tiles that share an exact placement vertex without an edge.
    pub fn generate_patch(
        &self,
        root: &str,
        radius: u32,
        neighbourhood: Neighbourhood,
    ) -> Result<Patch, PatchError> {
        Ok(self.generate_impl(root, radius, false, neighbourhood)?.0)
    }

    /// [`generate_with_geometry`](Self::generate_with_geometry) with an
    /// explicit adjacency relation.
    pub fn generate_patch_with_geometry(
        &self,
        root: &str,
        radius: u32,
        neighbourhood: Neighbourhood,
    ) -> Result<(Patch, Geometry), PatchError> {
        let (patch, geometry) = self.generate_impl(root, radius, true, neighbourhood)?;
        Ok((patch, geometry.expect("geometry was requested")))
    }

    fn generate_impl(
        &self,
        root: &str,
        radius: u32,
        want_geometry: bool,
        neighbourhood: Neighbourhood,
    ) -> Result<(Patch, Option<Geometry>), PatchError> {
        let cs = &self.sys.cs_fine;
        let root_addr = EPTileAddress::parse(root, &self.sys.tmap)
            .map_err(|e| PatchError(format!("bad root address: {e:?}")))?;
        self.validate(&root_addr)?;

        let mut index: HashMap<EPTileAddress, u32> = HashMap::new();
        let mut info: Vec<(EPTileAddress, u32)> = Vec::new();
        let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
        let mut queue: VecDeque<u32> = VecDeque::new();
        // Vertex adjacency needs the exact placements even when the
        // caller doesn't want render geometry.
        let want_placements =
            want_geometry || neighbourhood == Neighbourhood::Vertex;
        let od = want_placements.then(|| self.sys.output_driver());
        let mut placements: Vec<TilePlacement> = Vec::new();
        if let Some(od) = &od {
            placements.push(od.place_default(root_addr.base_type().unwrap()));
        }
        index.insert(root_addr.clone(), 0);
        info.push((root_addr, 0));
        queue.push_back(0);

        let mut seed_artifacts: BTreeSet<u32> = BTreeSet::new();
        while let Some(a) = queue.pop_front() {
            let (addr, dist) = info[a as usize].clone();
            let base = addr.base_type().unwrap();
            for e in 0..cs.lookup_tile_nedges(base) {
                let (naddr, back, accept) = addr
                    .neighbour_with_accept_point(e, &self.sys.tr)
                    .unwrap();
                if accept.is_none() {
                    seed_artifacts.insert(a);
                }
                if let Some(&b) = index.get(&naddr) {
                    if a == b {
                        seed_artifacts.insert(a);
                    } else {
                        edges.insert((a.min(b), a.max(b)));
                    }
                } else if dist < radius {
                    let b = info.len() as u32;
                    if let Some(od) = &od {
                        placements.push(od.place_next_to_tile(
                            naddr.base_type().unwrap(),
                            back,
                            &placements[a as usize],
                            e,
                        ));
                    }
                    index.insert(naddr.clone(), b);
                    info.push((naddr, dist + 1));
                    queue.push_back(b);
                    edges.insert((a, b));
                }
            }
        }

        // Vertex adjacency: bucket every exact placement vertex by its
        // canonical ring-element rendering (exact arithmetic, so equal
        // points have identical strings — no epsilon); tiles sharing a
        // bucket share that vertex. Edge-adjacent tiles share two
        // vertices, so this is a strict superset of the edge relation.
        if neighbourhood == Neighbourhood::Vertex {
            let od = od.as_ref().expect("placements exist for vertex adjacency");
            let mut buckets: HashMap<String, Vec<u32>> = HashMap::new();
            for (c, placement) in placements.iter().enumerate() {
                for v in od.tile_vertices(placement) {
                    let cells = buckets.entry(format!("{v}")).or_default();
                    if cells.last() != Some(&(c as u32)) {
                        cells.push(c as u32);
                    }
                }
            }
            for cells in buckets.values() {
                for (i, &a) in cells.iter().enumerate() {
                    for &b in &cells[i + 1..] {
                        if a != b {
                            edges.insert((a.min(b), a.max(b)));
                        }
                    }
                }
            }
        }

        let geometry = want_geometry.then(|| {
            let od = od.as_ref().expect("placements exist for geometry");
            let mut offsets = Vec::with_capacity(placements.len() + 1);
            offsets.push(0u32);
            let mut xy = Vec::new();
            for placement in &placements {
                for v in od.tile_vertices(placement) {
                    xy.push(v.to_float().expect("vertex converts to float"));
                }
                offsets.push(xy.len() as u32);
            }
            Geometry { offsets, xy }
        });

        let cells: Vec<CellMeta> = info
            .iter()
            .map(|(addr, distance)| {
                let TileAddrSymbol::Supertile {
                    parent_type,
                    subtile,
                } = addr.get(1).unwrap()
                else {
                    unreachable!("symbol 1 of a valid address is a supertile")
                };
                CellMeta {
                    address: format!("{}", addr.display(cs)),
                    class: self.class_index[&(parent_type, subtile)],
                    distance: *distance,
                }
            })
            .collect();

        let edge_list: Vec<(u32, u32)> = edges.into_iter().collect();
        let patch = Patch {
            graph: Graph::from_edges(info.len() as u32, &edge_list),
            root: cells[0].address.clone(),
            radius,
            neighbourhood,
            cells,
            classes: self.classes.clone(),
            seed_artifact_cells: seed_artifacts.into_iter().collect(),
        };
        Ok((patch, geometry))
    }
}

/// Render-only polygon geometry, index-aligned with a patch's cells.
/// The CA layer never consumes this; it exists purely for display.
#[derive(Clone, PartialEq, Debug)]
pub struct Geometry {
    /// Cell `c`'s polygon vertices are `xy[offsets[c]..offsets[c + 1]]`.
    pub offsets: Vec<u32>,
    /// Vertex coordinates in polygon order.
    pub xy: Vec<[f64; 2]>,
}

/// One step up a supertile chain: the child below is
/// `subtiles[parent][slot]` in the layer the step starts from.
type Step = (TileIndex, usize);

/// Find the layer in which repeating address cycles live: follow parents
/// from the base layer to a self-parent layer. (Systems whose layer
/// structure itself cycles with period > 1 are not handled; neither
/// builtin family needs that.)
fn cycle_layer(cs: &CombSystem) -> LayerIndex {
    let mut layer = cs.lookup_layer(cs.base_layer);
    for _ in 0..=cs.layers.len() {
        if layer.parent == layer.index {
            return layer.index;
        }
        layer = cs.lookup_layer(layer.parent);
    }
    panic!("no self-parent layer found");
}

/// All (parent type, slot) labels of a layer's deflations, in canonical
/// (parent index, slot) order.
fn sorted_labels(layer: &Layer) -> Vec<Step> {
    let mut parents: Vec<TileIndex> = layer.subtiles.keys().copied().collect();
    parents.sort();
    parents
        .iter()
        .flat_map(|&p| (0..layer.subtiles[&p].len()).map(move |i| (p, i)))
        .collect()
}

/// Is the closed walk of steps an *eventually interior* cycle?
///
/// Track every edge of the cycle's entry type upward through the
/// deflation adjacency: a chain survives a step only if the child edge
/// lies on the parent's outline (`Subedge::Ext`), continuing as that
/// parent edge; it dies when matched to a sibling subtile
/// (`Subedge::Int`). If every chain dies within finitely many periods,
/// each cycle supertile is eventually *strictly* inside a later one, so
/// the nested union exhausts the plane and the seed admits no cone apex.
///
/// (This tracks edge-arc contact only, not isolated-vertex contact;
/// `Tiling::generate` independently flags any patch whose neighbour
/// computations cross an infinite-order supertile boundary, which would
/// catch such a pathology empirically.)
fn cycle_is_interior(cs: &CombSystem, layer: &Layer, walk: &[Step]) -> bool {
    let entry = layer.subtiles[&walk[0].0][walk[0].1];
    let mut alive: BTreeSet<usize> =
        (0..cs.lookup_tile_nedges(entry)).collect();
    // A surviving chain's state is (period position, edge index); after
    // max_edges + 1 whole periods some period-start state must repeat,
    // so any chain still alive then survives forever.
    let max_edges = layer
        .tiles
        .iter()
        .map(|&t| cs.lookup_tile_nedges(t))
        .max()
        .unwrap_or(0);
    for _ in 0..=max_edges {
        if alive.is_empty() {
            return true;
        }
        for &(p, i) in walk {
            let Some(adjacency) = layer.adjacency.get(&p) else {
                return false; // can't prove interiority — be conservative
            };
            alive = alive
                .iter()
                .filter_map(|&e| {
                    match adjacency.get(&Subedge::Int { subtile: i, edge: e })
                    {
                        Some(&Subedge::Ext { edge, .. }) => Some(edge),
                        Some(&Subedge::Int { .. }) => None,
                        None => Some(usize::MAX), // unknown: keep alive
                    }
                })
                .collect();
            if alive.contains(&usize::MAX) {
                return false;
            }
        }
    }
    alive.is_empty()
}

/// The shortest, lexicographically least closed walk of (parent, slot)
/// steps in the self-parent layer that is eventually interior per
/// [`cycle_is_interior`]. Deterministic, so default roots are stable.
fn canonical_interior_cycle(cs: &CombSystem) -> (LayerIndex, Vec<Step>) {
    let layer_idx = cycle_layer(cs);
    let layer = cs.lookup_layer(layer_idx);
    let labels = sorted_labels(layer);

    fn dfs(
        cs: &CombSystem,
        layer: &Layer,
        labels: &[Step],
        walk: &mut Vec<Step>,
        len: usize,
    ) -> bool {
        if walk.len() == len {
            let entry = layer.subtiles[&walk[0].0][walk[0].1];
            return walk[len - 1].0 == entry
                && cycle_is_interior(cs, layer, walk);
        }
        for &(p, i) in labels {
            if let Some(&(prev_parent, _)) = walk.last()
                && layer.subtiles[&p][i] != prev_parent
            {
                continue;
            }
            walk.push((p, i));
            if dfs(cs, layer, labels, walk, len) {
                return true;
            }
            walk.pop();
        }
        false
    }

    for len in 1..=6 {
        let mut walk = Vec::with_capacity(len);
        if dfs(cs, layer, &labels, &mut walk, len) {
            return (layer_idx, walk);
        }
    }
    panic!("no eventually-interior cycle of length <= 6 found");
}

/// Canonical shortest chain of supertile steps from one (type, layer) to
/// another, taking labels in sorted order so the result is deterministic.
fn canonical_path(
    cs: &CombSystem,
    from: (TileIndex, LayerIndex),
    to: (TileIndex, LayerIndex),
) -> Option<Vec<Step>> {
    let mut prev: HashMap<(TileIndex, LayerIndex), ((TileIndex, LayerIndex), Step)> =
        HashMap::new();
    let mut queue = VecDeque::from([from]);
    let mut seen = BTreeSet::from([from]);
    while let Some((t, l)) = queue.pop_front() {
        if (t, l) == to {
            let mut path = Vec::new();
            let mut node = to;
            while node != from {
                let (parent_node, step) = prev[&node];
                path.push(step);
                node = parent_node;
            }
            path.reverse();
            return Some(path);
        }
        let layer = cs.lookup_layer(l);
        for &(p, i) in &sorted_labels(layer) {
            if layer.subtiles[&p][i] != t {
                continue;
            }
            let next = (p, layer.parent);
            if seen.insert(next) {
                prev.insert(next, ((t, l), (p, i)));
                queue.push_back(next);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hat() -> Tiling {
        Tiling::new(TilingFamily::Hat)
    }

    #[test]
    fn default_root_validates_and_round_trips() {
        let tiling = hat();
        let root = tiling.default_root();
        let addr =
            EPTileAddress::parse(&root, &tiling.sys.tmap).expect("parses");
        tiling.validate(&addr).expect("validates");
        assert_eq!(format!("{}", addr.display(&tiling.sys.cs_fine)), root);
    }

    #[test]
    fn neighbour_relation_is_symmetric() {
        let tiling = hat();
        let root =
            EPTileAddress::parse(&tiling.default_root(), &tiling.sys.tmap)
                .unwrap();
        let cs = &tiling.sys.cs_fine;
        let nedges = cs.lookup_tile_nedges(root.base_type().unwrap());
        for e in 0..nedges {
            let (n, back) = root.neighbour(e, &tiling.sys.tr).unwrap();
            let (back_home, back_edge) =
                n.neighbour(back, &tiling.sys.tr).unwrap();
            assert_eq!(back_home, root, "edge {e} does not invert");
            assert_eq!(back_edge, e, "edge {e} returns via wrong edge");
        }
    }

    #[test]
    fn hat_patch_is_deterministic_across_fresh_systems() {
        let (t1, t2) = (hat(), hat());
        let p1 = t1.generate(&t1.default_root(), 3).unwrap();
        let p2 = t2.generate(&t2.default_root(), 3).unwrap();
        assert_eq!(p1, p2);
    }

    #[test]
    fn hat_patch_has_bfs_structure_and_both_chiralities() {
        let tiling = hat();
        let patch = tiling.generate(&tiling.default_root(), 4).unwrap();
        assert_eq!(patch.cells[0].distance, 0);
        assert!(patch.cells.len() > 20);
        for c in 0..patch.graph.cells() {
            let dc = patch.cells[c as usize].distance;
            assert!(dc <= 4);
            for &n in patch.graph.neighbours(c) {
                let dn = patch.cells[n as usize].distance;
                assert!(dc.abs_diff(dn) <= 1, "BFS distance violated");
            }
        }
        let bases: BTreeSet<&str> = patch
            .cells
            .iter()
            .map(|c| patch.classes[c.class as usize].base.as_str())
            .collect();
        assert!(bases.contains("hat") && bases.contains("antihat"));
    }

    #[test]
    fn spectre_patch_generates_with_mystic_pair_classes() {
        // In the refined system the Mystic pair is the Gamma metatile,
        // the only one with two base-spectre slots (the skew spectre and
        // its ordinary partner); the other eight hexagon types hold one.
        let tiling = Tiling::new(TilingFamily::Spectre);
        let gamma_slots = tiling
            .classes()
            .iter()
            .filter(|c| c.parent == "Gamma")
            .count();
        assert_eq!(gamma_slots, 2, "classes: {:?}", tiling.classes());

        let patch = tiling.generate(&tiling.default_root(), 3).unwrap();
        assert!(patch.cells.len() > 10);
        let names: BTreeSet<&str> = patch
            .cells
            .iter()
            .map(|c| patch.classes[c.class as usize].name.as_str())
            .collect();
        assert!(
            names.contains("spectre@Gamma.0")
                && names.contains("spectre@Gamma.1"),
            "classes seen: {names:?}"
        );
        assert!(
            patch.seed_artifact_cells.is_empty(),
            "spectre default root showed no seed artifacts when written"
        );
    }

    #[test]
    fn chirality_stratification_splits_both_families() {
        // Hat: stratum 1 == antihats. Spectre: stratum 1 == Gamma-pair
        // (Mystic) spectres. Both yield exactly 2 strata, both non-empty.
        for (family, expect_stratum1) in [
            (TilingFamily::Hat, "antihat base"),
            (TilingFamily::Spectre, "Gamma parent"),
        ] {
            let tiling = Tiling::new(family);
            let patch = tiling.generate(&tiling.default_root(), 6).unwrap();
            let (strata, n) = patch.strata(Stratification::Chirality);
            assert_eq!(n, 2);
            assert_eq!(strata.len(), patch.cells.len());
            assert!(strata.contains(&0), "{expect_stratum1}");
            assert!(strata.contains(&1), "{expect_stratum1}");
            // stratum exactly matches the chirality predicate
            for (c, &s) in patch.cells.iter().zip(&strata) {
                let class = &patch.classes[c.class as usize];
                let want =
                    u8::from(class.base == "antihat" || class.parent == "Gamma");
                assert_eq!(s, want);
            }
            // uniform scheme is all-zero, single stratum
            let (u, un) = patch.strata(Stratification::Uniform);
            assert_eq!(un, 1);
            assert!(u.iter().all(|&s| s == 0));
        }
    }

    #[test]
    fn default_roots_have_no_seed_artifacts() {
        for family in [TilingFamily::Hat, TilingFamily::Spectre] {
            let tiling = Tiling::new(family);
            let patch = tiling.generate(&tiling.default_root(), 5).unwrap();
            assert!(
                patch.seed_artifact_cells.is_empty(),
                "{family:?}: artifacts at {:?}",
                patch.seed_artifact_cells
            );
        }
    }

    #[test]
    fn degenerate_cone_seed_root_is_flagged() {
        // The old default root before FINDINGS.md §2 was fixed: its
        // F0⊂F0 spine slot touches the parent boundary, so the
        // infinite-supertile union is a cone whose seam the transducer
        // glues to itself. Kept as a regression input for the artifact
        // detector (boundary-crossing and self-adjacency flagging).
        let tiling = hat();
        let old_root =
            "(tile antihat)(subtile 3 of H0)(subtile 1 of F0):(subtile 0 of F0)";
        let patch = tiling.generate(old_root, 3).unwrap();
        assert!(!patch.seed_artifact_cells.is_empty());
    }

    #[test]
    fn geometry_loop_closure_validates_adjacency() {
        // Placements propagate along the BFS tree only, so for every
        // non-tree graph edge the two endpoint polygons were positioned
        // via different paths — them still sharing an edge segment is an
        // independent consistency check of transducer against geometry.
        for (family, min_verts) in [
            (TilingFamily::Hat, 13),
            (TilingFamily::Spectre, 13),
            (TilingFamily::PenroseP2, 4),
            (TilingFamily::PenroseP3, 4),
        ] {
            let tiling = Tiling::new(family);
            let (patch, geom) = tiling
                .generate_with_geometry(&tiling.default_root(), 4)
                .unwrap();
            let verts = |c: u32| {
                &geom.xy[geom.offsets[c as usize] as usize
                    ..geom.offsets[c as usize + 1] as usize]
            };
            for c in 0..patch.graph.cells() {
                assert!(verts(c).len() >= min_verts, "degenerate polygon at {c}");
            }
            for a in 0..patch.graph.cells() {
                for &b in patch.graph.neighbours(a) {
                    if b < a {
                        continue;
                    }
                    let shared = verts(a)
                        .iter()
                        .filter(|va| {
                            verts(b).iter().any(|vb| {
                                (va[0] - vb[0]).hypot(va[1] - vb[1]) < 1e-6
                            })
                        })
                        .count();
                    assert!(
                        shared >= 2,
                        "{family:?}: adjacent cells {a},{b} share only \
                         {shared} vertices — geometry contradicts graph"
                    );
                }
            }
        }
    }

    #[test]
    fn penrose_patches_are_clean_and_deterministic() {
        for family in [TilingFamily::PenroseP2, TilingFamily::PenroseP3] {
            let (t1, t2) = (Tiling::new(family), Tiling::new(family));
            let root = t1.default_root();
            assert_eq!(root, t2.default_root(), "{family:?} root unstable");
            let p1 = t1.generate(&root, 5).unwrap();
            let p2 = t2.generate(&root, 5).unwrap();
            assert_eq!(p1, p2, "{family:?} patch not deterministic");
            assert!(
                p1.seed_artifact_cells.is_empty(),
                "{family:?}: artifacts at {:?}",
                p1.seed_artifact_cells
            );
            assert!(p1.cells.len() > 30, "{family:?}: only {}", p1.cells.len());
            // Quadrilateral tiles: every interior cell has exactly 4
            // edge-neighbours; boundary cells at most 4.
            for c in 0..p1.graph.cells() {
                assert!(p1.graph.degree(c) <= 4, "{family:?} cell {c}");
                if p1.cells[c as usize].distance < 5 {
                    assert_eq!(p1.graph.degree(c), 4, "{family:?} cell {c}");
                }
            }
            // Both tile types occur in the patch.
            let bases: BTreeSet<&str> = p1
                .cells
                .iter()
                .map(|c| p1.classes[c.class as usize].base.as_str())
                .collect();
            assert_eq!(bases.len(), 2, "{family:?}: {bases:?}");
        }
    }

    #[test]
    fn penrose_chirality_splits_tile_types() {
        for (family, stratum1_base) in [
            (TilingFamily::PenroseP2, "dart"),
            (TilingFamily::PenroseP3, "thin"),
        ] {
            let tiling = Tiling::new(family);
            let patch = tiling.generate(&tiling.default_root(), 4).unwrap();
            let (strata, n) = patch.strata(Stratification::Chirality);
            assert_eq!(n, 2);
            for (c, &s) in patch.cells.iter().zip(&strata) {
                let base = patch.classes[c.class as usize].base.as_str();
                assert_eq!(s, u8::from(base == stratum1_base));
            }
            assert!(strata.contains(&0) && strata.contains(&1));
        }
    }

    #[test]
    fn vertex_adjacency_is_a_superset_with_bounded_fan() {
        for family in [
            TilingFamily::Hat,
            TilingFamily::Spectre,
            TilingFamily::PenroseP2,
            TilingFamily::PenroseP3,
        ] {
            let penrose = matches!(
                family,
                TilingFamily::PenroseP2 | TilingFamily::PenroseP3
            );
            let tiling = Tiling::new(family);
            let root = tiling.default_root();
            let radius = 6;
            let edge = tiling.generate(&root, radius).unwrap();
            let vertex = tiling
                .generate_patch(&root, radius, Neighbourhood::Vertex)
                .unwrap();
            // The BFS (cells, metadata, distances) must be identical:
            // only the graph differs.
            assert_eq!(edge.cells, vertex.cells);
            assert_eq!(vertex.neighbourhood, Neighbourhood::Vertex);
            for c in 0..edge.graph.cells() {
                let en: BTreeSet<u32> =
                    edge.graph.neighbours(c).iter().copied().collect();
                let vn: BTreeSet<u32> =
                    vertex.graph.neighbours(c).iter().copied().collect();
                assert!(
                    en.is_subset(&vn),
                    "{family:?}: cell {c} vertex adjacency lost an edge \
                     neighbour — exact vertex matching is broken"
                );
                let d = edge.cells[c as usize].distance;
                if d + 3 <= radius {
                    // Interior invariants. All families: the fan around
                    // any shared vertex keeps vertex-neighbours within 3
                    // edge-BFS rings (measured margins: monotiles/P2 = 2,
                    // P3 = 3), and vertex degrees stay in the measured
                    // band (monotiles 6–7, Penrose 7–11).
                    assert!(
                        vn.len() >= en.len() && vn.len() >= 6 && vn.len() <= 16,
                        "{family:?} cell {c}: vertex degree {} (edge {})",
                        vn.len(),
                        en.len()
                    );
                    if penrose {
                        // Quadrilaterals: exactly 4 edge-neighbours and a
                        // strictly larger vertex neighbourhood.
                        assert_eq!(en.len(), 4);
                        assert!(vn.len() > 4);
                    }
                    for &n in &vn {
                        let dn = vertex.cells[n as usize].distance;
                        assert!(
                            d.abs_diff(dn) <= 3,
                            "{family:?}: fan bound violated ({d} vs {dn})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn growing_radius_preserves_cell_indices() {
        // Replaying a record on a larger patch relies on this: BFS
        // discovery order for cells within the smaller radius is
        // unaffected by enlarging it, so cell indices are stable and a
        // smaller patch's cells are a prefix of a larger patch's.
        let tiling = hat();
        let root = tiling.default_root();
        let small = tiling.generate(&root, 4).unwrap();
        let large = tiling.generate(&root, 7).unwrap();
        assert_eq!(
            small.cells[..],
            large.cells[..small.cells.len()],
            "smaller patch is not a prefix of the larger one"
        );
    }

    #[test]
    fn garbage_roots_are_rejected() {
        let tiling = hat();
        assert!(tiling.generate("nonsense", 2).is_err());
        // Structurally parseable but invalid trajectory: swap the base
        // tile's chirality without changing its claimed metatile slot.
        let root = tiling.default_root();
        let swapped = if root.starts_with("antihat") {
            root.replacen("antihat", "hat", 1)
        } else {
            root.replacen("hat", "antihat", 1)
        };
        assert!(tiling.generate(&swapped, 2).is_err());
    }
}
