//! Taming the B2/S256 spectre filament (FINDINGS §5.3, §6).
//!
//! The filament grows linearly along a fixed heading; its whole body
//! stays active. A glider is the head/tail balance point — if the tail
//! died at the rate the head advances, population would stay bounded and
//! the object would travel as a finite structure. The lever: per-class
//! survival, so trailing cells (in whatever classes the body occupies)
//! decay while the head's births still propagate.
//!
//! Phases: A characterise the filament (growth + chirality composition);
//! B uniform survival sweep (does merely lowering survival tame it?);
//! C chirality-stratified Hamming search around B2/S256.

use ca_engine::classify::{ClassifyParams, Outcome, RunReport, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use rayon::prelude::*;
use tiling_core::{Patch, Stratification, Tiling, TilingFamily};

// The record-136 igniting configuration (Sigma anchor + asymmetric seed).
const ROOT: &str =
    "(tile spectre)(subtile 0 of Sigma)(subtile 4 of Phi)(subtile 3 of Delta):(subtile 2 of Delta)";
const SEED: [u32; 4] = [0, 1, 2, 3];
const RADIUS: u32 = 56;
const HORIZON: u64 = 3000;
const BOUNDED_POP: u32 = 96; // a finite travelling structure, not a growing blob

fn ignite(
    patch: &Patch,
    strata: &[u8],
    rule: &StratifiedRule,
    params: &ClassifyParams,
) -> RunReport {
    let mut engine = Engine::with_strata(patch.graph.clone(), strata.to_vec());
    for &c in &SEED {
        engine.set_cell(c, true);
    }
    classify_run(&mut engine, rule, &patch.cells.iter().map(|c| c.distance).collect::<Vec<_>>(), params)
}

fn table_name(r: Rule) -> String {
    let d = |m: u32| (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>();
    format!("B{}/S{}", d(r.birth), d(r.survival))
}

fn rule_name(sr: &StratifiedRule) -> String {
    if sr.tables[0] == sr.tables[1] {
        table_name(sr.tables[0])
    } else {
        format!("plain[{}] mystic[{}]", table_name(sr.tables[0]), table_name(sr.tables[1]))
    }
}

/// A semantically-uniform 2-table rule (both strata identical), so it
/// matches the 2-stratum chirality engine without an index-out-of-range.
fn dup(r: Rule) -> StratifiedRule {
    StratifiedRule::new(vec![r, r])
}

fn main() {
    let tiling = Tiling::new(TilingFamily::Spectre);
    let patch = tiling.generate(ROOT, RADIUS).unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let (strata, nstrata) = patch.strata(Stratification::Chirality);
    assert_eq!(nstrata, 2);
    let params = ClassifyParams { max_generations: HORIZON, boundary_distance: RADIUS, population_cap: None };
    let filament = Rule::new(Rule::mask(&[2]), Rule::mask(&[2, 5, 6]));

    // --- A: characterise the uniform filament ---
    let mut engine = Engine::with_strata(patch.graph.clone(), strata.clone());
    for &c in &SEED {
        engine.set_cell(c, true);
    }
    let uni = dup(filament);
    println!("A: uniform B2/S256 filament");
    println!("  gen | pop | mystic-fraction of live cells");
    for generation in 1..=250u64 {
        engine.step(&uni);
        if generation % 50 == 0 {
            let (mut total, mut mystic) = (0u32, 0u32);
            for c in 0..patch.graph.cells() {
                if engine.cell(c) {
                    total += 1;
                    mystic += u32::from(strata[c as usize] == 1);
                }
            }
            println!("  {generation:3} | {total:4} | {:.0}%", 100.0 * f64::from(mystic) / f64::from(total.max(1)));
        }
    }

    // --- B: uniform survival sweep, birth fixed at B2 ---
    println!("\nB: uniform survival sweep (birth = B2), bounded travellers (maxpop <= {BOUNDED_POP}):");
    let mut b_hits: Vec<(u32, bool, u32, Outcome, u32)> = (0u32..256)
        .filter_map(|s| {
            let r = ignite(&patch, &strata, &dup(Rule::new(Rule::mask(&[2]), s)), &params);
            (r.max_population <= BOUNDED_POP && r.max_changed_distance >= 10).then_some((
                r.max_changed_distance,
                matches!(r.outcome, Outcome::ReachedBoundary { .. }),
                s,
                r.outcome,
                r.max_population,
            ))
        })
        .collect();
    b_hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    for (travel, escaped, s, outcome, maxpop) in b_hits.iter().take(12) {
        let d = |m: u32| (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>();
        println!("  travel {travel:2} B2/S{} maxpop {maxpop} {outcome:?} {}", d(*s), if *escaped { "ESCAPE" } else { "" });
    }
    if b_hits.is_empty() {
        println!("  (none — every birth-B2 survival rule either dies fast, traps, or grows unbounded)");
    }

    // --- C: chirality-stratified Hamming-ball search around B2/S256 ---
    let max_dist: usize = std::env::args().nth(1).map_or(2, |a| a.parse().expect("distance"));
    let mut ops: Vec<(usize, usize, u32)> = Vec::new();
    for table in 0..2 {
        for bit in 1..=7 { ops.push((table, 0, bit)); }
        for bit in 0..=7 { ops.push((table, 1, bit)); }
    }
    let apply = |t: &mut [Rule; 2], &(i, f, b): &(usize, usize, u32)| {
        if f == 0 { t[i].birth ^= 1 << b } else { t[i].survival ^= 1 << b }
    };
    let mut rules: Vec<[Rule; 2]> = Vec::new();
    let mut stack: Vec<(usize, usize, [Rule; 2])> = vec![(0, 0, [filament, filament])];
    while let Some((start, flips, t)) = stack.pop() {
        rules.push(t);
        if flips == max_dist { continue; }
        for (i, op) in ops.iter().enumerate().skip(start) {
            let mut next = t;
            apply(&mut next, op);
            stack.push((i + 1, flips + 1, next));
        }
    }
    eprintln!("C: {} stratified rules (Hamming <= {max_dist} of B2/S256)", rules.len());

    type Hit = (u32, bool, StratifiedRule, Outcome, u32);
    let mut hits: Vec<Hit> = rules
        .par_iter()
        .filter_map(|t| {
            let sr = StratifiedRule::new(t.to_vec());
            let r = ignite(&patch, &strata, &sr, &params);
            (r.max_population <= BOUNDED_POP && r.max_changed_distance >= 15).then_some((
                r.max_changed_distance,
                matches!(r.outcome, Outcome::ReachedBoundary { .. }),
                sr,
                r.outcome,
                r.max_population,
            ))
        })
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    let escapes = hits.iter().filter(|h| h.1).count();
    println!("\nC: {} bounded travellers (>=15 rings); {escapes} ESCAPED to boundary", hits.len());
    for (travel, escaped, sr, outcome, maxpop) in hits.iter().take(20) {
        println!("  travel {travel:2} {} maxpop {maxpop} {outcome:?} {}", rule_name(sr), if *escaped { "ESCAPE" } else { "" });
    }
}
