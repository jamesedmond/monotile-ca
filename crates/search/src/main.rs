//! Headless rule-space search over hat/spectre patches.
//!
//! Exhaustively sweeps the B0-free 2-state semi-totalistic rule space
//! (B ⊆ {1..max_degree}, S ⊆ {0..max_degree}; B0 excluded because a
//! strobing vacuum makes a dead-boundary patch unfaithful), classifying
//! every (rule, soup) run exactly via `ca_engine::classify`. Interesting
//! runs — long periods, still-active-at-horizon, and glider candidates
//! (bounded population that travelled to the boundary) — are written as
//! replayable `ResultRecord`s in JSONL, loadable by the web UI.
//!
//! Progress is checkpointed per rule chunk (`.progress` beside the
//! output); rerunning with the same parameters resumes.
//!
//! Usage: search [--family hat|spectre|both] [--radius N] [--horizon N]
//!               [--soups N] [--fill PERMILLE] [--within N] [--out-dir DIR]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use ca_engine::classify::{ClassifyParams, Outcome, RunReport, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use rayon::prelude::*;
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Tiling, TilingFamily};

const CHUNK: usize = 512;

/// Glider-candidate heuristic (flag-for-review, not proof): the run hit
/// the boundary with a small population whose final activity sat
/// entirely in the outer half of the patch.
const CANDIDATE_MAX_POPULATION: u32 = 64;

mod evolve;
mod evolve_table;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Soup census over the whole rule space.
    Sweep,
    /// Small seeds (single / pair / 1-ball / ring) at one anchor per
    /// tile class, hunting glider candidates and clean oscillators.
    Seeds,
    /// Genetic search over full per-class stratified rules for a glider.
    Evolve,
    /// §9.4 level-2 control: genetic search over generalised table rules
    /// on a vertex-neighbourhood patch (random init — rediscovery test).
    EvolveTable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternSet {
    /// single / cell+neighbour pairs / 1-ball / ring (~10 per anchor)
    Basic,
    /// every non-empty subset of the closed 1-ball (up to 255 per anchor)
    BallSubsets,
}

struct Args {
    mode: Mode,
    patterns: PatternSet,
    families: Vec<TilingFamily>,
    radius: u32,
    horizon: u64,
    soups: u64,
    fill_permille: u64,
    within: Option<u32>,
    out_dir: PathBuf,
    // evolve-mode parameters
    generations: usize,
    population: usize,
    states: u8,
    pop_cap: u32,
    balance: bool,
    seed_from: Option<PathBuf>,
    rng_seed: u64,
    /// None = the mode's default (evolve: edge, evolve-table: vertex).
    neighbourhood: Option<Neighbourhood>,
}

