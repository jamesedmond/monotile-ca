//! Shared substitution-system context: the refined combinatorial system,
//! its neighbour transducer, the exact-geometry output driver, the
//! class table, legal parent symbols per (layer, type), and the two
//! geometric constants the atlas region needs.

use std::collections::HashMap;

use substitution_tiling_transducers::builtin::BuiltinSystem;
use substitution_tiling_transducers::combinatorial::{CombSystem, Transducer};
use substitution_tiling_transducers::common::{LayerIndex, System, TileIndex};
use substitution_tiling_transducers::geometric::{OutputDriver, TilePlacement};
use tiling_core::{Tiling, TilingFamily};

pub struct Ctx<'a> {
    pub family: TilingFamily,
    pub tiling: &'a Tiling,
    pub sys: &'a BuiltinSystem,
    pub cs: &'a CombSystem,
    pub tr: &'a Transducer,
    pub od: OutputDriver,
    /// (level-1 parent type, slot) → class index, as `Tiling::classes()`.
    pub class_of: HashMap<(TileIndex, usize), u16>,
    /// class index → base tile type.
    pub base_of_class: Vec<TileIndex>,
    pub base_types: Vec<TileIndex>,
    /// Legal next address symbols for a tile of `type` living in `layer`:
    /// every (parent type, slot) whose deflation places `type` there.
    pub parents: HashMap<(LayerIndex, TileIndex), Vec<(TileIndex, usize)>>,
    /// Max circumradius of a base tile (centroid to farthest vertex).
    pub d_tile: f64,
    /// Max centroid distance between edge-adjacent base tiles.
    pub d_nb: f64,
    /// Canonical numbering of the transducer's DFA states: the library
    /// assigns state ids in hash order, which varies between processes,
    /// so keys use ids from a BFS over the DFA from its start state in
    /// sorted-symbol order.
    pub state_index: HashMap<usize, u16>,
}

impl<'a> Ctx<'a> {
    pub fn new(tiling: &'a Tiling) -> Self {
        let sys = tiling.system();
        let cs = &sys.cs_fine;
        let od = sys.output_driver();
        let base_layer = cs.lookup_layer(cs.base_layer);
        let mut base_types: Vec<TileIndex> = base_layer.tiles.clone();
        base_types.sort();

        let mut class_of = HashMap::new();
        let mut base_of_class = Vec::new();
        for (i, class) in tiling.classes().iter().enumerate() {
            let parent = cs
                .tile_by_name(&class.parent)
                .unwrap_or_else(|| panic!("class parent {} unknown", class.parent));
            class_of.insert((parent, class.subtile), i as u16);
            base_of_class.push(base_layer.subtiles[&parent][class.subtile]);
        }

        let mut parents: HashMap<(LayerIndex, TileIndex), Vec<(TileIndex, usize)>> =
            HashMap::new();
        for layer in &cs.layers {
            let mut ps: Vec<TileIndex> = layer.subtiles.keys().copied().collect();
            ps.sort();
            for p in ps {
                for (s, &child) in layer.subtiles[&p].iter().enumerate() {
                    parents.entry((layer.index, child)).or_default().push((p, s));
                }
            }
        }

        // Geometric constants from the prototiles and a small patch.
        let centroid = |od: &OutputDriver, p: &TilePlacement| -> [f64; 2] {
            let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
            for v in od.tile_vertices(p) {
                let [x, y] = v.to_float().expect("vertex converts");
                sx += x;
                sy += y;
                n += 1.0;
            }
            [sx / n, sy / n]
        };
        let mut d_tile: f64 = 0.0;
        for &t in &base_types {
            let p = od.place_default(t);
            let c = centroid(&od, &p);
            for v in od.tile_vertices(&p) {
                let [x, y] = v.to_float().unwrap();
                d_tile = d_tile.max((x - c[0]).hypot(y - c[1]));
            }
        }
        let (patch, geom) = tiling
            .generate_with_geometry(&tiling.default_root(), 6)
            .expect("small patch");
        let cent: Vec<[f64; 2]> = (0..patch.cells.len())
            .map(|c| {
                let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
                let m = (hi - lo) as f64;
                let (sx, sy) = geom.xy[lo..hi]
                    .iter()
                    .fold((0.0, 0.0), |(a, b), &[x, y]| (a + x, b + y));
                [sx / m, sy / m]
            })
            .collect();
        let mut d_nb: f64 = 0.0;
        for c in 0..patch.graph.cells() {
            for &n in patch.graph.neighbours(c) {
                let (a, b) = (cent[c as usize], cent[n as usize]);
                d_nb = d_nb.max((a[0] - b[0]).hypot(a[1] - b[1]));
            }
        }

        let tr = &sys.tr;
        let mut state_index: HashMap<usize, u16> = HashMap::new();
        let mut queue = std::collections::VecDeque::from([tr.start]);
        state_index.insert(tr.start, 0);
        while let Some(s) = queue.pop_front() {
            let mut syms: Vec<_> = tr.trans[&s].keys().copied().collect();
            syms.sort();
            for sym in syms {
                let next = tr.trans[&s][&sym].0;
                if !state_index.contains_key(&next) {
                    let id = state_index.len() as u16;
                    state_index.insert(next, id);
                    queue.push_back(next);
                }
            }
        }

        Self {
            family: tiling.family(),
            tiling,
            sys,
            cs,
            tr,
            od,
            class_of,
            base_of_class,
            base_types,
            parents,
            d_tile,
            d_nb,
            state_index,
        }
    }

    /// Canonical id of a DFA state.
    pub fn state_id(&self, state: usize) -> u16 {
        self.state_index[&state]
    }

    pub fn nedges(&self, t: TileIndex) -> usize {
        self.cs.lookup_tile_nedges(t)
    }

    pub fn tile_name(&self, t: TileIndex) -> &str {
        self.cs.lookup_tile_name(t)
    }

    pub fn centroid(&self, p: &TilePlacement) -> [f64; 2] {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for v in self.od.tile_vertices(p) {
            let [x, y] = v.to_float().expect("vertex converts");
            sx += x;
            sy += y;
            n += 1.0;
        }
        [sx / n, sy / n]
    }

    /// Midpoint of edge `e` of a placed tile (float).
    pub fn edge_midpoint(&self, p: &TilePlacement, e: usize) -> [f64; 2] {
        let verts: Vec<[f64; 2]> = self
            .od
            .tile_vertices(p)
            .map(|v| v.to_float().unwrap())
            .collect();
        let a = verts[e % verts.len()];
        let b = verts[(e + 1) % verts.len()];
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    }

    pub fn legal_parents(&self, layer: LayerIndex, top: TileIndex) -> &[(TileIndex, usize)] {
        self.parents
            .get(&(layer, top))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
