//! Render paper figures: replay a ResultRecord to chosen generations
//! and emit one SVG per generation, cropped to the union of the active
//! regions (fixed viewport across the strip, so motion is visible
//! against fixed terrain). Print palette: white/pale substrate with
//! chirality tinting, Okabe--Ito colorblind-safe state colors.
//!
//! Usage: render_frames <record.jsonl> [index] [radius] [gens] [pad] [out-prefix] [focus]
//!   gens       comma-separated, ascending (default "100,101,102,103")
//!   pad        viewport padding in world units (default 10)
//!   out-prefix output path prefix (default "frame"): writes
//!              <prefix>-g<generation>.svg
//!   focus      "all" (default) crops to every active cell; "far" crops
//!              to the cluster around the active cell farthest from the
//!              root — for records that launch several objects
//!   ground     "tint" (default) shades the ground state by chirality;
//!              "plain" renders all ground tiles white (outlines only)
//!   shape      "auto" (default) crops to the content bounding box;
//!              "square" expands the shorter axis to make it square
//!
//! Besides one SVG per generation, writes <prefix>-strip.svg: all
//! panels side by side with (a), (b), ... labels and generation
//! captions, ready for use as a single figure.

use std::fmt::Write as _;

use ca_engine::{CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

/// State fill colors, index 1..: saturation and luminance both encode
/// distance from state 1 (deep vermilion, then medium green, then pale
/// blue). Luminances are monotone (~0.47, 0.60, 0.79 against a 0.91+
/// substrate), so grayscale printing preserves the ordering as a shade
/// ladder; hues remain colorblind-distinguishable.
const STATE_FILL: [&str; 4] = ["", "#d55e00", "#5fb878", "#b7cde8"];

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(96, |a| a.parse().expect("radius"));
    let gens: Vec<u64> = args
        .next()
        .unwrap_or_else(|| "100,101,102,103".into())
        .split(',')
        .map(|g| g.parse().expect("generation"))
        .collect();
    let pad: f64 = args.next().map_or(10.0, |a| a.parse().expect("pad"));
    let prefix = args.next().unwrap_or_else(|| "frame".into());
    let focus = args.next().unwrap_or_else(|| "all".into());
    let ground = args.next().unwrap_or_else(|| "tint".into());
    let shape = args.next().unwrap_or_else(|| "auto".into());
    assert!(gens.windows(2).all(|w| w[0] < w[1]), "gens must ascend");

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
    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());

    // Replay, snapshotting the requested generations.
    let mut snaps: Vec<(u64, Vec<u8>)> = Vec::new();
    for &g in &gens {
        while engine.generation() < g {
            rule.step(&mut engine);
        }
        snaps.push((g, engine.state().to_vec()));
    }

    // Fixed viewport: union bounding box of active cells over the strip.
    let cells = patch.graph.cells() as usize;
    let poly = |c: usize| {
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        &geom.xy[lo..hi]
    };
    let centroid = |c: usize| {
        let pts = poly(c);
        let n = pts.len() as f64;
        let (sx, sy) = pts
            .iter()
            .fold((0.0, 0.0), |(ax, ay), &[x, y]| (ax + x, ay + y));
        (sx / n, sy / n)
    };
    // Focus "far": crop to the cluster around the active cell farthest
    // from the root at the middle snapshot (multi-object records).
    let in_focus: Box<dyn Fn(usize) -> bool> = if focus == "far" {
        let mid = &snaps[snaps.len() / 2].1;
        let (fx, fy) = (0..cells)
            .filter(|&c| mid[c] != 0)
            .map(centroid)
            .max_by(|a, b| {
                (a.0 * a.0 + a.1 * a.1).total_cmp(&(b.0 * b.0 + b.1 * b.1))
            })
            .expect("active cells at the middle generation");
        Box::new(move |c: usize| {
            let (x, y) = centroid(c);
            (x - fx).hypot(y - fy) < 16.0
        })
    } else {
        Box::new(|_| true)
    };
    let (mut minx, mut miny, mut maxx, mut maxy) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (_, state) in &snaps {
        for (c, &s) in state.iter().enumerate() {
            if s != 0 && in_focus(c) {
                for &[x, y] in poly(c) {
                    minx = minx.min(x);
                    miny = miny.min(y);
                    maxx = maxx.max(x);
                    maxy = maxy.max(y);
                }
            }
        }
    }
    minx -= pad;
    miny -= pad;
    maxx += pad;
    maxy += pad;
    if shape == "square" {
        let (w, h) = (maxx - minx, maxy - miny);
        if w > h {
            miny -= (w - h) / 2.0;
            maxy += (w - h) / 2.0;
        } else {
            minx -= (h - w) / 2.0;
            maxx += (h - w) / 2.0;
        }
    }
    println!(
        "viewport: [{minx:.1}, {miny:.1}] .. [{maxx:.1}, {maxy:.1}] ({} x {})",
        (maxx - minx).round(),
        (maxy - miny).round()
    );

    // Chirality tint: two pale substrate tones keyed on class base (or
    // class parity for single-base families), as in the UI.
    let bases: Vec<&str> = {
        let mut b: Vec<&str> =
            patch.classes.iter().map(|c| c.base.as_str()).collect();
        b.sort();
        b.dedup();
        b
    };
    let plain_ground = ground == "plain";
    let (patch_ref, classes_ref) = (&patch.cells, &patch.classes);
    let dead_fill = move |c: usize| {
        if plain_ground {
            return "#ffffff";
        }
        let class = &classes_ref[patch_ref[c].class as usize];
        let alt = if bases.len() > 1 {
            bases.iter().position(|&b| b == class.base).unwrap_or(0) % 2 == 1
        } else {
            patch_ref[c].class % 2 == 1
        };
        if alt { "#e7eaf0" } else { "#f8f9fb" }
    };

    // SVG y grows downward; world y grows upward — negate y and shift
    // the viewBox accordingly.
    let (vb_y, vb_h) = (-maxy, maxy - miny);
    let vb_w = maxx - minx;
    let mut panels: Vec<(u64, String)> = Vec::new();
    for (g, state) in &snaps {
        let mut body = String::new();
        writeln!(body, r#"<rect x="{minx:.2}" y="{vb_y:.2}" width="{vb_w:.2}" height="{vb_h:.2}" fill="white"/>"#).unwrap();
        for (c, &st) in state.iter().enumerate() {
            let pts = poly(c);
            if pts.iter().all(|&[x, _]| x < minx || x > maxx)
                || pts.iter().all(|&[_, y]| y < miny || y > maxy)
            {
                continue;
            }
            let fill = match st as usize {
                0 => dead_fill(c),
                s => STATE_FILL.get(s).copied().unwrap_or("#000000"),
            };
            let points: Vec<String> = pts
                .iter()
                .map(|&[x, y]| format!("{x:.2},{:.2}", -y))
                .collect();
            writeln!(
                body,
                r##"<polygon points="{}" fill="{fill}" stroke="#c3c7cf" stroke-width="0.12" stroke-linejoin="round"/>"##,
                points.join(" ")
            )
            .unwrap();
        }
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{minx:.2} {vb_y:.2} {vb_w:.2} {vb_h:.2}\" width=\"900\">\n{body}</svg>\n"
        );
        let out = format!("{prefix}-g{g}.svg");
        std::fs::write(&out, svg).expect("write svg");
        let live = state.iter().filter(|&&s| s != 0).count();
        println!("{out}: generation {g}, population {live}");
        panels.push((*g, body));
    }

    // Composed strip: panels side by side with (a), (b), ... labels.
    // Each panel gets a hairline border — on plain white grounds the
    // frames otherwise fuse into one field at print size.
    let gap = vb_w * 0.055;
    let label_h = vb_h * 0.145;
    let n = panels.len() as f64;
    let total_w = n * vb_w + (n - 1.0) * gap;
    let total_h = vb_h + label_h;
    let mut strip = String::new();
    writeln!(
        strip,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {total_w:.2} {total_h:.2}" width="1800">"#
    )
    .unwrap();
    for (i, (g, body)) in panels.iter().enumerate() {
        let x = i as f64 * (vb_w + gap);
        writeln!(
            strip,
            r#"<svg x="{x:.2}" y="0" width="{vb_w:.2}" height="{vb_h:.2}" viewBox="{minx:.2} {vb_y:.2} {vb_w:.2} {vb_h:.2}">"#
        )
        .unwrap();
        strip.push_str(body);
        writeln!(strip, "</svg>").unwrap();
        writeln!(
            strip,
            r##"<rect x="{x:.2}" y="0" width="{vb_w:.2}" height="{vb_h:.2}" fill="none" stroke="#b6bac2" stroke-width="{:.2}"/>"##,
            vb_w * 0.006,
        )
        .unwrap();
        writeln!(
            strip,
            r##"<text x="{:.2}" y="{:.2}" font-family="Helvetica, Arial, sans-serif" font-size="{:.2}" text-anchor="middle" fill="#333333">({}) gen {g}</text>"##,
            x + vb_w / 2.0,
            vb_h + label_h * 0.78,
            label_h * 0.68,
            (b'a' + i as u8) as char,
        )
        .unwrap();
    }
    writeln!(strip, "</svg>").unwrap();
    let out = format!("{prefix}-strip.svg");
    std::fs::write(&out, strip).expect("write strip svg");
    println!("{out}: {} panels", panels.len());
}
