//! wasm-bindgen shim over `ca-engine` + `tiling-core` for the browser UI.
//!
//! Exposes a [`Universe`]: a generated patch (graph + metadata +
//! triangulated render geometry) bound to a CA engine. The browser runs
//! semantics identical to the native search — the same `ca-engine` code
//! compiled to wasm32.

use std::cell::RefCell;
use std::collections::HashMap;

use ca_engine::{AnyRule, CaRule, Engine, Graph, Rule, StepStats, StratifiedRule, TableRule};
use tiling_core::results::ResultRecord;
use tiling_core::slide::{SlideConfig, Slider};
use tiling_core::{CellMeta, Geometry, Neighbourhood, Patch, TileClass, Tiling, TilingFamily};
use wasm_bindgen::prelude::*;

/// Hard ceiling on patch radius. Generation cost grows roughly with
/// radius²; around 100+ expect tens of seconds behind the loading
/// overlay (~50k tiles), 192+ minutes. 256 admits the s32 mega-looper
/// (radius 192 — both wanderers stay inside and lock; FINDINGS §10.7)
/// with headroom; the renderer handles the tile count fine.
const MAX_RADIUS: u32 = 256;

thread_local! {
    /// Transducer construction takes seconds; cache one `Tiling` per
    /// family across `Universe` regenerations.
    static TILINGS: RefCell<HashMap<TilingFamily, Tiling>> =
        RefCell::new(HashMap::new());
}

/// Snapshot interval for the step-backward history. Stepping back costs
/// at most this many forward steps from the nearest snapshot.
const SNAPSHOT_INTERVAL: u64 = 64;
/// History cap; periodic snapshots beyond it are evicted oldest-first
/// (pinned ones — generation 0 and every discontinuity — survive, so
/// stepping far back falls through to a longer exact replay).
const MAX_SNAPSHOTS: usize = 256;

/// A point on the actual state timeline. `rule` is the rule that was in
/// force *from* this generation on, so replaying any snapshot→snapshot
/// segment uses one rule even if the user changed rules mid-run.
struct Snapshot {
    generation: u64,
    rule: AnyRule,
    state: Vec<u8>,
    pinned: bool,
}

#[wasm_bindgen]
pub struct Universe {
    family_name: String,
    patch: Patch,
    geometry: Geometry,
    engine: Engine,
    rule: AnyRule,
    last: StepStats,
    history: Vec<Snapshot>,
    tri_vertices: Vec<f32>,
    tri_cells: Vec<u32>,
}

impl Universe {
    /// Record the current state as a periodic snapshot (no-op if one for
    /// this generation already exists).
    fn snapshot_periodic(&mut self) {
        let generation = self.engine.generation();
        if self.history.last().is_some_and(|s| s.generation >= generation) {
            return;
        }
        self.history.push(Snapshot {
            generation,
            rule: self.rule.clone(),
            state: self.engine.state().to_vec(),
            pinned: false,
        });
        if self.history.len() > MAX_SNAPSHOTS
            && let Some(i) = self.history.iter().position(|s| !s.pinned)
        {
            self.history.remove(i);
        }
    }

    /// The timeline forked at the current generation (state edit or rule
    /// change): drop now-invalid future snapshots, pin the new present.
    fn discontinuity(&mut self) {
        let generation = self.engine.generation();
        self.history.retain(|s| s.generation < generation);
        self.history.push(Snapshot {
            generation,
            rule: self.rule.clone(),
            state: self.engine.state().to_vec(),
            pinned: true,
        });
    }
}

fn parse_family(family: &str) -> Result<TilingFamily, JsError> {
    match family {
        "hat" => Ok(TilingFamily::Hat),
        "spectre" => Ok(TilingFamily::Spectre),
        "penrosep2" => Ok(TilingFamily::PenroseP2),
        "penrosep3" => Ok(TilingFamily::PenroseP3),
        other => Err(JsError::new(&format!(
            "unknown family '{other}' (expected 'hat', 'spectre', \
             'penrosep2' or 'penrosep3')"
        ))),
    }
}

fn build(
    family: TilingFamily,
    root: Option<&str>,
    radius: u32,
    neighbourhood: Neighbourhood,
) -> Result<Universe, JsError> {
    console_error_panic_hook::set_once();
    if radius > MAX_RADIUS {
        return Err(JsError::new(&format!(
            "radius {radius} exceeds maximum {MAX_RADIUS}"
        )));
    }
    let (patch, geometry) = TILINGS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let tiling =
            cache.entry(family).or_insert_with(|| Tiling::new(family));
        let root = match root {
            Some(r) => r.to_string(),
            None => tiling.default_root(),
        };
        tiling.generate_patch_with_geometry(&root, radius, neighbourhood)
    })
    .map_err(|e| JsError::new(&e.to_string()))?;

    let family_name = match family {
        TilingFamily::Hat => "hat",
        TilingFamily::Spectre => "spectre",
        TilingFamily::PenroseP2 => "penrosep2",
        TilingFamily::PenroseP3 => "penrosep3",
    };
    assemble(family_name.into(), patch, geometry)
}

