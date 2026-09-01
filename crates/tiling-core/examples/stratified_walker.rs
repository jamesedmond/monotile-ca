//! Chirality-stratified walker search (FINDINGS.md §5: the B25/S25
//! walker is killed by a period-2 blinker residue, the same on hat and
//! spectre). With per-class rule tables we can give hats and antihats
//! different survival/birth, looking for a rule where the walker
//! reaction still travels but the blinker residue is *not* stable.
//!
//! Steps: (1) confirm the uniform walker still traps and report the
//! chirality composition of the trapped residue; (2) search rules within
//! Hamming distance 2 of the uniform walker in the two-table
//! (hat,antihat) bit space, igniting the 1-ball seed at every anchor;
//! report the best bounded travel and flag any escape (bounded
//! population reaching the boundary).

use std::collections::BTreeMap;

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use rayon::prelude::*;
use tiling_core::{Patch, Stratification, Tiling, TilingFamily};

const RADIUS: u32 = 48;
const HORIZON: u64 = 4096;
const BOUNDED_POP: u32 = 64;

/// One anchor's patch, its distance table, and chirality strata.
struct Anchor {
    class: u16,
    patch: Patch,
    distance: Vec<u32>,
    strata: Vec<u8>,
}

fn ignite_ball(anchor: &Anchor, rule: &StratifiedRule, params: &ClassifyParams) -> ca_engine::classify::RunReport {
    let mut engine =
        Engine::with_strata(anchor.patch.graph.clone(), anchor.strata.clone());
    engine.set_cell(0, true);
    for &n in anchor.patch.graph.neighbours(0) {
        engine.set_cell(n, true);
    }
    classify_run(&mut engine, rule, &anchor.distance, params)
}

fn table_name(r: Rule) -> String {
    let d = |m: u32| (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>();
    format!("B{}/S{}", d(r.birth), d(r.survival))
}

fn rule_name(sr: &StratifiedRule) -> String {
    format!("hat[{}] antihat[{}]", table_name(sr.tables[0]), table_name(sr.tables[1]))
}

fn main() {
    let tiling = Tiling::new(TilingFamily::Hat);
    let base = tiling.generate(&tiling.default_root(), 10).unwrap();
    let mut roots: BTreeMap<u16, String> = BTreeMap::new();
    for cell in &base.cells {
        roots.entry(cell.class).or_insert_with(|| cell.address.clone());
    }
    let anchors: Vec<Anchor> = roots
        .par_iter()
        .map(|(&class, root)| {
            let patch = tiling.generate(root, RADIUS).unwrap();
            let distance = patch.cells.iter().map(|c| c.distance).collect();
            let (strata, n) = patch.strata(Stratification::Chirality);
            assert_eq!(n, 2);
            Anchor { class, patch, distance, strata }
        })
        .collect();
    let params = ClassifyParams { max_generations: HORIZON, boundary_distance: RADIUS, population_cap: None };

    let walker = Rule::new(Rule::mask(&[2, 5]), Rule::mask(&[2, 5]));
    let uniform = StratifiedRule::new(vec![walker, walker]);

    // (1) uniform walker on the class-1 anchor: trap + residue chirality.
    let a1 = anchors.iter().find(|a| a.class == 1).unwrap();
    let mut engine = Engine::with_strata(a1.patch.graph.clone(), a1.strata.clone());
    engine.set_cell(0, true);
    for &n in a1.patch.graph.neighbours(0) {
        engine.set_cell(n, true);
    }
    let mut last_changed = 0u64;
    for _ in 0..400 {
        let s = engine.step(&uniform);
        if s.changed == 0 { break; }
        last_changed += 1;
    }
    let (mut hat_live, mut antihat_live) = (0u32, 0u32);
    for c in 0..a1.patch.graph.cells() {
        if engine.cell(c) {
            if a1.strata[c as usize] == 1 { antihat_live += 1 } else { hat_live += 1 }
        }
    }
    println!(
        "uniform walker (class-1): settled after ~{last_changed} gens; trapped residue = {hat_live} hat + {antihat_live} antihat cells"
    );

    // (2) Hamming-ball search in the two-table bit space.
    // Flip ops: (table_index 0=hat/1=antihat, field 0=birth/1=survival, bit).
    let max_dist: usize =
        std::env::args().nth(1).map_or(2, |a| a.parse().expect("distance"));
    let mut ops: Vec<(usize, usize, u32)> = Vec::new();
    for table in 0..2 {
        for bit in 1..=6 { ops.push((table, 0, bit)); }   // birth bits 1..6
        for bit in 0..=6 { ops.push((table, 1, bit)); }   // survival bits 0..6
    }
    let apply = |tables: &mut [Rule; 2], &(t, f, b): &(usize, usize, u32)| {
        if f == 0 { tables[t].birth ^= 1 << b } else { tables[t].survival ^= 1 << b }
    };
    // Iterative-deepening enumeration of the Hamming ball up to max_dist.
    let mut rules: Vec<[Rule; 2]> = Vec::new();
    let mut stack: Vec<(usize, usize, [Rule; 2])> = vec![(0, 0, [walker, walker])];
    while let Some((start, flips, tables)) = stack.pop() {
        rules.push(tables);
        if flips == max_dist { continue; }
        for (i, op) in ops.iter().enumerate().skip(start) {
            let mut next = tables;
            apply(&mut next, op);
            stack.push((i + 1, flips + 1, next));
        }
    }
    eprintln!("{} stratified rules (Hamming <= {max_dist} of uniform walker), {} anchors", rules.len(), anchors.len());

    // (best bounded travel, escaped?, rule, anchor class, outcome, maxpop)
    type Hit = (u32, bool, StratifiedRule, u16, Outcome, u32);
    let mut hits: Vec<Hit> = rules
        .par_iter()
        .filter_map(|tables| {
            let sr = StratifiedRule::new(tables.to_vec());
            let mut best: Option<Hit> = None;
            for a in &anchors {
                let r = ignite_ball(a, &sr, &params);
                if r.max_population > BOUNDED_POP { continue; }
                let escaped = matches!(r.outcome, Outcome::ReachedBoundary { .. });
                if best.as_ref().is_none_or(|b| r.max_changed_distance > b.0) {
                    best = Some((r.max_changed_distance, escaped, sr.clone(), a.class, r.outcome, r.max_population));
                }
            }
            best.filter(|b| b.0 >= 20)
        })
        .collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));

    let escapes: Vec<&Hit> = hits.iter().filter(|h| h.1).collect();
    println!("\n{} bounded rules travelled >= 20 rings; {} ESCAPED to boundary", hits.len(), escapes.len());
    if !escapes.is_empty() {
        println!("ESCAPES (bounded population reaching the boundary — glider candidates):");
        for (travel, _, sr, class, outcome, maxpop) in escapes.iter().take(20) {
            println!("  travel {travel:2} {} class {class} maxpop {maxpop} {outcome:?}", rule_name(sr));
        }
    }
    println!("top bounded travels:");
    for (travel, escaped, sr, class, outcome, maxpop) in hits.iter().take(20) {
        let tag = if *escaped { "ESCAPE" } else { "" };
        println!("  travel {travel:2} {} class {class} maxpop {maxpop} {outcome:?} {tag}", rule_name(sr));
    }
}
