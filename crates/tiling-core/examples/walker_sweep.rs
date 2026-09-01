//! Rule-space neighbourhood sweep around the known wanderer rules
//! (FINDINGS.md §5.1): all rules within Hamming distance 3 of B25/S25
//! and distance 2 of B2/S26 (flippable bits: B1..B6, S0..S6 — B0 stays
//! off), launched as ball and ring seeds from one anchor per tile class
//! on radius-48 hat patches. Bounded runs (max population ≤ 64) that
//! travel ≥ 20 rings are written as replayable records.
//!
//! Usage: walker_sweep [out.jsonl]

use std::collections::BTreeSet;
use std::io::Write as _;

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use rayon::prelude::*;
use tiling_core::results::ResultRecord;
use tiling_core::{Tiling, TilingFamily};

const RADIUS: u32 = 48;
const HORIZON: u64 = 8192;
const BOUNDED_POP: u32 = 64;
const RECORD_TRAVEL: u32 = 20;

fn hamming_ball(base: Rule, dist: usize) -> BTreeSet<(u32, u32)> {
    // flippable bit positions: birth 1..=6 encoded 0..6, survival 0..=6
    // encoded 7..13
    let positions: Vec<(u32, u32)> = (1..=6)
        .map(|k| (1u32 << k, 0u32))
        .chain((0..=6).map(|k| (0u32, 1u32 << k)))
        .collect();
    let mut out = BTreeSet::new();
    let mut stack = vec![(0usize, 0usize, base.birth, base.survival)];
    while let Some((start, flips, b, s)) = stack.pop() {
        out.insert((b, s));
        if flips == dist {
            continue;
        }
        for (i, &(db, ds)) in positions.iter().enumerate().skip(start) {
            stack.push((i + 1, flips + 1, b ^ db, s ^ ds));
        }
    }
    out
}

fn rule_name(rule: Rule) -> String {
    let d = |m: u32| {
        (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>()
    };
    format!("B{}/S{}", d(rule.birth), d(rule.survival))
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "results/hat-walker-d3.jsonl".into());
    let tiling = Tiling::new(TilingFamily::Hat);
    let base = tiling.generate(&tiling.default_root(), 10).unwrap();
    let mut anchors: std::collections::BTreeMap<u16, String> = Default::default();
    for cell in &base.cells {
        anchors.entry(cell.class).or_insert_with(|| cell.address.clone());
    }
    let patches: Vec<(u16, tiling_core::Patch, Vec<u32>)> = anchors
        .par_iter()
        .map(|(&class, root)| {
            let p = tiling.generate(root, RADIUS).unwrap();
            assert!(p.seed_artifact_cells.is_empty());
            let d = p.cells.iter().map(|c| c.distance).collect();
            (class, p, d)
        })
        .collect();
    eprintln!("{} anchor patches ready", patches.len());

    let mut rule_set =
        hamming_ball(Rule::new(Rule::mask(&[2, 5]), Rule::mask(&[2, 5])), 3);
    rule_set
        .extend(hamming_ball(Rule::new(Rule::mask(&[2]), Rule::mask(&[2, 6])), 2));
    let rules: Vec<Rule> =
        rule_set.iter().map(|&(b, s)| Rule::new(b, s)).collect();
    eprintln!("{} rules in the union of Hamming balls", rules.len());

    let params = ClassifyParams {
        max_generations: HORIZON,
        boundary_distance: RADIUS,
        population_cap: None,
    };

    // (travel, rule, class, seed kind, outcome, max pop, seed cells)
    type Hit = (u32, Rule, u16, &'static str, Outcome, u32, Vec<u32>);
    let mut hits: Vec<Hit> = rules
        .par_iter()
        .flat_map_iter(|&rule| {
            let sr = StratifiedRule::uniform(rule);
            let mut best: Vec<Hit> = Vec::new();
            for (class, patch, distance) in &patches {
                let ring: Vec<u32> = patch.graph.neighbours(0).to_vec();
                let mut ball = vec![0u32];
                ball.extend(&ring);
                for (kind, seed) in [("ball", &ball), ("ring", &ring)] {
                    let mut engine = Engine::new(patch.graph.clone());
                    for &c in seed {
                        engine.set_cell(c, true);
                    }
                    let r = classify_run(&mut engine, &sr, distance, &params);
                    if r.max_population <= BOUNDED_POP
                        && r.max_changed_distance >= RECORD_TRAVEL
                    {
                        best.push((
                            r.max_changed_distance,
                            rule,
                            *class,
                            kind,
                            r.outcome,
                            r.max_population,
                            seed.clone(),
                        ));
                    }
                }
            }
            best
        })
        .collect();

    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    let mut out = std::fs::File::create(&out_path).expect("create output");
    for (travel, rule, class, kind, outcome, max_population, seed) in &hits {
        let record = ResultRecord {
            family: TilingFamily::Hat,
            root: patches.iter().find(|(c, ..)| c == class).unwrap().1.root.clone(),
            radius: RADIUS,
            rule: *rule,
            stratification: None,
            tables: Vec::new(),
            table_rule: None,
            neighbourhood: tiling_core::Neighbourhood::Edge,
            initial_cells: seed.clone(),
            initial_states: Vec::new(),
            generations: HORIZON,
            outcome: *outcome,
            max_population: *max_population,
            note: format!("walker-sweep travel {travel} ({kind})"),
        };
        writeln!(out, "{}", serde_json::to_string(&record).unwrap()).unwrap();
    }
    println!(
        "{} bounded runs travelled >= {RECORD_TRAVEL} rings -> {out_path}",
        hits.len()
    );
    for (travel, rule, class, kind, outcome, max_population, _) in hits.iter().take(20) {
        println!(
            "  travel {travel:2} {:16} class {class:2} {kind:4} {outcome:?} maxpop {max_population}",
            rule_name(*rule)
        );
    }
}