/// Wrap a finished (patch, geometry) pair as a Universe (shared by the
/// tiling path and the synthetic square-grid path).
fn assemble(
    family_name: String,
    patch: Patch,
    geometry: Geometry,
) -> Result<Universe, JsError> {
    let (tri_vertices, tri_cells) =
        triangulate(&geometry).map_err(|e| JsError::new(&e))?;
    let engine = Engine::new(patch.graph.clone());
    let rule = AnyRule::Generations(StratifiedRule::uniform(Rule::new(
        Rule::mask(&[3]),
        Rule::mask(&[2, 3]),
    )));
    let history = vec![Snapshot {
        generation: 0,
        rule: rule.clone(),
        state: engine.state().to_vec(),
        pinned: true,
    }];
    Ok(Universe {
        family_name,
        patch,
        geometry,
        engine,
        rule,
        last: StepStats {
            population: 0,
            changed: 0,
        },
        history,
        tri_vertices,
        tri_cells,
    })
}

#[wasm_bindgen]
impl Universe {
    /// An n-by-n square grid — the reference periodic substrate for the
    /// essay's CA-basics section. Edge adjacency is the von Neumann
    /// neighbourhood (4 neighbours), vertex is Moore (8). Same engine,
    /// same renderer: a grid is just another graph.
    #[wasm_bindgen(js_name = createGrid)]
    pub fn create_grid(n: u32, neighbourhood: &str) -> Result<Universe, JsError> {
        console_error_panic_hook::set_once();
        if !(2..=256).contains(&n) {
            return Err(JsError::new("grid size must be 2..=256"));
        }
        let nb = match neighbourhood {
            "edge" => Neighbourhood::Edge,
            "vertex" => Neighbourhood::Vertex,
            other => {
                return Err(JsError::new(&format!(
                    "unknown neighbourhood '{other}' (expected 'edge' or 'vertex')"
                )));
            }
        };
        let cells = n * n;
        let at = |x: u32, y: u32| y * n + x;
        let mut edges: Vec<(u32, u32)> = Vec::new();
        for y in 0..n {
            for x in 0..n {
                if x + 1 < n {
                    edges.push((at(x, y), at(x + 1, y)));
                }
                if y + 1 < n {
                    edges.push((at(x, y), at(x, y + 1)));
                }
                if nb == Neighbourhood::Vertex && y + 1 < n {
                    if x + 1 < n {
                        edges.push((at(x, y), at(x + 1, y + 1)));
                    }
                    if x >= 1 {
                        edges.push((at(x, y), at(x - 1, y + 1)));
                    }
                }
            }
        }
        let graph = Graph::from_edges(cells, &edges);
        let c = (f64::from(n) - 1.0) / 2.0;
        let cells_meta: Vec<CellMeta> = (0..cells)
            .map(|i| {
                let (x, y) = (i % n, i / n);
                CellMeta {
                    address: format!("{x},{y}"),
                    class: 0,
                    distance: (f64::from(x) - c)
                        .abs()
                        .max((f64::from(y) - c).abs())
                        .round() as u32,
                }
            })
            .collect();
        let classes = vec![TileClass {
            name: "square".into(),
            base: "square".into(),
            parent: "grid".into(),
            subtile: 0,
        }];
        let half = f64::from(n) / 2.0;
        let mut offsets = Vec::with_capacity(cells as usize + 1);
        let mut xy = Vec::with_capacity(cells as usize * 4);
        offsets.push(0u32);
        for i in 0..cells {
            let (x, y) = (f64::from(i % n) - half, f64::from(i / n) - half);
            xy.push([x, y]);
            xy.push([x + 1.0, y]);
            xy.push([x + 1.0, y + 1.0]);
            xy.push([x, y + 1.0]);
            offsets.push((i + 1) * 4);
        }
        let patch = Patch {
            graph,
            cells: cells_meta,
            classes,
            root: format!("grid {n}x{n}"),
            radius: n / 2,
            neighbourhood: nb,
            seed_artifact_cells: Vec::new(),
        };
        assemble("grid".into(), patch, Geometry { offsets, xy })
    }

