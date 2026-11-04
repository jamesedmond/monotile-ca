//! Where do loop circuits live in the hierarchy? For a trap-census launch (record, root, site,
//! partner, states) that ends periodically, decompose the settled cycle into its component
//! tracks (as `cycle_components` / `trap_census` do) and, for each track, find the smallest
//! supertile containing every cell of it: the longest common tail of the members' canonical
//! addresses. Addresses read upward, symbol p being the level-(p-1) object's slot in the
//! level-p supertile, so members agreeing at every position >= k share the level-(k-1)
//! supertile, whose type is the "of X" at position k-1. Also reported: how many of that
//! supertile's children (level k-2) the track touches, and the track's level-1 metatile
//! composition.
//!
//! Usage: track_supertiles <record.jsonl> '<root>' <radius> <horizon> <site> <partner> <sa> <sb> [--dump FILE [--margin 10] [--only PERIOD]]
//!   (one launch; prints one line per component track; --dump writes the tiles around the tracks
//!   with polygons and addresses for `crates/certify/figures/fig_track.py`)
//! or:    track_supertiles <record.jsonl> <trap.tsv> <roots.txt> <radius> <horizon> [root-index]
//!   (every distinct track of the census TSV for that root, from its first periodic launch)

use std::collections::{BTreeMap, HashMap, HashSet};

use ca_engine::classify::{ClassifyParams, Outcome, classify_run};
use ca_engine::{AnyRule, CaRule, Engine};
use tiling_core::Patch;
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn divisors(n: u64) -> Vec<u64> {
    (1..=n).filter(|k| n.is_multiple_of(*k)).collect()
}

/// Component tracks of the settled cycle: (period, member cells), after `cycle_components`.
fn tracks<F: Fn() -> Engine>(fresh: F, rule: &AnyRule, patch: &Patch, lambda0: u64, mu_horizon: u64) -> Option<(u64, Vec<(u64, Vec<usize>)>)> {
    let cells = patch.graph.cells() as usize;
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
    let mut n = 0;
    for start in 0..cells {
        if !in_cycle[start] || component[start] != usize::MAX {
            continue;
        }
        let mut queue = vec![start];
        component[start] = n;
        while let Some(c) = queue.pop() {
            for &m in patch.graph.neighbours(c as u32) {
                let m = m as usize;
                if in_cycle[m] && component[m] == usize::MAX {
                    component[m] = n;
                    queue.push(m);
                }
            }
        }
        n += 1;
    }
    let members: Vec<Vec<usize>> = (0..n).map(|j| (0..cells).filter(|&c| component[c] == j).collect()).collect();
    let horizon = mu + 2 * lambda;
    let mut hashes: Vec<Vec<u64>> = vec![Vec::with_capacity(horizon as usize + 1); n];
    let mut engine = fresh();
    for _ in 0..=horizon {
        for (j, cs) in members.iter().enumerate() {
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for &c in cs {
                let s = engine.state()[c];
                if s != 0 {
                    for &b in (c as u32).to_le_bytes().iter().chain(&[s]) {
                        h ^= u64::from(b);
                        h = h.wrapping_mul(0x0000_0100_0000_01b3);
                    }
                }
            }
            hashes[j].push(h);
        }
        rule.step(&mut engine);
    }
    let out = members
        .into_iter()
        .enumerate()
        .map(|(j, cs)| {
            let h = &hashes[j];
            let period = divisors(lambda)
                .into_iter()
                .find(|&d| (mu..=mu + lambda).all(|t| h[t as usize] == h[(t + d) as usize]))
                .unwrap();
            (period, cs)
        })
        .collect();
    Some((mu, out))
}

/// Symbols of a canonical address, the cycle expanded to at least `len` symbols.
fn symbols(address: &str, len: usize) -> Vec<String> {
    let (pre, cyc) = address.split_once(':').unwrap_or((address, ""));
    let split = |s: &str| -> Vec<String> {
        s.split(')').map(str::trim).filter(|t| !t.is_empty()).map(|t| format!("{})", t.trim_start_matches('('))).collect()
    };
    let mut v = split(pre);
    let c = split(cyc);
    if c.is_empty() {
        return v;
    }
    while v.len() < len {
        v.extend(c.iter().cloned());
    }
    v
}

fn type_of(sym: &str) -> &str {
    sym.rsplit(" of ").next().unwrap_or(sym).trim_end_matches(')')
}

