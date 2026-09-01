//! Pure graph cellular automaton engine.
//!
//! Consumes a CSR adjacency graph and a semi-totalistic rule; knows nothing
//! about geometry, tilings, or I/O. `no_std` + `alloc` so the exact same
//! code compiles natively for the headless search and to wasm32 for the
//! browser UI.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod classify;

use alloc::vec;
use alloc::vec::Vec;

/// Immutable adjacency graph in compressed sparse row layout: the
/// neighbours of cell `c` are `flat[offsets[c]..offsets[c + 1]]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Graph {
    offsets: Vec<u32>,
    flat: Vec<u32>,
}

impl Graph {
    /// Build from an undirected edge list: each `(a, b)` makes `b` a
    /// neighbour of `a` and vice versa.
    pub fn from_edges(cells: u32, edges: &[(u32, u32)]) -> Self {
        let n = cells as usize;
        let mut degree = vec![0u32; n];
        for &(a, b) in edges {
            assert!(a < cells && b < cells, "edge endpoint out of range");
            degree[a as usize] += 1;
            degree[b as usize] += 1;
        }
        let mut offsets = vec![0u32; n + 1];
        for i in 0..n {
            offsets[i + 1] = offsets[i] + degree[i];
        }
        let mut flat = vec![0u32; offsets[n] as usize];
        let mut cursor: Vec<u32> = offsets[..n].to_vec();
        for &(a, b) in edges {
            flat[cursor[a as usize] as usize] = b;
            cursor[a as usize] += 1;
            flat[cursor[b as usize] as usize] = a;
            cursor[b as usize] += 1;
        }
        Self { offsets, flat }
    }

    pub fn cells(&self) -> u32 {
        (self.offsets.len() - 1) as u32
    }

    pub fn neighbours(&self, cell: u32) -> &[u32] {
        &self.flat[self.offsets[cell as usize] as usize..self.offsets[cell as usize + 1] as usize]
    }

    pub fn degree(&self, cell: u32) -> u32 {
        self.offsets[cell as usize + 1] - self.offsets[cell as usize]
    }
}

/// Two-state semi-totalistic rule: bit `k` of `birth` / `survival` says
/// whether a dead / live cell with exactly `k` live neighbours is live in
/// the next generation. Neighbour counts on hat/spectre tilings are small,
/// so 32 bits is ample headroom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rule {
    pub birth: u32,
    pub survival: u32,
    /// Number of states `k` (Generations family). `k = 2` is plain
    /// 2-state Life. For `k > 2`, states `2..k-1` are deterministic
    /// "dying" phases that age toward death (`s → s+1`, then `→ 0`); a
    /// live cell (state 1) that fails to survive enters state 2 instead
    /// of dying outright. Only state-1 ("alive") neighbours are counted
    /// for birth/survival. The Penrose-tiling gliders live in this
    /// family (k = 4–5); Brian's Brain is `B2/S/3`.
    #[cfg_attr(
        feature = "serde",
        serde(default = "default_states", skip_serializing_if = "is_two")
    )]
    pub states: u8,
}

#[cfg(feature = "serde")]
fn default_states() -> u8 {
    2
}
#[cfg(feature = "serde")]
fn is_two(s: &u8) -> bool {
    *s == 2
}

impl Rule {
    /// A 2-state (Life-like) rule.
    pub const fn new(birth: u32, survival: u32) -> Self {
        Self {
            birth,
            survival,
            states: 2,
        }
    }

    /// A `k`-state Generations rule (`k >= 2`).
    pub const fn generations(birth: u32, survival: u32, states: u8) -> Self {
        assert!(states >= 2, "a rule needs at least 2 states");
        Self {
            birth,
            survival,
            states,
        }
    }

    /// `mask(&[2, 3])` → bits 2 and 3 set.
    pub fn mask(counts: &[u32]) -> u32 {
        counts.iter().fold(0, |m, &c| m | 1 << c)
    }