    /// Like [`create`](Self::create) with an explicit root address —
    /// including degenerate (cone) roots, for the essay's seam demo.
    #[wasm_bindgen(js_name = createWithRoot)]
    pub fn create_with_root(
        family: &str,
        root: &str,
        radius: u32,
    ) -> Result<Universe, JsError> {
        build(parse_family(family)?, Some(root), radius, Neighbourhood::Edge)
    }

    /// Indices of cells whose neighbour computation crossed a degenerate
    /// (cone/seam) boundary — empty on every sound patch.
    #[wasm_bindgen(js_name = seedArtifactCells)]
    pub fn seed_artifact_cells(&self) -> Vec<u32> {
        self.patch.seed_artifact_cells.clone()
    }

    /// Neighbours of one cell (for the essay's 1-ball recurrence demo).
    #[wasm_bindgen(js_name = neighboursOf)]
    pub fn neighbours_of(&self, cell: u32) -> Vec<u32> {
        if cell >= self.patch.graph.cells() {
            return Vec::new();
        }
        self.patch.graph.neighbours(cell).to_vec()
    }

    /// Per-cell neighbour counts (the graph's degrees).
    #[wasm_bindgen(js_name = cellDegrees)]
    pub fn cell_degrees(&self) -> Vec<u32> {
        (0..self.patch.graph.cells())
            .map(|c| self.patch.graph.neighbours(c).len() as u32)
            .collect()
    }

    pub fn create(family: &str, radius: u32) -> Result<Universe, JsError> {
        build(parse_family(family)?, None, radius, Neighbourhood::Edge)
    }

    /// Create on a chosen neighbourhood ('edge' | 'vertex') — the
    /// playground's custom-rule path.
    #[wasm_bindgen(js_name = createWithNeighbourhood)]
    pub fn create_with_neighbourhood(
        family: &str,
        radius: u32,
        neighbourhood: &str,
    ) -> Result<Universe, JsError> {
        let nb = match neighbourhood {
            "edge" => Neighbourhood::Edge,
            "vertex" => Neighbourhood::Vertex,
            other => return Err(JsError::new(&format!("unknown neighbourhood: {other}"))),
        };
        build(parse_family(family)?, None, radius, nb)
    }

    /// Rebuild a recorded search run: regenerate its exact patch, set its
    /// rule, and load its initial configuration at generation 0.
    #[wasm_bindgen(js_name = createFromResult)]
    pub fn create_from_result(json: &str) -> Result<Universe, JsError> {
        Self::replay(json, None)
    }

    /// Like [`createFromResult`](Self::create_from_result) but on a patch
    /// of the given radius instead of the recorded one — cell indices are
    /// BFS-stable as radius grows (tested), so the recorded initial cells
    /// land on the same tiles while the run gains room to evolve without
    /// dead-boundary interference.
    #[wasm_bindgen(js_name = createFromResultAt)]
    pub fn create_from_result_at(
        json: &str,
        radius: u32,
    ) -> Result<Universe, JsError> {
        Self::replay(json, Some(radius))
    }

    fn replay(json: &str, radius: Option<u32>) -> Result<Universe, JsError> {
        let record: ResultRecord = serde_json::from_str(json)
            .map_err(|e| JsError::new(&format!("bad result record: {e}")))?;
        let radius = radius.unwrap_or(record.radius);
        let mut universe = build(
            record.family,
            Some(&record.root),
            radius,
            record.neighbourhood,
        )?;
        // Honour a class-stratified record: rebuild the engine with the
        // record's strata (recomputed for this patch) and its per-class
        // rule. Uniform records yield all-zero strata + a one-table rule,
        // so this path also covers them.
        let (strata, rule) = record.replay_setup(&universe.patch);
        universe.engine = Engine::with_strata(universe.patch.graph.clone(), strata);
        universe.rule = rule;
        let state = record
            .initial_state(universe.patch.graph.cells())
            .map_err(|e| JsError::new(&e.to_string()))?;
        universe.engine.load_state(&state);
        universe.last = StepStats {
            population: universe.engine.population(),
            changed: 0,
        };
        universe.history = vec![Snapshot {
            generation: 0,
            rule: universe.rule.clone(),
            state: universe.engine.state().to_vec(),
            pinned: true,
        }];
        Ok(universe)
    }

    // --- static patch data ---

    #[wasm_bindgen(js_name = cellCount)]
    pub fn cell_count(&self) -> u32 {
        self.patch.graph.cells()
    }

    #[wasm_bindgen(js_name = triVertices)]
    pub fn tri_vertices(&self) -> Vec<f32> {
        self.tri_vertices.clone()
    }