fn parse_args() -> Args {
    let mut args = Args {
        mode: Mode::Sweep,
        patterns: PatternSet::Basic,
        families: vec![TilingFamily::Hat, TilingFamily::Spectre],
        radius: 16,
        horizon: 1024,
        soups: 3,
        fill_permille: 333,
        within: None,
        out_dir: PathBuf::from("results"),
        generations: 60,
        population: 256,
        states: 2,
        pop_cap: evolve::DEFAULT_POP_CAP,
        balance: false,
        seed_from: None,
        rng_seed: 1,
        neighbourhood: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .unwrap_or_else(|| panic!("missing value for {flag}"))
        };
        match flag.as_str() {
            "--mode" => {
                args.mode = match value().as_str() {
                    "sweep" => Mode::Sweep,
                    "seeds" => Mode::Seeds,
                    "evolve" => Mode::Evolve,
                    "evolve-table" => Mode::EvolveTable,
                    other => panic!("unknown mode '{other}'"),
                }
            }
            "--patterns" => {
                args.patterns = match value().as_str() {
                    "basic" => PatternSet::Basic,
                    "ball-subsets" => PatternSet::BallSubsets,
                    other => panic!("unknown pattern set '{other}'"),
                }
            }
            "--family" => {
                args.families = match value().as_str() {
                    "hat" => vec![TilingFamily::Hat],
                    "spectre" => vec![TilingFamily::Spectre],
                    "penrosep2" | "p2" => vec![TilingFamily::PenroseP2],
                    "penrosep3" | "p3" => vec![TilingFamily::PenroseP3],
                    "both" => vec![TilingFamily::Hat, TilingFamily::Spectre],
                    other => panic!("unknown family '{other}'"),
                }
            }
            "--neighbourhood" => {
                args.neighbourhood = Some(match value().as_str() {
                    "edge" => Neighbourhood::Edge,
                    "vertex" => Neighbourhood::Vertex,
                    other => panic!("unknown neighbourhood '{other}'"),
                })
            }
            "--radius" => args.radius = value().parse().expect("--radius"),
            "--horizon" => args.horizon = value().parse().expect("--horizon"),
            "--soups" => args.soups = value().parse().expect("--soups"),
            "--fill" => {
                args.fill_permille = value().parse().expect("--fill")
            }
            "--within" => {
                args.within = Some(value().parse().expect("--within"))
            }
            "--out-dir" => args.out_dir = PathBuf::from(value()),
            "--generations" => args.generations = value().parse().expect("--generations"),
            "--population" => args.population = value().parse().expect("--population"),
            "--states" => args.states = value().parse().expect("--states"),
            "--pop-cap" => args.pop_cap = value().parse().expect("--pop-cap"),
            "--balance" => args.balance = true,
            "--seed-from" => args.seed_from = Some(PathBuf::from(value())),
            "--rng-seed" => args.rng_seed = value().parse().expect("--rng-seed"),
            other => panic!("unknown flag '{other}'"),
        }
    }
    args
}

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Bucket {
    Died,
    StillLife,
    ShortPeriod, // 2..=16
    LongPeriod,  // > 16
    Boundary,
    Active,
}

fn bucket(outcome: Outcome) -> Bucket {
    match outcome {
        Outcome::Died { .. } => Bucket::Died,
        Outcome::Periodic { period: 1, .. } => Bucket::StillLife,
        Outcome::Periodic { period: 2..=16, .. } => Bucket::ShortPeriod,
        Outcome::Periodic { .. } => Bucket::LongPeriod,
        Outcome::ReachedBoundary { .. } | Outcome::Unbounded { .. } => Bucket::Boundary,
        Outcome::Active => Bucket::Active,
    }
}

fn rule_name(rule: Rule) -> String {
    let counts = |mask: u32| -> String {
        (0..8).filter(|i| mask >> i & 1 != 0).map(|i| i.to_string()).collect()
    };
    format!("B{}/S{}", counts(rule.birth), counts(rule.survival))
}

/// Why a run deserves a result record, if it does.
fn record_note(report: &RunReport, radius: u32) -> Option<&'static str> {
    match report.outcome {
        Outcome::Periodic { period, .. } if period > 16 => Some("long-period"),
        Outcome::Active => Some("active-at-horizon"),
        Outcome::ReachedBoundary { .. }
            if report.max_population <= CANDIDATE_MAX_POPULATION
                && report.final_min_changed_distance >= radius / 2 =>
        {
            Some("glider-candidate")
        }
        _ => None,
    }
}

fn main() {
    let args = parse_args();
    fs::create_dir_all(&args.out_dir).expect("create out dir");
    if args.mode == Mode::Evolve {
        evolve_mode(&args);
        return;
    }
    if args.mode == Mode::EvolveTable {
        evolve_table_mode(&args);
        return;
    }
    for &family in &args.families {
        match args.mode {
            Mode::Sweep => sweep(family, &args),
            Mode::Seeds => seeds(family, &args),
            Mode::Evolve | Mode::EvolveTable => unreachable!(),
        }
    }
}