    /// Next state of a cell currently in state `own` (0 = dead, 1 =
    /// alive, `2..k-1` = dying) given how many of its neighbours are
    /// alive (state 1). For `k = 2` this is exactly 2-state Life.
    #[inline]
    pub fn next_state(self, own: u8, live_neighbours: u32) -> u8 {
        match own {
            0 => u8::from(self.birth >> live_neighbours & 1 != 0),
            1 => {
                if self.survival >> live_neighbours & 1 != 0 {
                    1
                } else if self.states > 2 {
                    2 // begin dying
                } else {
                    0 // k = 2: die immediately
                }
            }
            // Dying phases age deterministically toward death.
            s => {
                let next = s + 1;
                if u16::from(next) < u16::from(self.states) {
                    next
                } else {
                    0
                }
            }
        }
    }
}

/// A semi-totalistic rule stratified by tile class: `tables[s]` is the
/// [`Rule`] applied to cells assigned to stratum `s`. The uniform case
/// is a single table; chirality stratification (hat/antihat, or the
/// spectre Mystic role) uses two — "free expressive power" from a
/// geometrically-determined colouring. Per-cell strata live in the
/// [`Engine`]; a cell's stratum byte indexes `tables`.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StratifiedRule {
    pub tables: Vec<Rule>,
}

impl StratifiedRule {
    /// One table for every cell — semantically identical to the bare
    /// [`Rule`], and the engine's default when no strata are set.
    pub fn uniform(rule: Rule) -> Self {
        Self { tables: vec![rule] }
    }

    pub fn new(tables: Vec<Rule>) -> Self {
        assert!(!tables.is_empty(), "a rule needs at least one table");
        Self { tables }
    }

    pub fn strata(&self) -> usize {
        self.tables.len()
    }

    #[inline]
    fn next_state(&self, stratum: u8, own: u8, live_neighbours: u32) -> u8 {
        self.tables[stratum as usize].next_state(own, live_neighbours)
    }

    /// Number of states `k` (from the first table; all tables share it).
    pub fn states(&self) -> u8 {
        self.tables[0].states
    }

    /// True if any stratum births into the empty neighbourhood (B0): a
    /// dead cell with no live neighbours turns live, so the vacuum is not
    /// a fixed point. [`classify::classify_run`](crate::classify) uses
    /// this so a B0 strobe is not mislabelled as death.
    pub fn vacuum_ignites(&self) -> bool {
        self.tables.iter().any(|r| r.birth & 1 != 0)
    }
}

impl From<Rule> for StratifiedRule {
    fn from(rule: Rule) -> Self {
        Self::uniform(rule)
    }
}

/// Highest state value the [`TableRule`] step counts per neighbour.
/// States at or above this are legal engine states but invisible to
/// table-rule conditions (no realistic rule needs 16+ states).
pub const MAX_COUNTED_STATES: usize = 16;

/// One row of a [`TableRule`]: fires when the cell's own state matches
/// `own` (`None` = any) and every condition holds; conditions are a
/// conjunction of per-state neighbour-count thresholds
/// `count(state) >= min`.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RuleRow {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub own: Option<u8>,
    /// `(state, min)` pairs, all of which must hold: the cell has at
    /// least `min` neighbours in exactly `state`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub conds: Vec<(u8, u8)>,
    pub next: u8,
}

/// A multi-state outer-totalistic rule given as a priority-ordered row
/// table: the first matching row decides the next state; if no row
/// matches, the cell becomes 0 (quiescent).
///
/// Strictly more expressive than the Generations [`Rule`] family on two
/// axes: conditions may count neighbours in *any* state (Generations
/// counts only state 1, so its dying phases are neighbour-invisible),
/// and a row may require a *conjunction* of counts in different states.
/// Any semi-totalistic [`Rule`] is encodable as descending-threshold
/// rows, so this family strictly contains it. The published Penrose
/// glider (Goucher 2012, FINDINGS §9) lives here and provably outside
/// the Generations family.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TableRule {
    pub states: u8,
    pub rows: Vec<RuleRow>,
}

impl TableRule {
    pub fn new(states: u8, rows: Vec<RuleRow>) -> Self {
        assert!(states >= 2, "a rule needs at least 2 states");
        assert!(
            (states as usize) <= MAX_COUNTED_STATES,
            "at most {MAX_COUNTED_STATES} states"
        );
        for row in &rows {
            assert!(row.next < states, "row next-state out of range");
            assert!(
                row.own.is_none_or(|o| o < states),
                "row own-state out of range"
            );
            assert!(
                row.conds.iter().all(|&(s, _)| s < states),
                "row condition state out of range"
            );
        }
        Self { states, rows }
    }

