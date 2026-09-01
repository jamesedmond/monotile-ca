//! Ignition-heading sweep (§10.8 compass follow-up): for a glider rule,
//! find which compass spokes its launches populate. The five headline
//! lanes all lie on one 120° triad per substrate; both tilings are
//! chiral, so the anti-triad spokes might launch gliders with a different
//! offset/κ/clock — or none. This sweeps generic two-tile seeds and bins
//! each glider-like launch by heading, so one clean seed per unmeasured
//! spoke can be picked and flown.
//!
//! Frame: seeds are placed on anchors of the SOURCE record's patch and
//! headings are measured in that patch's frame — the same frame the
//! headline lanes were measured in, so headings are directly comparable
//! (and the emitted records keep the source root, so a slide flight of
//! them stays in that frame too).
//!
//! For each anchor (interior, mid-patch) × shared edge × state order:
//! replay to boundary contact, classify glider-like (flat max-pop ≤ 4×
//! the record's), and measure the launch heading. A coarse boundary
//! heading is provisional — an object can still be turning when it reaches
//! the boundary (the hat lane-capture object launches near 282° but only
//! locks 345.5° ~600 rings out). So a launch is nominated only if its
//! heading is SETTLED: the first-half direction (anchor→midpoint) agrees
//! with the second-half velocity (midpoint→boundary) within a tolerance.
//! Still-turning launches are shown in the histogram (as '.') but never
//! emitted. Emits one settled seed record per spoke and prints a per-spoke
//! histogram. Run at a radius large enough for a genuine turn to manifest
//! within the patch: a launch is caught only once it has bent past the
//! tolerance, so a slow relay that locks its lane ~600 rings out (the hat
//! lane-capture object) needs r ≳ 160; sharper turns are caught sooner.
//!
//! Usage: ignition_headings <record.jsonl> [index] [radius] [horizon] [anchors_per_class] [out.jsonl]

use std::io::Write as _;

use ca_engine::{CaRule, Engine};
use rayon::prelude::*;
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

