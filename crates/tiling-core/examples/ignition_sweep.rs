//! Ignition-genericity sweep (§10 follow-up): does a glider rule
//! ignite from *generic* two-cell seeds, or was the discovery
//! launchpad special? Flight robustness is already established (the
//! gliders cross hundreds of rings of novel terrain); this tests the
//! other half — whether the seed → glider transition works elsewhere.
//!
//! For one anchor cell per tile class (interior, mid-patch), seed the
//! record's two initial states across every shared edge of the anchor,
//! in both orders, and classify. "Glider-like" = reached the boundary
//! with flat max-population (inner = overall) no larger than 4× the
//! record's.
//!
//! Usage: ignition_sweep <record.jsonl> [index] [radius] [horizon]

use ca_engine::Engine;
use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use rayon::prelude::*;
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(64, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(2500, |a| a.parse().expect("horizon"));

    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(index)
        .expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    assert_eq!(record.initial_cells.len(), 2, "expects a two-cell seed record");
    let s0 = record.initial_states.first().copied().unwrap_or(1);
    let s1 = record.initial_states.get(1).copied().unwrap_or(1);

    let tiling = Tiling::new(record.family);
    let patch = tiling
        .generate_patch(&record.root, radius, record.neighbourhood)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let edge = tiling.generate(&record.root, radius).unwrap();
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let dist = &distance;
    let margin = (0..patch.graph.cells())
        .flat_map(|c| {
            let dc = dist[c as usize];
            patch
                .graph
                .neighbours(c)
                .iter()
                .map(move |&n| dc.abs_diff(dist[n as usize]))
        })
        .max()
        .unwrap_or(1);
    let (_, rule) = record.replay_setup(&patch);
    let params = ClassifyParams {
        max_generations: horizon,
        boundary_distance: radius - margin,
        population_cap: Some(10_000),
    };
    println!(
        "ignition sweep: {:?}, state pair ({s0},{s1}) both orders, radius {radius}, {} classes — {}",
        record.family,
        patch.classes.len(),
        record.note
    );

    // One interior anchor per tile class, mid-patch (fresh terrain, far
    // from both the discovery site and the boundary).
    let band = (radius / 4)..=(radius / 2);
    let anchors: Vec<(u16, u32)> = (0..patch.classes.len() as u16)
        .filter_map(|class| {
            (0..patch.graph.cells()).find_map(|c| {
                (patch.cells[c as usize].class == class
                    && band.contains(&patch.cells[c as usize].distance))
                .then_some((class, c))
            })
        })
        .collect();

    // All seedings: (class, anchor, partner, ordered state pair).
    let seedings: Vec<(u16, u32, u32, u8, u8)> = anchors
        .iter()
        .flat_map(|&(class, a)| {
            edge.graph.neighbours(a).iter().flat_map(move |&n| {
                [(class, a, n, s0, s1), (class, a, n, s1, s0)]
            })
        })
        .collect();

    let results: Vec<(u16, &'static str)> = seedings
        .par_iter()
        .map(|&(class, a, n, sa, sb)| {
            let mut engine = Engine::new(patch.graph.clone());
            engine.set_state(a, sa);
            engine.set_state(n, sb);
            let r = classify_run(&mut engine, &rule, &distance, &params);
            let kind = match r.outcome {
                Outcome::ReachedBoundary { .. }
                    if r.max_population == r.max_population_inner
                        && r.max_population <= 4 * record.max_population =>
                {
                    "glider"
                }
                Outcome::ReachedBoundary { .. } | Outcome::Unbounded { .. } => "grower",
                Outcome::Died { .. } => "died",
                Outcome::Periodic { .. } => "trapped",
                Outcome::Active => "active",
            };
            (class, kind)
        })
        .collect();

    let kinds = ["glider", "grower", "died", "trapped", "active"];
    println!("\n{:<24} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}", "class", "seeds", "glider", "grower", "died", "trapped", "active");
    let mut totals = [0usize; 5];
    for &(class, anchor) in &anchors {
        let rows: Vec<&&str> = results
            .iter()
            .filter(|(c, _)| *c == class)
            .map(|(_, k)| k)
            .collect();
        let count = |k: &str| rows.iter().filter(|&&&r| r == k).count();
        let per: Vec<usize> = kinds.iter().map(|k| count(k)).collect();
        for (t, p) in totals.iter_mut().zip(&per) {
            *t += p;
        }
        println!(
            "{:<24} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}   (anchor {anchor})",
            patch.classes[class as usize].name, rows.len(), per[0], per[1], per[2], per[3], per[4]
        );
    }
    let total: usize = totals.iter().sum();
    println!(
        "\ntotal {total} seedings: {} glider-like ({:.0}%), {} grower, {} died, {} trapped, {} active",
        totals[0],
        100.0 * totals[0] as f64 / total as f64,
        totals[1],
        totals[2],
        totals[3],
        totals[4],
    );
}
