//! Do two gliders under different rules ride the same tiles? Replay two records that share a root on
//! one vertex patch, follow in each the component heading closest to a chosen direction, and compare
//! the paths: the union of tiles each ever occupies (overlap), how far each glider's cells lie from
//! the other's union track (graph distance), the perpendicular offset between the two centroid
//! lines, and, generation by generation, whether one glider's support lies inside the other's
//! support plus k-collar at the nearest phase (the certificate's notion of "same terrain, different
//! pattern").
//!
//! Usage: track_compare <recordA.jsonl> <recordB.jsonl> [radius=256] [gens=440] [heading=225]
//!        [from-gen=60] [k=4]

use std::collections::{HashMap, HashSet, VecDeque};

use ca_engine::{CaRule, Engine};
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Patch, Tiling};

fn load(path: &str) -> ResultRecord {
    let t = std::fs::read_to_string(path).expect("record");
    serde_json::from_str(t.lines().find(|l| !l.trim().is_empty()).unwrap()).expect("parse")
}

fn components(patch: &Patch, live: &[usize]) -> Vec<Vec<usize>> {
    let set: HashSet<usize> = live.iter().copied().collect();
    let mut seen: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for &s in live {
        if seen.contains(&s) {
            continue;
        }
        let mut comp = vec![s];
        seen.insert(s);
        let mut q = vec![s];
        while let Some(c) = q.pop() {
            for &n in patch.graph.neighbours(c as u32) {
                let n = n as usize;
                if set.contains(&n) && !seen.contains(&n) {
                    seen.insert(n);
                    comp.push(n);
                    q.push(n);
                }
            }
        }
        out.push(comp);
    }
    out
}

/// Per generation from `from` to `gens`: the cells of the component whose centroid heading from
/// the root is nearest `heading` (after `from`; before it, the largest component).
fn fly(record: &ResultRecord, patch: &Patch, cen: &[[f64; 2]], gens: u64, from: u64, heading: f64) -> Vec<(u64, Vec<usize>, [f64; 2])> {
    let (strata, rule) = record.replay_setup(patch);
    let mut e = Engine::with_strata(patch.graph.clone(), strata);
    e.load_state(&record.initial_state(patch.graph.cells()).unwrap());
    let mut out = Vec::new();
    for g in 0..=gens {
        let live: Vec<usize> = e.state().iter().enumerate().filter(|(_, s)| **s != 0).map(|(c, _)| c).collect();
        if live.is_empty() {
            break;
        }
        let comps = components(patch, &live);
        let centroid = |cs: &[usize]| -> [f64; 2] {
            let n = cs.len() as f64;
            [cs.iter().map(|&c| cen[c][0]).sum::<f64>() / n, cs.iter().map(|&c| cen[c][1]).sum::<f64>() / n]
        };
        let pick = if g < from {
            comps.iter().max_by_key(|c| c.len()).unwrap()
        } else {
            comps.iter().min_by(|a, b| {
                let ha = (centroid(a)[1].atan2(centroid(a)[0]).to_degrees() - heading + 180.0).rem_euclid(360.0) - 180.0;
                let hb = (centroid(b)[1].atan2(centroid(b)[0]).to_degrees() - heading + 180.0).rem_euclid(360.0) - 180.0;
                ha.abs().partial_cmp(&hb.abs()).unwrap()
            }).unwrap()
        };
        let c = centroid(pick);
        out.push((g, pick.clone(), c));
        rule.step(&mut e);
    }
    out
}

