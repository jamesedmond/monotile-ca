//! Evolutionary search for a glider via full tile-class stratified rules.
//!
//! Motivation (FINDINGS §6.1): the bounded-travel record rose 23→28→37
//! rings as rule expressiveness grew (uniform → chirality-stratified);
//! the residue that kills every traveller is mixed-class, so finer (full
//! per-class) rules are the lever. The per-class rule space is far past
//! enumeration (~2^(7·n_classes)), so this is a genetic algorithm.
//!
//! Genome: one [`Rule`] per tile class (a `Vec<Rule>`). Fitness: ignite a
//! known structure, classify exactly, and reward raw bounded-population
//! travel distance — no boundary bonus, which would let the GA game its
//! own patch edge (a mortal travelling ≥ radius looks like an escape).
//! The population cap (explosion filter) gives growing structures fitness
//! 0; a genuine glider is confirmed only by the cross-radius check at the
//! end (travel must keep growing with the patch). Crossover swaps whole
//! class tables between parents (classes are independent units); mutation
//! flips single birth/survival bits. Deterministic given a seed.

use std::io::Write as _;
use std::path::Path;

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{Engine, Rule, StratifiedRule};
use rayon::prelude::*;
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Patch, Stratification, Tiling, TilingFamily};

/// Default population cap above which a run is a growing blob rather than
/// a bounded object. A tight cap (`--pop-cap`) turns the search into a
/// *balance-point* hunt: growers exceed it (fitness 0), dyers travel
/// little, so the gradient points at bounded travellers — the death↔
/// growth boundary where a non-trapping glider would sit.
pub const DEFAULT_POP_CAP: u32 = 128;

