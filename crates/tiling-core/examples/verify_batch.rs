//! Bulk stage-2 verification: re-run every glider-candidate record from
//! a JSONL results file on a larger patch and tabulate the true
//! outcomes. Survivors = bounded runs whose travel beats the screening
//! radius by a clear margin.
//!
//! Usage: verify_batch <records.jsonl> [radius] [horizon]

use std::collections::{HashMap, HashSet};

use ca_engine::Engine;
use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use rayon::prelude::*;
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Patch, Tiling};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("records.jsonl");
    let radius: u32 = args.next().map_or(48, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(8192, |a| a.parse().expect("horizon"));

    let text = std::fs::read_to_string(&path).expect("read records");
    let records: Vec<ResultRecord> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse record"))
        .filter(|r: &ResultRecord| r.note == "glider-candidate")
        .collect();
    eprintln!("{} candidate records", records.len());
    let family = records.first().expect("at least one record").family;
    let tiling = Tiling::new(family);

    let keys: HashSet<(String, Neighbourhood)> = records
        .iter()
        .map(|r| (r.root.clone(), r.neighbourhood))
        .collect();
    let patches: HashMap<(String, Neighbourhood), (Patch, Vec<u32>)> = keys
        .par_iter()
        .map(|(root, nb)| {
            let p = tiling.generate_patch(root, radius, *nb).unwrap();
            assert!(p.seed_artifact_cells.is_empty());
            let d = p.cells.iter().map(|c| c.distance).collect();
            ((root.clone(), *nb), (p, d))
        })
        .collect();
    eprintln!("{} anchor patches ready at radius {radius}", patches.len());

    let params = ClassifyParams {
        max_generations: horizon,
        boundary_distance: radius,
        population_cap: None,
    };
    let mut results: Vec<(u32, u32, Outcome, &ResultRecord)> = records
        .par_iter()
        .map(|record| {
            let (patch, distance) =
                &patches[&(record.root.clone(), record.neighbourhood)];
            let (strata, rule) = record.replay_setup(patch);
            let mut engine = Engine::with_strata(patch.graph.clone(), strata);
            engine.load_state(
                &record.initial_state(patch.graph.cells()).unwrap(),
            );
            let r = classify_run(&mut engine, &rule, distance, &params);
            (
                if r.max_population <= 64 { r.max_changed_distance } else { 0 },
                r.max_population,
                r.outcome,
                record,
            )
        })
        .collect();

    results.sort_by_key(|&(travel, ..)| std::cmp::Reverse(travel));
    let rule_name = |r: ca_engine::Rule| {
        let d = |m: u32| {
            (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>()
        };
        format!("B{}/S{}", d(r.birth), d(r.survival))
    };
    println!("top bounded travels at radius {radius}:");
    for (travel, max_population, outcome, record) in results.iter().take(25) {
        println!(
            "  travel {travel:2} maxpop {max_population:3} {:16} seed {:?} {outcome:?}",
            rule_name(record.rule),
            record.initial_cells,
        );
    }
    let beat = results.iter().filter(|&&(t, ..)| t > 23).count();
    println!(
        "{} of {} candidates beat the known wanderer record (23 rings) with bounded population",
        beat,
        results.len()
    );
}
