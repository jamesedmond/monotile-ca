//! Re-root result records so their seed sits at the patch centre.
//!
//! The census (FINDINGS 10.11.5) seeds objects around ring r/4 of a large
//! patch and records them with the source record's root and radius, so
//! the seed cells are out of range on a patch of the stated radius. This
//! rewrites each record with the address of its first seed cell as the
//! root, a small radius, the seed cells re-indexed by canonical address
//! in the new patch, and the outcome / max population re-classified on
//! that patch — a self-contained record like every other in results/.
//!
//! Control: the re-rooted run must reproduce the original run exactly.
//! Both are stepped side by side and their live (address, state) sets
//! compared every generation until the first boundary contact of either.
//!
//! With a snapshot generation, the record's evolution is first run on the
//! source patch for that many generations and the live cells then (all of
//! them, or the vertex-connected component whose direction from the seed
//! is nearest `select-heading`) become the seed that is re-rooted — a
//! record that starts with a settled object at generation 0.
//!
//! Usage: reroot_record <in.jsonl> <out.jsonl> [radius=48] [horizon=2500] [source-radius=128] [snapshot-gen=0] [select-heading]

use std::collections::{BTreeSet, HashMap};

use ca_engine::classify::{ClassifyParams, classify_run};
use ca_engine::{CaRule, Engine};
use tiling_core::results::ResultRecord;
use tiling_core::slide::fan_margin;
use tiling_core::{Patch, Tiling};