pub struct EvolveParams {
    pub family: TilingFamily,
    pub root: String,
    /// Adjacency relation (§10.5 ablation: Generations rules on the
    /// vertex neighbourhood). Edge preserves all pre-§10.5 behaviour.
    pub neighbourhood: Neighbourhood,
    pub seed_cells: Vec<u32>,
    pub radius: u32,
    pub horizon: u64,
    pub population: usize,
    pub generations: usize,
    /// Number of CA states `k` (Generations family). `k = 2` is Life;
    /// `k >= 3` adds dying phases (the Penrose-glider regime).
    pub states: u8,
    /// Population cap (the explosion filter / balance-point knob).
    pub pop_cap: u32,
    /// Balance-point objective: reward travel weighted by *flatness*
    /// (inner-phase vs overall max population). A flat-population traveller
    /// (glider) scores ~travel; a grower whose population climbs in the
    /// outer half is discounted toward travel/2. Defeats the slow-grower
    /// gaming that a fixed cap alone allows.
    pub balance: bool,
    /// Optional champion record to seed the population from (e.g. the
    /// grower, for a balance-point search around it).
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

/// Evaluation context shared (read-only) across all fitness calls.
struct Arena {
    patch: Patch,
    strata: Vec<u8>,
    distance: Vec<u32>,
    n_classes: usize,
    params: ClassifyParams,
    seed_cells: Vec<u32>,
    balance: bool,
}

struct Evaluation {
    fitness: f64,
    travel: u32,
    max_population: u32,
    escaped: bool,
    outcome: Outcome,
}

impl Arena {
    fn evaluate(&self, genome: &[Rule]) -> Evaluation {
        let rule = StratifiedRule::new(genome.to_vec());
        let mut engine =
            Engine::with_strata(self.patch.graph.clone(), self.strata.clone());
        for &c in &self.seed_cells {
            engine.set_cell(c, true);
        }
        let r = classify_run(&mut engine, &rule, &self.distance, &self.params);
        let escaped = matches!(r.outcome, Outcome::ReachedBoundary { .. });
        // Unbounded growth (a blob) is worthless; among bounded runs,
        // reward raw travel distance. Deliberately NO boundary bonus: a
        // bonus for "reached radius R" lets the GA game its own patch
        // edge (a mortal travelling ≥ R looks like an escape), so fitness
        // is pure distance and escape just means travel == radius. A
        // genuine glider is then confirmed only by verifying that travel
        // keeps growing with the patch (see the cross-radius check).
        let cap = self.params.population_cap.unwrap_or(u32::MAX);
        let fitness = if r.max_population > cap {
            0.0
        } else if self.balance {
            // travel × flatness: a flat-population traveller (glider)
            // keeps ~all its travel; a grower whose population climbs in
            // the outer half is discounted toward travel/2.
            let flatness =
                f64::from(r.max_population_inner) / f64::from(r.max_population.max(1));
            f64::from(r.max_changed_distance) * flatness
        } else {
            f64::from(r.max_changed_distance)
        };
        Evaluation {
            fitness,
            travel: r.max_changed_distance,
            max_population: r.max_population,
            escaped,
            outcome: r.outcome,
        }
    }
}

/// All single-bit flips a mutation may apply: (class, field 0=birth/1=
/// survival, bit). Birth excludes bit 0 (no B0), survival includes 0.
fn flip_ops(n_classes: usize, max_degree: u32) -> Vec<(usize, u8, u32)> {
    let mut ops = Vec::new();
    for class in 0..n_classes {
        for bit in 1..=max_degree {
            ops.push((class, 0u8, bit));
        }
        for bit in 0..=max_degree {
            ops.push((class, 1u8, bit));
        }
    }
    ops
}

fn apply_flip(genome: &mut [Rule], &(class, field, bit): &(usize, u8, u32)) {
    if field == 0 {
        genome[class].birth ^= 1 << bit;
    } else {
        genome[class].survival ^= 1 << bit;
    }
}

/// Seed genomes from the strongest known leads, expanded to per-class.
/// Spectre: uniform filament B2/S256, and the chirality champion (plain
/// spectres B267/S25, Mystic/Gamma B2/S256). Hat: the uniform walker
/// B25/S25, and a variant weakening antihat survival (the §6 lever).
fn seed_genomes(arena: &Arena, family: TilingFamily, states: u8) -> Vec<Vec<Rule>> {
    let g = |b: u32, s: u32| Rule::generations(b, s, states);
    match family {
        TilingFamily::Spectre => {
            let filament = g(Rule::mask(&[2]), Rule::mask(&[2, 5, 6]));
            let plain = g(Rule::mask(&[2, 6, 7]), Rule::mask(&[2, 5]));
            let champion: Vec<Rule> = arena
                .patch
                .classes
                .iter()
                .map(|c| if c.parent == "Gamma" { filament } else { plain })
                .collect();
            vec![vec![filament; arena.n_classes], champion]
        }
        TilingFamily::Hat => {
            let walker = g(Rule::mask(&[2, 5]), Rule::mask(&[2, 5]));
            let weak = g(Rule::mask(&[2, 5]), Rule::mask(&[5]));
            let antihat_weak: Vec<Rule> = arena
                .patch
                .classes
                .iter()
                .map(|c| if c.base == "antihat" { weak } else { walker })
                .collect();
            vec![vec![walker; arena.n_classes], antihat_weak]
        }
        // Penrose (§9 control): no known leads; generic lively starters
        // for the 4-neighbour substrate (counts never exceed 4).
        TilingFamily::PenroseP2 | TilingFamily::PenroseP3 => vec![
            vec![g(Rule::mask(&[2]), Rule::mask(&[2, 3])); arena.n_classes],
            vec![g(Rule::mask(&[2, 3]), Rule::mask(&[1, 2])); arena.n_classes],
        ],
    }
}

/// Boundary distance for classification: the exact patch radius on the
/// edge neighbourhood (pre-§10.5 behaviour, all historic runs), minus
/// the measured fan margin on vertex (outer rings have incomplete
/// vertex neighbourhoods).
fn boundary_distance(patch: &Patch, radius: u32) -> u32 {
    if patch.neighbourhood == Neighbourhood::Edge {
        return radius;
    }
    let margin = (0..patch.graph.cells())
        .flat_map(|c| {
            let dc = patch.cells[c as usize].distance;
            patch
                .graph
                .neighbours(c)
                .iter()
                .map(move |&n| dc.abs_diff(patch.cells[n as usize].distance))
        })
        .max()
        .unwrap_or(1);
    radius - margin
}

pub fn run(p: &EvolveParams) {
    let tiling = Tiling::new(p.family);
    let patch = tiling
        .generate_patch(&p.root, p.radius, p.neighbourhood)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty(), "degenerate root");
    let (strata, n_classes) = patch.strata(Stratification::PerClass);
    let max_degree =
        (0..patch.graph.cells()).map(|c| patch.graph.degree(c)).max().unwrap();
    let distance = patch.cells.iter().map(|c| c.distance).collect();
    // Empty seed ⇒ the closed 1-ball of the root cell.
    let seed_cells = if p.seed_cells.is_empty() {
        let mut s = vec![0u32];
        s.extend(patch.graph.neighbours(0));
        s
    } else {
        p.seed_cells.clone()
    };
    let boundary = boundary_distance(&patch, p.radius);
    let arena = Arena {
        patch,
        strata,
        distance,
        n_classes,
        // The population cap aborts growing structures early (the
        // explosion filter), both penalising them (fitness 0) and keeping
        // the GA fast. Only bites if the run lasts long enough to grow
        // past it — hence a generous evaluation radius (see below).
        params: ClassifyParams {
            max_generations: p.horizon,
            boundary_distance: boundary,
            population_cap: Some(p.pop_cap),
        },
        seed_cells,
        balance: p.balance,
    };
    let ops = flip_ops(n_classes, max_degree);
    println!(
        "evolve {:?} ({:?}): {n_classes} classes, max degree {max_degree}, {} states, {} genome bits, root {}",
        p.family,
        p.neighbourhood,
        p.states,
        ops.len(),
        p.root
    );

