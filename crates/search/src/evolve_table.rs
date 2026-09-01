//! §9.4 level-2 control: can the *unmodified search strategy* rediscover
//! a glider, on a substrate where one is known to exist?
//!
//! Genetic search over generalised table rules ([`TableRule`]: priority
//! rows over per-state neighbour counts — the family that provably
//! contains Goucher's Penrose glider, FINDINGS §9.2) on a
//! vertex-neighbourhood patch. Everything else is the project's standard
//! machinery: exact classification, population-cap explosion filter,
//! travel × flatness fitness (the §8 balance objective), cross-radius
//! champion verification.
//!
//! Honesty constraints (this is a *rediscovery* control):
//! - the initial population is fully random — no known rule is planted;
//! - the seed bank is neutral in state roles (all ordered state pairs
//!   across one shared edge, plus single cells and a 1-ball), so no
//!   head-before-tail convention is baked in;
//! - fitness is the same travel objective used on hat/spectre.
//!
//! Pre-stated outcomes: finding a rule whose champion escapes at every
//! radius with flat max-population validates the search stack (it finds
//! gliders where gliders exist); a plateau on growers/oscillators
//! localises the hat/spectre negative in *search power* rather than
//! substrate. Both are informative.

use std::io::Write as _;

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{Engine, Rule, RuleRow, TableRule};
use rayon::prelude::*;
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Patch, Tiling, TilingFamily};

/// Genome shape bounds. Goucher's rule is 5 rows with ≤ 2 conditions,
/// comfortably interior to these; the bounds keep the space generic
/// without letting genomes bloat.
const MAX_ROWS: usize = 8;
const MAX_CONDS: usize = 2;
/// Condition thresholds range over 1..=MAX_MIN (a "≥ 4" condition on a
/// quadrilateral edge substrate is near-unsatisfiable; vertex degrees
/// reach ~10, and 3 is enough to express the published glider).
const MAX_MIN: u8 = 3;

pub struct EvolveTableParams {
    pub family: TilingFamily,
    pub root: String,
    /// Adjacency relation (§10.5 ablation: edge vs vertex).
    pub neighbourhood: Neighbourhood,
    pub radius: u32,
    pub horizon: u64,
    pub population: usize,
    pub generations: usize,
    pub states: u8,
    pub pop_cap: u32,
    /// Optional champion record (with `table_rule`) to seed from — for
    /// iterating on a previous run, NOT for the blind control.
    pub seed_from: Option<std::path::PathBuf>,
    pub rng_seed: u64,
    pub out_dir: std::path::PathBuf,
}

fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Largest edge-BFS distance step across any single graph edge — for a
/// vertex-neighbourhood patch this is the fan bound, measured on the
/// patch itself rather than assumed. Cells within `radius - margin` of
/// the root have complete neighbourhoods.
fn neighbour_distance_margin(patch: &Patch) -> u32 {
    (0..patch.graph.cells())
        .flat_map(|c| {
            let dc = patch.cells[c as usize].distance;
            patch
                .graph
                .neighbours(c)
                .iter()
                .map(move |&n| dc.abs_diff(patch.cells[n as usize].distance))
        })
        .max()
        .unwrap_or(1)
}

/// One seed: (cells, states), same convention as `ResultRecord`.
type Seed = (Vec<u32>, Vec<u8>);

/// Neutral multi-state seed bank at the root anchor: every single-cell
/// state, every ordered state pair across one shared (edge-adjacency)
/// edge, and the all-alive closed 1-ball.
fn seed_bank(edge_partner: u32, ball: &[u32], states: u8) -> Vec<(String, Seed)> {
    let mut bank = Vec::new();
    for s in 1..states {
        bank.push((format!("single {s}"), (vec![0], vec![s])));
    }
    for a in 1..states {
        for b in 1..states {
            bank.push((
                format!("pair {a},{b}"),
                (vec![0, edge_partner], vec![a, b]),
            ));
        }
    }
    bank.push((
        "1-ball of 1s".into(),
        (ball.to_vec(), vec![1; ball.len()]),
    ));
    bank
}

struct Arena {
    patch: Patch,
    distance: Vec<u32>,
    params: ClassifyParams,
    seeds: Vec<(String, Seed)>,
    states: u8,
    /// Fan margin: max BFS-distance step per graph edge — the speed of
    /// light in rings/generation for the causality filter.
    margin: u32,
}

#[derive(Clone)]
struct Evaluation {
    fitness: f64,
    travel: u32,
    max_population: u32,
    escaped: bool,
    outcome: Outcome,
    seed_idx: usize,
}