    /// The 4-state rule of Goucher 2012, *Gliders in cellular automata
    /// on Penrose tilings*, Table 1 — the first glider on an aperiodic
    /// tiling. States: 0 ground, 1 head, 2 tail, 3 wing. Defined for
    /// the vertex ("generalised Moore") neighbourhood; on the P3
    /// rhomb tiling a head+tail pair glides along a de Bruijn ribbon,
    /// on P2 it orbits loops (exact periods 20/40/200 reproduced;
    /// FINDINGS §9.5).
    ///
    /// NOTE: the paper's printed Table 1 has a typo — row 2's next
    /// state reads 3 (wing), which would make state 1 unreachable and
    /// the glider impossible. The correct value is 1 (a *new head* is
    /// born ahead), per the paper's own prose, the Ready reference
    /// implementation (`Goucher_loops.vtu` kernel: `n1>0 && n3>=2 →
    /// 1`), and reproduction on our substrate (as printed: dies at
    /// generation 4; corrected: glides indefinitely).
    pub fn goucher_glider() -> Self {
        let row = |own: u8, conds: &[(u8, u8)], next: u8| RuleRow {
            own: Some(own),
            conds: conds.to_vec(),
            next,
        };
        Self::new(
            4,
            vec![
                row(0, &[(1, 1), (2, 1)], 3), // ground by head + tail → wing
                row(0, &[(1, 1), (3, 2)], 1), // ground by head + 2 wings → NEW head
                row(1, &[(3, 1)], 2),         // head touched by a wing → tail
                row(1, &[], 1),               // head persists
                row(2, &[], 3),               // tail → wing
                                              // (no match, incl. wing) → 0
            ],
        )
    }

    /// Next state given the cell's own state and per-state neighbour
    /// counts (`counts[s]` = neighbours in exactly state `s`).
    #[inline]
    pub fn next_state(&self, own: u8, counts: &[u8; MAX_COUNTED_STATES]) -> u8 {
        for row in &self.rows {
            if row.own.is_some_and(|o| o != own) {
                continue;
            }
            if row.conds.iter().all(|&(s, m)| counts[s as usize] >= m) {
                return row.next;
            }
        }
        0
    }

    /// Could a dead cell in an all-dead neighbourhood change state?
    /// Conservative: a vacuum-eligible row whose conditions only
    /// reference state 0 is assumed satisfiable (its threshold may
    /// exceed some cells' degrees), so `true` may be reported for rules
    /// that never actually ignite — erring away from mislabelling a
    /// B0-style strobe as death.
    pub fn vacuum_ignites(&self) -> bool {
        for row in &self.rows {
            if row.own.is_some_and(|o| o != 0) {
                continue;
            }
            // Rows conditioned on any non-zero state cannot fire in a
            // vacuum; the first row that could fire decides.
            if row.conds.iter().all(|&(s, _)| s == 0) {
                return row.next != 0;
            }
        }
        false
    }
}

/// A rule of either family, for code paths (replay, UI) that must step
/// whatever a result record contains. Use the [`CaRule`] impl to step.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AnyRule {
    Generations(StratifiedRule),
    Table(TableRule),
}

impl From<StratifiedRule> for AnyRule {
    fn from(rule: StratifiedRule) -> Self {
        Self::Generations(rule)
    }
}

impl From<TableRule> for AnyRule {
    fn from(rule: TableRule) -> Self {
        Self::Table(rule)
    }
}

/// A rule the engine can step: implemented by [`StratifiedRule`] (the
/// Generations family, the fast path all pre-§9 results used),
/// [`TableRule`], and [`AnyRule`]. [`classify::classify_run`] is generic
/// over this, so classification semantics are identical across families.
pub trait CaRule {
    /// Number of states `k` (0 = dead, 1 = alive; renderers use this).
    fn states(&self) -> u8;
    /// See [`StratifiedRule::vacuum_ignites`] / [`TableRule::vacuum_ignites`].
    fn vacuum_ignites(&self) -> bool;
    fn step_with_changes(&self, engine: &mut Engine, changed: &mut Vec<u32>) -> StepStats;
    fn step(&self, engine: &mut Engine) -> StepStats;
}

