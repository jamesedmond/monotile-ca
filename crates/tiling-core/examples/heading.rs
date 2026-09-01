//! Heading analysis for travelling objects (§10.1 follow-up): replay a
//! record, track each connected component of active cells as an object,
//! and quantify heading quantization against the substrate's symmetry
//! fan (6-/12-fold for hat/spectre, 10-fold for Penrose) plus
//! transverse wobble about each object's best-fit line.
//!
//! Offset-free test: pairwise heading *differences* between objects
//! must be multiples of the fan step if headings are compass-locked
//! (no fan-orientation fit needed). Fan-offset fits are also reported
//! (brute-force φ minimising mean circular residual of windowed
//! headings).
//!
//! Usage: heading <record.jsonl> [index] [radius] [horizon] [window]
//!        [cluster_units] [match_units]

use ca_engine::{CaRule, Engine};
use std::collections::VecDeque;
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

/// Thresholds in units of the substrate's mean neighbour-centroid
/// spacing (tile sizes differ hugely between families).
const MATCH_UNITS: f64 = 4.0;
/// Graph-components closer than this are one object (relay mechanisms
/// transiently fragment into nearby islands each generation).
const CLUSTER_UNITS: f64 = 6.0;

struct Object {
    id: usize,
    /// (generation, centroid) samples while alive.
    path: Vec<(u64, [f64; 2])>,
    dead: Option<u64>,
}

fn angle_deg(v: [f64; 2]) -> f64 {
    let a = v[1].atan2(v[0]).to_degrees();
    if a < 0.0 { a + 360.0 } else { a }
}