impl Arena {
    /// Fitness = best over the seed bank of travel × flatness, with the
    /// population cap zeroing growers — identical objective to the §8
    /// balance search, just over table rules and multi-state seeds.
    fn evaluate(&self, rows: &[RuleRow]) -> Evaluation {
        let rule = TableRule::new(self.states, rows.to_vec());
        let cap = self.params.population_cap.unwrap_or(u32::MAX);
        let mut best = Evaluation {
            fitness: 0.0,
            travel: 0,
            max_population: 0,
            escaped: false,
            outcome: Outcome::Active,
            seed_idx: 0,
        };
        for (i, (_, (cells, sts))) in self.seeds.iter().enumerate() {
            let mut engine = Engine::new(self.patch.graph.clone());
            for (&c, &s) in cells.iter().zip(sts) {
                engine.set_state(c, s);
            }
            let r = classify_run(&mut engine, &rule, &self.distance, &self.params);
            // Causality filter (FINDINGS §10.5): activity must stay
            // inside the seed's light cone (travel <= seed extent +
            // generations x fan margin). Kills "boundary sniffing" —
            // degree-threshold rules that ignite the patch edge
            // directly (reachable only on edge patches, whose corner
            // cells have degree < the interior minimum).
            let final_generation = match r.outcome {
                Outcome::ReachedBoundary { generation }
                | Outcome::Died { generation }
                | Outcome::Unbounded { generation } => generation,
                Outcome::Periodic { detected_by, .. } => detected_by,
                Outcome::Active => self.params.max_generations,
            };
            let seed_extent =
                cells.iter().map(|&c| self.distance[c as usize]).max().unwrap_or(0);
            let causal = u64::from(r.max_changed_distance)
                <= u64::from(seed_extent) + final_generation * u64::from(self.margin);
            let fitness = if !causal || r.max_population > cap {
                0.0
            } else {
                let flatness = f64::from(r.max_population_inner)
                    / f64::from(r.max_population.max(1));
                f64::from(r.max_changed_distance) * flatness
            };
            if i == 0 || fitness > best.fitness {
                best = Evaluation {
                    fitness,
                    travel: r.max_changed_distance,
                    max_population: r.max_population,
                    escaped: matches!(r.outcome, Outcome::ReachedBoundary { .. }),
                    outcome: r.outcome,
                    seed_idx: i,
                };
            }
        }
        best
    }
}

fn random_cond(rng: &mut u64, states: u8) -> (u8, u8) {
    (
        (xorshift64(rng) % u64::from(states)) as u8,
        1 + (xorshift64(rng) % u64::from(MAX_MIN)) as u8,
    )
}

fn random_row(rng: &mut u64, states: u8) -> RuleRow {
    let own = match xorshift64(rng) % (u64::from(states) + 1) {
        0 => None,
        s => Some((s - 1) as u8),
    };
    let conds = (0..(xorshift64(rng) as usize % (MAX_CONDS + 1)))
        .map(|_| random_cond(rng, states))
        .collect();
    RuleRow {
        own,
        conds,
        next: (xorshift64(rng) % u64::from(states)) as u8,
    }
}

fn random_genome(rng: &mut u64, states: u8) -> Vec<RuleRow> {
    (0..(2 + xorshift64(rng) as usize % 5))
        .map(|_| random_row(rng, states))
        .collect()
}

fn mutate(rng: &mut u64, genome: &mut Vec<RuleRow>, states: u8) {
    let pick = |rng: &mut u64, n: usize| (xorshift64(rng) as usize) % n;
    match xorshift64(rng) % 8 {
        0 if genome.len() < MAX_ROWS => {
            let at = pick(rng, genome.len() + 1);
            let row = random_row(rng, states);
            genome.insert(at, row);
        }
        1 if genome.len() > 1 => {
            let at = pick(rng, genome.len());
            genome.remove(at);
        }
        2 if genome.len() > 1 => {
            let at = pick(rng, genome.len() - 1);
            genome.swap(at, at + 1); // priority reorder
        }
        _ => {
            let at = pick(rng, genome.len());
            let row = &mut genome[at];
            match xorshift64(rng) % 4 {
                0 => {
                    row.own = match xorshift64(rng) % (u64::from(states) + 1) {
                        0 => None,
                        s => Some((s - 1) as u8),
                    }
                }
                1 => row.next = (xorshift64(rng) % u64::from(states)) as u8,
                2 if !row.conds.is_empty() => {
                    let c = pick(rng, row.conds.len());
                    row.conds[c] = random_cond(rng, states);
                }
                _ => {
                    if row.conds.len() < MAX_CONDS {
                        let cond = random_cond(rng, states);
                        row.conds.push(cond);
                    } else if !row.conds.is_empty() {
                        let c = pick(rng, row.conds.len());
                        row.conds.remove(c);
                    }
                }
            }
        }
    }
}