impl CaRule for StratifiedRule {
    fn states(&self) -> u8 {
        StratifiedRule::states(self)
    }
    fn vacuum_ignites(&self) -> bool {
        StratifiedRule::vacuum_ignites(self)
    }
    fn step_with_changes(&self, engine: &mut Engine, changed: &mut Vec<u32>) -> StepStats {
        engine.step_with_changes(self, changed)
    }
    fn step(&self, engine: &mut Engine) -> StepStats {
        engine.step(self)
    }
}

impl CaRule for TableRule {
    fn states(&self) -> u8 {
        self.states
    }
    fn vacuum_ignites(&self) -> bool {
        TableRule::vacuum_ignites(self)
    }
    fn step_with_changes(&self, engine: &mut Engine, changed: &mut Vec<u32>) -> StepStats {
        changed.clear();
        engine.step_table_impl(self, Some(changed))
    }
    fn step(&self, engine: &mut Engine) -> StepStats {
        engine.step_table_impl(self, None)
    }
}

impl CaRule for AnyRule {
    fn states(&self) -> u8 {
        match self {
            AnyRule::Generations(r) => CaRule::states(r),
            AnyRule::Table(r) => CaRule::states(r),
        }
    }
    fn vacuum_ignites(&self) -> bool {
        match self {
            AnyRule::Generations(r) => CaRule::vacuum_ignites(r),
            AnyRule::Table(r) => CaRule::vacuum_ignites(r),
        }
    }
    fn step_with_changes(&self, engine: &mut Engine, changed: &mut Vec<u32>) -> StepStats {
        match self {
            AnyRule::Generations(r) => r.step_with_changes(engine, changed),
            AnyRule::Table(r) => r.step_with_changes(engine, changed),
        }
    }
    fn step(&self, engine: &mut Engine) -> StepStats {
        match self {
            AnyRule::Generations(r) => CaRule::step(r, engine),
            AnyRule::Table(r) => CaRule::step(r, engine),
        }
    }
}

/// Per-generation statistics, computed during the step at no extra cost.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StepStats {
    pub population: u32,
    pub changed: u32,
}

/// Double-buffered CA state over a fixed graph. Cells outside the graph do
/// not exist: finite patches have an implicit dead, fixed boundary.
pub struct Engine {
    graph: Graph,
    /// Per-cell rule-stratum index (all 0 = uniform). A cell's byte
    /// indexes a [`StratifiedRule`]'s table.
    strata: Vec<u8>,
    state: Vec<u8>,
    next: Vec<u8>,
    generation: u64,
}

impl Engine {
    /// Uniform engine: every cell is in stratum 0, so a one-table
    /// [`StratifiedRule`] (or a bare [`Rule`] via `.into()`) applies.
    pub fn new(graph: Graph) -> Self {
        let n = graph.cells() as usize;
        Self::with_strata(graph, vec![0; n])
    }