    // Initial population: known-good seeds, then mutated copies of them.
    // Scramble the seed (splitmix64-style) so distinct CLI seeds give
    // distinct streams; `seed | 1` (used formerly) collides even/odd
    // pairs (e.g. 4 and 5) and silently duplicates runs.
    let mut rng = (p.rng_seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x1234_5678_9ABC_DEF1) | 1;
    let mut seeds = seed_genomes(&arena, p.family, p.states);
    // Optionally seed from a committed champion (e.g. the grower, for a
    // balance-point search around it). Its table count must match.
    if let Some(path) = &p.seed_from {
        let text = std::fs::read_to_string(path).expect("read --seed-from record");
        let rec: ResultRecord =
            serde_json::from_str(text.lines().next().expect("empty record file"))
                .expect("parse --seed-from record");
        assert_eq!(
            rec.tables.len(),
            n_classes,
            "--seed-from rule has {} tables, patch has {n_classes} classes",
            rec.tables.len()
        );
        println!("seeded from {} ({} tables)", path.display(), rec.tables.len());
        seeds.push(rec.tables);
    }
    let mut pop: Vec<Vec<Rule>> = seeds.clone();
    while pop.len() < p.population {
        let mut g = seeds[(xorshift64(&mut rng) as usize) % seeds.len()].clone();
        let n_mut = 1 + (xorshift64(&mut rng) % 4);
        for _ in 0..n_mut {
            apply_flip(&mut g, &ops[(xorshift64(&mut rng) as usize) % ops.len()]);
        }
        pop.push(g);
    }

    let mut best_ever: Option<(f64, Vec<Rule>, Evaluation)> = None;
    for generation in 0..p.generations {
        // Parallel fitness over the whole population.
        let scored: Vec<(f64, Evaluation)> =
            pop.par_iter().map(|g| {
                let e = arena.evaluate(g);
                (e.fitness, e)
            }).collect();
        let mut order: Vec<usize> = (0..pop.len()).collect();
        order.sort_by(|&a, &b| scored[b].0.partial_cmp(&scored[a].0).unwrap());

        let best_idx = order[0];
        let (bf, ref be) = scored[best_idx];
        if best_ever.as_ref().is_none_or(|(f, ..)| bf > *f) {
            best_ever = Some((bf, pop[best_idx].clone(), arena.evaluate(&pop[best_idx])));
        }
        if generation % 5 == 0 || generation + 1 == p.generations {
            println!(
                "gen {generation:3}: best fitness {bf:.0} (travel {} maxpop {} {}{:?})",
                be.travel,
                be.max_population,
                if be.escaped { "ESCAPE " } else { "" },
                be.outcome
            );
        }
        if be.escaped {
            println!("  ESCAPE at generation {generation} — bounded object crossed the patch!");
        }

        // Next generation: elitism + tournament selection + crossover + mutation.
        let elite = (p.population / 16).max(2);
        let mut next: Vec<Vec<Rule>> = order[..elite].iter().map(|&i| pop[i].clone()).collect();
        let tournament = |rng: &mut u64| -> usize {
            let mut best = order[(xorshift64(rng) as usize) % pop.len()];
            for _ in 0..2 {
                let c = order[(xorshift64(rng) as usize) % pop.len()];
                if scored[c].0 > scored[best].0 {
                    best = c;
                }
            }
            best
        };
        while next.len() < p.population {
            let a = tournament(&mut rng);
            let b = tournament(&mut rng);
            // Per-class uniform crossover.
            let mut child: Vec<Rule> = (0..n_classes)
                .map(|c| if xorshift64(&mut rng) & 1 == 0 { pop[a][c] } else { pop[b][c] })
                .collect();
            // Mutation: expected ~1.5 flips.
            if xorshift64(&mut rng).is_multiple_of(2) {
                apply_flip(&mut child, &ops[(xorshift64(&mut rng) as usize) % ops.len()]);
            }
            if xorshift64(&mut rng).is_multiple_of(2) {
                apply_flip(&mut child, &ops[(xorshift64(&mut rng) as usize) % ops.len()]);
            }
            next.push(child);
        }
        pop = next;
    }