/// One-point row splice (priority order is part of the genotype).
fn crossover(rng: &mut u64, a: &[RuleRow], b: &[RuleRow], states: u8) -> Vec<RuleRow> {
    let i = (xorshift64(rng) as usize) % (a.len() + 1);
    let j = (xorshift64(rng) as usize) % (b.len() + 1);
    let mut child: Vec<RuleRow> =
        a[..i].iter().chain(b[j..].iter()).cloned().collect();
    child.truncate(MAX_ROWS);
    if child.is_empty() {
        child.push(random_row(rng, states));
    }
    child
}

fn row_str(r: &RuleRow) -> String {
    let own = r.own.map_or("*".to_string(), |o| o.to_string());
    let conds = if r.conds.is_empty() {
        "always".to_string()
    } else {
        r.conds
            .iter()
            .map(|(s, m)| format!("n{s}>={m}"))
            .collect::<Vec<_>>()
            .join(" & ")
    };
    format!("{own} | {conds} -> {}", r.next)
}

pub fn run(p: &EvolveTableParams) {
    let tiling = Tiling::new(p.family);
    let patch = tiling
        .generate_patch(&p.root, p.radius, p.neighbourhood)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty(), "degenerate root");
    let edge = tiling.generate(&p.root, p.radius).unwrap();
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let margin = neighbour_distance_margin(&patch);
    let ball: Vec<u32> = {
        let mut b = vec![0u32];
        b.extend(edge.graph.neighbours(0));
        b
    };
    let seeds = seed_bank(edge.graph.neighbours(0)[0], &ball, p.states);
    println!(
        "evolve-table {:?} ({:?}): {} cells, fan margin {margin}, {} states, {} seeds/genome, root {}",
        p.family,
        p.neighbourhood,
        patch.graph.cells(),
        p.states,
        seeds.len(),
        p.root
    );

    let arena = Arena {
        params: ClassifyParams {
            max_generations: p.horizon,
            boundary_distance: p.radius - margin,
            population_cap: Some(p.pop_cap),
        },
        patch,
        distance,
        seeds,
        states: p.states,
        margin,
    };

    // Fully random initial population (the rediscovery-control condition)
    // unless explicitly iterating on a previous champion.
    // Scramble the seed (splitmix64-style) so distinct CLI seeds give
    // distinct streams; `seed | 1` (used formerly) collides even/odd
    // pairs (e.g. 4 and 5) and silently duplicates runs.
    let mut rng = (p.rng_seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x1234_5678_9ABC_DEF1) | 1;
    let mut pop: Vec<Vec<RuleRow>> = Vec::new();
    if let Some(path) = &p.seed_from {
        let text = std::fs::read_to_string(path).expect("read --seed-from record");
        let rec: ResultRecord =
            serde_json::from_str(text.lines().next().expect("empty record file"))
                .expect("parse --seed-from record");
        let table = rec.table_rule.expect("--seed-from record has no table_rule");
        assert_eq!(table.states, p.states, "--seed-from states mismatch");
        println!("seeded from {} (NOT a blind control)", path.display());
        pop.push(table.rows);
    }
    while pop.len() < p.population {
        pop.push(random_genome(&mut rng, p.states));
    }

    let mut best_ever: Option<(f64, Vec<RuleRow>, Evaluation)> = None;
    for generation in 0..p.generations {
        let scored: Vec<Evaluation> =
            pop.par_iter().map(|g| arena.evaluate(g)).collect();
        let mut order: Vec<usize> = (0..pop.len()).collect();
        order.sort_by(|&a, &b| {
            scored[b].fitness.partial_cmp(&scored[a].fitness).unwrap()
        });
        let be = &scored[order[0]];
        if best_ever.as_ref().is_none_or(|(f, ..)| be.fitness > *f) {
            best_ever = Some((be.fitness, pop[order[0]].clone(), be.clone()));
        }
        if generation % 5 == 0 || generation + 1 == p.generations {
            println!(
                "gen {generation:3}: best fitness {:.0} (travel {} maxpop {} seed '{}' {}{:?})",
                be.fitness,
                be.travel,
                be.max_population,
                arena.seeds[be.seed_idx].0,
                if be.escaped { "ESCAPE " } else { "" },
                be.outcome
            );
        }

        let elite = (p.population / 16).max(2);
        let mut next: Vec<Vec<RuleRow>> =
            order[..elite.min(pop.len())].iter().map(|&i| pop[i].clone()).collect();
        let tournament = |rng: &mut u64| -> usize {
            let mut best = order[(xorshift64(rng) as usize) % pop.len()];
            for _ in 0..2 {
                let c = order[(xorshift64(rng) as usize) % pop.len()];
                if scored[c].fitness > scored[best].fitness {
                    best = c;
                }
            }
            best
        };
        while next.len() < p.population {
            let a = tournament(&mut rng);
            let b = tournament(&mut rng);
            let mut child = crossover(&mut rng, &pop[a], &pop[b], p.states);
            // Expected ~1.5 mutations per child, as in the per-class GA.
            if xorshift64(&mut rng).is_multiple_of(2) {
                mutate(&mut rng, &mut child, p.states);
            }
            if xorshift64(&mut rng).is_multiple_of(2) {
                mutate(&mut rng, &mut child, p.states);
            }
            next.push(child);
        }
        pop = next;
    }

    let (fitness, genome, eval) = best_ever.expect("at least one generation");
    let (seed_name, (seed_cells, seed_states)) = arena.seeds[eval.seed_idx].clone();
    println!(
        "\nbest: fitness {fitness:.0}, travel {} rings, maxpop {}, seed '{seed_name}', {}{:?}",
        eval.travel,
        eval.max_population,
        if eval.escaped { "ESCAPE " } else { "" },
        eval.outcome
    );
    for row in &genome {
        println!("  {}", row_str(row));
    }

    let champion = TableRule::new(p.states, genome.clone());
    std::fs::create_dir_all(&p.out_dir).ok();
    let record = ResultRecord {
        family: p.family,
        root: p.root.clone(),
        radius: p.radius,
        rule: Rule::generations(0, 0, p.states), // display placeholder
        stratification: None,
        tables: Vec::new(),
        table_rule: Some(champion.clone()),
        neighbourhood: p.neighbourhood,
        initial_cells: seed_cells.clone(),
        initial_states: seed_states.clone(),
        generations: p.horizon,
        outcome: eval.outcome,
        max_population: eval.max_population,
        note: format!(
            "table-evolve champion travel {} {} (seed '{seed_name}')",
            eval.travel,
            match eval.outcome {
                Outcome::ReachedBoundary { .. } => "escape",
                Outcome::Died { .. } => "died",
                Outcome::Periodic { .. } => "trapped",
                Outcome::Unbounded { .. } => "unbounded",
                Outcome::Active => "active",
            }
        ),
    };
    let path = p.out_dir.join(format!(
        "{}-tableevolve-r{}-s{}.jsonl",
        format!("{:?}", p.family).to_lowercase(),
        p.radius,
        p.rng_seed
    ));
    let mut f = std::fs::File::create(&path).expect("create champion file");
    writeln!(f, "{}", serde_json::to_string(&record).unwrap()).unwrap();
    println!("champion -> {}", path.display());

    // Standard cross-radius verification (multiplicative spread): escape
    // at every radius with FLAT max-population ⇒ glider; growing
    // max-population ⇒ grower; a travel plateau ⇒ mortal.
    println!("champion verification across radii (escape + FLAT max-pop ⇒ glider):");
    for &r in &[p.radius, p.radius * 2, p.radius * 4] {
        let patch = tiling
            .generate_patch(&p.root, r, p.neighbourhood)
            .unwrap();
        let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
        let margin = neighbour_distance_margin(&patch);
        let mut engine = Engine::new(patch.graph.clone());
        for (&c, &s) in seed_cells.iter().zip(&seed_states) {
            engine.set_state(c, s);
        }
        let params = ClassifyParams {
            max_generations: u64::from(r) * 120,
            boundary_distance: r - margin,
            population_cap: None,
        };
        let rep = classify_run(&mut engine, &champion, &distance, &params);
        let escaped = matches!(rep.outcome, Outcome::ReachedBoundary { .. });
        println!(
            "  radius {r:3}: travel {:3} maxpop {:3} (inner {:3}) {}{:?}",
            rep.max_changed_distance,
            rep.max_population,
            rep.max_population_inner,
            if escaped { "ESCAPE " } else { "" },
            rep.outcome
        );
    }
}
