//! Objects' tracks against the graph-metric ball of their seed.
//!
//! Replays a record on a patch with geometry and writes, for every
//! generation, each vertex-connected component of the live cells: its
//! population, its centroid relative to the root cell's centroid, and
//! the smallest and largest graph distance from the root among its
//! cells. Also writes the outline of the ball at each requested ring:
//! the farthest reach of the cells at exactly that graph distance, per
//! direction bin. Everything is in the record's own frame (its root,
//! `place_default` pose, its neighbourhood), so a figure can draw the
//! tracks over the ball they travel in.
//!
//! Usage: ball_tracks <record.jsonl> [index=0] [radius=256] [gens=2*radius] [rings=radius-8] [bin-deg=0.5] [out.json]
//!
//! `rings` is a comma-separated list. The run stops early when a live
//! cell comes within three rings of the patch boundary, unless
//! `BALL_TRACKS_THROUGH` is set (to compare a small patch against a
//! large one after its boundary has been reached).

use std::fmt::Write as _;

use ca_engine::{CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(256, |a| a.parse().expect("radius"));
    let gens: u64 = args
        .next()
        .map_or(2 * u64::from(radius), |a| a.parse().expect("gens"));
    let rings: Vec<u32> = args.next().map_or(vec![radius - 8], |a| {
        a.split(',').map(|r| r.parse().expect("ring")).collect()
    });
    let bin_deg: f64 = args.next().map_or(0.5, |a| a.parse().expect("bin degrees"));
    let out = args.next().unwrap_or_else(|| "ball-tracks.json".into());

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
    let cells = patch.graph.cells();
    let centroid = |c: u32| {
        let (lo, hi) = (
            geom.offsets[c as usize] as usize,
            geom.offsets[c as usize + 1] as usize,
        );
        let pts = &geom.xy[lo..hi];
        let n = pts.len() as f64;
        let (sx, sy) = pts
            .iter()
            .fold((0.0, 0.0), |(a, b), &[x, y]| (a + x, b + y));
        (sx / n, sy / n)
    };
    let origin = centroid(0);

    // Ball outlines: per direction bin, the farthest vertex of any cell
    // at exactly the ring's graph distance.
    let bins = (360.0 / bin_deg).round() as usize;
    let mut s = String::new();
    write!(
        s,
        "{{\"family\":\"{:?}\",\"root\":{:?},\"radius\":{radius},\"neighbourhood\":\"{:?}\",\"bin_deg\":{bin_deg},\"outlines\":[",
        record.family, record.root, record.neighbourhood
    )
    .unwrap();
    for (k, &ring) in rings.iter().enumerate() {
        assert!(ring <= radius, "ring {ring} is outside the patch");
        let mut reach = vec![0.0f64; bins];
        let mut count = 0u32;
        for c in 0..cells {
            if patch.cells[c as usize].distance != ring {
                continue;
            }
            count += 1;
            let (lo, hi) = (
                geom.offsets[c as usize] as usize,
                geom.offsets[c as usize + 1] as usize,
            );
            for &[x, y] in &geom.xy[lo..hi] {
                let (dx, dy) = (x - origin.0, y - origin.1);
                let b = ((dy.atan2(dx).to_degrees().rem_euclid(360.0)) / bin_deg) as usize % bins;
                reach[b] = reach[b].max(dx.hypot(dy));
            }
        }
        let rho: Vec<String> = reach.iter().map(|r| format!("{r:.2}")).collect();
        write!(
            s,
            "{}{{\"ring\":{ring},\"cells\":{count},\"reach\":[{}]}}",
            if k > 0 { "," } else { "" },
            rho.join(",")
        )
        .unwrap();
        println!("ring {ring}: {count} cells");
    }
    s.push_str("],\"generations\":[");

    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(cells).unwrap());
    let through = std::env::var_os("BALL_TRACKS_THROUGH").is_some();
    let mut seen = vec![u64::MAX; cells as usize];
    let mut last = 0;
    for g in 0..=gens {
        let state = engine.state();
        let mut comps: Vec<String> = Vec::new();
        let mut far = 0u32;
        for c in 0..cells {
            if state[c as usize] == 0 || seen[c as usize] == g {
                continue;
            }
            // One vertex-connected component.
            let (mut n, mut sx, mut sy, mut lo, mut hi) = (0u32, 0.0, 0.0, u32::MAX, 0u32);
            let mut stack = vec![c];
            seen[c as usize] = g;
            while let Some(x) = stack.pop() {
                let (cx, cy) = centroid(x);
                n += 1;
                sx += cx;
                sy += cy;
                lo = lo.min(patch.cells[x as usize].distance);
                hi = hi.max(patch.cells[x as usize].distance);
                for &y in patch.graph.neighbours(x) {
                    if state[y as usize] != 0 && seen[y as usize] != g {
                        seen[y as usize] = g;
                        stack.push(y);
                    }
                }
            }
            far = far.max(hi);
            comps.push(format!(
                "[{n},{:.3},{:.3},{lo},{hi}]",
                sx / f64::from(n) - origin.0,
                sy / f64::from(n) - origin.1
            ));
        }
        write!(s, "{}[{}]", if g > 0 { "," } else { "" }, comps.join(",")).unwrap();
        last = g;
        if comps.is_empty() || (far + 3 >= radius && !through) || g == gens {
            break;
        }
        rule.step(&mut engine);
    }
    s.push_str("]}");
    std::fs::write(&out, s).expect("write");
    println!(
        "{out}: {} generations of components [cells, x, y, nearest ring, farthest ring]",
        last + 1
    );
}
