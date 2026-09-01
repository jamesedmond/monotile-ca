//! Result serialisation (design brief §4): search hits are small,
//! shareable, exactly replayable records. Graphs and geometry are never
//! stored — the patch regenerates deterministically from
//! (family, root, radius), and the initial configuration references
//! cells by index into that deterministic numbering.

use serde::{Deserialize, Serialize};

use ca_engine::classify::Outcome;
use ca_engine::{AnyRule, Rule, StratifiedRule, TableRule};

use crate::{Neighbourhood, Patch, Stratification, TilingFamily};

/// One recorded run. Serialised as JSON; sweep outputs are JSONL files
/// (one record per line) under `results/`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ResultRecord {
    pub family: TilingFamily,
    /// Eventually-periodic root address, parseable by [`crate::Tiling::generate`].
    pub root: String,
    pub radius: u32,
    /// The rule for uniform runs; for stratified runs, `tables[0]` (kept
    /// for display and backward compatibility). See [`Self::rule`].
    pub rule: Rule,
    /// Stratification scheme for a class-stratified rule. Absent (and
    /// `tables` empty) for uniform records, which stay byte-identical on
    /// disk to pre-stratification outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stratification: Option<Stratification>,
    /// Per-stratum rule tables for a stratified run (empty ⇒ uniform,
    /// use `rule`). Length must equal the scheme's stratum count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<Rule>,
    /// Generalised priority-row rule (FINDINGS §9). When present it is
    /// the run's actual rule and `rule`/`tables` are display
    /// placeholders. Absent for Generations records, which stay
    /// byte-identical on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_rule: Option<TableRule>,
    /// Adjacency relation the run used (defaults to edge — every pre-§9
    /// record). The patch must be regenerated with the same value.
    #[serde(default, skip_serializing_if = "Neighbourhood::is_edge")]
    pub neighbourhood: Neighbourhood,
    /// Indices of live cells at generation 0, in the deterministic
    /// breadth-first cell numbering of the regenerated patch.
    pub initial_cells: Vec<u32>,
    /// Initial state of each `initial_cells` entry (parallel array).
    /// Empty ⇒ all state 1, the pre-§9 convention; multi-state rules
    /// (e.g. Goucher's head+tail seed) need explicit states.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_states: Vec<u8>,
    /// Horizon the run was classified under.
    pub generations: u64,
    pub outcome: Outcome,
    /// Largest population at any generation of the run.
    pub max_population: u32,
    /// Why this run was recorded, e.g. "long-period", "active-at-horizon",
    /// "glider-candidate".
    pub note: String,
}

impl ResultRecord {
    /// Reconstruct the generation-0 state vector for an engine over the
    /// regenerated patch. Fails if any cell index is out of range or if
    /// `initial_states` is present with the wrong length.
    pub fn initial_state(&self, cells: u32) -> Result<Vec<u8>, crate::PatchError> {
        if !self.initial_states.is_empty()
            && self.initial_states.len() != self.initial_cells.len()
        {
            return Err(crate::PatchError(format!(
                "initial_states length {} != initial_cells length {}",
                self.initial_states.len(),
                self.initial_cells.len()
            )));
        }
        let mut state = vec![0u8; cells as usize];
        for (i, &c) in self.initial_cells.iter().enumerate() {
            *state
                .get_mut(c as usize)
                .ok_or_else(|| {
                    crate::PatchError(format!(
                        "initial cell {c} out of range for {cells}-cell patch"
                    ))
                })? = self.initial_states.get(i).copied().unwrap_or(1);
        }
        Ok(state)
    }

    /// The stratification scheme (defaults to [`Stratification::Uniform`]).
    pub fn scheme(&self) -> Stratification {
        self.stratification.unwrap_or(Stratification::Uniform)
    }

    /// The rule as a [`StratifiedRule`]: the recorded `tables` if present,
    /// else the uniform `rule`. Ignores `table_rule` — use
    /// [`Self::ruleset`] for replay.
    pub fn stratified_rule(&self) -> StratifiedRule {
        if self.tables.is_empty() {
            StratifiedRule::uniform(self.rule)
        } else {
            StratifiedRule::new(self.tables.clone())
        }
    }

    /// The run's actual rule, whichever family it belongs to.
    pub fn ruleset(&self) -> AnyRule {
        match &self.table_rule {
            Some(t) => AnyRule::Table(t.clone()),
            None => AnyRule::Generations(self.stratified_rule()),
        }
    }

