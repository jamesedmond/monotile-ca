//! Dump the tiles of a record's patch that lie within a band around a
//! line through the origin (the lane), with their canonical addresses and
//! polygons, so a figure can draw the supertile hierarchy along the lane
//! (FINDINGS 10.11.13).
//!
//! Usage: hier_dump <record.jsonl> [index] [radius] [out.json] [band_u] [heading_deg]

use std::fmt::Write as _;

use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(256, |a| a.parse().expect("radius"));
    let out = args.next().unwrap_or_else(|| "hier.json".into());
    let band: f64 = args.next().map_or(200.0, |a| a.parse().expect("band"));
    let heading: f64 = args.next().map_or(225.5225, |a| a.parse().expect("heading"));
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
    assert!(patch.seed_artifact_cells.is_empty());
    let (dx, dy) = (heading.to_radians().cos(), heading.to_radians().sin());
    let mut s = String::new();
    write!(
        s,
        "{{\"family\":\"{:?}\",\"root\":{:?},\"radius\":{radius},\"band\":{band},\"heading\":{heading},\"cells\":[",
        record.family, record.root
    )
    .unwrap();
    let mut n = 0usize;
    for (c, cell) in patch.cells.iter().enumerate() {
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        let pts = &geom.xy[lo..hi];
        let (cx, cy) = pts
            .iter()
            .fold((0.0, 0.0), |(ax, ay), &[x, y]| (ax + x, ay + y));
        let (cx, cy) = (cx / pts.len() as f64, cy / pts.len() as f64);
        // signed transverse distance from the line through the origin
        let t = -cx * dy + cy * dx;
        if t.abs() > band {
            continue;
        }
        if n > 0 {
            s.push(',');
        }
        n += 1;
        let poly: Vec<String> = pts.iter().map(|&[x, y]| format!("[{x:.2},{y:.2}]")).collect();
        write!(
            s,
            "{{\"i\":{c},\"d\":{},\"a\":{:?},\"c\":[{cx:.2},{cy:.2}],\"xy\":[{}]}}",
            cell.distance,
            cell.address,
            poly.join(",")
        )
        .unwrap();
    }
    s.push_str("]}");
    std::fs::write(&out, s).expect("write");
    println!("{out}: {n} of {} cells within {band} u of the lane", patch.cells.len());
}
