//! Trap census (FINDINGS open item (d), designed 2026-08-15; run 2026-10-29 for Trevorrow's
//! question about the spectre looper's periods): the loop-period spectrum of a wanderer rule
//! across terrain. A loop is a closed circuit of the terrain that a wanderer entered with the
//! right phase, so the variable that matters is the root address, not the soup. For each root in
//! a list: generate a radius-R patch; launch the record's two-cell seed at the root and at
//! satellite sites (cells at a chosen ring, spread evenly by angle), partner = every edge
//! neighbour of the site, both state orders; classify with Brent's exact cycle detection; and
//! decompose every periodic endgame into its spatially disjoint components (as
//! `cycle_components` does), reporting each component's own period, track size, ring span and
//! lock generation. The track's cell set identifies a circuit within one patch, so the same
//! circuit reached from several seeds counts once; across patches only (period, size) can be
//! compared.
//!
//! Usage:
//!   trap_census pick <hat|spectre> '<root>' <big-radius> <ring> <n>
//!       print the addresses of n cells at graph distance `ring` from the root, spread by angle
//!       (ring 0 prints the canonical root itself)
//!   trap_census run <record.jsonl> <roots.txt> [--radius 320] [--horizon 20000]
//!       [--satellites 6] [--sat-ring 60] [--pop-cap 10000] [--out FILE] [--sites C1,C2,...]
//!       roots.txt: one address per line, `#` comments; the TSV goes to --out (default stdout
//!       gets only the summaries)

use std::collections::{BTreeMap, HashSet};
use std::f64::consts::PI;
use std::io::Write as _;
use std::time::Instant;

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{AnyRule, CaRule, Engine};
use rayon::prelude::*;
use tiling_core::results::ResultRecord;
use tiling_core::{Geometry, Patch, Tiling, TilingFamily};

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn divisors(n: u64) -> Vec<u64> {
    (1..=n).filter(|k| n.is_multiple_of(*k)).collect()
}

fn centroid(geom: &Geometry, c: u32) -> [f64; 2] {
    let (a, b) = (geom.offsets[c as usize] as usize, geom.offsets[c as usize + 1] as usize);
    let k = (b - a) as f64;
    let (mut x, mut y) = (0.0, 0.0);
    for p in &geom.xy[a..b] {
        x += p[0];
        y += p[1];
    }
    [x / k, y / k]
}

/// `n` cells at graph distance `ring` from the root, one nearest each of `n` evenly spaced
/// bearings (deduplicated; fewer if the ring is sparse).
fn spread_by_angle(patch: &Patch, geom: &Geometry, ring: u32, n: usize) -> Vec<u32> {
    if ring == 0 || n == 0 {
        return if n == 0 { Vec::new() } else { vec![0] };
    }
    let c0 = centroid(geom, 0);
    let cands: Vec<(u32, f64)> = (0..patch.graph.cells())
        .filter(|&c| patch.cells[c as usize].distance == ring)
        .map(|c| {
            let p = centroid(geom, c);
            (c, (p[1] - c0[1]).atan2(p[0] - c0[0]))
        })
        .collect();
    let angdiff = |a: f64, b: f64| {
        let d = (a - b).rem_euclid(2.0 * PI);
        d.min(2.0 * PI - d)
    };
    let mut out: Vec<u32> = Vec::new();
    for k in 0..n {
        let target = -PI + 2.0 * PI * (k as f64 + 0.5) / n as f64;
        if let Some(&(c, _)) = cands
            .iter()
            .min_by(|a, b| angdiff(a.1, target).partial_cmp(&angdiff(b.1, target)).unwrap())
            && !out.contains(&c)
        {
            out.push(c);
        }
    }
    out
}

#[derive(Clone, Debug)]
struct Comp {
    period: u64,
    size: usize,
    ring_lo: u32,
    ring_hi: u32,
    lock: u64,
    track: u64,
}