    /// Build everything needed to replay this record on `patch`: the
    /// per-cell strata (under [`Self::scheme`]) and the run's rule.
    /// Pair the strata with `Engine::with_strata` and step with the
    /// rule via [`ca_engine::CaRule`]. This is the *single* replay
    /// path — UI and verification must both use it (a second path is
    /// how the stratified-replay bug happened). Panics if the patch was
    /// generated under a different neighbourhood than the record's.
    pub fn replay_setup(&self, patch: &Patch) -> (Vec<u8>, AnyRule) {
        assert_eq!(
            patch.neighbourhood, self.neighbourhood,
            "patch neighbourhood does not match the record's"
        );
        (patch.strata(self.scheme()).0, self.ruleset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tiling;
    use ca_engine::{Engine, StratifiedRule};
    use ca_engine::classify::{ClassifyParams, classify_run};

    /// The whole point of result records: a run found in one process
    /// replays to the identical outcome in a fresh one.
    #[test]
    fn record_round_trips_and_replays_identically() {
        let tiling = Tiling::new(TilingFamily::Hat);
        let radius = 5;
        let patch = tiling.generate(&tiling.default_root(), radius).unwrap();
        let distance: Vec<u32> =
            patch.cells.iter().map(|c| c.distance).collect();
        let params = ClassifyParams {
            max_generations: 256,
            boundary_distance: radius,
            population_cap: None,
        };

        // A run worth recording: diagonal-ish blinker rule from a small seed.
        let rule = Rule::new(Rule::mask(&[1, 2]), 0);
        let initial_cells: Vec<u32> = vec![0];
        let mut engine = Engine::new(patch.graph.clone());
        engine.set_cell(0, true);
        let strat = StratifiedRule::uniform(rule);
        let original = classify_run(&mut engine, &strat, &distance, &params);

        let record = ResultRecord {
            family: TilingFamily::Hat,
            root: patch.root.clone(),
            radius,
            rule,
            stratification: None,
            tables: Vec::new(),
            table_rule: None,
            neighbourhood: Neighbourhood::Edge,
            initial_cells,
            initial_states: Vec::new(),
            generations: params.max_generations,
            outcome: original.outcome,
            max_population: original.max_population,
            note: "test".into(),
        };
        let json = serde_json::to_string(&record).unwrap();
        // Uniform edge-adjacency records stay byte-identical to earlier
        // outputs: every optional field is skipped at its default.
        assert!(!json.contains("stratification") && !json.contains("tables"));
        assert!(
            !json.contains("table_rule")
                && !json.contains("neighbourhood")
                && !json.contains("initial_states")
        );
        let parsed: ResultRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, record);

        // Fresh system, fresh patch, replay from the record alone.
        let tiling2 = Tiling::new(parsed.family);
        let patch2 = tiling2.generate(&parsed.root, parsed.radius).unwrap();
        assert_eq!(patch2, patch);
        let mut engine2 = Engine::new(patch2.graph.clone());
        engine2
            .load_state(&parsed.initial_state(patch2.graph.cells()).unwrap());
        let distance2: Vec<u32> =
            patch2.cells.iter().map(|c| c.distance).collect();
        let replayed = classify_run(
            &mut engine2,
            &StratifiedRule::uniform(parsed.rule),
            &distance2,
            &params,
        );
        assert_eq!(replayed, original);
        assert_eq!(replayed.outcome, parsed.outcome);
    }

    #[test]
    fn stratified_record_round_trips_and_replays() {
        // A class-stratified record must regenerate its strata from the
        // recorded scheme and replay to the identical outcome.
        let tiling = Tiling::new(TilingFamily::Spectre);
        let radius = 6;
        let patch = tiling.generate(&tiling.default_root(), radius).unwrap();
        let (strata, n) = patch.strata(Stratification::PerClass);
        let distance: Vec<u32> =
            patch.cells.iter().map(|c| c.distance).collect();
        let params = ClassifyParams { max_generations: 128, boundary_distance: radius, population_cap: None };

        // Distinct table per class (alternating B2/S256 and B25/S25).
        let tables: Vec<Rule> = (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    Rule::new(Rule::mask(&[2]), Rule::mask(&[2, 5, 6]))
                } else {
                    Rule::new(Rule::mask(&[2, 5]), Rule::mask(&[2, 5]))
                }
            })
            .collect();
        let sr = StratifiedRule::new(tables.clone());
        let mut engine = Engine::with_strata(patch.graph.clone(), strata.clone());
        for c in [0u32, 1, 2, 3] {
            engine.set_cell(c, true);
        }
        let original = classify_run(&mut engine, &sr, &distance, &params);