    #[wasm_bindgen(js_name = triCells)]
    pub fn tri_cells(&self) -> Vec<u32> {
        self.tri_cells.clone()
    }

    #[wasm_bindgen(js_name = polygonOffsets)]
    pub fn polygon_offsets(&self) -> Vec<u32> {
        self.geometry.offsets.clone()
    }

    #[wasm_bindgen(js_name = polygonXy)]
    pub fn polygon_xy(&self) -> Vec<f32> {
        self.geometry
            .xy
            .iter()
            .flat_map(|&[x, y]| [x as f32, y as f32])
            .collect()
    }

    #[wasm_bindgen(js_name = cellClasses)]
    pub fn cell_classes(&self) -> Vec<u16> {
        self.patch.cells.iter().map(|c| c.class).collect()
    }

    #[wasm_bindgen(js_name = classInfoJson)]
    pub fn class_info_json(&self) -> String {
        let classes: Vec<serde_json::Value> = self
            .patch
            .classes
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "base": c.base,
                    "parent": c.parent,
                    "subtile": c.subtile,
                })
            })
            .collect();
        serde_json::Value::Array(classes).to_string()
    }

    #[wasm_bindgen(js_name = cellDistances)]
    pub fn cell_distances(&self) -> Vec<u32> {
        self.patch.cells.iter().map(|c| c.distance).collect()
    }

    #[wasm_bindgen(js_name = addressOf)]
    pub fn address_of(&self, cell: u32) -> Result<String, JsError> {
        self.patch
            .cells
            .get(cell as usize)
            .map(|c| c.address.clone())
            .ok_or_else(|| JsError::new("cell index out of range"))
    }

    pub fn root(&self) -> String {
        self.patch.root.clone()
    }

    pub fn family(&self) -> String {
        self.family_name.clone()
    }

    pub fn radius(&self) -> u32 {
        self.patch.radius
    }

    /// Number of CA states `k` (2 = Life; >2 = Generations with dying
    /// phases). The renderer fades states `2..k-1` from alive toward dead.
    pub fn states(&self) -> u32 {
        u32::from(CaRule::states(&self.rule))
    }

    /// Birth mask of table 0, for the B/S rule editor. A generalised
    /// table-rule record has no B/S masks; the editor shows 0/0 and any
    /// edit replaces the rule (see [`setRule`](Self::set_rule)).
    #[wasm_bindgen(js_name = ruleBirth)]
    pub fn rule_birth(&self) -> u32 {
        match &self.rule {
            AnyRule::Generations(r) => r.tables[0].birth,
            AnyRule::Table(_) => 0,
        }
    }

    #[wasm_bindgen(js_name = ruleSurvival)]
    pub fn rule_survival(&self) -> u32 {
        match &self.rule {
            AnyRule::Generations(r) => r.tables[0].survival,
            AnyRule::Table(_) => 0,
        }
    }

    #[wasm_bindgen(js_name = seedArtifactCount)]
    pub fn seed_artifact_count(&self) -> u32 {
        self.patch.seed_artifact_cells.len() as u32
    }

    // --- engine ---

    #[wasm_bindgen(js_name = setRule)]
    pub fn set_rule(&mut self, birth: u32, survival: u32) {
        // Replicate across the current table count so editing keeps the
        // rule's table count matching the engine's strata (editing a
        // per-class champion collapses it to a uniform rule — the only
        // sane thing the single B/S editor can express — without an
        // out-of-range stratum index).
        let table = Rule::new(birth, survival);
        let tables = match &self.rule {
            AnyRule::Generations(r) => r.tables.len(),
            // Editing away from a table-rule record collapses to uniform.
            AnyRule::Table(_) => 1,
        };
        let rule = AnyRule::Generations(StratifiedRule::new(vec![table; tables]));
        if rule != self.rule {
            self.rule = rule;
            self.discontinuity();
        }
    }

    /// Set a k-state Generations rule (k = 2 is plain Life-like); same
    /// table-count replication as [`set_rule`](Self::set_rule).
    #[wasm_bindgen(js_name = setGenerationsRule)]
    pub fn set_generations_rule(&mut self, birth: u32, survival: u32, states: u8) {
        let table = Rule::generations(birth, survival, states);
        let tables = match &self.rule {
            AnyRule::Generations(r) => r.tables.len(),
            AnyRule::Table(_) => 1,
        };
        let rule = AnyRule::Generations(StratifiedRule::new(vec![table; tables]));
        if rule != self.rule {
            self.rule = rule;
            self.discontinuity();
        }
    }

    /// Install a priority-table rule from its JSON encoding — the same
    /// shape result records carry:
    /// `{"states":k,"rows":[{"own":s,"conds":[[state,min],..],"next":s},..]}`
    /// (`own` absent = wildcard; first matching row wins; no match -> 0).
    #[wasm_bindgen(js_name = setTableRuleJson)]
    pub fn set_table_rule_json(&mut self, json: &str) -> Result<(), JsError> {
        let table: TableRule = serde_json::from_str(json)
            .map_err(|e| JsError::new(&format!("bad table rule: {e}")))?;
        let rule = AnyRule::Table(table);
        if rule != self.rule {
            self.rule = rule;
            self.discontinuity();
        }
        Ok(())
    }

    /// Restore the recorded initial state (the pinned generation-0
    /// snapshot) — cheap looping for essay panels, no patch rebuild.
    #[wasm_bindgen(js_name = restartFromInitial)]
    pub fn restart_from_initial(&mut self) {
        let state = self.history.first().expect("gen-0 snapshot").state.clone();
        self.engine.restore(&state, 0);
        self.history.truncate(1);
        self.last = StepStats {
            population: self.engine.population(),
            changed: 0,
        };
    }

    pub fn step(&mut self, generations: u32) {
        for _ in 0..generations.min(100_000) {
            self.last = self.rule.step(&mut self.engine);
            if self.engine.generation().is_multiple_of(SNAPSHOT_INTERVAL) {
                self.snapshot_periodic();
            }
        }
    }

    /// Recompute the previous generation from the snapshot history.
    /// Costs at most SNAPSHOT_INTERVAL forward steps. Returns false at
    /// generation 0. Replays use each segment's recorded rule, so the
    /// reconstruction is exact even across mid-run rule changes; the
    /// *current* rule is left untouched.
    #[wasm_bindgen(js_name = stepBack)]
    pub fn step_back(&mut self) -> bool {
        let generation = self.engine.generation();
        if generation == 0 {
            return false;
        }
        let target = generation - 1;
        let snap = self
            .history
            .iter()
            .rev()
            .find(|s| s.generation <= target)
            .expect("a generation-0 snapshot always exists");
        let snap_generation = snap.generation;
        let snap_rule = snap.rule.clone();
        let state = snap.state.clone();
        self.engine.restore(&state, snap_generation);
        self.last = StepStats {
            population: self.engine.population(),
            changed: 0,
        };
        while self.engine.generation() < target {
            self.last = snap_rule.step(&mut self.engine);
        }
        true
    }

    pub fn generation(&self) -> f64 {
        self.engine.generation() as f64
    }

    pub fn population(&self) -> u32 {
        self.engine.population()
    }

    #[wasm_bindgen(js_name = lastChanged)]
    pub fn last_changed(&self) -> u32 {
        self.last.changed
    }

    #[wasm_bindgen(js_name = statePtr)]
    pub fn state_ptr(&self) -> *const u8 {
        self.engine.state().as_ptr()
    }

    /// Return to the most recent pinned snapshot — the state at load,
    /// or after the last edit (paint / clear / scatter / rule change).
    #[wasm_bindgen(js_name = resetToPinned)]
    pub fn reset_to_pinned(&mut self) {
        if let Some(s) = self.history.iter().rev().find(|s| s.pinned) {
            let state = s.state.clone();
            let g = s.generation;
            self.engine.restore(&state, g);
            self.history.retain(|h| h.generation <= g);
            self.last = StepStats {
                population: self.engine.population(),
                changed: 0,
            };
        }
    }

    /// Scatter `state` over ~fill_permille of the cells within
    /// `within_distance` of the root WITHOUT clearing — layers over the
    /// current board (Clear first for a fresh soup). The state is mixed
    /// into the rng stream so each state's scatter differs at one seed.
    #[wasm_bindgen(js_name = scatterState)]
    pub fn scatter_state(
        &mut self,
        fill_permille: u32,
        seed: u32,
        within_distance: u32,
        state: u8,
    ) {
        let k = CaRule::states(&self.rule).max(2);
        let s = state.min(k - 1);
        let mut rng = (u64::from(seed) << 16) ^ (u64::from(state) << 8) | 0x9e37_79b9;
        let mut v = self.engine.state().to_vec();
        for c in 0..self.patch.graph.cells() {
            if self.patch.cells[c as usize].distance <= within_distance
                && xorshift64(&mut rng) % 1000 < u64::from(fill_permille)
            {
                v[c as usize] = s;
            }
        }
        let g = self.engine.generation();
        self.engine.restore(&v, g);
        self.last = StepStats {
            population: self.engine.population(),
            changed: 0,
        };
        self.discontinuity();
    }

    /// Paint one cell to an arbitrary state (clamped to the current
    /// rule's state count) — the playground's multi-state seeding brush.
    #[wasm_bindgen(js_name = setCellState)]
    pub fn set_cell_state(&mut self, cell: u32, state: u8) {
        if cell >= self.patch.graph.cells() {
            return;
        }
        let k = CaRule::states(&self.rule).max(2);
        let mut s = self.engine.state().to_vec();
        s[cell as usize] = state.min(k - 1);
        let g = self.engine.generation();
        self.engine.restore(&s, g);
        self.last = StepStats {
            population: self.engine.population(),
            changed: 0,
        };
        self.discontinuity();
    }

    #[wasm_bindgen(js_name = toggleCell)]
    pub fn toggle_cell(&mut self, cell: u32) {
        if cell < self.patch.graph.cells() {
            self.engine.set_cell(cell, !self.engine.cell(cell));
            self.discontinuity();
        }
    }

    pub fn clear(&mut self) {
        self.engine.clear();
        self.last = StepStats {
            population: 0,
            changed: 0,
        };
        self.discontinuity();
    }

    pub fn randomize(
        &mut self,
        fill_permille: u32,
        seed: u32,
        within_distance: u32,
    ) {
        self.engine.clear();
        let mut rng = u64::from(seed) << 16 | 0x9e37_79b9;
        for c in 0..self.patch.graph.cells() {
            if self.patch.cells[c as usize].distance <= within_distance {
                let alive = xorshift64(&mut rng) % 1000 < u64::from(fill_permille);
                self.engine.set_cell(c, alive);
            }
        }
        self.last = StepStats {
            population: self.engine.population(),
            changed: 0,
        };
        self.discontinuity();
    }
}

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Ear-clip every cell polygon into a flat triangle list with a parallel
/// per-vertex cell-id array (hat/spectre outlines are non-convex, so a
/// simple fan won't do).
fn triangulate(geometry: &Geometry) -> Result<(Vec<f32>, Vec<u32>), String> {
    let mut tri_vertices = Vec::new();
    let mut tri_cells = Vec::new();
    for c in 0..geometry.offsets.len() - 1 {
        let lo = geometry.offsets[c] as usize;
        let hi = geometry.offsets[c + 1] as usize;
        let flat: Vec<f64> =
            geometry.xy[lo..hi].iter().flat_map(|&[x, y]| [x, y]).collect();
        let indices = earcutr::earcut(&flat, &[], 2)
            .map_err(|e| format!("triangulation failed on cell {c}: {e:?}"))?;
        for i in indices {
            tri_vertices.push(flat[2 * i] as f32);
            tri_vertices.push(flat[2 * i + 1] as f32);
            tri_cells.push(c as u32);
        }
    }
    Ok((tri_vertices, tri_cells))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shoelace(poly: &[[f64; 2]]) -> f64 {
        let n = poly.len();
        (0..n)
            .map(|i| {
                let [x0, y0] = poly[i];
                let [x1, y1] = poly[(i + 1) % n];
                x0 * y1 - x1 * y0
            })
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn universe_smoke_and_triangulation_conserves_area() {
        let mut u = Universe::create("hat", 4).unwrap();
        let n = u.cell_count();
        assert!(n > 50);
        assert_eq!(u.seed_artifact_count(), 0);

        // Triangle areas must sum to the polygon areas (the hat outline
        // has a collinear vertex, so triangle *counts* aren't reliable).
        let poly_area: f64 = (0..n as usize)
            .map(|c| {
                let lo = u.geometry.offsets[c] as usize;
                let hi = u.geometry.offsets[c + 1] as usize;
                shoelace(&u.geometry.xy[lo..hi])
            })
            .sum();
        let tri_area: f64 = u
            .tri_vertices
            .chunks_exact(6)
            .map(|t| {
                let t: Vec<f64> = t.iter().map(|&v| f64::from(v)).collect();
                ((t[2] - t[0]) * (t[5] - t[1])
                    - (t[4] - t[0]) * (t[3] - t[1]))
                    .abs()
                    / 2.0
            })
            .sum();
        assert!(
            (poly_area - tri_area).abs() / poly_area < 1e-4,
            "polygon area {poly_area} vs triangle area {tri_area}"
        );

        u.set_rule(Rule::mask(&[2]), Rule::mask(&[1, 2]));
        u.randomize(300, 42, 2);
        assert!(u.population() > 0);
        u.step(5);
        assert_eq!(u.generation(), 5.0);
        u.clear();
        assert_eq!(u.population(), 0);
    }

    #[test]
    fn step_back_reconstructs_history_exactly() {
        let mut u = Universe::create("hat", 4).unwrap();
        u.set_rule(Rule::mask(&[2]), Rule::mask(&[1, 2]));
        u.randomize(300, 7, 2);
        assert!(!u.step_back(), "cannot step back from generation 0");

        // Record the true timeline, with a rule change at generation 100.
        let mut timeline = vec![u.engine.state().to_vec()];
        for g in 0..200 {
            if g == 100 {
                u.set_rule(Rule::mask(&[2, 3]), Rule::mask(&[1, 2, 3]));
            }
            u.step(1);
            timeline.push(u.engine.state().to_vec());
        }

        // Walk all the way back; every generation must match, including
        // across the rule-change discontinuity and snapshot gaps.
        for g in (0..200u64).rev() {
            assert!(u.step_back());
            assert_eq!(u.generation(), g as f64);
            assert_eq!(
                u.engine.state(),
                &timeline[g as usize][..],
                "state mismatch at generation {g}"
            );
        }
        assert!(!u.step_back());

        // Stepping forward again re-derives the same timeline (and the
        // current rule must have been left as the latest one set).
        assert_eq!(
            u.rule,
            AnyRule::Generations(StratifiedRule::uniform(Rule::new(
                Rule::mask(&[2, 3]),
                Rule::mask(&[1, 2, 3])
            )))
        );
    }
}

// --- Infinite flight ---------------------------------------------------

thread_local! {
    /// Per-family tilings leaked to `'static` so a [`Slider`] (which
    /// borrows its `Tiling`) can live inside a wasm-bindgen object.
    /// Families are few and tilings are cached forever anyway.
    static STATIC_TILINGS: RefCell<HashMap<TilingFamily, &'static Tiling>> =
        RefCell::new(HashMap::new());
}

fn tiling_static(family: TilingFamily) -> &'static Tiling {
    STATIC_TILINGS.with(|cache| {
        *cache
            .borrow_mut()
            .entry(family)
            .or_insert_with(|| Box::leak(Box::new(Tiling::new(family))))
    })
}

