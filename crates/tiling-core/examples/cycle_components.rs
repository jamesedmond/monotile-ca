//! Anatomy of a periodic endgame: decompose a record's settled cycle
//! into spatially disjoint components and measure each component's own
//! period and lock-in generation. The full-patch Brent period reported
//! by classification is a property of the whole state vector; when the
//! run contains several far-apart objects (the mega-looper's two
//! wanderers), it is the lcm of the component periods and says nothing
//! about any single orbit. This tool recovers the per-object story:
//!
//!   1. find mu = first generation with state(t) = state(t + lambda0)
//!      (two engines stepping in lockstep, one lambda0 ahead);
//!   2. verify the true full-state period lambda as the minimal
//!      divisor of lambda0 at which state(mu) recurs;
//!   3. mark every cell active in [mu, mu+lambda) and split the marks
//!      into graph-connected components (the settled tracks);
//!   4. per component, hash its restricted state each generation and
//!      report the minimal divisor of lambda that is its period over a
//!      full two-cycle window, plus the generation it locked.
//!
//! Usage: cycle_components <record.jsonl> [index] [radius] [lambda0] [mu_horizon]
//!
//! lambda0 defaults to 3300 (the s32 record); pass the classified
//! period of whatever record is under study.

use ca_engine::{AnyRule, CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn divisors(n: u64) -> Vec<u64> {
    let mut d: Vec<u64> = (1..=n).filter(|k| n.is_multiple_of(*k)).collect();
    d.sort_unstable();
    d
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(192, |a| a.parse().expect("radius"));
    let lambda0: u64 = args.next().map_or(3300, |a| a.parse().expect("lambda0"));
    let mu_horizon: u64 = args.next().map_or(20_000, |a| a.parse().expect("mu_horizon"));

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
    let cells = patch.graph.cells() as usize;
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let (strata, rule) = record.replay_setup(&patch);
    let AnyRule::Table(rule) = rule else {
        panic!("cycle_components expects a table-rule record");
    };
    let seed = record.initial_state(cells as u32).unwrap();
    let fresh = || {
        let mut e = Engine::with_strata(patch.graph.clone(), strata.clone());
        e.load_state(&seed);
        e
    };

    // 1. mu: lead engine runs lambda0 ahead, then both step together.
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
    let mu = mu.unwrap_or_else(|| {
        panic!("no recurrence at offset {lambda0} within {mu_horizon} generations")
    });
    println!("mu = {mu}: state({mu}) = state({})", mu + lambda0);

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
    println!("lambda = {lambda} (full-state period; classification said {lambda0})");

    // 3. Cells active during one settled cycle -> connected components.
    // (`engine` currently sits somewhere within the cycle; restart it.)
    let mut engine = fresh();
    let mut ever_active_pre = vec![false; cells]; // transient, for contrast
    let mut max_ring = (0u32, 0u64);
    for t in 0..mu {
        for (c, &s) in engine.state().iter().enumerate() {
            if s != 0 {
                ever_active_pre[c] = true;
                if distance[c] > max_ring.0 {
                    max_ring = (distance[c], t);
                }
            }
        }
        rule.step(&mut engine);
    }
    let mut in_cycle = vec![false; cells];
    for t in 0..lambda {
        for (c, &s) in engine.state().iter().enumerate() {
            if s != 0 {
                in_cycle[c] = true;
                if distance[c] > max_ring.0 {
                    max_ring = (distance[c], mu + t);
                }
            }
        }
        rule.step(&mut engine);
    }
    println!(
        "max ring ever active: {} (at generation {}); transient-only cells: {}",
        max_ring.0,
        max_ring.1,
        (0..cells).filter(|&c| ever_active_pre[c] && !in_cycle[c]).count()
    );
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
    println!("settled cycle occupies {n_components} disjoint component(s)");

    // 4. Per-component hash sequences over [0, mu + 2*lambda], then the
    // minimal divisor-of-lambda period verified across a full
    // two-cycle window, and the generation each component locked.
    let horizon = mu + 2 * lambda;
    let members: Vec<Vec<usize>> = (0..n_components)
        .map(|j| (0..cells).filter(|&c| component[c] == j).collect())
        .collect();
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
    let mut periods = Vec::new();
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
        periods.push(period);
        println!(
            "component {j}: {} cells, rings {}..{}, period {period}, locked at generation {lock}",
            cs.len(),
            rings.iter().min().unwrap(),
            rings.iter().max().unwrap(),
        );
    }
    let lcm = periods.iter().fold(1u64, |a, &b| a / gcd(a, b) * b);
    println!("lcm of component periods = {lcm} (full-state lambda = {lambda})");
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}