        let record = ResultRecord {
            family: TilingFamily::Spectre,
            root: patch.root.clone(),
            radius,
            rule: tables[0],
            stratification: Some(Stratification::PerClass),
            tables,
            table_rule: None,
            neighbourhood: Neighbourhood::Edge,
            initial_cells: vec![0, 1, 2, 3],
            initial_states: Vec::new(),
            generations: params.max_generations,
            outcome: original.outcome,
            max_population: original.max_population,
            note: "evolve".into(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: ResultRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, record);

        // Replay from the record alone via replay_setup.
        let patch2 =
            Tiling::new(parsed.family).generate(&parsed.root, parsed.radius).unwrap();
        let (strata2, rule2) = parsed.replay_setup(&patch2);
        assert_eq!(strata2, strata);
        let mut engine2 = Engine::with_strata(patch2.graph.clone(), strata2);
        engine2.load_state(&parsed.initial_state(patch2.graph.cells()).unwrap());
        let distance2: Vec<u32> =
            patch2.cells.iter().map(|c| c.distance).collect();
        let replayed = classify_run(&mut engine2, &rule2, &distance2, &params);
        assert_eq!(replayed, original);
    }

    #[test]
    fn out_of_range_initial_cells_are_rejected() {
        let record = ResultRecord {
            family: TilingFamily::Hat,
            root: "x".into(),
            radius: 1,
            rule: Rule::new(0, 0),
            stratification: None,
            tables: Vec::new(),
            table_rule: None,
            neighbourhood: Neighbourhood::Edge,
            initial_cells: vec![10],
            initial_states: Vec::new(),
            generations: 1,
            outcome: Outcome::Active,
            max_population: 1,
            note: String::new(),
        };
        assert!(record.initial_state(5).is_err());
        // A present-but-mismatched states array is rejected too.
        let bad = ResultRecord {
            initial_cells: vec![0, 1],
            initial_states: vec![1],
            ..record
        };
        assert!(bad.initial_state(5).is_err());
    }

    #[test]
    fn legacy_record_json_still_parses() {
        // A hand-written pre-§9 line: none of the newer optional fields.
        let json = r#"{"family":"hat","root":"r","radius":3,
            "rule":{"birth":4,"survival":4},"initial_cells":[0],
            "generations":16,"outcome":{"Died":{"generation":2}},
            "max_population":1,"note":""}"#;
        let r: ResultRecord = serde_json::from_str(json).unwrap();
        assert_eq!(r.neighbourhood, Neighbourhood::Edge);
        assert!(r.table_rule.is_none() && r.initial_states.is_empty());
        assert_eq!(r.initial_state(2).unwrap(), vec![1, 0]);
    }

    #[test]
    fn table_rule_vertex_record_round_trips_and_replays() {
        // The §9 control shape: Goucher's rule, vertex neighbourhood,
        // multi-state (head + tail) seed on a P3 patch.
        use ca_engine::TableRule;
        let tiling = Tiling::new(TilingFamily::PenroseP3);
        let radius = 5;
        let patch = tiling
            .generate_patch(
                &tiling.default_root(),
                radius,
                Neighbourhood::Vertex,
            )
            .unwrap();
        let distance: Vec<u32> =
            patch.cells.iter().map(|c| c.distance).collect();
        let params = ClassifyParams {
            max_generations: 128,
            boundary_distance: radius.saturating_sub(3),
            population_cap: None,
        };
        let rule = TableRule::goucher_glider();
        let head = 0u32;
        let tail = patch.graph.neighbours(0)[0];
        let mut engine = Engine::new(patch.graph.clone());
        engine.set_state(head, 1);
        engine.set_state(tail, 2);
        let original =
            classify_run(&mut engine, &rule, &distance, &params);

        let record = ResultRecord {
            family: TilingFamily::PenroseP3,
            root: patch.root.clone(),
            radius,
            rule: Rule::generations(0, 0, 4), // display placeholder
            stratification: None,
            tables: Vec::new(),
            table_rule: Some(rule),
            neighbourhood: Neighbourhood::Vertex,
            initial_cells: vec![head, tail],
            initial_states: vec![1, 2],
            generations: params.max_generations,
            outcome: original.outcome,
            max_population: original.max_population,
            note: "goucher control".into(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: ResultRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, record);

        // Fresh system: regenerate under the recorded neighbourhood and
        // replay through the single replay path.
        let tiling2 = Tiling::new(parsed.family);
        let patch2 = tiling2
            .generate_patch(&parsed.root, parsed.radius, parsed.neighbourhood)
            .unwrap();
        assert_eq!(patch2, patch);
        let (strata2, rule2) = parsed.replay_setup(&patch2);
        let mut engine2 = Engine::with_strata(patch2.graph.clone(), strata2);
        engine2
            .load_state(&parsed.initial_state(patch2.graph.cells()).unwrap());
        let distance2: Vec<u32> =
            patch2.cells.iter().map(|c| c.distance).collect();
        let replayed =
            classify_run(&mut engine2, &rule2, &distance2, &params);
        assert_eq!(replayed, original);
    }
}
