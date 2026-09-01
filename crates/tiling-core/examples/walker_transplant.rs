//! Transplant experiment: capture the B25/S25 walker's mid-flight forms
//! (its live-cell sets at several generations, before the fatal
//! debris-shedding) and use them as seeds under every B0-free rule at
//! the same position. A rule that cannot ignite a walker from a ball
//! might still carry one that is already formed.
//!
//! Same-patch transplants only: moving a shape to a different anchor
//! needs the canonical local-configuration mapping (open question).

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use rayon::prelude::*;
use tiling_core::{Tiling, TilingFamily};

const RADIUS: u32 = 48;
const HORIZON: u64 = 8192;
const CAPTURE_AT: [u64; 5] = [16, 32, 48, 64, 80];

fn main() {
    let tiling = Tiling::new(TilingFamily::Hat);
    let base = tiling.generate(&tiling.default_root(), 10).unwrap();
    let root = base
        .cells
        .iter()
        .find(|c| c.class == 1)
        .map(|c| c.address.clone())
        .expect("class-1 anchor");
    let patch = tiling.generate(&root, RADIUS).unwrap();
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();

    // Capture the walker's forms.
    let walker = StratifiedRule::uniform(Rule::new(Rule::mask(&[2, 5]), Rule::mask(&[2, 5])));
    let mut engine = Engine::new(patch.graph.clone());
    engine.set_cell(0, true);
    for &n in patch.graph.neighbours(0) {
        engine.set_cell(n, true);
    }
    let mut shapes: Vec<(u64, Vec<u32>, u32)> = Vec::new(); // (gen, cells, start max dist)
    for g in 1..=*CAPTURE_AT.last().unwrap() {
        engine.step(&walker);
        if CAPTURE_AT.contains(&g) {
            let cells: Vec<u32> = engine
                .state()
                .iter()
                .enumerate()
                .filter(|&(_, &s)| s != 0)
                .map(|(i, _)| i as u32)
                .collect();
            let start_max =
                cells.iter().map(|&c| distance[c as usize]).max().unwrap();
            shapes.push((g, cells, start_max));
        }
    }
    for (g, cells, start_max) in &shapes {
        eprintln!("shape@{g}: {} cells, outer edge at ring {start_max}", cells.len());
    }

    let rules: Vec<Rule> = (0u32..64)
        .flat_map(|b| (0u32..128).map(move |s| Rule::new(b << 1, s)))
        .collect();
    let params = ClassifyParams {
        max_generations: HORIZON,
        boundary_distance: RADIUS,
        population_cap: None,
    };
    let rule_name = |r: Rule| {
        let d = |m: u32| {
            (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>()
        };
        format!("B{}/S{}", d(r.birth), d(r.survival))
    };

    // (onward travel, rule, shape gen, outcome, maxpop)
    let mut hits: Vec<(i64, Rule, u64, Outcome, u32)> = rules
        .par_iter()
        .map_init(
            || Engine::new(patch.graph.clone()),
            |engine, &rule| {
                let mut best: Option<(i64, Rule, u64, Outcome, u32)> = None;
                for (g, cells, start_max) in &shapes {
                    let mut state = vec![0u8; patch.graph.cells() as usize];
                    for &c in cells {
                        state[c as usize] = 1;
                    }
                    engine.load_state(&state);
                    let sr = StratifiedRule::uniform(rule);
                    let r = classify_run(engine, &sr, &distance, &params);
                    if r.max_population > 64 {
                        continue;
                    }
                    let onward =
                        i64::from(r.max_changed_distance) - i64::from(*start_max);
                    if best.is_none_or(|b| onward > b.0) {
                        best = Some((onward, rule, *g, r.outcome, r.max_population));
                    }
                }
                best
            },
        )
        .flatten()
        .collect();

    hits.sort_by_key(|&(onward, ..)| std::cmp::Reverse(onward));
    println!("top onward travel (rings beyond the shape's starting outer edge):");
    for (onward, rule, g, outcome, maxpop) in hits.iter().take(15) {
        println!(
            "  +{onward:3} {:16} shape@{g} maxpop {maxpop:3} {outcome:?}",
            rule_name(*rule)
        );
    }
}