/// The smallest supertile containing all the cells: (level, type, number of its children
/// touched, level-1 metatile composition).
fn common_supertile(patch: &Patch, cells: &[usize]) -> (usize, String, usize, String) {
    const L: usize = 40;
    let syms: Vec<Vec<String>> = cells.iter().map(|&c| symbols(&patch.cells[c].address, L)).collect();
    let agree = |p: usize| syms.iter().all(|s| s.get(p) == syms[0].get(p));
    let k = (0..L).find(|&k| (k..L).all(agree)).unwrap_or(L);
    let level = k.saturating_sub(1);
    let ty = if level == 0 { type_of(&syms[0][0]).to_string() } else { type_of(&syms[0][level]).to_string() };
    // children of that supertile touched: distinct symbol sequences at positions [level-1 .. ] i.e.
    // distinct slots at position `level` (which child), when level >= 1
    let children: HashSet<&String> = if level >= 1 { syms.iter().map(|s| &s[level]).collect() } else { HashSet::new() };
    let mut comp: BTreeMap<String, usize> = BTreeMap::new();
    for s in &syms {
        *comp.entry(type_of(&s[1]).to_string()).or_insert(0) += 1;
    }
    let composition = comp.iter().map(|(t, n)| format!("{t}:{n}")).collect::<Vec<_>>().join(" ");
    (level, ty, children.len(), composition)
}

