//! Run a priority-table rule from random soups on a hat or spectre patch
//! with the vertex neighbourhood, report what survives to the boundary,
//! optionally log the leading live cell's address every generation
//! (`TRACK` lines, for a heading fit against the parametric table) and
//! write the tracked soup as a replayable record for `verify_candidate`
//! and the certificate machinery.
//!
//! The rule is a `TableRule` in the record format, read from a JSON file
//! (`{"states":4,"rows":[{"own":1,"conds":[[0,3],[2,1]],"next":2},...]}`),
//! so a table published by someone else can be transcribed once, as data,
//! and run through the identical pipeline as our own objects. First use:
//! Gabbay's `TABLE:HAT-WAKE` (Aperiodic Life 3.7.x, October 2026).
//!
//! Usage: table_soups <rule.json> [family] [radius] [horizon] [soups]
//!        [density] [rng-seed] [seed-radius] [track-soup] [record-out]

use ca_engine::classify::Outcome;
use ca_engine::{CaRule, Engine, Rule, TableRule};
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Tiling, TilingFamily};

/// Connected components of the live set: (cells, min ring, max ring).
fn components(graph: &ca_engine::Graph, states: &[u8], distance: &[u32]) -> Vec<(usize, u32, u32)> {
    let mut seen = vec![false; states.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..states.len() {
        if states[start] == 0 || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start as u32);
        let (mut size, mut dmin, mut dmax) = (0usize, u32::MAX, 0u32);
        while let Some(c) = stack.pop() {
            size += 1;
            dmin = dmin.min(distance[c as usize]);
            dmax = dmax.max(distance[c as usize]);
            for &nb in graph.neighbours(c) {
                if states[nb as usize] != 0 && !seen[nb as usize] {
                    seen[nb as usize] = true;
                    stack.push(nb);
                }
            }
        }
        out.push((size, dmin, dmax));
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let rule_path = args.next().expect("rule JSON file");
    let family = match args.next().as_deref() {
        None | Some("hat") => TilingFamily::Hat,
        Some("spectre") => TilingFamily::Spectre,
        Some(other) => panic!("unknown family {other}"),
    };
    let radius: u32 = args.next().map_or(48, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(4000, |a| a.parse().expect("horizon"));
    let soups: u32 = args.next().map_or(12, |a| a.parse().expect("soups"));
    let density: u32 = args.next().map_or(35, |a| a.parse().expect("density"));
    let mut rng: u64 = args.next().map_or(0x5eed, |a| a.parse().expect("rng seed"));
    let rng_seed = rng;
    let seed_radius: u32 = args
        .next()
        .map_or(radius / 3, |a| a.parse().expect("seed radius"));
    let track: Option<u32> = args.next().map(|a| a.parse().expect("track soup"));
    let record_out: Option<String> = args.next();

    let rule: TableRule =
        serde_json::from_str(&std::fs::read_to_string(&rule_path).expect("read rule")).expect("rule JSON");
    let states = rule.states;
    assert!(states >= 2, "need at least two states");

    let tiling = Tiling::new(family);
    let patch = tiling
        .generate_patch(&tiling.default_root(), radius, Neighbourhood::Vertex)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty(), "degenerate root");
    let n = patch.graph.cells() as usize;
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    println!(
        "{family:?} vertex patch radius {radius}: {n} cells; rule {rule_path} ({states} states, {} rows); \
         horizon {horizon}, {soups} soups at density {density}% within ring {seed_radius}, rng {rng_seed}",
        rule.rows.len()
    );

    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    println!(
        "\n{:>4} {:>6} {:>7} {:>8} {:>7} {:>6} {:>5}  outcome",
        "soup", "seeded", "peak", "ended", "gens", "reach", "comps"
    );
    let mut travelled = Vec::new();
    for soup in 0..soups {
        let mut cur = vec![0u8; n];
        let mut seeded = 0;
        for c in 0..n {
            if distance[c] <= seed_radius && (rand() % 100) < density as u64 {
                cur[c] = 1 + (rand() % (states as u64 - 1)) as u8;
                seeded += 1;
            }
        }
        if track == Some(soup)
            && let Some(path) = &record_out
        {
            let cells: Vec<u32> = (0..n as u32).filter(|&c| cur[c as usize] != 0).collect();
            let st: Vec<u8> = cells.iter().map(|&c| cur[c as usize]).collect();
            let record = ResultRecord {
                family,
                root: patch.root.clone(),
                radius,
                rule: Rule::generations(0, 0, states), // display placeholder
                stratification: None,
                tables: Vec::new(),
                table_rule: Some(rule.clone()),
                table_rules: Vec::new(),
                neighbourhood: Neighbourhood::Vertex,
                initial_cells: cells,
                initial_states: st,
                generations: horizon,
                outcome: Outcome::Active,
                max_population: seeded as u32,
                note: format!(
                    "table rule from {rule_path}, soup {soup} of rng {rng_seed} at {density}% within ring {seed_radius}"
                ),
            };
            std::fs::write(path, format!("{}\n", serde_json::to_string(&record).unwrap())).unwrap();
            println!("        record written to {path}");
        }
        let mut engine = Engine::new(patch.graph.clone());
        engine.load_state(&cur);
        let (mut peak, mut gens, mut reach) = (seeded, 0u64, 0u32);
        let mut outcome = "horizon";
        for g in 1..=horizon {
            CaRule::step(&rule, &mut engine);
            let cur = engine.state();
            let pop = cur.iter().filter(|s| **s != 0).count();
            gens = g;
            peak = peak.max(pop);
            let far = (0..n)
                .filter(|&c| cur[c] != 0)
                .map(|c| distance[c])
                .max()
                .unwrap_or(0);
            reach = reach.max(far);
            if pop == 0 {
                outcome = "died";
                break;
            }
            if track == Some(soup)
                && let Some(lead) = (0..n).filter(|&c| cur[c] != 0).max_by_key(|&c| distance[c])
            {
                println!("TRACK\t{g}\t{lead}\t{}\t{}", distance[lead], patch.cells[lead].address);
            }
            if far + 3 >= radius {
                outcome = "boundary";
                break;
            }
        }
        let cur = engine.state();
        let pop = cur.iter().filter(|s| **s != 0).count();
        let comps = components(&patch.graph, cur, &distance);
        println!(
            "{soup:>4} {seeded:>6} {peak:>7} {pop:>8} {gens:>7} {reach:>6} {:>5}  {outcome}",
            comps.len()
        );
        if outcome != "died" {
            let mut shape: Vec<_> = comps.clone();
            shape.sort_by_key(|c| std::cmp::Reverse(c.2));
            let desc: Vec<String> = shape
                .iter()
                .take(4)
                .map(|(size, dmin, dmax)| format!("{size} cells, rings {dmin}-{dmax} (extent {})", dmax - dmin))
                .collect();
            println!("        components, farthest first: {}", desc.join("; "));
            travelled.push((soup, gens, reach, pop, comps.len()));
        }
    }

    println!();
    if travelled.is_empty() {
        println!("every soup died inside the patch at radius {radius}.");
    } else {
        println!("{} of {soups} soups still had live cells at the end:", travelled.len());
        for (soup, gens, reach, pop, comps) in &travelled {
            println!("  soup {soup}: {pop} cells in {comps} component(s) after {gens} generations, reached ring {reach}");
        }
    }
}
