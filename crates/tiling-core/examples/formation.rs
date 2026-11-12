//! Formation geometry of a two-glider object (FINDINGS 10.11.61).
//!
//! Replays a record whose live cells are one convoy and nothing else. Each
//! sampled generation the live cells are split in two (their two
//! vertex-connected pieces when there are exactly two, otherwise the two
//! sides of their principal axis), and the vector from one half's centroid
//! to the other's is resolved
//! along and across the direction of travel (the displacement of the whole
//! object's centroid over the sampled span). "Behind" is the separation
//! along the track, "aside" the separation across it (both unsigned); a pair in file has
//! aside ≈ 0, a pair abreast has behind ≈ 0. Also reported: how many
//! generations the live cells form one vertex-connected piece.
//!
//! Usage: formation <record.jsonl> [index=0] [radius=150] [from=20] [to=240]
//! Set FORMATION_ROWS=1 to print every sampled generation.

use ca_engine::{CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

/// One sampled generation: the whole object's centroid, each half's
/// centroid and cell count, and the number of connected pieces.
struct Sample {
    centre: (f64, f64),
    halves: [(f64, f64); 2],
    cells: [usize; 2],
    pieces: usize,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(150, |a| a.parse().expect("radius"));
    let from: u64 = args.next().map_or(20, |a| a.parse().expect("from"));
    let to: u64 = args.next().map_or(240, |a| a.parse().expect("to"));

    let text = std::fs::read_to_string(&path).expect("read record");
    let line = text.lines().filter(|l| !l.trim().is_empty()).nth(index).expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    let tiling = Tiling::new(record.family);
    let (patch, geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .expect("patch");
    let centroid: Vec<(f64, f64)> = (0..patch.graph.cells() as usize)
        .map(|c| {
            let pts = &geom.xy[geom.offsets[c] as usize..geom.offsets[c + 1] as usize];
            let n = pts.len() as f64;
            let (sx, sy) = pts.iter().fold((0.0, 0.0), |(a, b), &[x, y]| (a + x, b + y));
            (sx / n, sy / n)
        })
        .collect();
    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());

    let mut rows: Vec<Sample> = Vec::new();
    for generation in 0..=to {
        if generation >= from {
            let state = engine.state();
            let live: Vec<usize> = (0..state.len()).filter(|&c| state[c] != 0).collect();
            assert!(!live.is_empty(), "nothing alive at generation {generation}");
            let n = live.len() as f64;
            let (mx, my) = live.iter().fold((0.0, 0.0), |(a, b), &c| (a + centroid[c].0 / n, b + centroid[c].1 / n));
            let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
            for &c in &live {
                let (dx, dy) = (centroid[c].0 - mx, centroid[c].1 - my);
                sxx += dx * dx;
                sxy += dx * dy;
                syy += dy * dy;
            }
            let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
            // Vertex-connected pieces of the live set.
            let mut piece = vec![usize::MAX; state.len()];
            let mut pieces = 0;
            for &c in &live {
                if piece[c] != usize::MAX {
                    continue;
                }
                let mut stack = vec![c as u32];
                piece[c] = pieces;
                while let Some(x) = stack.pop() {
                    for &y in patch.graph.neighbours(x) {
                        if state[y as usize] != 0 && piece[y as usize] == usize::MAX {
                            piece[y as usize] = pieces;
                            stack.push(y);
                        }
                    }
                }
                pieces += 1;
            }
            // The two halves: the two pieces when there are exactly two,
            // otherwise the two sides of the principal axis.
            let (mut a, mut b) = ((0.0, 0.0, 0usize), (0.0, 0.0, 0usize));
            for &c in &live {
                let first = if pieces == 2 {
                    piece[c] == 0
                } else {
                    (centroid[c].0 - mx) * theta.cos() + (centroid[c].1 - my) * theta.sin() < 0.0
                };
                let half = if first { &mut a } else { &mut b };
                half.0 += centroid[c].0;
                half.1 += centroid[c].1;
                half.2 += 1;
            }
            let half = |h: (f64, f64, usize)| (h.0 / h.2.max(1) as f64, h.1 / h.2.max(1) as f64);
            rows.push(Sample { centre: (mx, my), halves: [half(a), half(b)], cells: [a.2, b.2], pieces });
        }
        rule.step(&mut engine);
    }
    let (first, last) = (rows[0].centre, rows[rows.len() - 1].centre);
    let (hx, hy) = (last.0 - first.0, last.1 - first.1);
    let hl = hx.hypot(hy);
    let (mut behind, mut aside) = (Vec::new(), Vec::new());
    for r in &rows {
        let (dx, dy) = (r.halves[1].0 - r.halves[0].0, r.halves[1].1 - r.halves[0].1);
        let along = (dx * hx + dy * hy) / hl;
        let across = (-dx * hy + dy * hx) / hl;
        if std::env::var("FORMATION_ROWS").is_ok() {
            println!("    halves {}+{} cells, pieces {}, along {along:.2}, across {across:.2}", r.cells[0], r.cells[1], r.pieces);
        }
        behind.push(along.abs());
        aside.push(across.abs());
    }
    let stats = |v: &[f64]| {
        let n = v.len() as f64;
        (v.iter().cloned().fold(f64::INFINITY, f64::min), v.iter().sum::<f64>() / n, v.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
    };
    let (b, a) = (stats(&behind), stats(&aside));
    let one_piece = rows.iter().filter(|r| r.pieces == 1).count();
    let pops: Vec<usize> = rows.iter().map(|r| r.cells[0] + r.cells[1]).collect();
    println!(
        "{path}[{index}]: generations {from}-{to}, heading {:.2}°, speed {:.3} units/generation, population {}-{}",
        hy.atan2(hx).to_degrees().rem_euclid(360.0),
        hl / (to - from) as f64,
        pops.iter().min().unwrap(),
        pops.iter().max().unwrap()
    );
    println!(
        "  behind (along the track): {:.1} to {:.1}, mean {:.1} units; aside (across it): {:.1} to {:.1}, mean {:.1} units; offset {:.0}° from the track",
        b.0,
        b.2,
        b.1,
        a.0,
        a.2,
        a.1,
        a.1.atan2(b.1).to_degrees()
    );
    println!("  one connected piece in {one_piece} of {} generations", rows.len());
}
