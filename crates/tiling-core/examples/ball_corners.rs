//! Graph-metric ball anisotropy probe: dump (distance, theta, rho) per
//! cell of a large patch so the ball's corner directions — the maxima
//! of Euclidean reach per ring, i.e. the pseudo-hexagon's corners —
//! can be located precisely and compared against the measured glider
//! lane headings (same frame: same root, same place_default pose).
//!
//! Usage: ball_corners <record.jsonl> [index] [radius] [out.csv] [min_dist]

use std::fmt::Write as _;

use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(384, |a| a.parse().expect("radius"));
    let out = args.next().unwrap_or_else(|| "ball.csv".into());
    let min_dist: u32 = args.next().map_or(radius / 2, |a| a.parse().expect("min_dist"));

    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(index)
        .expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    let tiling = Tiling::new(record.family);
    let (patch, geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .unwrap();
    let mut csv = String::from("dist,theta_deg,rho\n");
    let mut n = 0u32;
    for (c, cell) in patch.cells.iter().enumerate() {
        if cell.distance < min_dist {
            continue;
        }
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        let pts = &geom.xy[lo..hi];
        let m = pts.len() as f64;
        let (sx, sy) = pts
            .iter()
            .fold((0.0, 0.0), |(ax, ay), &[x, y]| (ax + x, ay + y));
        let (x, y) = (sx / m, sy / m);
        let theta = y.atan2(x).to_degrees().rem_euclid(360.0);
        let rho = x.hypot(y);
        writeln!(csv, "{},{theta:.4},{rho:.3}", cell.distance).unwrap();
        n += 1;
    }
    std::fs::write(&out, csv).expect("write csv");
    println!("{out}: {n} cells with distance >= {min_dist} (radius {radius})");
}