/// Per level m = 2..=top: the level-m supertile instances the cells touch (identified by the
/// address tail from position m+1, typed by position m), with cell counts, largest first.
fn level_profile(patch: &Patch, cells: &[usize], top: usize) -> String {
    const L: usize = 40;
    let syms: Vec<Vec<String>> = cells.iter().map(|&c| symbols(&patch.cells[c].address, L)).collect();
    let mut lines = Vec::new();
    for m in 2..=top.min(L - 2) {
        let mut inst: HashMap<(String, Vec<String>), usize> = HashMap::new();
        for s in &syms {
            *inst.entry((type_of(&s[m]).to_string(), s[m + 1..].to_vec())).or_insert(0) += 1;
        }
        let mut v: Vec<(String, usize)> = inst.into_iter().map(|((t, _), n)| (t, n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        lines.push(format!("L{m}: {}", v.iter().map(|(t, n)| format!("{t}({n})")).collect::<Vec<_>>().join("+")));
    }
    lines.join("  ")
}

fn report(record: &ResultRecord, tiling: &Tiling, root: &str, radius: u32, horizon: u64, launches: &[(u32, u32, u8, u8)]) {
    let patch = tiling.generate_patch(root, radius, record.neighbourhood).unwrap_or_else(|e| panic!("{e:?}"));
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
    let params = ClassifyParams { max_generations: horizon, boundary_distance: radius - margin, population_cap: Some(10_000) };
    for &(site, partner, sa, sb) in launches {
        let fresh = || {
            let mut e = Engine::with_strata(patch.graph.clone(), strata.clone());
            e.set_state(site, sa);
            e.set_state(partner, sb);
            e
        };
        let mut engine = fresh();
        let r = classify_run(&mut engine, &rule, &distance, &params);
        let Outcome::Periodic { period, detected_by } = r.outcome else {
            println!("launch site {site} partner {partner} states ({sa},{sb}): {:?}", r.outcome);
            continue;
        };
        let Some((mu, tr)) = tracks(fresh, &rule, &patch, period, detected_by + period) else {
            println!("launch site {site} partner {partner}: no recurrence found");
            continue;
        };
        for (p, cs) in &tr {
            let (level, ty, nchildren, composition) = common_supertile(&patch, cs);
            let rings: Vec<u32> = cs.iter().map(|&c| distance[c]).collect();
            let anchor = cs.iter().min().unwrap();
            let tail: Vec<String> = symbols(&patch.cells[*anchor].address, 40)[level.max(1)..level + 4].to_vec();
            println!(
                "launch site {site} partner {partner} ({sa},{sb}) mu {mu}: period {p}, {} cells, rings {}-{}: smallest common supertile level {level} type {ty}, touches {nchildren} of its children; above it {} ; level-1 composition {composition}",
                cs.len(),
                rings.iter().min().unwrap(),
                rings.iter().max().unwrap(),
                tail.join("")
            );
            println!("    {}", level_profile(&patch, cs, level));
        }
    }
}

/// `--dump FILE`: every tile within `margin` rings of a track of the launch, with its polygon,
/// address, ring and track index, plus the wanderer of the shortest-period track at six phases
/// of its period, for `fig_track.py`.
#[allow(clippy::too_many_arguments)]
fn dump(record: &ResultRecord, tiling: &Tiling, root: &str, radius: u32, horizon: u64, l: (u32, u32, u8, u8), margin: u32, only: Option<u64>, file: &str) {
    use std::fmt::Write as _;
    let (patch, geom) = tiling.generate_patch_with_geometry(root, radius, record.neighbourhood).unwrap_or_else(|e| panic!("{e:?}"));
    let cells = patch.graph.cells() as usize;
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let dist = &distance;
    let mg = (0..patch.graph.cells())
        .flat_map(|c| {
            let dc = dist[c as usize];
            patch.graph.neighbours(c).iter().map(move |&n| dc.abs_diff(dist[n as usize]))
        })
        .max()
        .unwrap_or(1);
    let (strata, rule) = record.replay_setup(&patch);
    let params = ClassifyParams { max_generations: horizon, boundary_distance: radius - mg, population_cap: Some(10_000) };
    let (site, partner, sa, sb) = l;
    let fresh = || {
        let mut e = Engine::with_strata(patch.graph.clone(), strata.clone());
        e.set_state(site, sa);
        e.set_state(partner, sb);
        e
    };
    let mut engine = fresh();
    let r = classify_run(&mut engine, &rule, &distance, &params);
    let Outcome::Periodic { period, detected_by } = r.outcome else { panic!("launch is not periodic: {:?}", r.outcome) };
    let (mu, tr) = tracks(fresh, &rule, &patch, period, detected_by + period).expect("recurrence");
    let tr: Vec<(u64, Vec<usize>)> = tr.into_iter().filter(|(p, _)| only.is_none_or(|o| o == *p)).collect();
    assert!(!tr.is_empty(), "no track with the requested period");
    let mut track_of = vec![-1i64; cells];
    for (k, (_, cs)) in tr.iter().enumerate() {
        for &c in cs {
            track_of[c] = k as i64;
        }
    }
    // margin band by BFS from every track cell
    let mut m = vec![u32::MAX; cells];
    let mut q = std::collections::VecDeque::new();
    for (c, &t) in track_of.iter().enumerate() {
        if t >= 0 {
            m[c] = 0;
            q.push_back(c);
        }
    }
    while let Some(c) = q.pop_front() {
        if m[c] >= margin {
            continue;
        }
        for &n in patch.graph.neighbours(c as u32) {
            let n = n as usize;
            if m[n] == u32::MAX {
                m[n] = m[c] + 1;
                q.push_back(n);
            }
        }
    }
    // six phases of the shortest-period track's wanderer
    let (k0, (p0, cs0)) = tr.iter().enumerate().min_by_key(|(_, (p, _))| *p).unwrap();
    let in0: HashSet<usize> = cs0.iter().copied().collect();
    let mut engine = fresh();
    for _ in 0..mu {
        rule.step(&mut engine);
    }
    let mut frames: Vec<String> = Vec::new();
    for g in 0..*p0 {
        if g % (p0 / 6).max(1) == 0 && frames.len() < 6 {
            let live: Vec<String> = engine.state().iter().enumerate().filter(|(c, s)| **s != 0 && in0.contains(c)).map(|(c, s)| format!("[{c},{s}]")).collect();
            frames.push(format!("{{\"track\":{k0},\"phase\":{g},\"live\":[{}]}}", live.join(",")));
        }
        rule.step(&mut engine);
    }
    // per track, the centroid of its live cells (and of its state-1 cells) at every generation of
    // one period from mu: the wanderer's path, for heading analysis
    let cen = |c: usize| -> [f64; 2] {
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        let k = (hi - lo) as f64;
        let (mut x, mut y) = (0.0, 0.0);
        for p in &geom.xy[lo..hi] {
            x += p[0];
            y += p[1];
        }
        [x / k, y / k]
    };
    let lambda = tr.iter().map(|(p, _)| *p).max().unwrap_or(1);
    let mut engine = fresh();
    for _ in 0..mu {
        rule.step(&mut engine);
    }
    let mut paths: Vec<Vec<String>> = vec![Vec::new(); tr.len()];
    for _ in 0..lambda {
        let st = engine.state();
        for (k, (_, cs)) in tr.iter().enumerate() {
            let (mut x, mut y, mut n, mut hx, mut hy, mut hn): (f64, f64, f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for &c in cs {
                if st[c] != 0 {
                    let p = cen(c);
                    x += p[0];
                    y += p[1];
                    n += 1.0;
                    if st[c] == 1 {
                        hx += p[0];
                        hy += p[1];
                        hn += 1.0;
                    }
                }
            }
            paths[k].push(format!("[{:.3},{:.3},{:.3},{:.3}]", x / n.max(1.0), y / n.max(1.0), hx / hn.max(1.0), hy / hn.max(1.0)));
        }
        rule.step(&mut engine);
    }
    let mut s = String::new();
    write!(s, "{{\"root\":{root:?},\"radius\":{radius},\"launch\":[{site},{partner},{sa},{sb}],\"mu\":{mu},\"paths\":[{}],\"tracks\":[{}],\"cells\":[",
        paths.iter().map(|p| format!("[{}]", p.join(","))).collect::<Vec<_>>().join(","),
        tr.iter().map(|(p, cs)| format!("{{\"period\":{p},\"size\":{}}}", cs.len())).collect::<Vec<_>>().join(",")).unwrap();
    let mut n = 0;
    for c in 0..cells {
        if m[c] == u32::MAX {
            continue;
        }
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        let poly: Vec<String> = geom.xy[lo..hi].iter().map(|&[x, y]| format!("[{x:.2},{y:.2}]")).collect();
        if n > 0 {
            s.push(',');
        }
        n += 1;
        write!(s, "{{\"i\":{c},\"a\":{:?},\"d\":{},\"t\":{},\"m\":{},\"xy\":[{}]}}", patch.cells[c].address, distance[c], track_of[c], m[c], poly.join(",")).unwrap();
    }
    write!(s, "],\"frames\":[{}]}}", frames.join(",")).unwrap();
    std::fs::write(file, s).expect("write dump");
    println!("{file}: {n} cells within {margin} rings of {} track(s): {}", tr.len(), tr.iter().map(|(p, cs)| format!("p{p}/{}", cs.len())).collect::<Vec<_>>().join(", "));
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut a: Vec<String> = Vec::new();
    let (mut dump_file, mut margin, mut only) = (None::<String>, 10u32, None::<u64>);
    let mut it = raw.into_iter();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--dump" => dump_file = it.next(),
            "--margin" => margin = it.next().unwrap().parse().unwrap(),
            "--only" => only = Some(it.next().unwrap().parse().unwrap()),
            _ => a.push(x),
        }
    }
    let text = std::fs::read_to_string(&a[0]).expect("record");
    let record: ResultRecord = serde_json::from_str(text.lines().find(|l| !l.trim().is_empty()).unwrap()).expect("parse record");
    let tiling = Tiling::new(record.family);
    if a.len() >= 8 {
        let (root, radius, horizon) = (&a[1], a[2].parse().unwrap(), a[3].parse().unwrap());
        let l = (a[4].parse().unwrap(), a[5].parse().unwrap(), a[6].parse().unwrap(), a[7].parse().unwrap());
        if let Some(f) = dump_file {
            dump(&record, &tiling, root, radius, horizon, l, margin, only, &f);
            return;
        }
        report(&record, &tiling, root, radius, horizon, &[l]);
        return;
    }
    // census TSV mode: first periodic launch per distinct track of the chosen root
    let (tsv, roots_path, radius, horizon) = (&a[1], &a[2], a[3].parse::<u32>().unwrap(), a[4].parse::<u64>().unwrap());
    let root_index: usize = a.get(5).map_or(0, |s| s.parse().unwrap());
    let roots: Vec<String> = std::fs::read_to_string(roots_path)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    let mut seen: HashMap<String, (u32, u32, u8, u8)> = HashMap::new();
    let mut launches: Vec<(u32, u32, u8, u8)> = Vec::new();
    for line in std::fs::read_to_string(tsv).unwrap().lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f[0].parse::<usize>().ok() != Some(root_index) || f[7] != "periodic" {
            continue;
        }
        let l = (f[2].parse().unwrap(), f[4].parse().unwrap(), f[5].parse().unwrap(), f[6].parse().unwrap());
        let mut new = false;
        for comp in f[13].split(';').filter(|s| !s.is_empty()) {
            let track = comp.rsplit('/').next().unwrap().to_string();
            if !seen.contains_key(&track) {
                seen.insert(track, l);
                new = true;
            }
        }
        if new {
            launches.push(l);
        }
    }
    println!("root {root_index}: {} distinct tracks from {} launches", seen.len(), launches.len());
    report(&record, &tiling, &roots[root_index], radius, horizon, &launches);
}