fn live(patch: &Patch, state: &[u8]) -> BTreeSet<(String, u8)> {
    state
        .iter()
        .enumerate()
        .filter(|&(_, &s)| s != 0)
        .map(|(c, &s)| (patch.cells[c].address.clone(), s))
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("input record file");
    let output = args.next().expect("output record file");
    let radius: u32 = args.next().map_or(48, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(2500, |a| a.parse().expect("horizon"));
    let source_radius: u32 = args.next().map_or(128, |a| a.parse().expect("source radius"));
    let snapshot_gen: u64 = args.next().map_or(0, |a| a.parse().expect("snapshot generation"));
    let select_heading: Option<f64> = args.next().map(|a| a.parse().expect("select heading"));

    let text = std::fs::read_to_string(&input).expect("read input");
    let mut out = String::new();
    for (i, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let mut record: ResultRecord = serde_json::from_str(line).expect("parse record");
        let tiling = Tiling::new(record.family);
        let source = tiling
            .generate_patch(&record.root, source_radius, record.neighbourhood)
            .expect("source patch");
        if snapshot_gen > 0 {
            // Snapshot the live cells after `snapshot_gen` generations.
            let (strata, rule) = record.replay_setup(&source);
            let mut engine = Engine::with_strata(source.graph.clone(), strata);
            engine.load_state(&record.initial_state(source.graph.cells()).unwrap());
            for _ in 0..snapshot_gen {
                rule.step(&mut engine);
            }
            let state = engine.state();
            let mut live: Vec<u32> = (0..source.graph.cells()).filter(|&c| state[c as usize] != 0).collect();
            assert!(!live.is_empty(), "record {i}: nothing alive at generation {snapshot_gen}");
            if let Some(h) = select_heading {
                // Vertex-connected components (the patch graph is the record's
                // neighbourhood); pick the one whose centroid direction from
                // the original seed's centroid is nearest the requested heading.
                let (sgeom_patch, sgeom) = tiling
                    .generate_patch_with_geometry(&record.root, source_radius, record.neighbourhood)
                    .expect("source geometry");
                assert_eq!(sgeom_patch.graph.cells(), source.graph.cells());
                let centroid = |c: u32| {
                    let (lo, hi) = (sgeom.offsets[c as usize] as usize, sgeom.offsets[c as usize + 1] as usize);
                    let pts = &sgeom.xy[lo..hi];
                    let n = pts.len() as f64;
                    let (sx, sy) = pts.iter().fold((0.0, 0.0), |(a, b), &[x, y]| (a + x, b + y));
                    (sx / n, sy / n)
                };
                let seed_c = {
                    let cs: Vec<(f64, f64)> = record.initial_cells.iter().map(|&c| centroid(c)).collect();
                    let n = cs.len() as f64;
                    (cs.iter().map(|p| p.0).sum::<f64>() / n, cs.iter().map(|p| p.1).sum::<f64>() / n)
                };
                let mut seen: HashMap<u32, usize> = HashMap::new();
                let mut comps: Vec<Vec<u32>> = Vec::new();
                for &c in &live {
                    if seen.contains_key(&c) {
                        continue;
                    }
                    let id = comps.len();
                    let mut comp = vec![c];
                    let mut stack = vec![c];
                    seen.insert(c, id);
                    while let Some(x) = stack.pop() {
                        for &y in source.graph.neighbours(x) {
                            if state[y as usize] != 0 && !seen.contains_key(&y) {
                                seen.insert(y, id);
                                comp.push(y);
                                stack.push(y);
                            }
                        }
                    }
                    comp.sort_unstable();
                    comps.push(comp);
                }
                let best = comps
                    .iter()
                    .min_by(|a, b| {
                        let dir = |comp: &Vec<u32>| {
                            let n = comp.len() as f64;
                            let (sx, sy) = comp.iter().map(|&c| centroid(c)).fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
                            let ang = (sy / n - seed_c.1).atan2(sx / n - seed_c.0).to_degrees();
                            let d = (ang - h).rem_euclid(360.0);
                            d.min(360.0 - d)
                        };
                        dir(a).partial_cmp(&dir(b)).unwrap()
                    })
                    .expect("components");
                println!(
                    "record {i}: generation {snapshot_gen}, {} components; selected {} cells nearest heading {h}°",
                    comps.len(),
                    best.len()
                );
                live = best.clone();
            }
            record.initial_states = live.iter().map(|&c| state[c as usize]).collect();
            record.initial_cells = live;
        }
        let max_index = *record.initial_cells.iter().max().expect("seed cells");
        assert!(
            max_index < source.graph.cells(),
            "record {i}: seed cell {max_index} beyond the radius-{source_radius} source patch"
        );
        let seed_addr: Vec<&str> = record
            .initial_cells
            .iter()
            .map(|&c| source.cells[c as usize].address.as_str())
            .collect();
        let root = seed_addr[0].to_string();
        let patch = tiling
            .generate_patch(&root, radius, record.neighbourhood)
            .expect("re-rooted patch");
        assert!(patch.seed_artifact_cells.is_empty(), "record {i}: re-rooted patch has seed artifacts");
        let index: HashMap<&str, u32> = patch
            .cells
            .iter()
            .enumerate()
            .map(|(c, m)| (m.address.as_str(), c as u32))
            .collect();
        let initial_cells: Vec<u32> = seed_addr
            .iter()
            .map(|a| *index.get(a).unwrap_or_else(|| panic!("record {i}: seed address {a} absent from re-rooted patch")))
            .collect();

        // Re-classify on the new patch, as the search pipeline would.
        let margin = fan_margin(&patch);
        let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
        let mut rerooted = ResultRecord {
            root: root.clone(),
            radius,
            initial_cells,
            generations: horizon,
            ..record.clone()
        };
        let (strata, rule) = rerooted.replay_setup(&patch);
        let mut engine = Engine::with_strata(patch.graph.clone(), strata);
        engine.load_state(&rerooted.initial_state(patch.graph.cells()).unwrap());
        let report = classify_run(
            &mut engine,
            &rule,
            &distance,
            &ClassifyParams {
                max_generations: horizon,
                boundary_distance: radius - margin,
                population_cap: None,
            },
        );
        rerooted.outcome = report.outcome;
        rerooted.max_population = report.max_population;

        // Control: the two runs agree cell for cell (by address) until the
        // first boundary contact of either.
        let (s_strata, s_rule) = record.replay_setup(&source);
        let mut s_engine = Engine::with_strata(source.graph.clone(), s_strata);
        s_engine.load_state(&record.initial_state(source.graph.cells()).unwrap());
        let (n_strata, n_rule) = rerooted.replay_setup(&patch);
        let mut n_engine = Engine::with_strata(patch.graph.clone(), n_strata);
        n_engine.load_state(&rerooted.initial_state(patch.graph.cells()).unwrap());
        let s_dist: Vec<u32> = source.cells.iter().map(|c| c.distance).collect();
        let s_margin = fan_margin(&source);
        let mut changed = Vec::new();
        let mut compared = 0u64;
        for generation in 0..=horizon {
            assert_eq!(
                live(&source, s_engine.state()),
                live(&patch, n_engine.state()),
                "record {i}: runs differ at generation {generation}"
            );
            compared = generation;
            if generation == horizon {
                break;
            }
            s_rule.step_with_changes(&mut s_engine, &mut changed);
            let s_boundary = changed.iter().any(|&c| s_dist[c as usize] >= source_radius - s_margin);
            n_rule.step_with_changes(&mut n_engine, &mut changed);
            let n_boundary = changed.iter().any(|&c| distance[c as usize] >= radius - margin);
            if s_boundary || n_boundary {
                break;
            }
        }
        println!(
            "record {i}: root {} -> {root}; cells {:?} -> {:?}; {:?}, max pop {}; control: identical for {compared} generations",
            record.root, record.initial_cells, rerooted.initial_cells, rerooted.outcome, rerooted.max_population
        );
        out.push_str(&serde_json::to_string(&rerooted).unwrap());
        out.push('\n');
    }
    std::fs::write(&output, out).expect("write output");
}
