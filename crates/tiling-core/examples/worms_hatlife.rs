//! Trevorrow's HatLife "Worms" rule (May 2023) on our own hat patches.
//!
//! HatLife (conwaylife.com/forums/viewtopic.php?t=6015) is an interactive
//! Lua script that runs CA rules on a fixed 21,781-hat patch. Its
//! hand-designed `Worms` rule "was deliberately designed to support
//! spaceships (called 'worms') that travel within the wide gaps between
//! two parallel lines of reflected hats (the worms don't survive for very
//! long within the thin gaps). The worms are extensible and can have any
//! length of at least 4 hats."
//!
//! This example transcribes `DoWormsRule` from HatLife.lua and runs it on
//! patches generated from an address, so a worm can be followed past the
//! edge of any fixed patch. Four states: 0 empty, 1 body, 2 head, 3 tail
//! precursor. Reflected hats (our antihats) are killed every generation
//! and can never be born, so they act as permanent walls; the rule reads
//! only the counts of neighbours in states 1, 2 and 3, so forcing
//! antihats to 0 reproduces HatLife exactly. Neighbours are hats sharing
//! an edge *or* a vertex, which is our `Neighbourhood::Vertex`.
//!
//! The question this answers: does a worm travel indefinitely on a generic
//! hat tiling, or does it die when its corridor narrows?
//!
//! Usage: worms_hatlife [radius] [horizon] [soups] [density] [rng-seed]

use std::collections::HashMap;

use ca_engine::classify::Outcome;
use ca_engine::{CaRule, Engine, Rule, RuleRow, StratifiedTableRule, TableRule};
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Stratification, Tiling, TilingFamily};

/// `DoWormsRule` as a priority table (first matching row fires; no row
/// ⇒ 0), for the record format and the certificate machinery. Exact
/// equalities become ordered thresholds: a row for `n ≥ k+1` precedes
/// the row for `n ≥ k`.
fn worms_table() -> TableRule {
    let row = |own: u8, conds: &[(u8, u8)], next: u8| RuleRow {
        own: Some(own),
        conds: conds.to_vec(),
        next,
    };
    TableRule::new(
        4,
        vec![
            // empty: exactly one head neighbour and no body → tail precursor
            row(0, &[(2, 2)], 0),
            row(0, &[(1, 1)], 0),
            row(0, &[(2, 1)], 3),
            // body
            row(1, &[(1, 3)], 0),
            row(1, &[(1, 2)], 1),
            row(1, &[(1, 1), (2, 2), (3, 2)], 0),
            row(1, &[(1, 1), (2, 2), (3, 1)], 1),
            row(1, &[(1, 1), (2, 2)], 3),
            row(1, &[(1, 1), (2, 1)], 1),
            row(1, &[(1, 1), (3, 2)], 0),
            row(1, &[(1, 1), (3, 1)], 1),
            row(1, &[(1, 1)], 3),
            // head
            row(2, &[(1, 2)], 0),
            row(2, &[(1, 1), (3, 4)], 2),
            row(2, &[(1, 1), (3, 3)], 1),
            row(2, &[(1, 1)], 2),
            // tail precursor
            row(3, &[(3, 3)], 0),
            row(3, &[(2, 2)], 0),
            row(3, &[(3, 2), (2, 1)], 2),
        ],
    )
}

/// Stratum 0 (hats): the worms table. Stratum 1 (reflected hats): no
/// rows, so always 0 — a permanent wall.
fn worms_rule() -> StratifiedTableRule {
    StratifiedTableRule::new(vec![worms_table(), TableRule::new(4, vec![])])
}