/// The per-object story behind a Brent period `lambda0` detected by `detected_by`: the lock
/// generation `mu`, the true full-state period `lambda`, and the settled cycle's components with
/// their own periods (minimal divisors of `lambda` verified over a two-cycle window).
fn decompose<F: Fn() -> Engine>(
    fresh: F,
    rule: &AnyRule,
    patch: &Patch,
    distance: &[u32],
    lambda0: u64,
    mu_horizon: u64,
) -> Option<(u64, u64, Vec<Comp>)> {
    let cells = patch.graph.cells() as usize;
    // 1. mu by lockstep: lead runs lambda0 ahead.
    let (mut trail, mut lead) = (fresh(), fresh());
    for _ in 0..lambda0 {
        rule.step(&mut lead);
    }
    let mut mu = None;
    for t in 0..=mu_horizon {
        if trail.state() == lead.state() {
            mu = Some(t);
            break;
        }
        rule.step(&mut trail);
        rule.step(&mut lead);
    }
    let mu = mu?;
    // 2. True lambda: minimal divisor of lambda0 at which state(mu) recurs.
    let mut engine = fresh();
    for _ in 0..mu {
        rule.step(&mut engine);
    }
    let settled = engine.state().to_vec();
    let mut lambda = lambda0;
    for t in 1..=lambda0 {
        rule.step(&mut engine);
        if lambda0.is_multiple_of(t) && engine.state() == &settled[..] {
            lambda = t;
            break;
        }
    }
    // 3. Cells active in one settled cycle -> graph components (the tracks).
    let mut engine = fresh();
    for _ in 0..mu {
        rule.step(&mut engine);
    }
    let mut in_cycle = vec![false; cells];
    for _ in 0..lambda {
        for (c, &s) in engine.state().iter().enumerate() {
            if s != 0 {
                in_cycle[c] = true;
            }
        }
        rule.step(&mut engine);
    }
    let mut component = vec![usize::MAX; cells];
    let mut n_components = 0;
    for start in 0..cells {
        if !in_cycle[start] || component[start] != usize::MAX {
            continue;
        }
        let mut queue = vec![start];
        component[start] = n_components;
        while let Some(c) = queue.pop() {
            for &n in patch.graph.neighbours(c as u32) {
                let n = n as usize;
                if in_cycle[n] && component[n] == usize::MAX {
                    component[n] = n_components;
                    queue.push(n);
                }
            }
        }
        n_components += 1;
    }
    let members: Vec<Vec<usize>> = (0..n_components)
        .map(|j| (0..cells).filter(|&c| component[c] == j).collect())
        .collect();
    // 4. Per-component hash sequences over [0, mu + 2 lambda]; minimal period and lock.
    let horizon = mu + 2 * lambda;
    let mut hashes: Vec<Vec<u64>> = vec![Vec::with_capacity(horizon as usize + 1); n_components];
    let mut engine = fresh();
    for _ in 0..=horizon {
        for (j, cs) in members.iter().enumerate() {
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for &c in cs {
                let s = engine.state()[c];
                if s != 0 {
                    fnv(&mut h, &(c as u32).to_le_bytes());
                    fnv(&mut h, &[s]);
                }
            }
            hashes[j].push(h);
        }
        rule.step(&mut engine);
    }
    let mut comps = Vec::new();
    for (j, cs) in members.iter().enumerate() {
        let h = &hashes[j];
        let period = divisors(lambda)
            .into_iter()
            .find(|&d| (mu..=mu + lambda).all(|t| h[t as usize] == h[(t + d) as usize]))
            .expect("lambda itself always qualifies");
        let lock = (0..=mu + lambda)
            .rev()
            .find(|&t| h[t as usize] != h[(t + period) as usize])
            .map_or(0, |t| t + 1);
        let rings: Vec<u32> = cs.iter().map(|&c| distance[c]).collect();
        let mut track = 0xcbf2_9ce4_8422_2325u64;
        for &c in cs {
            fnv(&mut track, &(c as u32).to_le_bytes());
        }
        comps.push(Comp {
            period,
            size: cs.len(),
            ring_lo: *rings.iter().min().unwrap(),
            ring_hi: *rings.iter().max().unwrap(),
            lock,
            track,
        });
    }
    Some((mu, lambda, comps))
}

struct Launch {
    site: u32,
    site_ring: u32,
    partner: u32,
    sa: u8,
    sb: u8,
}

struct Row {
    outcome: &'static str,
    period: u64,
    at: u64,
    max_pop: u32,
    mu: i64,
    comps: Vec<Comp>,
}

fn flag<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T
where
    T::Err: std::fmt::Debug,
{
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse().expect("flag value"))
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("pick") => pick(&args[1..]),
        Some("run") => run(&args[1..]),
        _ => {
            eprintln!("usage: trap_census pick <family> '<root>' <big-radius> <ring> <n> | run <record.jsonl> <roots.txt> [--radius 320] [--horizon 20000] [--satellites 6] [--sat-ring 60] [--pop-cap 10000] [--out FILE]");
            std::process::exit(2);
        }
    }
}

