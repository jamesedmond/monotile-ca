//! Run classification: boring-rule filters and exact period detection.
//!
//! A finite patch under a deterministic rule is an orbit in a finite
//! state space, so periodicity is exact recurrence — no translation
//! canonicalisation exists or is needed. Cycle detection is Brent's
//! algorithm over full state snapshots (a population prefilter makes the
//! memcmp rare), so detected periods are exact, never hash collisions.
//!
//! Boundary handling: cells at maximum BFS distance have truncated
//! neighbourhoods, so the run stops being a faithful simulation of the
//! infinite tiling once activity reaches them. [`classify_run`] reports
//! that as [`Outcome::ReachedBoundary`] — it doubles as the explosion
//! filter, since space-filling rules get there in O(radius) generations.

use alloc::vec::Vec;

use crate::{CaRule, Engine};

/// Parameters for [`classify_run`].
#[derive(Clone, Copy, Debug)]
pub struct ClassifyParams {
    /// Give up and report [`Outcome::Active`] after this many generations.
    pub max_generations: u64,
    /// A changed cell at graph distance >= this is boundary contact
    /// (pass the patch radius: that ring has truncated neighbourhoods).
    pub boundary_distance: u32,
    /// If set, abort as [`Outcome::Unbounded`] once population exceeds
    /// this — the explosion filter. A bounded object (glider, oscillator)
    /// never trips it; growing structures (blobs, filaments) do, which
    /// also short-circuits their otherwise-long runs. `None` disables it.
    pub population_cap: Option<u32>,
}

/// How a run ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Outcome {
    /// Population hit zero under a rule that cannot ignite the vacuum
    /// (no B0): permanently dead.
    Died { generation: u64 },
    /// Exact cycle: `period == 1` is a still life. `detected_by` is when
    /// Brent's confirmed it, an upper bound on transient + period.
    Periodic { period: u64, detected_by: u64 },
    /// Activity reached the patch boundary; the run is not a faithful
    /// simulation of the infinite tiling beyond this generation.
    ReachedBoundary { generation: u64 },
    /// Population exceeded `population_cap`: a growing/exploding structure.
    Unbounded { generation: u64 },
    /// Still aperiodic at `max_generations`.
    Active,
}

/// Everything observed about a run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RunReport {
    pub outcome: Outcome,
    /// Maximum graph distance (per the supplied table) of any cell that
    /// changed during the run — the activity bounding radius.
    pub max_changed_distance: u32,
    /// Largest population seen at any generation, including the initial
    /// state. Small values alongside [`Outcome::ReachedBoundary`] are the
    /// glider-candidate signature: bounded activity that travelled.
    pub max_population: u32,
    /// Minimum graph distance among cells changed in the *final*
    /// generation (`u32::MAX` if nothing changed) — high values mean the
    /// activity had moved wholesale away from the reference cell.
    pub final_min_changed_distance: u32,
    /// Largest population seen *before* activity first reached half the
    /// boundary distance (the "inner" phase). Compared with
    /// `max_population`, this is a growth-slope probe: a flat traveller
    /// (glider) has `max_population_inner ≈ max_population`, while a
    /// grower's population keeps climbing in the outer half, so
    /// `max_population_inner` is much smaller. Used by the balance-point
    /// search to reward flat-population travel.
    pub max_population_inner: u32,
}

