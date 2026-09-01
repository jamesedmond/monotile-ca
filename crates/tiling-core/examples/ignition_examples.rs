//! Curate replayable alternative starting conditions for the UI from
//! the §10.3 ignition-sweep space. For each source glider record
//! (hat s21/s22, spectre s33): the reversed-order seed at the original
//! site, two glider-like ignitions at anchors of *other* tile classes,
//! and two "grower"-labelled ignitions (so the shower-vs-blob question
//! can be eyeballed). Each entry is re-classified here so its recorded
//! outcome is accurate. Writes `results/ignition-examples.jsonl`.

use std::io::Write as _;

use ca_engine::Engine;
use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

const SOURCES: &[(&str, &str)] = &[
    ("s21", "results/hat-tableevolve-r48-s21.jsonl"),
    ("s22", "results/hat-tableevolve-r48-s22.jsonl"),
    ("s33", "results/spectre-tableevolve-r48-s33.jsonl"),
];
const RADIUS: u32 = 64;
const HORIZON: u64 = 2500;

fn main() {
    let mut out: Vec<ResultRecord> = Vec::new();
    for &(name, path) in SOURCES {
        let text = std::fs::read_to_string(path).expect("read source record");
        let src: ResultRecord =
            serde_json::from_str(text.lines().next().unwrap()).expect("parse");
        let (s0, s1) = (src.initial_states[0], src.initial_states[1]);
        let tiling = Tiling::new(src.family);
        let patch = tiling
            .generate_patch(&src.root, RADIUS, src.neighbourhood)
            .unwrap();
        let edge = tiling.generate(&src.root, RADIUS).unwrap();
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
        let (_, rule) = src.replay_setup(&patch);
        let params = ClassifyParams {
            max_generations: HORIZON,
            boundary_distance: RADIUS - margin,
            population_cap: Some(10_000),
        };
        let classify = |a: u32, n: u32, sa: u8, sb: u8| {
            let mut engine = Engine::new(patch.graph.clone());
            engine.set_state(a, sa);
            engine.set_state(n, sb);
            classify_run(&mut engine, &rule, &distance, &params)
        };
        let make = |label: String, a: u32, n: u32, sa: u8, sb: u8| {
            let r = classify(a, n, sa, sb);
            ResultRecord {
                family: src.family,
                root: src.root.clone(),
                radius: RADIUS,
                rule: src.rule,
                stratification: None,
                tables: Vec::new(),
                table_rule: src.table_rule.clone(),
                neighbourhood: src.neighbourhood,
                initial_cells: vec![a, n],
                initial_states: vec![sa, sb],
                generations: HORIZON,
                outcome: r.outcome,
                max_population: r.max_population,
                note: label,
            }
        };

        // 1. Reversed state order at the original launch site.
        let partner = edge.graph.neighbours(0)[0];
        out.push(make(
            format!("{name} rule — reversed seed order ({s1},{s0}) at the original site"),
            0,
            partner,
            s1,
            s0,
        ));

        // 2./3. Alternative anchors, one per tile class, mid-patch:
        // collect two glider-like and two grower-labelled examples from
        // distinct classes.
        let band = (RADIUS / 4)..=(RADIUS / 2);
        let (mut gliders, mut growers) = (0, 0);
        for class in 0..patch.classes.len() as u16 {
            if gliders >= 2 && growers >= 2 {
                break;
            }
            let Some(a) = (0..patch.graph.cells()).find(|&c| {
                patch.cells[c as usize].class == class
                    && band.contains(&patch.cells[c as usize].distance)
            }) else {
                continue;
            };
            let class_name = &patch.classes[class as usize].name;
            for &n in edge.graph.neighbours(a) {
                if gliders >= 2 && growers >= 2 {
                    break;
                }
                let r = classify(a, n, s0, s1);
                let glider_like = matches!(r.outcome, Outcome::ReachedBoundary { .. })
                    && r.max_population == r.max_population_inner
                    && r.max_population <= 4 * src.max_population;
                if glider_like && gliders < 2 {
                    gliders += 1;
                    out.push(make(
                        format!(
                            "{name} rule — glider-like ignition at a {class_name} anchor (cell {a})"
                        ),
                        a,
                        n,
                        s0,
                        s1,
                    ));
                    break; // at most one glider example per class
                } else if !glider_like
                    && matches!(
                        r.outcome,
                        Outcome::ReachedBoundary { .. } | Outcome::Unbounded { .. }
                    )
                    && growers < 2
                {
                    growers += 1;
                    out.push(make(
                        format!(
                            "{name} rule — 'grower'-labelled ignition at a {class_name} anchor \
                             (cell {a}): shower of gliders or blob? eyeball me"
                        ),
                        a,
                        n,
                        s0,
                        s1,
                    ));
                    break; // at most one grower example per class
                }
            }
        }
    }

    let path = "results/ignition-examples.jsonl";
    let mut f = std::fs::File::create(path).expect("create output");
    for r in &out {
        writeln!(f, "{}", serde_json::to_string(r).unwrap()).unwrap();
        println!("{:60} {:?} maxpop {}", r.note.chars().take(60).collect::<String>(), r.outcome, r.max_population);
    }
    println!("\n{} records -> {path}", out.len());
}