/// §9.4 level-2 search-rediscovery control (see evolve_table.rs docs).
/// Defaults to Penrose P3 (the substrate with the known glider) when no
/// --family was given; recommend `--states 4` (the published regime).
fn evolve_table_mode(args: &Args) {
    let family = if args.families.len() == 1 {
        args.families[0]
    } else {
        println!("evolve-table: no --family given, defaulting to penrosep3");
        TilingFamily::PenroseP3
    };
    if args.states < 4 {
        println!(
            "note: --states {} (< 4, the published-glider regime)",
            args.states
        );
    }
    evolve_table::run(&evolve_table::EvolveTableParams {
        family,
        root: Tiling::new(family).default_root(),
        neighbourhood: args.neighbourhood.unwrap_or(Neighbourhood::Vertex),
        radius: args.radius,
        horizon: args.horizon,
        population: args.population,
        generations: args.generations,
        states: args.states,
        pop_cap: args.pop_cap,
        seed_from: args.seed_from.clone(),
        rng_seed: args.rng_seed,
        out_dir: args.out_dir.clone(),
    });
}

/// Genetic search for a glider over per-class stratified rules, seeded
/// from the strongest known leads per family (FINDINGS §6.1).
fn evolve_mode(args: &Args) {
    let family = args.families[0];
    // Igniting configuration: spectre = the record-136 filament; hat =
    // the class-1 walker anchor with a 1-ball seed (empty ⇒ resolved to
    // [0] + neighbours inside the driver).
    let (root, seed_cells) = match family {
        TilingFamily::Spectre => (
            "(tile spectre)(subtile 0 of Sigma)(subtile 4 of Phi)(subtile 3 of Delta):(subtile 2 of Delta)".to_string(),
            vec![0u32, 1, 2, 3],
        ),
        TilingFamily::Hat => {
            let tiling = Tiling::new(family);
            let base = tiling.generate(&tiling.default_root(), 10).unwrap();
            let root = base
                .cells
                .iter()
                .find(|c| c.class == 1)
                .expect("class-1 anchor")
                .address
                .clone();
            (root, Vec::new())
        }
        // Penrose (§9 control substrate): no known lead yet — default
        // root, 1-ball seed.
        TilingFamily::PenroseP2 | TilingFamily::PenroseP3 => {
            (Tiling::new(family).default_root(), Vec::new())
        }
    };
    evolve::run(&evolve::EvolveParams {
        family,
        root,
        seed_cells,
        neighbourhood: args.neighbourhood.unwrap_or(Neighbourhood::Edge),
        radius: args.radius,
        horizon: args.horizon,
        population: args.population,
        generations: args.generations,
        states: args.states,
        pop_cap: args.pop_cap,
        balance: args.balance,
        seed_from: args.seed_from.clone(),
        rng_seed: args.rng_seed,
        out_dir: args.out_dir.clone(),
    });
}

