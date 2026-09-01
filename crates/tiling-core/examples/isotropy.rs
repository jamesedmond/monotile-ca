//! Growth-front isotropy: aperiodic monotiles vs periodic lattices.
//!
//! Tests whether CA growth on the hat/spectre is more isotropic than on a
//! periodic lattice (a dynamical analogue of quasicrystalline isotropy).
//! Two measurements on each substrate: `metric` — the raw graph-distance
//! ball shell (intrinsic geometry); and `chaotic` — the front of a
//! chaotic sub-light growth rule (B2/S12346) from a central soup (a
//! first-passage/shape-theorem front). Anisotropy is the normalised
//! 4-fold and 6-fold Fourier amplitude of the front radius r(θ); the
//! leading harmonic reflects the substrate's rotational symmetry
//! (square → 4-fold, hex/monotile → 6-fold).
//!
//! Controls: a square lattice (4-neighbour, 4-fold) and a hex lattice
//! (6-neighbour, 6-fold), each with a fixed, scale-invariant limit shape.

use ca_engine::{Engine, Graph, Rule, StratifiedRule};
use std::collections::HashMap;
use tiling_core::{Tiling, TilingFamily};

const NBINS: usize = 60;

/// (4-fold, 6-fold) normalised Fourier amplitude of the front radius, or
/// `None` if any angular bin is empty (front not a closed loop).
fn anisotropy(pts: impl Iterator<Item = [f64; 2]>, o: [f64; 2]) -> Option<(f64, f64)> {
    let mut r = [0f64; NBINS];
    let mut seen = [false; NBINS];
    for [x, y] in pts {
        let (dx, dy) = (x - o[0], y - o[1]);
        let mut a = dy.atan2(dx);
        if a < 0.0 {
            a += std::f64::consts::TAU;
        }
        let b = ((a / std::f64::consts::TAU) * NBINS as f64) as usize % NBINS;
        r[b] = r[b].max(dx.hypot(dy));
        seen[b] = true;
    }
    if !seen.iter().all(|&s| s) {
        return None;
    }
    let mean = r.iter().sum::<f64>() / NBINS as f64;
    let amp = |k: f64| {
        let (mut c, mut s) = (0f64, 0f64);
        for (i, &ri) in r.iter().enumerate() {
            let th = i as f64 / NBINS as f64 * std::f64::consts::TAU;
            c += ri * (k * th).cos();
            s += ri * (k * th).sin();
        }
        2.0 * (c * c + s * s).sqrt() / NBINS as f64 / mean
    };
    Some((amp(4.0), amp(6.0)))
}

struct Substrate {
    name: &'static str,
    graph: Graph,
    coords: Vec<[f64; 2]>,
    dist: Vec<u32>,
}

fn square_lattice(radius: i32) -> Substrate {
    // 4-neighbour (von Neumann); graph distance = Manhattan (diamond ball).
    let mut index = HashMap::new();
    let mut cells = Vec::new();
    for x in -radius..=radius {
        for y in -radius..=radius {
            if x.abs() + y.abs() <= radius {
                cells.push((x, y));
            }
        }
    }
    let o = cells.iter().position(|&c| c == (0, 0)).unwrap();
    cells.swap(0, o);
    for (i, &c) in cells.iter().enumerate() {
        index.insert(c, i as u32);
    }
    let mut edges = Vec::new();
    for (i, &(x, y)) in cells.iter().enumerate() {
        for (dx, dy) in [(1, 0), (0, 1)] {
            if let Some(&j) = index.get(&(x + dx, y + dy)) {
                edges.push((i as u32, j));
            }
        }
    }
    Substrate {
        name: "square(4-fold)",
        coords: cells.iter().map(|&(x, y)| [x as f64, y as f64]).collect(),
        dist: cells.iter().map(|&(x, y)| (x.abs() + y.abs()) as u32).collect(),
        graph: Graph::from_edges(cells.len() as u32, &edges),
    }
}