fn pick(a: &[String]) {
    let family = match a[0].as_str() {
        "hat" => TilingFamily::Hat,
        "spectre" => TilingFamily::Spectre,
        other => panic!("unknown family {other}"),
    };
    let (root, big, ring, n): (&str, u32, u32, usize) =
        (&a[1], a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap());
    let tiling = Tiling::new(family);
    let t0 = Instant::now();
    let (patch, geom) = tiling.generate_with_geometry(root, big).unwrap_or_else(|e| panic!("{e:?}"));
    eprintln!("# pick: {} cells at radius {big} in {:.0}s; ring {ring}, {n} bearings", patch.graph.cells(), t0.elapsed().as_secs_f64());
    if ring == 0 {
        println!("{}", patch.root);
        return;
    }
    for c in spread_by_angle(&patch, &geom, ring, n) {
        println!("{}", patch.cells[c as usize].address);
    }
}

fn run(a: &[String]) {
    let record_path = &a[0];
    let roots_path = &a[1];
    let radius: u32 = flag(a, "--radius", 320);
    let horizon: u64 = flag(a, "--horizon", 20_000);
    let satellites: usize = flag(a, "--satellites", 6);
    let sat_ring: u32 = flag(a, "--sat-ring", 60);
    let pop_cap: u32 = flag(a, "--pop-cap", 10_000);
    let out_path: String = flag(a, "--out", String::new());
    // --sites C1,C2,...: launch at these cells instead of the root and satellites (one root only)
    let sites_arg: String = flag(a, "--sites", String::new());

    let text = std::fs::read_to_string(record_path).expect("read record file");
    let line = text.lines().find(|l| !l.trim().is_empty()).expect("record");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    assert_eq!(record.initial_cells.len(), 2, "expects a two-cell seed record");
    let (s0, s1) = (record.initial_states[0], record.initial_states[1]);
    let roots: Vec<String> = std::fs::read_to_string(roots_path)
        .expect("read roots")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    let tiling = Tiling::new(record.family);
    let mut out: Box<dyn std::io::Write> = if out_path.is_empty() {
        Box::new(std::io::sink())
    } else {
        Box::new(std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create out")))
    };
    writeln!(out, "root\troot_address\tsite\tsite_ring\tpartner\tsa\tsb\toutcome\tperiod\tgen\tmax_pop\tmu\tncomp\tcomponents(period/size/rings/lock/track)").unwrap();
    println!(
        "trap census: {:?} rule \"{}\", seed states ({s0},{s1}) both orders, radius {radius}, horizon {horizon}, {satellites} satellites at ring {sat_ring}, {} roots",
        record.family, record.note, roots.len()
    );

    // period -> (components, roots seen in, distinct tracks)
    let mut spectrum: BTreeMap<u64, (usize, HashSet<usize>, HashSet<(usize, u64)>)> = BTreeMap::new();
    let mut totals: BTreeMap<&'static str, usize> = BTreeMap::new();

    for (ri, root) in roots.iter().enumerate() {
        let t0 = Instant::now();
        let (patch, geom) = match tiling.generate_patch_with_geometry(root, radius, record.neighbourhood) {
            Ok(p) => p,
            Err(e) => {
                println!("root {ri}: {root}: generation failed: {e:?}");
                continue;
            }
        };
        let edge = tiling.generate(root, radius).expect("edge patch");
        let artifacts = patch.seed_artifact_cells.len();
        let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
        let dist = &distance;
        let margin = (0..patch.graph.cells())
            .flat_map(|c| {
                let dc = dist[c as usize];
                patch.graph.neighbours(c).iter().map(move |&n| dc.abs_diff(dist[n as usize]))
            })
            .max()
            .unwrap_or(1);
        let (strata, rule) = record.replay_setup(&patch);
        let params = ClassifyParams {
            max_generations: horizon,
            boundary_distance: radius - margin,
            population_cap: Some(pop_cap),
        };
        let mut sites = vec![0u32];
        sites.extend(spread_by_angle(&patch, &geom, sat_ring, satellites));
        if !sites_arg.is_empty() {
            sites = sites_arg.split(',').map(|c| c.trim().parse().expect("site cell")).collect();
        }
        let launches: Vec<Launch> = sites
            .iter()
            .flat_map(|&s| {
                edge.graph.neighbours(s).iter().flat_map(move |&n| {
                    [(s, n, s0, s1), (s, n, s1, s0)]
                })
            })
            .map(|(site, partner, sa, sb)| Launch { site, site_ring: distance[site as usize], partner, sa, sb })
            .collect();
        let gen_s = t0.elapsed().as_secs_f64();

        let rows: Vec<Row> = launches
            .par_iter()
            .map(|l| {
                let fresh = || {
                    let mut e = Engine::with_strata(patch.graph.clone(), strata.clone());
                    e.set_state(l.site, l.sa);
                    e.set_state(l.partner, l.sb);
                    e
                };
                let mut engine = fresh();
                let r = classify_run(&mut engine, &rule, &distance, &params);
                match r.outcome {
                    Outcome::Periodic { period, detected_by } => {
                        match decompose(fresh, &rule, &patch, &distance, period, detected_by + period) {
                            Some((mu, lambda, comps)) => Row { outcome: "periodic", period: lambda, at: detected_by, max_pop: r.max_population, mu: mu as i64, comps },
                            None => Row { outcome: "periodic-undecomposed", period, at: detected_by, max_pop: r.max_population, mu: -1, comps: Vec::new() },
                        }
                    }
                    Outcome::Died { generation } => Row { outcome: "died", period: 0, at: generation, max_pop: r.max_population, mu: -1, comps: Vec::new() },
                    Outcome::ReachedBoundary { generation } => Row { outcome: "escaped", period: 0, at: generation, max_pop: r.max_population, mu: -1, comps: Vec::new() },
                    Outcome::Unbounded { generation } => Row { outcome: "unbounded", period: 0, at: generation, max_pop: r.max_population, mu: -1, comps: Vec::new() },
                    Outcome::Active => Row { outcome: "active", period: 0, at: horizon, max_pop: r.max_population, mu: -1, comps: Vec::new() },
                }
            })
            .collect();

        let mut root_counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut root_tracks: BTreeMap<u64, (u64, usize, u32, u32, usize)> = BTreeMap::new(); // track -> (period, size, lo, hi, hits)
        for (l, row) in launches.iter().zip(&rows) {
            *root_counts.entry(row.outcome).or_insert(0) += 1;
            *totals.entry(row.outcome).or_insert(0) += 1;
            let comps: Vec<String> = row
                .comps
                .iter()
                .map(|c| format!("{}/{}/{}-{}/{}/{:016x}", c.period, c.size, c.ring_lo, c.ring_hi, c.lock, c.track))
                .collect();
            writeln!(
                out,
                "{ri}\t{root}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                l.site, l.site_ring, l.partner, l.sa, l.sb, row.outcome, row.period, row.at, row.max_pop, row.mu, row.comps.len(), comps.join(";")
            )
            .unwrap();
            for c in &row.comps {
                let e = spectrum.entry(c.period).or_insert_with(|| (0, HashSet::new(), HashSet::new()));
                e.0 += 1;
                e.1.insert(ri);
                e.2.insert((ri, c.track));
                let t = root_tracks.entry(c.track).or_insert((c.period, c.size, c.ring_lo, c.ring_hi, 0));
                t.4 += 1;
            }
        }
        out.flush().unwrap();
        let mut circuits: Vec<_> = root_tracks.values().collect();
        circuits.sort_by_key(|t| (t.0, t.1));
        println!(
            "root {ri} ({} cells, {artifacts} seed artifacts, patches {gen_s:.0}s, launches {:.0}s): {} launches at {} sites -> {}",
            patch.graph.cells(),
            t0.elapsed().as_secs_f64() - gen_s,
            launches.len(),
            sites.len(),
            root_counts.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")
        );
        println!(
            "  {} distinct circuits: {}",
            circuits.len(),
            circuits.iter().map(|t| format!("p{} ({} cells, rings {}-{}, x{})", t.0, t.1, t.2, t.3, t.4)).collect::<Vec<_>>().join("; ")
        );
    }
    println!("\n== period spectrum over {} roots ==", roots.len());
    println!("{:>8} {:>10} {:>6} {:>16}", "period", "components", "roots", "distinct circuits");
    for (p, (n, rs, tracks)) in &spectrum {
        println!("{p:>8} {n:>10} {:>6} {:>16}", rs.len(), tracks.len());
    }
    println!("outcomes: {}", totals.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
}
