//! §9 Penrose detection control: run Goucher's 4-state glider rule
//! (corrected Table 1 — see `TableRule::goucher_glider`) on Penrose
//! patches with the vertex neighbourhood and classify with the
//! project's standard machinery.
//!
//! On P3 (rhombs), a head+tail pair seeded across any shared edge lies
//! on the de Bruijn ribbon crossing that edge and must glide to the
//! patch boundary with a small, *flat* population (the exact signature
//! the classifier calls a glider candidate). On P2 (kites and darts)
//! the same rule instead produces *loopers* — closed orbits whose
//! published periods (20, 40, 200, ...) Brent's cycle detection
//! reports exactly.
//!
//! Usage: goucher_smoke [radius] [horizon] [p2|p3] [record-out.jsonl]
//!
//! With a fourth argument, the first seeding's run is written as a
//! replayable `ResultRecord` (table rule + vertex neighbourhood +
//! multi-state seed) for the web UI / verify_candidate.

use ca_engine::classify::{ClassifyParams, classify_run};
use ca_engine::{Engine, Rule, TableRule};
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Tiling, TilingFamily};

fn main() {
    let mut args = std::env::args().skip(1);
    let radius: u32 = args.next().map_or(24, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(4000, |a| a.parse().expect("horizon"));
    let family = match args.next().as_deref() {
        None | Some("p3") => TilingFamily::PenroseP3,
        Some("p2") => TilingFamily::PenroseP2,
        Some(other) => panic!("unknown family '{other}'"),
    };
    let record_out = args.next();

    let tiling = Tiling::new(family);
    let patch = tiling
        .generate_patch(&tiling.default_root(), radius, Neighbourhood::Vertex)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    println!(
        "{family:?} vertex patch radius {radius}: {} cells (root class {})",
        patch.graph.cells(),
        patch.classes[patch.cells[0].class as usize].name,
    );

    // Vertex neighbourhoods are only complete a few rings in (fan bound).
    let params = ClassifyParams {
        max_generations: horizon,
        boundary_distance: radius - 3,
        population_cap: Some(10_000),
    };
    let rule = TableRule::goucher_glider();

    // The edge-neighbours of the head are the 4 tiles sharing a full
    // edge; find them via the *edge* patch (same BFS indices).
    let edge_patch = tiling.generate(&tiling.default_root(), radius).unwrap();
    println!("seeding head+tail across each shared edge of several anchors:");
    for head in [0u32, 7, 23, 61, 150] {
        if head >= patch.graph.cells()
            || patch.cells[head as usize].distance > radius / 2
        {
            continue;
        }
        for &tail in edge_patch.graph.neighbours(head) {
            let mut engine = Engine::new(patch.graph.clone());
            engine.set_state(head, 1);
            engine.set_state(tail, 2);
            let r = classify_run(&mut engine, &rule, &distance, &params);
            println!(
                "head {head:3} tail {tail:3}: {:?} travel {} maxpop {} (inner {})",
                r.outcome, r.max_changed_distance, r.max_population, r.max_population_inner,
            );
        }
    }

    if let Some(path) = record_out {
        let tail = edge_patch.graph.neighbours(0)[0];
        let mut engine = Engine::new(patch.graph.clone());
        engine.set_state(0, 1);
        engine.set_state(tail, 2);
        let r = classify_run(&mut engine, &rule, &distance, &params);
        let note = match family {
            TilingFamily::PenroseP3 => format!(
                "Goucher 2012 glider — head+tail on a P3 ribbon (vertex \
                 neighbourhood, corrected Table 1): flat population {} to \
                 the boundary, ~2 generations/ring",
                r.max_population
            ),
            _ => format!(
                "Goucher 2012 looper — the same glider rule loops on P2 \
                 ({:?})",
                r.outcome
            ),
        };
        let record = ResultRecord {
            family,
            root: patch.root.clone(),
            radius,
            rule: Rule::generations(0, 0, 4), // display placeholder
            stratification: None,
            tables: Vec::new(),
            table_rule: Some(rule),
            neighbourhood: Neighbourhood::Vertex,
            initial_cells: vec![0, tail],
            initial_states: vec![1, 2],
            generations: horizon,
            outcome: r.outcome,
            max_population: r.max_population,
            note,
        };
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&record).unwrap()),
        )
        .unwrap();
        println!("record written to {path}");
    }
}