fn dist_from(patch: &Patch, sources: &HashSet<usize>, maxd: u32) -> HashMap<usize, u32> {
    let mut d: HashMap<usize, u32> = sources.iter().map(|&c| (c, 0)).collect();
    let mut q: VecDeque<usize> = sources.iter().copied().collect();
    while let Some(c) = q.pop_front() {
        let dc = d[&c];
        if dc >= maxd {
            continue;
        }
        for &n in patch.graph.neighbours(c as u32) {
            let n = n as usize;
            if !d.contains_key(&n) {
                d.insert(n, dc + 1);
                q.push_back(n);
            }
        }
    }
    d
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (ra, rb) = (load(&a[0]), load(&a[1]));
    let radius: u32 = a.get(2).map_or(256, |s| s.parse().unwrap());
    let gens: u64 = a.get(3).map_or(440, |s| s.parse().unwrap());
    let heading: f64 = a.get(4).map_or(225.0, |s| s.parse().unwrap());
    let from: u64 = a.get(5).map_or(60, |s| s.parse().unwrap());
    let k: u32 = a.get(6).map_or(4, |s| s.parse().unwrap());
    assert_eq!(ra.root, rb.root, "records must share a root");
    let tiling = Tiling::new(ra.family);
    let (patch, geom) = tiling.generate_patch_with_geometry(&ra.root, radius, Neighbourhood::Vertex).unwrap();
    let cen: Vec<[f64; 2]> = (0..patch.cells.len())
        .map(|c| {
            let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
            let m = (hi - lo) as f64;
            [geom.xy[lo..hi].iter().map(|p| p[0]).sum::<f64>() / m, geom.xy[lo..hi].iter().map(|p| p[1]).sum::<f64>() / m]
        })
        .collect();
    let fa = fly(&ra, &patch, &cen, gens, from, heading);
    let fb = fly(&rb, &patch, &cen, gens, from, heading);
    println!("A: {} ({} gens followed); B: {} ({} gens)", ra.note, fa.len(), rb.note, fb.len());
    let ua: HashSet<usize> = fa.iter().filter(|(g, _, _)| *g >= from).flat_map(|(_, cs, _)| cs.iter().copied()).collect();
    let ub: HashSet<usize> = fb.iter().filter(|(g, _, _)| *g >= from).flat_map(|(_, cs, _)| cs.iter().copied()).collect();
    let inter = ua.intersection(&ub).count();
    println!("union tracks from generation {from}: A {} tiles, B {} tiles, shared {} (Jaccard {:.3})", ua.len(), ub.len(), inter, inter as f64 / (ua.len() + ub.len() - inter) as f64);
    let da = dist_from(&patch, &ua, 8);
    let db = dist_from(&patch, &ub, 8);
    for (name, u, d) in [("B's tiles from A's track", &ub, &da), ("A's tiles from B's track", &ua, &db)] {
        let mut hist = [0usize; 10];
        for &c in u {
            hist[d.get(&c).map_or(9, |&x| x.min(9) as usize)] += 1;
        }
        println!("  {name}: distance 0 {} | 1 {} | 2 {} | 3 {} | 4 {} | >4 {}", hist[0], hist[1], hist[2], hist[3], hist[4], hist[5..].iter().sum::<usize>());
    }
    // centroid lines: heading and perpendicular offset of B's centroids from A's fitted line
    let fit = |f: &[(u64, Vec<usize>, [f64; 2])]| -> ([f64; 2], [f64; 2]) {
        let pts: Vec<[f64; 2]> = f.iter().filter(|(g, _, _)| *g >= from).map(|(_, _, c)| *c).collect();
        let n = pts.len() as f64;
        let m = [pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n];
        let (sxx, sxy, syy) = pts.iter().fold((0.0, 0.0, 0.0), |(a, b, c), p| {
            let (x, y) = (p[0] - m[0], p[1] - m[1]);
            (a + x * x, b + x * y, c + y * y)
        });
        let ang = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        (m, [ang.cos(), ang.sin()])
    };
    let (ma, ua_dir) = fit(&fa);
    let (mb, ub_dir) = fit(&fb);
    let perp = |p: [f64; 2], m: [f64; 2], u: [f64; 2]| -> f64 { -(p[0] - m[0]) * u[1] + (p[1] - m[1]) * u[0] };
    let offs: Vec<f64> = fb.iter().filter(|(g, _, _)| *g >= from).map(|(_, _, c)| perp(*c, ma, ua_dir)).collect();
    let offa: Vec<f64> = fa.iter().filter(|(g, _, _)| *g >= from).map(|(_, _, c)| perp(*c, ma, ua_dir)).collect();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let rms = |v: &[f64], m: f64| (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64).sqrt();
    println!(
        "centroid lines: A heading {:.3} deg, B heading {:.3} deg; B's centroids sit {:+.2} u (rms {:.2}) across A's line; A's own scatter rms {:.2} u",
        ua_dir[1].atan2(ua_dir[0]).to_degrees().rem_euclid(360.0),
        ub_dir[1].atan2(ub_dir[0]).to_degrees().rem_euclid(360.0),
        mean(&offs),
        rms(&offs, mean(&offs)),
        rms(&offa, mean(&offa))
    );
    let _ = mb;
    // generation by generation: B's support inside A's support + k-collar at A's nearest phase, and vice versa
    let contain = |x: &[(u64, Vec<usize>, [f64; 2])], y: &[(u64, Vec<usize>, [f64; 2])], label: &str| {
        let mut ok = 0;
        let mut tot = 0;
        let mut worst = 0usize;
        for (g, cs, c) in x.iter().filter(|(g, _, _)| *g >= from) {
            let (_, ys, _) = y
                .iter()
                .filter(|(gy, _, _)| *gy >= from)
                .min_by(|p, q| {
                    let dp = (p.2[0] - c[0]).hypot(p.2[1] - c[1]);
                    let dq = (q.2[0] - c[0]).hypot(q.2[1] - c[1]);
                    dp.partial_cmp(&dq).unwrap()
                })
                .unwrap();
            let yset: HashSet<usize> = ys.iter().copied().collect();
            let d = dist_from(&patch, &yset, k);
            let outside = cs.iter().filter(|c| !d.contains_key(c)).count();
            tot += 1;
            if outside == 0 {
                ok += 1;
            }
            worst = worst.max(outside);
            let _ = g;
        }
        println!("  {label}: inside at {ok} of {tot} generations (worst case {worst} cells outside the {k}-collar)");
    };
    contain(&fb, &fa, "B's support within A's support + collar at A's nearest phase");
    contain(&fa, &fb, "A's support within B's support + collar at B's nearest phase");
    // generation by generation at equal times: populations, shared cells, and the state pairs on
    // shared cells (is one pattern the other relabelled?), plus the states of the unshared cells
    let (strata_a, rule_a) = ra.replay_setup(&patch);
    let (strata_b, rule_b) = rb.replay_setup(&patch);
    let mut ea = Engine::with_strata(patch.graph.clone(), strata_a);
    let mut eb = Engine::with_strata(patch.graph.clone(), strata_b);
    ea.load_state(&ra.initial_state(patch.graph.cells()).unwrap());
    eb.load_state(&rb.initial_state(patch.graph.cells()).unwrap());
    let mut same_support = 0;
    let mut b_in_a = 0;
    let mut a_in_b = 0;
    let mut n = 0;
    let mut pairs: HashMap<(u8, u8), usize> = HashMap::new();
    let mut only_a: HashMap<u8, usize> = HashMap::new();
    let mut only_b: HashMap<u8, usize> = HashMap::new();
    let mut pops: Vec<(usize, usize, usize)> = Vec::new();
    for g in 0..=gens {
        if g >= from && (g as usize) < fa.len() && (g as usize) < fb.len() {
            let sa: HashSet<usize> = fa[g as usize].1.iter().copied().collect();
            let sb: HashSet<usize> = fb[g as usize].1.iter().copied().collect();
            let shared = sa.intersection(&sb).count();
            if sa == sb {
                same_support += 1;
            }
            if sb.is_subset(&sa) {
                b_in_a += 1;
            }
            if sa.is_subset(&sb) {
                a_in_b += 1;
            }
            for &c in sa.intersection(&sb) {
                *pairs.entry((ea.state()[c], eb.state()[c])).or_insert(0) += 1;
            }
            for &c in sa.difference(&sb) {
                *only_a.entry(ea.state()[c]).or_insert(0) += 1;
            }
            for &c in sb.difference(&sa) {
                *only_b.entry(eb.state()[c]).or_insert(0) += 1;
            }
            n += 1;
            if pops.len() < 6 {
                pops.push((sa.len(), sb.len(), shared));
            }
        }
        rule_a.step(&mut ea);
        rule_b.step(&mut eb);
    }
    let mut pv: Vec<_> = pairs.into_iter().collect();
    pv.sort();
    let mut oa: Vec<_> = only_a.into_iter().collect();
    oa.sort();
    let mut ob: Vec<_> = only_b.into_iter().collect();
    ob.sort();
    println!(
        "at equal generations ({n}): identical supports {same_support}; B's support inside A's {b_in_a}; A's inside B's {a_in_b}; first generations (|A|, |B|, shared) {pops:?}"
    );
    println!("  state pairs (A state, B state) on shared cells, summed over generations: {pv:?}");
    println!("  states of cells only in A: {oa:?}; only in B: {ob:?}");
}