/// Run the engine from its current state until the outcome is known.
///
/// `distance` is a per-cell graph distance from a reference cell
/// (typically the patch root, as produced by patch generation). Generic
/// over the rule family ([`CaRule`]): classification semantics are
/// identical for Generations and table rules.
pub fn classify_run<R: CaRule>(
    engine: &mut Engine,
    rule: &R,
    distance: &[u32],
    params: &ClassifyParams,
) -> RunReport {
    assert_eq!(
        distance.len(),
        engine.graph().cells() as usize,
        "distance table length mismatch"
    );
    let vacuum_ignites = rule.vacuum_ignites();
    let mut max_changed_distance = 0;
    let mut max_population = engine.population();
    let mut max_population_inner = max_population;
    let inner_radius = params.boundary_distance / 2;
    let mut final_min_changed_distance = u32::MAX;

    if max_population == 0 && !vacuum_ignites {
        return RunReport {
            outcome: Outcome::Died {
                generation: engine.generation(),
            },
            max_changed_distance: 0,
            max_population: 0,
            final_min_changed_distance: u32::MAX,
            max_population_inner: 0,
        };
    }

    // Brent's cycle detection: the snapshot ("tortoise") sits still while
    // the live state ("hare") advances `power` generations, then jumps to
    // the hare and the window doubles.
    let mut snapshot = engine.state().to_vec();
    let mut snapshot_population = engine.population();
    let mut power: u64 = 1;
    let mut lam: u64 = 0;
    let mut changed: Vec<u32> = Vec::new();

    let mut outcome = Outcome::Active;
    'run: for _ in 0..params.max_generations {
        if lam == power {
            snapshot.copy_from_slice(engine.state());
            snapshot_population = engine.population();
            power *= 2;
            lam = 0;
        }
        let stats = rule.step_with_changes(engine, &mut changed);
        lam += 1;
        let generation = engine.generation();
        max_population = max_population.max(stats.population);

        let mut step_max = 0;
        let mut step_min = u32::MAX;
        for &c in &changed {
            let d = distance[c as usize];
            step_max = step_max.max(d);
            step_min = step_min.min(d);
        }
        max_changed_distance = max_changed_distance.max(step_max);
        final_min_changed_distance = step_min;
        // Inner-phase population: while activity hasn't yet reached half
        // the boundary, this tracks the structure's "settled" size; growth
        // beyond it shows up as max_population pulling ahead.
        if max_changed_distance < inner_radius {
            max_population_inner = max_population_inner.max(stats.population);
        }

        if params.population_cap.is_some_and(|cap| stats.population > cap) {
            outcome = Outcome::Unbounded { generation };
            break 'run;
        }
        if step_max >= params.boundary_distance && !changed.is_empty() {
            outcome = Outcome::ReachedBoundary { generation };
            break 'run;
        }
        if stats.population == 0 && !vacuum_ignites {
            outcome = Outcome::Died { generation };
            break 'run;
        }
        if stats.changed == 0 {
            outcome = Outcome::Periodic {
                period: 1,
                detected_by: generation,
            };
            break 'run;
        }
        if stats.population == snapshot_population
            && engine.state() == &snapshot[..]
        {
            outcome = Outcome::Periodic {
                period: lam,
                detected_by: generation,
            };
            break 'run;
        }
    }
    RunReport {
        outcome,
        max_changed_distance,
        max_population,
        final_min_changed_distance,
        max_population_inner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Graph, Rule, StratifiedRule};

    /// Ring of n cells; distances measured from cell 0 around both ways.
    fn ring_engine(n: u32) -> (Engine, Vec<u32>) {
        let edges: Vec<(u32, u32)> = (0..n).map(|i| (i, (i + 1) % n)).collect();
        let distance = (0..n).map(|i| i.min(n - i)).collect();
        (Engine::new(Graph::from_edges(n, &edges)), distance)
    }

    fn params(boundary: u32) -> ClassifyParams {
        ClassifyParams {
            max_generations: 1000,
            boundary_distance: boundary,
            population_cap: None,
        }
    }

    fn uni(birth: u32, survival: u32) -> StratifiedRule {
        StratifiedRule::uniform(Rule::new(birth, survival))
    }

    #[test]
    fn empty_patch_is_dead_immediately_without_b0() {
        let (mut engine, dist) = ring_engine(8);
        let rule = uni(Rule::mask(&[1, 2]), Rule::mask(&[1, 2]));
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert_eq!(r.outcome, Outcome::Died { generation: 0 });
    }

    #[test]
    fn b0_vacuum_strobe_is_period_2_not_death() {
        // Empty state, B = {0}, S = {}: vacuum -> full -> vacuum -> ...
        let (mut engine, dist) = ring_engine(8);
        let rule = uni(Rule::mask(&[0]), 0);
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert_eq!(
            r.outcome,
            Outcome::Periodic {
                period: 2,
                detected_by: 3
            }
        );
    }

    #[test]
    fn full_ring_under_s2_is_a_still_life() {
        let (mut engine, dist) = ring_engine(6);
        for c in 0..6 {
            engine.set_cell(c, true);
        }
        let rule = uni(0, Rule::mask(&[2]));
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert_eq!(
            r.outcome,
            Outcome::Periodic {
                period: 1,
                detected_by: 1
            }
        );
    }

    #[test]
    fn b1_single_seed_on_ring8_dies_at_4() {
        // Hand-computed: {0} -> {1,7} -> {2,6} -> {1,3,5,7} -> {} .
        let (mut engine, dist) = ring_engine(8);
        engine.set_cell(0, true);
        let rule = uni(Rule::mask(&[1]), 0);
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert_eq!(r.outcome, Outcome::Died { generation: 4 });
        assert_eq!(r.max_changed_distance, 3);
    }

    #[test]
    fn same_trajectory_flags_boundary_when_horizon_is_tight() {
        // Same run as above, but distance 3 now counts as the boundary:
        // generation 3 births cells at distance 3.
        let (mut engine, dist) = ring_engine(8);
        engine.set_cell(0, true);
        let rule = uni(Rule::mask(&[1]), 0);
        let r = classify_run(&mut engine, &rule, &dist, &params(3));
        assert_eq!(r.outcome, Outcome::ReachedBoundary { generation: 3 });
    }

    #[test]
    fn b12_diagonal_blinker_has_period_2_after_transient() {
        // Ring 4, B = {1,2}, S = {}: {0} -> {1,3} -> {0,2} -> {1,3} ...
        let (mut engine, dist) = ring_engine(4);
        engine.set_cell(0, true);
        let rule = uni(Rule::mask(&[1, 2]), 0);
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert_eq!(
            r.outcome,
            Outcome::Periodic {
                period: 2,
                detected_by: 3
            }
        );
    }

    #[test]
    fn stratified_rule_can_break_a_uniform_oscillator() {
        // Under uniform B12/S∅ the ring-4 single seed is the period-2
        // blinker {1,3}↔{0,2} (the test above). Putting the alternating
        // cells in two strata and weakening stratum 1's birth to B1
        // removes the 2-neighbour birth that regenerates the orbit, so it
        // dies instead — the blinker-destabilising lever the chirality
        // search exploits: same reaction, residue no longer stable.
        let edges: Vec<(u32, u32)> = (0..4).map(|i| (i, (i + 1) % 4)).collect();
        let mut engine =
            Engine::with_strata(Graph::from_edges(4, &edges), vec![0, 1, 0, 1]);
        engine.set_cell(0, true);
        let dist: Vec<u32> = (0..4u32).map(|i| i.min(4 - i)).collect();
        let rule = StratifiedRule::new(vec![
            Rule::new(Rule::mask(&[1, 2]), 0), // stratum 0: B12/S∅
            Rule::new(Rule::mask(&[1]), 0),    // stratum 1: B1/S∅
        ]);
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert_eq!(r.outcome, Outcome::Died { generation: 3 });
    }

    #[test]
    fn table_rule_classifies_via_the_same_machinery() {
        // Hand-computed: table rule "birth on >= 1 live neighbour, no
        // survival" on ring 8 from {0}: {0} → {1,7} → {0,2,6} →
        // {1,3,5,7} → {0,2,4,6} → {1,3,5,7} → ... period 2 after the
        // transient, touching distance 4 (cell 4) on the way.
        use crate::{RuleRow, TableRule};
        let (mut engine, dist) = ring_engine(8);
        engine.set_cell(0, true);
        let rule = TableRule::new(
            2,
            vec![RuleRow { own: Some(0), conds: vec![(1, 1)], next: 1 }],
        );
        let r = classify_run(&mut engine, &rule, &dist, &params(100));
        assert!(
            matches!(r.outcome, Outcome::Periodic { period: 2, .. }),
            "got {:?}",
            r.outcome
        );
        assert_eq!(r.max_changed_distance, 4);
    }

    #[test]
    fn load_state_resets_generation() {
        let (mut engine, _) = ring_engine(4);
        engine.step(&uni(0, 0));
        assert_eq!(engine.generation(), 1);
        engine.load_state(&[1, 0, 1, 0]);
        assert_eq!(engine.generation(), 0);
        assert_eq!(engine.population(), 2);
    }
}
