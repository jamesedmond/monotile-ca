//! Re-run a recorded search hit on a larger patch and trace its
//! trajectory: population and activity-distance band per generation
//! window. A genuine glider-like object keeps a bounded population while
//! its activity band marches outward; debris-leaving growth or radial
//! flukes don't.
//!
//! Usage: verify_candidate <record.json line file> [record index] [radius] [horizon]

use std::collections::BTreeSet;

use ca_engine::{CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("path to record file (json/jsonl)");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(48, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(4096, |a| a.parse().expect("horizon"));

    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text.lines().filter(|l| !l.trim().is_empty()).nth(index).expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    println!(
        "verifying {:?} rule B{:x}/S{:x} from root {} at radius {radius}, horizon {horizon}",
        record.family, record.rule.birth, record.rule.survival, record.root
    );

    let tiling = Tiling::new(record.family);
    let patch = tiling
        .generate_patch(&record.root, radius, record.neighbourhood)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    // replay_setup honours a class-stratified record (per-cell strata +
    // per-class rule); for a uniform record it gives all-zero strata + a
    // one-table rule, so this covers both.
    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());

    let mut changed: Vec<u32> = Vec::new();
    let mut max_pop = engine.population();
    let mut window_min = u32::MAX;
    let mut window_max = 0u32;
    let mut history: BTreeSet<Vec<u8>> = BTreeSet::new();
    println!("  gen | pop | activity distance band (window of 32)");
    for generation in 1..=horizon {
        let stats = rule.step_with_changes(&mut engine, &mut changed);
        max_pop = max_pop.max(stats.population);
        for &c in &changed {
            window_min = window_min.min(distance[c as usize]);
            window_max = window_max.max(distance[c as usize]);
        }
        if generation % 32 == 0 {
            println!(
                "  {generation:4} | {:3} | {}..{}",
                stats.population,
                if window_min == u32::MAX { 0 } else { window_min },
                window_max
            );
            window_min = u32::MAX;
            window_max = 0;
        }
        if stats.population == 0 {
            println!("  died at generation {generation}");
            return;
        }
        if stats.changed == 0 {
            println!("  froze at generation {generation}");
            return;
        }
        if changed.iter().any(|&c| distance[c as usize] >= radius) {
            println!(
                "  reached boundary at generation {generation}: population {}, max over run {max_pop}",
                stats.population
            );
            return;
        }
        if !history.insert(engine.state().to_vec()) {
            println!("  state recurrence at generation {generation} (periodic)");
            return;
        }
    }
    println!("  still active at horizon; max population {max_pop}");
}