/// An unbounded sliding-window flight — the instrument behind the paper's
/// million-ring measurements — exposed to the browser: the identical
/// `tiling_core::slide::Slider`, stepped generation by generation, its
/// window re-rooted ahead of the glider whenever activity nears the
/// margin. Render geometry is window-local; [`frame`](Self::frame) maps
/// it into the launch frame exactly, so the renderer keeps GPU
/// coordinates bounded regardless of flight length.
#[wasm_bindgen]
pub struct FlightUniverse {
    slider: Slider<'static>,
    states: u8,
    tri_vertices: Vec<f32>,
    tri_cells: Vec<u32>,
}

impl FlightUniverse {
    /// Re-triangulate the current window's geometry (retained by the
    /// slider — no patch regeneration) after a hop.
    fn regen_geometry(&mut self) -> Result<(), JsError> {
        let geometry = self
            .slider
            .window_geometry()
            .ok_or_else(|| JsError::new("flight has no geometry (frames untracked)"))?;
        let (tri_vertices, tri_cells) =
            triangulate(geometry).map_err(|e| JsError::new(&e))?;
        self.tri_vertices = tri_vertices;
        self.tri_cells = tri_cells;
        Ok(())
    }
}

#[wasm_bindgen]
impl FlightUniverse {
    /// Launch a flight from a committed record (one JSONL line). Config
    /// mirrors the flight campaign's CLI defaults: pop bands from the
    /// record's peak population, window auto-grow capped at 4x.
    /// `select_heading` keeps the launch component nearest that heading
    /// (degrees CCW from +x) when the seed emits several objects.
    pub fn launch(
        json: &str,
        window_radius: u32,
        launch_radius: u32,
        launch_gens: u32,
        select_heading: Option<f64>,
    ) -> Result<FlightUniverse, JsError> {
        console_error_panic_hook::set_once();
        let record: ResultRecord = serde_json::from_str(json)
            .map_err(|e| JsError::new(&format!("bad result record: {e}")))?;
        let states = record
            .table_rule
            .as_ref()
            .map_or(record.rule.states, |t| t.states);
        let cfg = SlideConfig {
            window_radius,
            launch_radius,
            launch_gens: u64::from(launch_gens),
            max_window_radius: window_radius * 4,
            pop_band: record.max_population,
            launch_pop_band: record.max_population,
            track_frames: true,
            select_heading,
        };
        let tiling = tiling_static(record.family);
        let slider = Slider::launch(tiling, &record, &cfg)
            .map_err(|e| JsError::new(&e.to_string()))?;
        let mut flight = FlightUniverse {
            slider,
            states,
            tri_vertices: Vec::new(),
            tri_cells: Vec::new(),
        };
        flight.regen_geometry()?;
        Ok(flight)
    }

