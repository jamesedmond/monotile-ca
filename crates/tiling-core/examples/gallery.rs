//! Build a curated gallery of interesting cases as one replayable JSONL
//! file (`results/gallery.jsonl`) for the web UI's record selector. Each
//! entry is classified here so its recorded outcome/max-population are
//! accurate, and labelled in `note`. Mix of hand-constructed showcases
//! and the standout records pulled from the committed sweep files.

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use std::io::Write as _;
use tiling_core::results::ResultRecord;
use tiling_core::{Stratification, Tiling, TilingFamily};

const HAT_WALKER_ROOT: &str =
    "(tile hat)(subtile 1 of F0)(subtile 0 of F0):(subtile 4 of F0)";
const SPECTRE_FILAMENT_ROOT: &str =
    "(tile spectre)(subtile 0 of Sigma)(subtile 4 of Phi)(subtile 3 of Delta):(subtile 2 of Delta)";

enum Seed {
    Ball,            // root cell + its neighbours
    Cells(Vec<u32>), // explicit
}

struct Entry {
    label: &'static str,
    family: TilingFamily,
    root: Option<String>, // None = default root
    radius: u32,
    rule: StratifiedRule,
    seed: Seed,
    horizon: u64,
}

fn classify_and_record(e: &Entry) -> ResultRecord {
    let tiling = Tiling::new(e.family);
    let root = e.root.clone().unwrap_or_else(|| tiling.default_root());
    let patch = tiling.generate(&root, e.radius).unwrap();
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let seed_cells = match &e.seed {
        Seed::Cells(c) => c.clone(),
        Seed::Ball => {
            let mut s = vec![0u32];
            s.extend(patch.graph.neighbours(0));
            s
        }
    };
    // Strata sized to the rule's table count (uniform → all-zero).
    let scheme = if e.rule.tables.len() == 1 {
        Stratification::Uniform
    } else if e.rule.tables.len() == 2 {
        Stratification::Chirality
    } else {
        Stratification::PerClass
    };
    let (strata, _) = patch.strata(scheme);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    for &c in &seed_cells {
        engine.set_cell(c, true);
    }
    let params = ClassifyParams {
        max_generations: e.horizon,
        boundary_distance: e.radius,
        population_cap: None,
    };
    let report = classify_run(&mut engine, &e.rule, &distance, &params);
    let tables = if e.rule.tables.len() == 1 { Vec::new() } else { e.rule.tables.clone() };
    ResultRecord {
        family: e.family,
        root,
        radius: e.radius,
        rule: e.rule.tables[0],
        stratification: if tables.is_empty() { None } else { Some(scheme) },
        tables,
        table_rule: None,
        neighbourhood: tiling_core::Neighbourhood::Edge,
        initial_cells: seed_cells,
        initial_states: Vec::new(),
        generations: e.horizon,
        outcome: report.outcome,
        max_population: report.max_population,
        note: e.label.to_string(),
    }
}

/// Pull the single record with the longest period from a committed sweep
/// file, relabelled.
fn longest_period(path: &str, label: &'static str) -> Option<ResultRecord> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut best: Option<(u64, ResultRecord)> = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let r: ResultRecord = serde_json::from_str(line).ok()?;
        if let Outcome::Periodic { period, .. } = r.outcome
            && best.as_ref().is_none_or(|(p, _)| period > *p)
        {
            best = Some((period, r));
        }
    }
    best.map(|(period, mut r)| {
        r.note = format!("{label} (period {period})");
        r
    })
}

/// Load a single-record champion file and relabel it.
fn load_relabel(path: &str, label: &'static str) -> Option<ResultRecord> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut r: ResultRecord = serde_json::from_str(text.lines().next()?).ok()?;
    r.note = label.to_string();
    Some(r)
}

fn main() {
    let entries = [
        Entry {
            label: "hat walker B25/S25 — travels ~23 rings, then a period-2 blinker",
            family: TilingFamily::Hat,
            root: Some(HAT_WALKER_ROOT.into()),
            radius: 48,
            rule: StratifiedRule::uniform(Rule::new(Rule::mask(&[2, 5]), Rule::mask(&[2, 5]))),
            seed: Seed::Ball,
            horizon: 4096,
        },
        Entry {
            label: "spectre filament B2/S256 — directed linear growth on a fixed heading",
            family: TilingFamily::Spectre,
            root: Some(SPECTRE_FILAMENT_ROOT.into()),
            radius: 96,
            rule: StratifiedRule::uniform(Rule::new(Rule::mask(&[2]), Rule::mask(&[2, 5, 6]))),
            seed: Seed::Cells(vec![0, 1, 2, 3]),
            horizon: 6000,
        },
    ];

    let mut records: Vec<ResultRecord> = entries.iter().map(classify_and_record).collect();

    // Evolved champions (verified bounded) — showcase per-class and
    // k-state rules, including the dying-phase colours of the k=4 one.
    records.extend(load_relabel(
        "results/hat-evolve-r64-s1.jsonl",
        "hat per-class champion — bounded traveller ~65 rings, then period-2",
    ));
    records.extend(load_relabel(
        "results/spectre-evolve-r80-s1.jsonl",
        "spectre k=4 champion — travels, then a period-4 oscillator (dying-phase colours)",
    ));
    records.extend(load_relabel(
        "results/hat-evolve-r72-s2.jsonl",
        "hat k=5 directed grower — the glider near-miss: looks bounded at close \
         radii but its population grows with distance (reload at radius 200+)",
    ));
    records.extend(load_relabel(
        "results/spectre-evolve-r72-s1.jsonl",
        "spectre k=5 directed grower — a bushier, faster-forking grower on the \
         other tiling (population ~100/452/957 at radius 72/144/288; reload large)",
    ));

    // Standout oscillators harvested from the committed sweeps.
    records.extend(longest_period(
        "results/hat-seeds-1ball-r12-h512.jsonl",
        "hat small-seed oscillator",
    ));
    records.extend(longest_period(
        "results/spectre-seeds-1ball-r12-h512.jsonl",
        "spectre small-seed oscillator",
    ));

    let mut out = std::fs::File::create("results/gallery.jsonl").unwrap();
    for r in &records {
        writeln!(out, "{}", serde_json::to_string(r).unwrap()).unwrap();
        println!(
            "{:60} {:?} maxpop {}",
            r.note.chars().take(60).collect::<String>(),
            r.outcome,
            r.max_population
        );
    }
    println!("\n{} records -> results/gallery.jsonl", records.len());
}
