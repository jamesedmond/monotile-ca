//! Full-lifetime trail figure: replay a record to the patch boundary
//! and render one overhead SVG of the entire run — every tile that was
//! ever non-quiescent, tinted by its first activation time (pale sand
//! early, vermilion late; luminance decreases monotonically, so the
//! ordering survives grayscale), with the final generation's cells in
//! the standard state palette. Substrate outlines are drawn only in a
//! corridor around the trail (a full radius-384 patch would swamp the
//! SVG), plus the patch boundary ring for scale.
//!
//! Usage: render_trail <record.jsonl> [index] [radius] [out.svg] [horizon] [fit]
//!
//! fit = "patch" (default) frames the whole patch including its
//! boundary ring; "trail" zooms to the visited cells only (for
//! objects, like the looper, whose lifetime occupies a small region).
//!
//! The default horizon (8 x radius) suits gliders, which stop the run
//! at boundary contact; loopers never arrive, so pass a horizon long
//! enough to trace the full orbit.

use std::fmt::Write as _;

use ca_engine::{CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

const STATE_FILL: [&str; 4] = ["", "#d55e00", "#5fb878", "#b7cde8"];

/// Trail colormap: 0..1 -> sand -> vermilion. The input is compressed
/// so even the earliest trail is clearly visible (fades only a little
/// toward the launch end).
fn trail_color(t: f64) -> String {
    let t = 0.45 + 0.55 * t;
    let stops = [(0xf2, 0xe4, 0xd5), (0xe8, 0xa3, 0x66), (0xd5, 0x5e, 0x00)];
    let x = t.clamp(0.0, 1.0) * 2.0;
    let (i, f) = if x < 1.0 { (0, x) } else { (1, x - 1.0) };
    let lerp = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * f) as u8;
    let (a, b) = (stops[i], stops[i + 1]);
    format!("#{:02x}{:02x}{:02x}", lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(384, |a| a.parse().expect("radius"));
    let out = args.next().unwrap_or_else(|| "trail.svg".into());
    let horizon_override: Option<u64> = args.next().map(|a| a.parse().expect("horizon"));
    let fit = args.next().unwrap_or_else(|| "patch".into());

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
    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());

    // Replay to boundary contact, recording first activation per cell.
    let cells = patch.graph.cells() as usize;
    let mut first_active: Vec<u32> = vec![u32::MAX; cells];
    for (c, &s) in engine.state().iter().enumerate() {
        if s != 0 {
            first_active[c] = 0;
        }
    }
    let horizon = horizon_override.unwrap_or(u64::from(radius) * 8);
    let mut final_gen = 0u64;
    for generation in 1..=horizon {
        rule.step(&mut engine);
        final_gen = generation;
        let mut touched_boundary = false;
        for (c, &s) in engine.state().iter().enumerate() {
            if s != 0 {
                if first_active[c] == u32::MAX {
                    first_active[c] = generation as u32;
                }
                if distance[c] + margin >= radius {
                    touched_boundary = true;
                }
            }
        }
        if touched_boundary || engine.population() == 0 {
            break;
        }
    }
    let visited: Vec<usize> = (0..cells).filter(|&c| first_active[c] != u32::MAX).collect();
    println!(
        "{}: {} generations, {} cells visited",
        out,
        final_gen,
        visited.len()
    );

    let poly = |c: usize| {
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        &geom.xy[lo..hi]
    };

    // Viewport: bounding box of visited cells plus the boundary ring.
    let (mut minx, mut miny, mut maxx, mut maxy) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |c: usize| {
        for &[x, y] in poly(c) {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
    };
    for &c in &visited {
        grow(c);
    }
    if fit != "trail" {
        for (c, &d) in distance.iter().enumerate() {
            if d >= radius - 1 {
                grow(c);
            }
        }
    }
    let pad = 12.0;
    minx -= pad;
    miny -= pad;
    maxx += pad;
    maxy += pad;
    // Square viewport (the atlas lays trails out as squares).
    let (w, h) = (maxx - minx, maxy - miny);
    if w > h {
        miny -= (w - h) / 2.0;
        maxy += (w - h) / 2.0;
    } else {
        minx -= (h - w) / 2.0;
        maxx += (h - w) / 2.0;
    }

    // Corridor context: substrate outlines within graph distance 2 of a
    // visited cell (cheap approximation via geometric distance would
    // need a spatial index; the graph 2-ball is exact and simple).
    let mut context = vec![false; cells];
    for &c in &visited {
        for &n in patch.graph.neighbours(c as u32) {
            context[n as usize] = true;
            for &m in patch.graph.neighbours(n) {
                context[m as usize] = true;
            }
        }
    }

    let mut svg = String::new();
    let (vb_y, vb_h) = (-maxy, maxy - miny);
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{minx:.1} {vb_y:.1} {:.1} {vb_h:.1}" width="1400">"#,
        maxx - minx
    )
    .unwrap();
    writeln!(svg, r#"<rect x="{minx:.1}" y="{vb_y:.1}" width="{:.1}" height="{vb_h:.1}" fill="white"/>"#, maxx - minx).unwrap();

    let points = |c: usize| -> String {
        poly(c)
            .iter()
            .map(|&[x, y]| format!("{x:.1},{:.1}", -y))
            .collect::<Vec<_>>()
            .join(" ")
    };

    // Corridor outlines (under the trail), then boundary ring.
    for c in 0..cells {
        if context[c] && first_active[c] == u32::MAX {
            writeln!(
                svg,
                r##"<polygon points="{}" fill="none" stroke="#dde1e7" stroke-width="0.3"/>"##,
                points(c)
            )
            .unwrap();
        }
    }
    for (c, &d) in distance.iter().enumerate() {
        if d >= radius - 1 {
            writeln!(
                svg,
                r##"<polygon points="{}" fill="#eef1f5" stroke="none"/>"##,
                points(c)
            )
            .unwrap();
        }
    }

    // Trail fills by first activation; final state on top in state colors.
    let final_state = engine.state();
    for &c in &visited {
        if final_state[c] != 0 {
            continue;
        }
        let t = f64::from(first_active[c]) / final_gen as f64;
        writeln!(
            svg,
            r#"<polygon points="{}" fill="{}"/>"#,
            points(c),
            trail_color(t)
        )
        .unwrap();
    }
    for &c in &visited {
        let s = final_state[c] as usize;
        if s != 0 {
            writeln!(
                svg,
                r##"<polygon points="{}" fill="{}" stroke="#333333" stroke-width="0.4"/>"##,
                points(c),
                STATE_FILL.get(s).copied().unwrap_or("#000000")
            )
            .unwrap();
        }
    }
    writeln!(svg, "</svg>").unwrap();
    std::fs::write(&out, svg).expect("write svg");
}