/// Circular residual of `a` against the fan {φ + k·step}.
fn fan_residual(a: f64, phi: f64, step: f64) -> f64 {
    let r = (a - phi).rem_euclid(step);
    r.min(step - r)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(128, |a| a.parse().expect("radius"));
    let horizon: u64 = args.next().map_or(20000, |a| a.parse().expect("horizon"));
    let window: usize = args.next().map_or(32, |a| a.parse().expect("window"));
    let cluster_units: f64 =
        args.next().map_or(CLUSTER_UNITS, |a| a.parse().expect("cluster_units"));
    let match_units: f64 =
        args.next().map_or(MATCH_UNITS, |a| a.parse().expect("match_units"));

    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(index)
        .expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    println!(
        "heading analysis: {:?} radius {radius} window {window} — {}",
        record.family, record.note
    );

    let tiling = Tiling::new(record.family);
    let (patch, geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let centroid: Vec<[f64; 2]> = (0..patch.graph.cells() as usize)
        .map(|c| {
            let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
            let n = (hi - lo) as f64;
            let (sx, sy) = geom.xy[lo..hi]
                .iter()
                .fold((0.0, 0.0), |(ax, ay), &[x, y]| (ax + x, ay + y));
            [sx / n, sy / n]
        })
        .collect();
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

    // Substrate length unit: mean centroid distance across graph edges.
    let (mut edge_sum, mut edge_n) = (0.0f64, 0u64);
    for c in 0..patch.graph.cells() {
        let pc = centroid[c as usize];
        for &nb in patch.graph.neighbours(c) {
            if nb > c {
                let pn = centroid[nb as usize];
                edge_sum += (pc[0] - pn[0]).hypot(pc[1] - pn[1]);
                edge_n += 1;
            }
        }
    }
    let unit = edge_sum / edge_n as f64;
    let match_d = match_units * unit;
    let cluster_d = cluster_units * unit;
    println!("length unit (mean neighbour spacing): {unit:.2}; cluster {cluster_d:.1}, match {match_d:.1}");

    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());

    // --- track connected components of the active set ---
    let mut objects: Vec<Object> = Vec::new();
    let mut visited = vec![false; patch.graph.cells() as usize];
    for generation in 0..horizon {
        let active: Vec<u32> = (0..patch.graph.cells())
            .filter(|&c| engine.state()[c as usize] != 0)
            .collect();
        if active.is_empty() {
            println!("population died at generation {generation}");
            break;
        }
        if active.iter().any(|&c| distance[c as usize] + margin > radius) {
            println!("boundary contact at generation {generation}");
            break;
        }
        // Connected components over the patch graph.
        for &c in &active {
            visited[c as usize] = false;
        }
        let mut comps: Vec<([f64; 2], f64)> = Vec::new();
        for &start in &active {
            if visited[start as usize] {
                continue;
            }
            let mut queue = VecDeque::from([start]);
            visited[start as usize] = true;
            let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
            while let Some(c) = queue.pop_front() {
                sx += centroid[c as usize][0];
                sy += centroid[c as usize][1];
                n += 1.0;
                for &nb in patch.graph.neighbours(c) {
                    if engine.state()[nb as usize] != 0 && !visited[nb as usize] {
                        visited[nb as usize] = true;
                        queue.push_back(nb);
                    }
                }
            }
            comps.push(([sx / n, sy / n], n));
        }
        // Single-linkage agglomeration: nearby components are one object.
        let mut centroids: Vec<[f64; 2]> = Vec::new();
        let mut weights: Vec<f64> = Vec::new();
        for (p, w) in comps {
            let near = centroids.iter().position(|q| {
                (p[0] - q[0]).hypot(p[1] - q[1]) < cluster_d
            });
            match near {
                Some(i) => {
                    let t = weights[i] + w;
                    centroids[i] = [
                        (centroids[i][0] * weights[i] + p[0] * w) / t,
                        (centroids[i][1] * weights[i] + p[1] * w) / t,
                    ];
                    weights[i] = t;
                }
                None => {
                    centroids.push(p);
                    weights.push(w);
                }
            }
        }
        // Greedy nearest matching to live objects.
        let mut claimed = vec![false; centroids.len()];
        for obj in objects.iter_mut().filter(|o| o.dead.is_none()) {
            let last = obj.path.last().unwrap().1;
            let best = centroids
                .iter()
                .enumerate()
                .filter(|(i, _)| !claimed[*i])
                .map(|(i, p)| (i, (p[0] - last[0]).hypot(p[1] - last[1])))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            match best {
                Some((i, d)) if d <= match_d => {
                    claimed[i] = true;
                    obj.path.push((generation, centroids[i]));
                }
                _ => obj.dead = Some(generation),
            }
        }
        for (i, &p) in centroids.iter().enumerate() {
            if !claimed[i] {
                let id = objects.len();
                objects.push(Object { id, path: vec![(generation, p)], dead: None });
            }
        }
        rule.step(&mut engine);
    }

    // --- per-object report ---
    let long: Vec<&Object> = objects
        .iter()
        .filter(|o| o.path.len() >= 3 * window)
        .collect();
    println!(
        "{} objects tracked, {} long-lived (>= {} generations):\n",
        objects.len(),
        long.len(),
        3 * window
    );
    let mut headings: Vec<(usize, f64)> = Vec::new();
    let mut all_windowed: Vec<f64> = Vec::new();
    for obj in &long {
        let (g0, p0) = obj.path[0];
        let (g1, p1) = *obj.path.last().unwrap();
        let net = [p1[0] - p0[0], p1[1] - p0[1]];
        let disp = net[0].hypot(net[1]);
        let heading = angle_deg(net);
        headings.push((obj.id, heading));
        // Windowed headings.
        let mut windows: Vec<f64> = Vec::new();
        let mut i = 0;
        while i + window < obj.path.len() {
            let a = obj.path[i].1;
            let b = obj.path[i + window].1;
            windows.push(angle_deg([b[0] - a[0], b[1] - a[1]]));
            i += window;
        }
        all_windowed.extend(&windows);
        // Transverse deviation about the net-displacement line through p0.
        let axis = [net[0] / disp, net[1] / disp];
        let mut devs: Vec<f64> = obj
            .path
            .iter()
            .map(|(_, p)| (p[0] - p0[0]) * (-axis[1]) + (p[1] - p0[1]) * axis[0])
            .collect();
        let rms = |v: &[f64]| (v.iter().map(|d| d * d).sum::<f64>() / v.len() as f64).sqrt();
        let half = devs.len() / 2;
        let (rms1, rms2) = (rms(&devs[..half]), rms(&devs[half..]));
        let max_dev = devs.iter().cloned().fold(0.0f64, |m, d| m.max(d.abs()));
        devs.clear();
        println!(
            "object {:2}: gens {}..{}{}, displacement {:6.1}, speed {:.3}/gen, heading {:7.2}°",
            obj.id,
            g0,
            g1,
            obj.dead.map_or(" (alive)".into(), |d| format!(" (died {d})")),
            disp,
            disp / (g1 - g0) as f64,
            heading
        );
        println!(
            "            windowed headings: {}",
            windows.iter().map(|h| format!("{h:.1}")).collect::<Vec<_>>().join(" ")
        );
        // Settled heading: circular mean of the last few windows — the
        // lane direction, free of launch-transient bias in the net
        // start->end displacement.
        if windows.len() >= 3 {
            let tail = &windows[windows.len().saturating_sub(5)..];
            let (s, c) = tail.iter().fold((0.0f64, 0.0f64), |(s, c), &h| {
                let r = h.to_radians();
                (s + r.sin(), c + r.cos())
            });
            let settled = angle_deg([c, s]);
            println!(
                "            settled heading (last {} windows): {settled:.2}°",
                tail.len()
            );
        }
        println!(
            "            transverse: rms {rms1:.2} → {rms2:.2} (halves), max |dev| {max_dev:.2}"
        );
    }

    // --- pairwise heading differences (offset-free quantization test) ---
    if headings.len() > 1 {
        println!("\npairwise heading differences (fan-step residuals):");
        for i in 0..headings.len() {
            for j in i + 1..headings.len() {
                let d = (headings[i].1 - headings[j].1).abs();
                let d = d.min(360.0 - d);
                println!(
                    "  obj {} vs obj {}: {:7.2}°   mod 60°: {:5.2}   mod 36°: {:5.2}   mod 30°: {:5.2}",
                    headings[i].0,
                    headings[j].0,
                    d,
                    fan_residual(d, 0.0, 60.0),
                    fan_residual(d, 0.0, 36.0),
                    fan_residual(d, 0.0, 30.0),
                );
            }
        }
    }

    // --- fan-offset fits over all windowed headings ---
    if all_windowed.is_empty() {
        return;
    }
    println!("\nfan fits over {} windowed headings (mean|max residual, random ≈ step/4):", all_windowed.len());
    for (name, step) in [("6-fold (60°)", 60.0), ("10-fold (36°)", 36.0), ("12-fold (30°)", 30.0)] {
        let mut best = (0.0f64, f64::MAX, 0.0f64);
        let mut phi = 0.0;
        while phi < step {
            let (mut sum, mut max) = (0.0, 0.0f64);
            for &h in &all_windowed {
                let r = fan_residual(h, phi, step);
                sum += r;
                max = max.max(r);
            }
            let mean = sum / all_windowed.len() as f64;
            if mean < best.1 {
                best = (phi, mean, max);
            }
            phi += 0.25;
        }
        println!(
            "  {name}: offset {:5.2}°, mean {:4.2}°, max {:5.2}° (random mean ≈ {:.1}°)",
            best.0,
            best.1,
            best.2,
            step / 4.0
        );
    }
}