/// One generation of HatLife's `DoWormsRule`. `wall[i]` marks a
/// reflected hat: always 0, never born.
fn step(graph: &ca_engine::Graph, wall: &[bool], cur: &[u8], next: &mut [u8]) {
    for cell in 0..cur.len() {
        if wall[cell] {
            next[cell] = 0;
            continue;
        }
        let (mut n1, mut n2, mut n3) = (0u32, 0u32, 0u32);
        for &nb in graph.neighbours(cell as u32) {
            match cur[nb as usize] {
                1 => n1 += 1,
                2 => n2 += 1,
                3 => n3 += 1,
                _ => {}
            }
        }
        next[cell] = match cur[cell] {
            // empty: a head's lone empty neighbour becomes the next tail
            0 => {
                if n2 == 1 && n1 == 0 {
                    3
                } else {
                    0
                }
            }
            1 => {
                if n1 == 2 {
                    1
                } else if n1 == 1 {
                    if n2 == 1 {
                        1 // body survives
                    } else if n3 == 0 {
                        3 // new tail is coming
                    } else if n3 == 1 {
                        1 // new tail
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
            2 => {
                if n1 != 1 {
                    0 // not part of a worm
                } else if n3 == 3 {
                    1 // new head is coming
                } else {
                    2
                }
            }
            _ => {
                if n3 == 2 && n2 == 1 {
                    2 // new head
                } else {
                    0
                }
            }
        };
    }
}

/// Connected components of the live set, as (cells, heads, max distance).
fn components(
    graph: &ca_engine::Graph,
    states: &[u8],
    distance: &[u32],
) -> Vec<(usize, usize, u32, u32)> {
    let mut seen = vec![false; states.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..states.len() {
        if states[start] == 0 || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start as u32);
        let (mut size, mut heads, mut dmin, mut dmax) = (0usize, 0usize, u32::MAX, 0u32);
        while let Some(c) = stack.pop() {
            size += 1;
            if states[c as usize] == 2 {
                heads += 1;
            }
            dmin = dmin.min(distance[c as usize]);
            dmax = dmax.max(distance[c as usize]);
            for &nb in graph.neighbours(c) {
                if states[nb as usize] != 0 && !seen[nb as usize] {
                    seen[nb as usize] = true;
                    stack.push(nb);
                }
            }
        }
        out.push((size, heads, dmin, dmax));
    }
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let radius: u32 = args.next().map_or(48, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(4000, |a| a.parse().expect("horizon"));
    let soups: u32 = args.next().map_or(12, |a| a.parse().expect("soups"));
    let density: u32 = args.next().map_or(35, |a| a.parse().expect("density"));
    let mut rng: u64 = args.next().map_or(0x5eed, |a| a.parse().expect("rng seed"));
    let rng_seed = rng;
    let seed_radius: u32 = args
        .next()
        .map_or(radius / 3, |a| a.parse().expect("seed radius"));
    // Optional: for this soup, log the leading live cell's address every
    // generation, so its heading can be read off the parametric table.
    let track: Option<u32> = args.next().map(|a| a.parse().expect("track soup"));
    // Optional: write the tracked soup as a replayable record (JSONL) for
    // `certify closure` / `certify trace`, after checking that the table
    // form of the rule steps identically to the transcription above.
    let record_out: Option<String> = args.next();

    let tiling = Tiling::new(TilingFamily::Hat);
    let patch = tiling
        .generate_patch(&tiling.default_root(), radius, Neighbourhood::Vertex)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty(), "degenerate root");
    let n = patch.graph.cells() as usize;
    let wall: Vec<bool> = patch
        .cells
        .iter()
        .map(|c| patch.classes[c.class as usize].base == "antihat")
        .collect();
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let walls = wall.iter().filter(|w| **w).count();
    println!(
        "hat vertex patch radius {radius}: {n} cells, {walls} reflected ({:.1}%), \
         horizon {horizon}, {soups} soups at density {density}% within ring {seed_radius}",
        100.0 * walls as f64 / n as f64,
    );
    // Degree check against HatLife: reflected hats 6, unreflected 6 or 7.
    let mut deg: HashMap<(bool, usize), usize> = HashMap::new();
    for c in 0..n {
        if distance[c] + 3 < radius {
            *deg.entry((wall[c], patch.graph.neighbours(c as u32).len()))
                .or_default() += 1;
        }
    }
    let mut keys: Vec<_> = deg.keys().copied().collect();
    keys.sort();
    let shown: Vec<String> = keys
        .iter()
        .map(|k| {
            format!(
                "{} deg {}: {}",
                if k.0 { "reflected" } else { "unreflected" },
                k.1,
                deg[k]
            )
        })
        .collect();
    println!("interior degrees — {}", shown.join(", "));

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
                cur[c] = 1 + (rand() % 3) as u8;
                seeded += 1;
            }
        }
        if track == Some(soup)
            && let Some(path) = &record_out
        {
            // Self-check: the engine with the stratified table rule must
            // agree with the hand-coded step, generation by generation.
            let rule = worms_rule();
            let (strata, _) = patch.strata(Stratification::Chirality);
            let mut engine = Engine::with_strata(patch.graph.clone(), strata);
            engine.load_state(&cur);
            let (mut a, mut b) = (cur.clone(), vec![0u8; n]);
            for g in 1..=400u32 {
                step(&patch.graph, &wall, &a, &mut b);
                std::mem::swap(&mut a, &mut b);
                CaRule::step(&rule, &mut engine);
                assert!(engine.state() == a.as_slice(), "table rule diverges at generation {g}");
            }
            println!("        table rule agrees with the transcription for 400 generations");
            let cells: Vec<u32> = (0..n as u32).filter(|&c| cur[c as usize] != 0).collect();
            let states: Vec<u8> = cells.iter().map(|&c| cur[c as usize]).collect();
            let record = ResultRecord {
                family: TilingFamily::Hat,
                root: patch.root.clone(),
                radius,
                rule: Rule::generations(0, 0, 4), // display placeholder
                stratification: Some(Stratification::Chirality),
                tables: Vec::new(),
                table_rule: None,
                table_rules: rule.tables.clone(),
                neighbourhood: Neighbourhood::Vertex,
                initial_cells: cells,
                initial_states: states,
                generations: horizon,
                outcome: Outcome::Active,
                max_population: seeded as u32,
                note: format!(
                    "HatLife Worms (Trevorrow, May 2023), soup {soup} of rng {rng_seed} at {density}% within ring {seed_radius}"
                ),
            };
            std::fs::write(path, format!("{}\n", serde_json::to_string(&record).unwrap())).unwrap();
            println!("        record written to {path}");
        }
        let mut next = vec![0u8; n];
        let (mut peak, mut gens, mut reach) = (seeded, 0u64, 0u32);
        let mut outcome = "horizon";
        for g in 1..=horizon {
            step(&patch.graph, &wall, &cur, &mut next);
            std::mem::swap(&mut cur, &mut next);
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
                println!(
                    "TRACK\t{g}\t{lead}\t{}\t{}",
                    distance[lead], patch.cells[lead].address
                );
            }
            if far + 3 >= radius {
                outcome = "boundary";
                break;
            }
        }
        let pop = cur.iter().filter(|s| **s != 0).count();
        let comps = components(&patch.graph, &cur, &distance);
        let worms = comps.iter().filter(|c| c.1 == 1 && c.0 >= 4).count();
        println!(
            "{soup:>4} {seeded:>6} {peak:>7} {pop:>8} {gens:>7} {reach:>6} {:>5}  {outcome}{}",
            comps.len(),
            if worms > 0 {
                format!(" ({worms} worm-shaped: one head, >= 4 cells)")
            } else {
                String::new()
            },
        );
        if outcome != "died" {
            let mut shape: Vec<_> = comps.clone();
            shape.sort_by_key(|c| std::cmp::Reverse(c.3));
            let desc: Vec<String> = shape
                .iter()
                .take(4)
                .map(|(size, heads, dmin, dmax)| {
                    format!("{size} cells/{heads} head, rings {dmin}-{dmax} (extent {})", dmax - dmin)
                })
                .collect();
            println!("        components, farthest first: {}", desc.join("; "));
            travelled.push((soup, gens, reach, pop, worms));
        }
    }

    println!();
    if travelled.is_empty() {
        println!(
            "every soup died inside the patch: no object of this rule survived \
             to the boundary at radius {radius}."
        );
    } else {
        println!(
            "{} of {soups} soups still had live cells at the end:",
            travelled.len()
        );
        for (soup, gens, reach, pop, worms) in &travelled {
            println!(
                "  soup {soup}: {pop} cells after {gens} generations, reached ring {reach}, \
                 {worms} worm-shaped component(s)"
            );
        }
    }
}