fn sweep(family: TilingFamily, args: &Args) {
    let start = Instant::now();
    let tiling = Tiling::new(family);
    let patch = tiling
        .generate(&tiling.default_root(), args.radius)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let max_degree = (0..patch.graph.cells())
        .map(|c| patch.graph.degree(c))
        .max()
        .unwrap();
    let within = args.within.unwrap_or(args.radius / 2);
    println!(
        "\n=== {family:?}: {} cells, radius {}, max degree {max_degree}, root {}",
        patch.graph.cells(),
        args.radius,
        patch.root
    );

    // Identical soups for every rule.
    let soups: Vec<Vec<u8>> = (1..=args.soups)
        .map(|seed| {
            let mut rng = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
            (0..patch.graph.cells())
                .map(|c| {
                    let inside = distance[c as usize] <= within;
                    (inside
                        && xorshift64(&mut rng) % 1000 < args.fill_permille)
                        as u8
                })
                .collect()
        })
        .collect();
    let soup_cells: Vec<Vec<u32>> = soups
        .iter()
        .map(|s| {
            s.iter()
                .enumerate()
                .filter(|&(_, &v)| v != 0)
                .map(|(i, _)| i as u32)
                .collect()
        })
        .collect();

    let births = 1u32 << max_degree;
    let survivals = 1u32 << (max_degree + 1);
    let all_rules: Vec<Rule> = (0..births)
        .flat_map(|b| (0..survivals).map(move |s| Rule::new(b << 1, s)))
        .collect();

    // Resume: skip rules already in the progress file.
    let stem = format!(
        "{}-r{}-h{}",
        format!("{family:?}").to_lowercase(),
        args.radius,
        args.horizon
    );
    let out_path = args.out_dir.join(format!("{stem}.jsonl"));
    let progress_path = args.out_dir.join(format!("{stem}.progress"));
    let done: BTreeSet<(u32, u32)> = fs::read_to_string(&progress_path)
        .map(|text| {
            text.lines()
                .filter_map(|l| {
                    let (b, s) = l.split_once(' ')?;
                    Some((b.parse().ok()?, s.parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default();
    let rules: Vec<Rule> = all_rules
        .iter()
        .filter(|r| !done.contains(&(r.birth, r.survival)))
        .copied()
        .collect();
    if !done.is_empty() {
        println!(
            "resuming: {} of {} rules already done",
            done.len(),
            all_rules.len()
        );
    }

    let params = ClassifyParams {
        max_generations: args.horizon,
        boundary_distance: args.radius,
        population_cap: None,
    };
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .expect("open results file");
    let mut progress = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&progress_path)
        .expect("open progress file");

    let mut run_tally: BTreeMap<Bucket, u32> = BTreeMap::new();
    let mut best_tally: BTreeMap<Bucket, u32> = BTreeMap::new();
    let mut candidates: Vec<(Rule, u64, u32)> = Vec::new();
    let mut recorded = 0u32;

    for chunk in rules.chunks(CHUNK) {
        let results: Vec<(Rule, Vec<RunReport>)> = chunk
            .par_iter()
            .map_init(
                || Engine::new(patch.graph.clone()),
                |engine, &rule| {
                    let sr = StratifiedRule::uniform(rule);
                    let reports = soups
                        .iter()
                        .map(|soup| {
                            engine.load_state(soup);
                            classify_run(engine, &sr, &distance, &params)
                        })
                        .collect();
                    (rule, reports)
                },
            )
            .collect();

        for (rule, reports) in &results {
            let mut top = Bucket::Died;
            for (soup_idx, report) in reports.iter().enumerate() {
                *run_tally.entry(bucket(report.outcome)).or_default() += 1;
                top = top.max(bucket(report.outcome));
                if let Some(note) = record_note(report, args.radius) {
                    if note == "glider-candidate"
                        && let Outcome::ReachedBoundary { generation } =
                            report.outcome
                    {
                        candidates.push((
                            *rule,
                            generation,
                            report.max_population,
                        ));
                    }
                    let record = ResultRecord {
                        family,
                        root: patch.root.clone(),
                        radius: args.radius,
                        rule: *rule,
                        stratification: None,
                        tables: Vec::new(),
                        table_rule: None,
                        neighbourhood: tiling_core::Neighbourhood::Edge,
                        initial_cells: soup_cells[soup_idx].clone(),
                        initial_states: Vec::new(),
                        generations: args.horizon,
                        outcome: report.outcome,
                        max_population: report.max_population,
                        note: note.into(),
                    };
                    let line = serde_json::to_string(&record).unwrap();
                    writeln!(out, "{line}").unwrap();
                    recorded += 1;
                }
            }
            *best_tally.entry(top).or_default() += 1;
            writeln!(progress, "{} {}", rule.birth, rule.survival).unwrap();
        }
        out.flush().unwrap();
        progress.flush().unwrap();
    }

    println!(
        "{} rules x {} soups, horizon {} ({:.1}s): {} records -> {}",
        rules.len(),
        soups.len(),
        args.horizon,
        start.elapsed().as_secs_f64(),
        recorded,
        out_path.display()
    );
    println!("runs:  {run_tally:?}");
    println!("rules by best soup: {best_tally:?}");
    if !candidates.is_empty() {
        println!("glider candidates (bounded population reaching boundary):");
        for (rule, generation, max_population) in candidates.iter().take(30) {
            println!(
                "  {:18} boundary@{generation}, max population {max_population}",
                rule_name(*rule)
            );
        }
    }
}

/// The small seed patterns at a patch's root cell. `Basic`: the cell
/// itself, each cell+neighbour pair, the closed 1-ball, the open ring.
/// `BallSubsets`: every non-empty subset of the closed 1-ball —
/// exhaustive over that neighbourhood (2^(degree+1) - 1 seeds).
fn seed_patterns(
    patch: &tiling_core::Patch,
    set: PatternSet,
) -> Vec<Vec<u32>> {
    let ring: Vec<u32> = patch.graph.neighbours(0).to_vec();
    let mut ball = vec![0u32];
    ball.extend(&ring);
    match set {
        PatternSet::Basic => {
            let mut patterns = vec![vec![0]];
            for &n in &ring {
                patterns.push(vec![0, n]);
            }
            patterns.push(ball);
            patterns.push(ring);
            patterns
        }
        PatternSet::BallSubsets => (1u32..1 << ball.len())
            .map(|mask| {
                ball.iter()
                    .enumerate()
                    .filter(|&(i, _)| mask >> i & 1 == 1)
                    .map(|(_, &c)| c)
                    .collect()
            })
            .collect(),
    }
}

/// Small-seed search: re-root a patch at one representative cell of every
/// tile class (any cell address is a valid root, and the anchor becomes
/// cell 0, so the patch's distance table measures escape from the seed),
/// then run every B0-free rule from every seed pattern.
fn seeds(family: TilingFamily, args: &Args) {
    let start = Instant::now();
    let tiling = Tiling::new(family);

    // One anchor per tile class, chosen deterministically from a base patch.
    let base = tiling.generate(&tiling.default_root(), 10).unwrap();
    let mut anchors: BTreeMap<u16, String> = BTreeMap::new();
    for cell in &base.cells {
        anchors.entry(cell.class).or_insert_with(|| cell.address.clone());
    }
    println!(
        "\n=== {family:?} seeds: {} anchors (one per class), radius {}, horizon {}",
        anchors.len(),
        args.radius,
        args.horizon
    );

    let stem = format!(
        "{}-seeds{}-r{}-h{}",
        format!("{family:?}").to_lowercase(),
        match args.patterns {
            PatternSet::Basic => "",
            PatternSet::BallSubsets => "-1ball",
        },
        args.radius,
        args.horizon
    );
    let out_path = args.out_dir.join(format!("{stem}.jsonl"));
    let progress_path = args.out_dir.join(format!("{stem}.progress"));
    let done: BTreeSet<String> = fs::read_to_string(&progress_path)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .expect("open results file");
    let mut progress = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&progress_path)
        .expect("open progress file");

    let params = ClassifyParams {
        max_generations: args.horizon,
        boundary_distance: args.radius,
        population_cap: None,
    };
    let mut run_tally: BTreeMap<Bucket, u64> = BTreeMap::new();
    let mut candidates: Vec<(Rule, u16, usize, u64, u32)> = Vec::new();
    let mut oscillators: Vec<(u64, Rule, u16, usize)> = Vec::new();
    let mut recorded = 0u32;

    for (&class, root) in &anchors {
        if done.contains(root) {
            continue;
        }
        let patch = tiling.generate(root, args.radius).unwrap();
        assert!(patch.seed_artifact_cells.is_empty());
        let distance: Vec<u32> =
            patch.cells.iter().map(|c| c.distance).collect();
        let max_degree = (0..patch.graph.cells())
            .map(|c| patch.graph.degree(c))
            .max()
            .unwrap();
        let patterns = seed_patterns(&patch, args.patterns);
        let states: Vec<Vec<u8>> = patterns
            .iter()
            .map(|cells| {
                let mut s = vec![0u8; patch.graph.cells() as usize];
                for &c in cells {
                    s[c as usize] = 1;
                }
                s
            })
            .collect();

        let births = 1u32 << max_degree;
        let survivals = 1u32 << (max_degree + 1);
        let rules: Vec<Rule> = (0..births)
            .flat_map(|b| (0..survivals).map(move |s| Rule::new(b << 1, s)))
            .collect();

        let results: Vec<(Rule, Vec<RunReport>)> = rules
            .par_iter()
            .map_init(
                || Engine::new(patch.graph.clone()),
                |engine, &rule| {
                    let sr = StratifiedRule::uniform(rule);
                    let reports = states
                        .iter()
                        .map(|state| {
                            engine.load_state(state);
                            classify_run(engine, &sr, &distance, &params)
                        })
                        .collect();
                    (rule, reports)
                },
            )
            .collect();

        for (rule, reports) in &results {
            for (pattern_idx, report) in reports.iter().enumerate() {
                *run_tally.entry(bucket(report.outcome)).or_default() += 1;
                let Some(note) = record_note(report, args.radius) else {
                    continue;
                };
                match report.outcome {
                    Outcome::ReachedBoundary { generation }
                        if note == "glider-candidate" =>
                    {
                        candidates.push((
                            *rule,
                            class,
                            pattern_idx,
                            generation,
                            report.max_population,
                        ));
                    }
                    Outcome::Periodic { period, .. } => {
                        oscillators.push((period, *rule, class, pattern_idx));
                    }
                    _ => {}
                }
                let record = ResultRecord {
                    family,
                    root: root.clone(),
                    radius: args.radius,
                    rule: *rule,
                    stratification: None,
                    tables: Vec::new(),
                    table_rule: None,
                    neighbourhood: tiling_core::Neighbourhood::Edge,
                    initial_cells: patterns[pattern_idx].clone(),
                    initial_states: Vec::new(),
                    generations: args.horizon,
                    outcome: report.outcome,
                    max_population: report.max_population,
                    note: note.into(),
                };
                writeln!(out, "{}", serde_json::to_string(&record).unwrap())
                    .unwrap();
                recorded += 1;
            }
        }
        out.flush().unwrap();
        writeln!(progress, "{root}").unwrap();
        progress.flush().unwrap();
        println!(
            "  class {class:2} ({} rules x {} patterns) done at {:.1}s",
            rules.len(),
            patterns.len(),
            start.elapsed().as_secs_f64()
        );
    }

    println!(
        "seed search finished in {:.1}s: {recorded} records -> {}",
        start.elapsed().as_secs_f64(),
        out_path.display()
    );
    println!("runs: {run_tally:?}");
    oscillators.sort_by_key(|&(period, ..)| std::cmp::Reverse(period));
    if !oscillators.is_empty() {
        println!("longest small-seed oscillators:");
        for (period, rule, class, pattern) in oscillators.iter().take(12) {
            println!(
                "  p{period:<5} {:18} class {class}, pattern {pattern}",
                rule_name(*rule)
            );
        }
    }
    if candidates.is_empty() {
        println!("no glider candidates");
    } else {
        println!("GLIDER CANDIDATES (bounded population reaching boundary):");
        for (rule, class, pattern, generation, max_population) in
            candidates.iter().take(40)
        {
            println!(
                "  {:18} class {class}, pattern {pattern}: boundary@{generation}, max pop {max_population}",
                rule_name(*rule)
            );
        }
    }
}