    /// Advance `generations` steps; returns the number of window hops
    /// that occurred (nonzero: geometry changed, re-fetch it). Errors
    /// are terminal — the same margin/pop-band conditions that abort a
    /// native flight.
    pub fn step(&mut self, generations: u32) -> Result<u32, JsError> {
        let mut hops = 0u32;
        for _ in 0..generations {
            if self
                .slider
                .step()
                .map_err(|e| JsError::new(&e.to_string()))?
                .is_some()
            {
                hops += 1;
            }
        }
        if hops > 0 {
            self.regen_geometry()?;
        }
        Ok(hops)
    }

    // --- window-local render data (re-fetch after any hop) ---

    #[wasm_bindgen(js_name = cellCount)]
    pub fn cell_count(&self) -> u32 {
        self.slider.window_patch().graph.cells()
    }
    #[wasm_bindgen(js_name = triVertices)]
    pub fn tri_vertices(&self) -> Vec<f32> {
        self.tri_vertices.clone()
    }
    #[wasm_bindgen(js_name = triCells)]
    pub fn tri_cells(&self) -> Vec<u32> {
        self.tri_cells.clone()
    }
    #[wasm_bindgen(js_name = polygonOffsets)]
    pub fn polygon_offsets(&self) -> Vec<u32> {
        self.slider
            .window_geometry()
            .map(|g| g.offsets.clone())
            .unwrap_or_default()
    }
    #[wasm_bindgen(js_name = polygonXy)]
    pub fn polygon_xy(&self) -> Vec<f32> {
        self.slider
            .window_geometry()
            .map(|g| {
                g.xy.iter()
                    .flat_map(|&[x, y]| [x as f32, y as f32])
                    .collect()
            })
            .unwrap_or_default()
    }
    #[wasm_bindgen(js_name = cellClasses)]
    pub fn cell_classes(&self) -> Vec<u16> {
        self.slider
            .window_patch()
            .cells
            .iter()
            .map(|c| c.class)
            .collect()
    }
    #[wasm_bindgen(js_name = classInfoJson)]
    pub fn class_info_json(&self) -> String {
        let classes: Vec<serde_json::Value> = self
            .slider
            .window_patch()
            .classes
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "base": c.base,
                    "parent": c.parent,
                    "subtile": c.subtile,
                })
            })
            .collect();
        serde_json::Value::Array(classes).to_string()
    }
    /// Pointer into wasm memory for the per-cell state bytes (length
    /// [`cellCount`](Self::cell_count)); rebuild views after any hop.
    #[wasm_bindgen(js_name = statePtr)]
    pub fn state_ptr(&self) -> *const u8 {
        self.slider.window_state().as_ptr()
    }
    /// Number of CA states `k` (palette size).
    pub fn states(&self) -> u32 {
        u32::from(self.states)
    }

    // --- the global frame ---

    /// Window-local to launch-frame transform as
    /// `[m00, m01, m10, m11, tx, ty]` (exact rigid; rotations land on
    /// the tiling's finite orientation set).
    pub fn frame(&self) -> Vec<f64> {
        self.slider.frame().coeffs().to_vec()
    }
    /// Object position in the launch frame (the current window root).
    pub fn global(&self) -> Vec<f64> {
        self.slider.global().to_vec()
    }

    // --- odometer ---

    pub fn generation(&self) -> f64 {
        self.slider.generation() as f64
    }
    pub fn hops(&self) -> f64 {
        self.slider.hops() as f64
    }
    #[wasm_bindgen(js_name = pathRings)]
    pub fn path_rings(&self) -> f64 {
        self.slider.path_length_rings() as f64
    }
    #[wasm_bindgen(js_name = euclidPath)]
    pub fn euclid_path(&self) -> f64 {
        self.slider.euclid_path_length()
    }
    pub fn displacement(&self) -> f64 {
        self.slider.displacement()
    }
    #[wasm_bindgen(js_name = headingDeg)]
    pub fn heading_deg(&self) -> f64 {
        self.slider.heading_deg()
    }
    #[wasm_bindgen(js_name = windowedHeadingDeg)]
    pub fn windowed_heading_deg(&self) -> f64 {
        self.slider.windowed_heading_deg()
    }
    pub fn population(&self) -> u32 {
        self.slider.population()
    }
    #[wasm_bindgen(js_name = windowRadius)]
    pub fn window_radius(&self) -> u32 {
        self.slider.current_radius()
    }
}