fn hex_lattice(radius: i32) -> Substrate {
    let dirs = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];
    let hd = |q: i32, r: i32| ((q.abs() + r.abs() + (q + r).abs()) / 2) as u32;
    let mut index = HashMap::new();
    let mut cells = Vec::new();
    for q in -radius..=radius {
        for r in -radius..=radius {
            if hd(q, r) <= radius as u32 {
                cells.push((q, r));
            }
        }
    }
    let o = cells.iter().position(|&c| c == (0, 0)).unwrap();
    cells.swap(0, o);
    for (i, &c) in cells.iter().enumerate() {
        index.insert(c, i as u32);
    }
    let mut edges = Vec::new();
    for (i, &(q, r)) in cells.iter().enumerate() {
        for (dq, dr) in dirs {
            if let Some(&j) = index.get(&(q + dq, r + dr))
                && (i as u32) < j
            {
                edges.push((i as u32, j));
            }
        }
    }
    Substrate {
        name: "hex(6-fold)",
        coords: cells.iter().map(|&(q, r)| [3f64.sqrt() * (q as f64 + r as f64 / 2.0), 1.5 * r as f64]).collect(),
        dist: cells.iter().map(|&(q, r)| hd(q, r)).collect(),
        graph: Graph::from_edges(cells.len() as u32, &edges),
    }
}

fn monotile(name: &'static str, family: TilingFamily, radius: u32) -> Substrate {
    let tiling = Tiling::new(family);
    let (patch, geom) = tiling
        .generate_with_geometry(&tiling.default_root(), radius)
        .unwrap();
    let coords = (0..patch.graph.cells() as usize)
        .map(|c| {
            let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
            let n = (hi - lo) as f64;
            let (sx, sy) = geom.xy[lo..hi].iter().fold((0.0, 0.0), |(ax, ay), &[x, y]| (ax + x, ay + y));
            [sx / n, sy / n]
        })
        .collect();
    Substrate {
        name,
        dist: patch.cells.iter().map(|c| c.distance).collect(),
        coords,
        graph: patch.graph,
    }
}

fn xorshift(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x
}

fn main() {
    let checkpoints = [30u32, 50, 70, 90, 110];
    let subs = [
        square_lattice(170),
        hex_lattice(140),
        monotile("hat", TilingFamily::Hat, 128),
        monotile("spectre", TilingFamily::Spectre, 128),
    ];
    println!("Anisotropy = normalised Fourier amplitude of front radius (leading harmonic bold).");
    println!("{:15} {:8} | {}", "substrate", "measure", checkpoints.iter().map(|r| format!("R{r:<3}")).collect::<Vec<_>>().join("     "));

    for s in &subs {
        // metric — graph-distance ball shell {R-2..R}
        let mut line = format!("{:15} {:8} |", s.name, "metric");
        for &rr in &checkpoints {
            let shell = (0..s.graph.cells())
                .filter(|&c| (rr.saturating_sub(2)..=rr).contains(&s.dist[c as usize]))
                .map(|c| s.coords[c as usize]);
            line += &fmt(anisotropy(shell, s.coords[0]));
        }
        println!("{line}");

        // chaotic — B2/S12346 front from a deterministic central soup
        let rule = StratifiedRule::uniform(Rule::new(Rule::mask(&[2]), Rule::mask(&[1, 2, 3, 4, 6])));
        let mut eng = Engine::new(s.graph.clone());
        let mut rng = 0x1234_5678u64;
        for c in 0..s.graph.cells() {
            if s.dist[c as usize] <= 6 && xorshift(&mut rng) % 5 < 2 {
                eng.set_cell(c, true);
            }
        }
        let mut line = format!("{:15} {:8} |", s.name, "chaotic");
        let mut cp = 0usize;
        for _ in 0..40000u64 {
            if eng.step(&rule).population == 0 {
                break;
            }
            let frontier = (0..s.graph.cells()).filter(|&c| eng.cell(c)).map(|c| s.dist[c as usize]).max().unwrap_or(0);
            while cp < checkpoints.len() && frontier >= checkpoints[cp] {
                let live = (0..s.graph.cells()).filter(|&c| eng.cell(c)).map(|c| s.coords[c as usize]);
                line += &fmt(anisotropy(live, s.coords[0]));
                cp += 1;
            }
            if cp >= checkpoints.len() {
                break;
            }
        }
        println!("{line}\n");
    }
    println!("(each cell: 4-fold/6-fold %; the substrate's leading harmonic is the meaningful one)");
}

fn fmt(a: Option<(f64, f64)>) -> String {
    match a {
        Some((a4, a6)) => format!("  {:4.1}/{:4.1}", a4 * 100.0, a6 * 100.0),
        None => "   -- /-- ".into(),
    }
}
