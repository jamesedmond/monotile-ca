//! Which rows of a table rule actually matter on a given run? Greedy
//! bottom-up pruning by replay equivalence: for each row, last to
//! first, delete it and replay the record in lockstep against the
//! current reference; if every generation up to the run's end is
//! identical, the row is inert and stays deleted. The surviving table
//! is behaviourally equivalent to the full genome on this run by
//! construction (each deletion is verified against the previous
//! reference, and equality is transitive). Trying rows bottom-up
//! resolves duplicate pairs the right way round: the later copy is
//! deleted, the earlier kept.
//!
//! Note this is stronger than "never fires": a row that only ever
//! fires dead-to-dead can still be load-bearing by shadowing a later
//! birth row (a blocker), and pruning keeps such rows.
//!
//! Used to gray out vestigial rows in the paper's atlas tables.
//!
//! Usage: row_usage <record.jsonl> [index] [radius] [horizon]
//!
//! The reference run ends at boundary contact, extinction, or the
//! horizon (default 8 x radius); pass a long horizon for loopers so
//! the full orbit is covered.

use ca_engine::{AnyRule, CaRule, Engine, TableRule};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(384, |a| a.parse().expect("radius"));
    let horizon_override: Option<u64> = args.next().map(|a| a.parse().expect("horizon"));

    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(index)
        .expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    let tiling = Tiling::new(record.family);
    let patch = tiling
        .generate_patch(&record.root, radius, record.neighbourhood)
        .unwrap();
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
    let (strata, rule) = record.replay_setup(&patch);
    let AnyRule::Table(full) = rule else {
        panic!("row_usage expects a table-rule record");
    };
    let seed = record.initial_state(patch.graph.cells()).unwrap();
    let fresh = || {
        let mut e = Engine::with_strata(patch.graph.clone(), strata.clone());
        e.load_state(&seed);
        e
    };

    // Reference run end: boundary contact, extinction, or horizon.
    let horizon = horizon_override.unwrap_or(u64::from(radius) * 8);
    let mut engine = fresh();
    let mut end = horizon;
    for generation in 1..=horizon {
        let stats = full.step(&mut engine);
        let touched = engine
            .state()
            .iter()
            .enumerate()
            .any(|(c, &s)| s != 0 && distance[c] + margin >= radius);
        if touched || stats.population == 0 {
            end = generation;
            break;
        }
    }
    println!(
        "{path}[{index}] radius {radius}: reference run ends at generation {end} ({} rows)",
        full.rows.len()
    );

    // Lockstep comparison: first generation where the candidate's state
    // differs from the current reference rule's, if any.
    let diverges = |reference: &TableRule, candidate: &TableRule| -> Option<u64> {
        let (mut a, mut b) = (fresh(), fresh());
        for generation in 1..=end {
            reference.step(&mut a);
            candidate.step(&mut b);
            if a.state() != b.state() {
                return Some(generation);
            }
        }
        None
    };

    let n = full.rows.len();
    let mut removed = vec![false; n];
    let mut verdict: Vec<String> = vec![String::new(); n];
    for i in (0..n).rev() {
        let keep = |skip: usize, removed: &[bool]| -> TableRule {
            let rows = full
                .rows
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != skip && !removed[j])
                .map(|(_, r)| r.clone())
                .collect();
            TableRule::new(full.states, rows)
        };
        let reference = keep(usize::MAX, &removed);
        let candidate = keep(i, &removed);
        match diverges(&reference, &candidate) {
            None => {
                removed[i] = true;
                verdict[i] = "INERT (deleted; run identical)".into();
            }
            Some(g) => verdict[i] = format!("kept (diverges at generation {g})"),
        }
    }

    for (i, row) in full.rows.iter().enumerate() {
        let own = row.own.map_or("any".into(), |o| o.to_string());
        let conds = if row.conds.is_empty() {
            "always".into()
        } else {
            row.conds
                .iter()
                .map(|&(s, m)| format!("n{s}>={m}"))
                .collect::<Vec<_>>()
                .join(" & ")
        };
        println!(
            "row {:>2}: {own} | {conds:24} -> {}   {}",
            i + 1,
            row.next,
            verdict[i]
        );
    }
    let inert: Vec<usize> = (0..n).filter(|&i| removed[i]).map(|i| i + 1).collect();
    println!("inert rows (1-based): {inert:?}");
}