    let (fitness, genome, eval) = best_ever.expect("at least one generation");
    let counts = |m: u32| (0..8).filter(|i| m >> i & 1 != 0).map(|i| i.to_string()).collect::<String>();
    println!(
        "\nbest: fitness {fitness:.0}, travel {} rings, maxpop {}, {}{:?}",
        eval.travel,
        eval.max_population,
        if eval.escaped { "ESCAPE " } else { "" },
        eval.outcome
    );
    for (i, r) in genome.iter().enumerate() {
        if *r != genome.first().copied().unwrap_or(*r) || i == 0 {
            println!("  class {i:2} {}: B{}/S{}", arena.patch.classes[i].name, counts(r.birth), counts(r.survival));
        }
    }

    // Persist the champion as a replayable stratified record.
    std::fs::create_dir_all(&p.out_dir).ok();
    let record = ResultRecord {
        family: p.family,
        root: p.root.clone(),
        radius: p.radius,
        rule: genome[0],
        stratification: Some(Stratification::PerClass),
        tables: genome,
        table_rule: None,
        neighbourhood: p.neighbourhood,
        // The *resolved* seed (an empty input means the 1-ball, expanded
        // inside the arena) — `p.seed_cells` may be empty and would make
        // an unreplayable record.
        initial_cells: arena.seed_cells.clone(),
        initial_states: Vec::new(),
        generations: p.horizon,
        outcome: eval.outcome,
        max_population: eval.max_population,
        note: format!(
            "evolve-champion travel {} {}",
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
        "{}-evolve-r{}-s{}.jsonl",
        format!("{:?}", p.family).to_lowercase(),
        p.radius,
        p.rng_seed
    ));
    write_record(&path, &record);
    println!("champion -> {}", path.display());

    // A bounded run reaching a radius-R boundary only proves travel ≥ R;
    // the 37-ring mortal traveller "escapes" any patch with R ≤ 37. A
    // real glider escapes at EVERY radius, so re-verify the champion on
    // progressively larger patches: monotone escape ⇒ glider; a plateau
    // ⇒ mortal, trapping at its true travel distance.
    // Multiplicative radius spread: a glider's travel grows with R while
    // its population stays flat; a slow grower's population creeps up over
    // a wide span (tightly-spaced radii can miss it). escape at all radii
    // WITH flat max-population ⇒ glider; growing max-population ⇒ grower.
    println!("champion verification across radii (escape + FLAT max-pop ⇒ glider):");
    let tiling = Tiling::new(p.family);
    for &r in &[p.radius, p.radius * 2, p.radius * 4] {
        let patch = tiling.generate_patch(&p.root, r, p.neighbourhood).unwrap();
        let (strata, _) = patch.strata(Stratification::PerClass);
        let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
        let seed = if p.seed_cells.is_empty() {
            let mut s = vec![0u32];
            s.extend(patch.graph.neighbours(0));
            s
        } else {
            p.seed_cells.clone()
        };
        let mut engine = Engine::with_strata(patch.graph.clone(), strata);
        for &c in &seed {
            engine.set_cell(c, true);
        }
        let rule = StratifiedRule::new(record.tables.clone());
        // Horizon scaled so a boundary-reaching traveller has time to
        // arrive; no population cap here — we want to SEE growth, which is
        // exactly what distinguishes a glider from a slow filament.
        let params = ClassifyParams {
            max_generations: u64::from(r) * 120,
            boundary_distance: boundary_distance(&patch, r),
            population_cap: None,
        };
        let rep = classify_run(&mut engine, &rule, &distance, &params);
        let escaped = matches!(rep.outcome, Outcome::ReachedBoundary { .. });
        // inner→overall max-pop: equal ⇒ flat (glider); rising ⇒ grower.
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

fn write_record(path: &Path, record: &ResultRecord) {
    let mut f = std::fs::File::create(path).expect("create champion file");
    writeln!(f, "{}", serde_json::to_string(record).unwrap()).unwrap();
}