/// One glider-like launch:
/// (class, anchor, neighbour, state_a, state_b, heading°, max_pop, settled?).
type Hit = (u16, u32, u32, u8, u8, f64, u32, bool);

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().unwrap());
    let radius: u32 = args.next().map_or(48, |a| a.parse().unwrap());
    let horizon: u64 = args.next().map_or(2000, |a| a.parse().unwrap());
    let per_class: usize = args.next().map_or(2, |a| a.parse().unwrap());
    let out_path = args.next();

    let text = std::fs::read_to_string(&path).unwrap();
    let record: ResultRecord =
        serde_json::from_str(text.lines().filter(|l| !l.trim().is_empty()).nth(index).unwrap())
            .unwrap();
    assert_eq!(record.initial_cells.len(), 2, "expects a two-cell seed record");
    let s0 = record.initial_states.first().copied().unwrap_or(1);
    let s1 = record.initial_states.get(1).copied().unwrap_or(1);

    let tiling = Tiling::new(record.family);
    let (patch, geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let distance: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let dist = &distance;
    let margin = (0..patch.graph.cells())
        .flat_map(|c| {
            let dc = dist[c as usize];
            patch.graph.neighbours(c).iter().map(move |&n| dc.abs_diff(dist[n as usize]))
        })
        .max()
        .unwrap_or(1);
    let (_, rule) = record.replay_setup(&patch);
    let centroid = |c: usize| {
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        let pts = &geom.xy[lo..hi];
        let n = pts.len() as f64;
        let s = pts.iter().fold([0.0; 2], |a, p| [a[0] + p[0], a[1] + p[1]]);
        [s[0] / n, s[1] / n]
    };

    // Interior anchors, mid-patch: up to `per_class` per tile class.
    let band = (radius / 5)..=(radius / 2);
    let mut anchors: Vec<(u16, u32)> = Vec::new();
    for class in 0..patch.classes.len() as u16 {
        let mut n = 0;
        for c in 0..patch.graph.cells() {
            if patch.cells[c as usize].class == class && band.contains(&dist[c as usize]) {
                anchors.push((class, c));
                n += 1;
                if n >= per_class {
                    break;
                }
            }
        }
    }

    let seedings: Vec<(u16, u32, u32, u8, u8)> = anchors
        .iter()
        .flat_map(|&(class, a)| {
            patch.graph.neighbours(a).iter().flat_map(move |&nb| {
                [(class, a, nb, s0, s1), (class, a, nb, s1, s0)]
            })
        })
        .collect();

    // Settledness gate on nomination. A launch's coarse boundary heading is
    // provisional: the object may still be turning when it reaches the
    // boundary. The hat lane-capture object launches near 282° but locks
    // 345.5° only ~600 rings out, so at small radius its boundary heading
    // reads ~282° — a mis-nomination. Nominate a seed only if its heading is
    // STABLE: the direction over the first half of the travelled path
    // (anchor→midpoint centroid) must agree with the second-half velocity
    // (midpoint→boundary centroid) to within SETTLE_TOL. Assessing this needs
    // the object to clear the launch region well before boundary contact, so
    // run the sweep at a radius/horizon large enough for a genuine turn to
    // manifest; a straight glider passes at any radius, a turner is caught
    // only once it has bent by more than the tolerance (empirically the hat
    // lane-capture object needs boundary displacement ≳ 400, i.e. r ≳ 160).
    const SETTLE_TOL: f64 = 10.0; // degrees, early-vs-late heading agreement
    const SETTLE_MIN_GENS: usize = 30; // path too short to judge ⇒ unsettled
    const SETTLE_MIN_TRAVEL: f64 = 24.0; // geom units of net displacement

    // Replay one seed: (heading°, max_pop, settled?). Heading is the recent
    // (second-half) velocity direction when the path is long enough to judge,
    // else the coarse anchor→front direction (reported but flagged unsettled).
    let run = |a: u32, nb: u32, sa: u8, sb: u8| -> Option<(f64, u32, bool)> {
        let mut engine = Engine::new(patch.graph.clone());
        engine.set_state(a, sa);
        engine.set_state(nb, sb);
        let anchor = centroid(a as usize);
        let mut max_pop = 0u32;
        let mut path: Vec<[f64; 2]> = Vec::new(); // active-set centroid per gen
        for _ in 1..=horizon {
            let st = rule.step(&mut engine);
            max_pop = max_pop.max(st.population);
            if st.population == 0 {
                return None; // died
            }
            if max_pop > 4 * record.max_population {
                return None; // grower
            }
            // Single pass: active-set centroid + farthest active cell.
            let (mut sx, mut sy, mut n) = (0.0f64, 0.0f64, 0.0f64);
            let mut far: Option<(usize, u32)> = None;
            for (c, &s) in engine.state().iter().enumerate() {
                if s == 0 {
                    continue;
                }
                let p = centroid(c);
                sx += p[0];
                sy += p[1];
                n += 1.0;
                let d = dist[c];
                if far.is_none_or(|(_, fd)| d > fd) {
                    far = Some((c, d));
                }
            }
            path.push([sx / n, sy / n]);
            if let Some((c, _)) = far
                && dist[c] + margin >= radius
            {
                // Boundary contact — glider-like. Coarse heading = anchor→front.
                let pf = centroid(c);
                let front_h =
                    (pf[1] - anchor[1]).atan2(pf[0] - anchor[0]).to_degrees().rem_euclid(360.0);
                // Settledness over the travelled path (centroid trajectory).
                let t = path.len();
                let end = path[t - 1];
                let net = (end[0] - anchor[0]).hypot(end[1] - anchor[1]);
                if t < SETTLE_MIN_GENS || net < SETTLE_MIN_TRAVEL {
                    return Some((front_h, max_pop, false));
                }
                let mid = path[t / 2];
                let early = (mid[1] - anchor[1]).atan2(mid[0] - anchor[0]).to_degrees();
                let late = (end[1] - mid[1]).atan2(end[0] - mid[0]).to_degrees();
                let diff = ((early - late + 540.0).rem_euclid(360.0) - 180.0).abs();
                return Some((late.rem_euclid(360.0), max_pop, diff <= SETTLE_TOL));
            }
        }
        None // active-at-horizon (not a clean glider)
    };

    let hits: Vec<Hit> = seedings
        .par_iter()
        .filter_map(|&(class, a, nb, sa, sb)| {
            run(a, nb, sa, sb).map(|(h, mp, settled)| (class, a, nb, sa, sb, h, mp, settled))
        })
        .collect();

    let settled_hits = hits.iter().filter(|h| h.7).count();
    println!(
        "ignition-heading sweep: {:?} {}\n  {} seedings, {} glider-like, {} settled (nominatable), {} unsettled/provisional",
        record.family, record.note.chars().take(50).collect::<String>(),
        seedings.len(), hits.len(), settled_hits, hits.len() - settled_hits
    );
    // Histogram over 6-fold spokes (10° bins). '#' = settled, '.' = unsettled.
    let mut bins = [(0usize, 0usize); 36];
    for &(.., h, _, settled) in &hits {
        let b = &mut bins[(h / 10.0) as usize % 36];
        if settled { b.0 += 1 } else { b.1 += 1 }
    }
    println!("  heading histogram (10° bins; # settled, . unsettled):");
    for (i, &(s, u)) in bins.iter().enumerate() {
        if s + u > 0 {
            println!(
                "    {:>3}–{:>3}°: {}{}",
                i * 10,
                i * 10 + 10,
                "#".repeat(s.min(60)),
                ".".repeat(u.min(60))
            );
        }
    }

    // Emit one representative record per 10° bin with a SETTLED hit (closest
    // to the bin centre by population = cleanest), rooted at the SOURCE root.
    // Unsettled (still-turning) launches are never nominated.
    let mut best: std::collections::HashMap<usize, (u32, u32, u8, u8, f64, u32)> =
        std::collections::HashMap::new();
    for &(_class, a, nb, sa, sb, h, mp, settled) in &hits {
        if !settled {
            continue;
        }
        let bin = (h / 10.0) as usize % 36;
        let e = best.entry(bin).or_insert((a, nb, sa, sb, h, u32::MAX));
        if mp < e.5 {
            *e = (a, nb, sa, sb, h, mp);
        }
    }
    let mut recs: Vec<ResultRecord> = Vec::new();
    let mut keys: Vec<usize> = best.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let (a, nb, sa, sb, h, mp) = best[&k];
        recs.push(ResultRecord {
            initial_cells: vec![a, nb],
            initial_states: vec![sa, sb],
            note: format!(
                "ignition heading {h:.1}° (bin {}-{}°, settled) maxpop {mp}, anchor cell {a}",
                k * 10,
                k * 10 + 10
            ),
            ..record.clone()
        });
    }
    println!("\n  {} representative seeds (one per hit bin):", recs.len());
    for r in &recs {
        println!("    {}", r.note);
    }
    if let Some(op) = out_path {
        let mut f = std::fs::File::create(&op).unwrap();
        for r in &recs {
            writeln!(f, "{}", serde_json::to_string(r).unwrap()).unwrap();
        }
        println!("  wrote {} seeds -> {op}", recs.len());
    }
}