    /// Engine with per-cell strata (length must equal the cell count).
    /// Strata are typically derived from tile class/chirality.
    pub fn with_strata(graph: Graph, strata: Vec<u8>) -> Self {
        let n = graph.cells() as usize;
        assert_eq!(strata.len(), n, "strata length must equal cell count");
        Self {
            graph,
            strata,
            state: vec![0; n],
            next: vec![0; n],
            generation: 0,
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn strata(&self) -> &[u8] {
        &self.strata
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// One byte per cell: 0 = dead, 1 = alive, `2..k-1` = dying phases
    /// (Generations). The renderer reads this directly as a per-tile
    /// colour attribute.
    pub fn state(&self) -> &[u8] {
        &self.state
    }

    pub fn cell(&self, cell: u32) -> bool {
        self.state[cell as usize] != 0
    }

    pub fn set_cell(&mut self, cell: u32, alive: bool) {
        self.state[cell as usize] = alive as u8;
    }

    /// Set a cell to an explicit state (0..k); for k-state seeding.
    pub fn set_state(&mut self, cell: u32, state: u8) {
        self.state[cell as usize] = state;
    }

    /// Replace the whole state (one byte per cell, 0/1) and restart the
    /// generation counter — for loading initial configurations.
    pub fn load_state(&mut self, state: &[u8]) {
        self.restore(state, 0);
    }

    /// Restore a previously captured state at its generation — for
    /// history/replay tooling (e.g. stepping backward from snapshots).
    pub fn restore(&mut self, state: &[u8], generation: u64) {
        assert_eq!(state.len(), self.state.len(), "state length mismatch");
        self.state.copy_from_slice(state);
        self.generation = generation;
    }

    /// Number of non-quiescent cells (any state != 0, including dying).
    pub fn population(&self) -> u32 {
        self.state.iter().map(|&s| u32::from(s != 0)).sum()
    }

    pub fn clear(&mut self) {
        self.state.fill(0);
        self.generation = 0;
    }

    pub fn step(&mut self, rule: &StratifiedRule) -> StepStats {
        self.step_impl(rule, None)
    }

    /// Like [`step`](Self::step), also collecting the indices of cells
    /// whose state changed (cleared and reused — no per-step allocation).
    pub fn step_with_changes(
        &mut self,
        rule: &StratifiedRule,
        changed: &mut Vec<u32>,
    ) -> StepStats {
        changed.clear();
        self.step_impl(rule, Some(changed))
    }

    fn step_impl(
        &mut self,
        rule: &StratifiedRule,
        mut changed_out: Option<&mut Vec<u32>>,
    ) -> StepStats {
        let mut population = 0;
        let mut changed = 0;
        for cell in 0..self.graph.cells() {
            // Only state-1 ("alive") neighbours count — dying phases do
            // not contribute to birth/survival (the Generations rule).
            let live = self
                .graph
                .neighbours(cell)
                .iter()
                .filter(|&&n| self.state[n as usize] == 1)
                .count() as u32;
            let own = self.state[cell as usize];
            let next = rule.next_state(self.strata[cell as usize], own, live);
            self.next[cell as usize] = next;
            population += u32::from(next != 0);
            if next != own {
                changed += 1;
                if let Some(out) = changed_out.as_deref_mut() {
                    out.push(cell);
                }
            }
        }
        core::mem::swap(&mut self.state, &mut self.next);
        self.generation += 1;
        StepStats {
            population,
            changed,
        }
    }

    /// [`TableRule`] step: same double-buffered sweep as
    /// [`step_impl`](Self::step_impl), but tallying neighbours per state
    /// (the table's conditions may reference any state, unlike
    /// Generations where only state 1 is visible). Kept separate so the
    /// Generations path — which all pre-§9 results were produced on —
    /// stays byte-for-byte unchanged.
    fn step_table_impl(
        &mut self,
        rule: &TableRule,
        mut changed_out: Option<&mut Vec<u32>>,
    ) -> StepStats {
        let mut population = 0;
        let mut changed = 0;
        let mut counts = [0u8; MAX_COUNTED_STATES];
        for cell in 0..self.graph.cells() {
            counts.fill(0);
            for &n in self.graph.neighbours(cell) {
                if let Some(slot) = counts.get_mut(self.state[n as usize] as usize) {
                    *slot = slot.saturating_add(1);
                }
            }
            let own = self.state[cell as usize];
            let next = rule.next_state(own, &counts);
            self.next[cell as usize] = next;
            population += u32::from(next != 0);
            if next != own {
                changed += 1;
                if let Some(out) = changed_out.as_deref_mut() {
                    out.push(cell);
                }
            }
        }
        core::mem::swap(&mut self.state, &mut self.next);
        self.generation += 1;
        StepStats {
            population,
            changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(n: u32) -> Graph {
        let edges: Vec<(u32, u32)> = (0..n).map(|i| (i, (i + 1) % n)).collect();
        Graph::from_edges(n, &edges)
    }

    #[test]
    fn csr_layout_from_edge_list() {
        let g = Graph::from_edges(3, &[(0, 1), (1, 2)]);
        assert_eq!(g.cells(), 3);
        assert_eq!(g.neighbours(0), &[1]);
        assert_eq!(g.neighbours(1), &[0, 2]);
        assert_eq!(g.neighbours(2), &[1]);
        assert_eq!(g.degree(1), 2);
    }

    fn uniform(birth: u32, survival: u32) -> StratifiedRule {
        StratifiedRule::uniform(Rule::new(birth, survival))
    }

    #[test]
    fn single_seed_spreads_under_b1() {
        // B1/S∅: a dead cell with exactly one live neighbour is born and
        // nothing survives, so one seed becomes its two ring neighbours.
        let mut engine = Engine::new(ring(8));
        engine.set_cell(4, true);
        let stats = engine.step(&uniform(Rule::mask(&[1]), 0));
        assert_eq!(
            stats,
            StepStats {
                population: 2,
                changed: 3
            }
        );
        assert!(!engine.cell(4));
        assert!(engine.cell(3) && engine.cell(5));
        assert_eq!(engine.generation(), 1);
    }

    #[test]
    fn full_ring_is_a_still_life_under_s2() {
        let mut engine = Engine::new(ring(6));
        for c in 0..6 {
            engine.set_cell(c, true);
        }
        let stats = engine.step(&uniform(0, Rule::mask(&[2])));
        assert_eq!(
            stats,
            StepStats {
                population: 6,
                changed: 0
            }
        );
    }

    #[test]
    fn empty_patch_stays_empty_without_b0() {
        let mut engine = Engine::new(ring(5));
        let stats =
            engine.step(&uniform(Rule::mask(&[1, 2]), Rule::mask(&[1, 2])));
        assert_eq!(stats.population, 0);
        assert_eq!(stats.changed, 0);
    }

    #[test]
    fn generations_rule_ages_dying_states() {
        // 4-state rule, no survival: a live cell that doesn't survive
        // walks 1 → 2 → 3 → 0 deterministically, ignoring neighbours.
        let rule = Rule::generations(0, 0, 4);
        assert_eq!(rule.next_state(1, 0), 2, "alive, no survival → dying");
        assert_eq!(rule.next_state(2, 0), 3, "dying phases age");
        assert_eq!(rule.next_state(2, 5), 3, "...regardless of neighbours");
        assert_eq!(rule.next_state(3, 0), 0, "last dying phase → dead");
        // Dead cell births to state 1 on the right count.
        let b = Rule::generations(Rule::mask(&[2]), 0, 4);
        assert_eq!(b.next_state(0, 2), 1);
        assert_eq!(b.next_state(0, 1), 0);
    }

    #[test]
    fn two_state_generations_equals_life() {
        // k = 2: a non-surviving live cell dies immediately (no phase 2).
        let life = Rule::new(Rule::mask(&[2]), Rule::mask(&[2, 3]));
        let gen2 = Rule::generations(Rule::mask(&[2]), Rule::mask(&[2, 3]), 2);
        assert_eq!(life, gen2);
        assert_eq!(life.next_state(1, 1), 0, "no survival at 1 → dead, not dying");
    }

    #[test]
    fn dying_neighbours_do_not_feed_births() {
        // Two cells joined; one alive (1), one dead (0). Under B1 the dead
        // cell is born. But make the alive cell *dying* (state 2) instead:
        // it no longer counts as a live neighbour, so no birth.
        let rule = StratifiedRule::uniform(Rule::generations(Rule::mask(&[1]), 0, 4));
        let graph = Graph::from_edges(2, &[(0, 1)]);
        let mut a = Engine::new(graph.clone());
        a.set_state(0, 1); // alive
        a.step(&rule);
        assert!(a.cell(1), "live neighbour births cell 1");

        let mut b = Engine::new(graph);
        b.set_state(0, 2); // dying, not alive
        b.step(&rule);
        assert!(!b.cell(1), "dying neighbour must not trigger a birth");
        assert_eq!(b.state()[0], 3, "the dying cell ages 2 → 3");
    }

    #[test]
    fn uniform_stratified_rule_matches_bare_rule() {
        // A single-table StratifiedRule must reproduce the bare Rule it
        // wraps, step for step — the "one engine" invariant under which
        // all prior uniform results stay valid.
        let edges: Vec<(u32, u32)> =
            (0..40).flat_map(|i| [(i, (i + 1) % 40), (i, (i + 7) % 40)]).collect();
        let graph = Graph::from_edges(40, &edges);
        let rule = Rule::new(Rule::mask(&[2, 3]), Rule::mask(&[1, 2]));
        let strat = StratifiedRule::uniform(rule);
        let mut a = Engine::new(graph.clone());
        let mut b = Engine::new(graph);
        for c in [3u32, 4, 5, 11, 19, 20] {
            a.set_cell(c, true);
            b.set_cell(c, true);
        }
        for _ in 0..50 {
            // `a` via the bare-rule path, `b` via a 1-table stratified rule
            a.step(&StratifiedRule::uniform(rule));
            b.step(&strat);
            assert_eq!(a.state(), b.state());
        }
    }

    /// Encode a Generations [`Rule`] as an exact-count [`TableRule`]:
    /// for each own-state, descending-threshold rows on the state-1
    /// count select the exact-count bucket (the first row whose `>= c`
    /// holds has `c == count`). Total, so the encoding is exact.
    fn encode_as_table(rule: Rule, max_degree: u8) -> TableRule {
        let mut rows = Vec::new();
        for c in (0..=max_degree).rev() {
            rows.push(RuleRow {
                own: Some(0),
                conds: vec![(1, c)],
                next: u8::from(rule.birth >> c & 1 != 0),
            });
            rows.push(RuleRow {
                own: Some(1),
                conds: vec![(1, c)],
                next: if rule.survival >> c & 1 != 0 {
                    1
                } else if rule.states > 2 {
                    2
                } else {
                    0
                },
            });
        }
        for s in 2..rule.states {
            rows.push(RuleRow {
                own: Some(s),
                conds: Vec::new(),
                next: if s + 1 < rule.states { s + 1 } else { 0 },
            });
        }
        TableRule::new(rule.states, rows)
    }

    #[test]
    fn table_rule_reproduces_generations_exactly() {
        // The strongest cross-check of the table step path: an encoded
        // Generations rule must evolve identically to the native path,
        // generation by generation, from a mixed-state soup.
        let edges: Vec<(u32, u32)> =
            (0..40).flat_map(|i| [(i, (i + 1) % 40), (i, (i + 7) % 40)]).collect();
        let graph = Graph::from_edges(40, &edges);
        let rule = Rule::generations(Rule::mask(&[2, 3]), Rule::mask(&[1, 2]), 4);
        let table = encode_as_table(rule, 4);
        let mut a = Engine::new(graph.clone());
        let mut b = Engine::new(graph);
        for (c, s) in [(3u32, 1u8), (4, 1), (5, 2), (11, 1), (19, 3), (20, 1)] {
            a.set_state(c, s);
            b.set_state(c, s);
        }
        let strat = StratifiedRule::uniform(rule);
        for generation in 0..60 {
            let sa = CaRule::step(&strat, &mut a);
            let sb = CaRule::step(&table, &mut b);
            assert_eq!(sa, sb, "stats diverge at generation {generation}");
            assert_eq!(a.state(), b.state(), "states diverge at {generation}");
        }
    }

    #[test]
    fn table_rule_first_matching_row_wins() {
        let rule = TableRule::goucher_glider();
        let mut counts = [0u8; MAX_COUNTED_STATES];
        // Ground with no signals stays ground; head persists.
        assert_eq!(rule.next_state(0, &counts), 0);
        assert_eq!(rule.next_state(1, &counts), 1);
        // Tail always becomes wing; wing always dies.
        assert_eq!(rule.next_state(2, &counts), 3);
        assert_eq!(rule.next_state(3, &counts), 0);
        // Ground with head+tail neighbours births a wing.
        counts[1] = 1;
        counts[2] = 1;
        assert_eq!(rule.next_state(0, &counts), 3);
        // Head + one wing is not enough for ground; head + two wings
        // births a NEW HEAD (the corrected row 2 — the paper's printed
        // "3" here is the typo that makes the glider impossible).
        counts[2] = 0;
        counts[3] = 1;
        assert_eq!(rule.next_state(0, &counts), 0);
        counts[3] = 2;
        assert_eq!(rule.next_state(0, &counts), 1);
        // A head touched by a wing retreats to tail.
        assert_eq!(rule.next_state(1, &counts), 2);
        // Neither ground rule fires without a head neighbour.
        counts[1] = 0;
        assert_eq!(rule.next_state(0, &counts), 0);
    }

    #[test]
    fn goucher_head_tail_advances_on_a_path() {
        // Path graph 0-1-2-3-4 (each cell sees its line neighbours; edge
        // adjacency is enough to exercise the mechanics). Head at 2, tail
        // at 1: cell 3 (head+... no tail) — on a path only cell 1's other
        // neighbour 0 sees head? Work it through: neighbours-of-both-head-
        // and-tail is empty on a path, so wings never form and the head
        // marches nowhere — but tail decay must still run 2→3→0 while the
        // head persists. This pins the state machinery.
        let graph = Graph::from_edges(5, &[(0, 1), (1, 2), (2, 3), (3, 4)]);
        let rule = TableRule::goucher_glider();
        let mut e = Engine::new(graph);
        e.set_state(1, 2); // tail
        e.set_state(2, 1); // head
        CaRule::step(&rule, &mut e);
        assert_eq!(e.state(), &[0, 3, 1, 0, 0], "tail→wing, head persists");
        CaRule::step(&rule, &mut e);
        assert_eq!(e.state(), &[0, 0, 2, 0, 0], "wing dies and demotes head");
        CaRule::step(&rule, &mut e);
        assert_eq!(e.state(), &[0, 0, 3, 0, 0], "lone tail→wing");
        CaRule::step(&rule, &mut e);
        assert_eq!(e.state(), &[0, 0, 0, 0, 0], "wing dies out");
    }

    #[test]
    fn table_rule_vacuum_ignition_detection() {
        // Goucher's rule cannot ignite the vacuum (its ground rows need
        // head/tail/wing neighbours).
        assert!(!TableRule::goucher_glider().vacuum_ignites());
        // A rule birthing on "no live neighbours" (B0-style) does.
        let b0 = TableRule::new(
            2,
            vec![RuleRow { own: Some(0), conds: Vec::new(), next: 1 }],
        );
        assert!(b0.vacuum_ignites());
        // A ground row conditioned on state-0 neighbours counts too
        // (conservatively satisfiable in a vacuum).
        let deg = TableRule::new(
            2,
            vec![RuleRow { own: Some(0), conds: vec![(0, 3)], next: 1 }],
        );
        assert!(deg.vacuum_ignites());
    }

    #[test]
    fn any_rule_dispatches_to_both_families() {
        let graph = Graph::from_edges(2, &[(0, 1)]);
        let strat = StratifiedRule::uniform(Rule::new(Rule::mask(&[1]), 0));
        let any: AnyRule = strat.clone().into();
        let mut a = Engine::new(graph.clone());
        let mut b = Engine::new(graph.clone());
        a.set_cell(0, true);
        b.set_cell(0, true);
        CaRule::step(&strat, &mut a);
        CaRule::step(&any, &mut b);
        assert_eq!(a.state(), b.state());
        assert_eq!(CaRule::states(&any), 2);
        let table: AnyRule = TableRule::goucher_glider().into();
        assert_eq!(CaRule::states(&table), 4);
        assert!(!table.vacuum_ignites());
    }

    #[test]
    fn strata_select_different_tables() {
        // Two cells, two strata: stratum 0 survives a lone neighbour,
        // stratum 1 does not. Both start live with one live neighbour.
        let graph = Graph::from_edges(2, &[(0, 1)]);
        let rule = StratifiedRule::new(vec![
            Rule::new(0, Rule::mask(&[1])), // stratum 0: S1
            Rule::new(0, 0),                // stratum 1: S∅
        ]);
        let mut engine = Engine::with_strata(graph, vec![0, 1]);
        engine.set_cell(0, true);
        engine.set_cell(1, true);
        let stats = engine.step(&rule);
        assert!(engine.cell(0), "stratum-0 cell should survive");
        assert!(!engine.cell(1), "stratum-1 cell should die");
        assert_eq!(stats, StepStats { population: 1, changed: 1 });
    }
}
