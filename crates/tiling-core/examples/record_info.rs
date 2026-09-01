//! Print a human-readable description of `ResultRecord`s: substrate,
//! rule table, and exact seed (cell indices, states, tile addresses).
//! Source data for GLIDERS.md.
//!
//! Usage: record_info <file.jsonl> [more files...]

use std::collections::HashMap;

use ca_engine::RuleRow;
use tiling_core::results::ResultRecord;
use tiling_core::{Tiling, TilingFamily};

fn row_str(r: &RuleRow) -> String {
    let own = r.own.map_or("any".to_string(), |o| o.to_string());
    let conds = if r.conds.is_empty() {
        "always".to_string()
    } else {
        r.conds
            .iter()
            .map(|(s, m)| format!("n{s} >= {m}"))
            .collect::<Vec<_>>()
            .join(" and ")
    };
    format!("own {own} | {conds} -> {}", r.next)
}

fn main() {
    let mut tilings: HashMap<TilingFamily, Tiling> = HashMap::new();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("read record file");
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let r: ResultRecord = serde_json::from_str(line).expect("parse record");
            println!("## {path}");
            println!(
                "family: {:?}   radius: {}   neighbourhood: {:?}",
                r.family, r.radius, r.neighbourhood
            );
            println!("root: {}", r.root);
            println!("note: {}", r.note);
            println!(
                "recorded outcome: {:?}   max population: {}",
                r.outcome, r.max_population
            );
            match &r.table_rule {
                Some(t) => {
                    println!(
                        "rule: {}-state priority table (first matching row wins; no match -> 0):",
                        t.states
                    );
                    for row in &t.rows {
                        println!("  {}", row_str(row));
                    }
                }
                None => println!(
                    "rule: B{:#x}/S{:#x}, {} states",
                    r.rule.birth, r.rule.survival, r.rule.states
                ),
            }
            // Cell indices are BFS-stable, so a small patch suffices to
            // resolve seed addresses.
            let tiling = tilings
                .entry(r.family)
                .or_insert_with(|| Tiling::new(r.family));
            let patch = tiling.generate(&r.root, 3).unwrap();
            println!("seed:");
            for (i, &c) in r.initial_cells.iter().enumerate() {
                let state = r.initial_states.get(i).copied().unwrap_or(1);
                let tag = if c == 0 {
                    "(root) "
                } else if patch.graph.neighbours(0).contains(&c) {
                    "(edge-neighbour of root) "
                } else {
                    ""
                };
                let addr = patch
                    .cells
                    .get(c as usize)
                    .map_or("(outside preview patch)", |m| m.address.as_str());
                println!("  cell {c}: state {state}  {tag}{addr}");
            }
            println!();
        }
    }
}
