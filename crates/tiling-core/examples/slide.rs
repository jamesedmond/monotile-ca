//! Sliding-window glider flight tracker (FINDINGS §"Open questions" (h)).
//!
//! Tracks a single glider over unbounded graph distance at constant memory
//! by hopping a small patch window along the flight (mechanism in
//! `tiling_core::slide`). Three modes:
//!
//!   * default — fly to a target (rings or wall-clock), logging each hop,
//!     writing per-hop checkpoint records (each independently replayable);
//!   * `--validate R` — the soundness control: reproduce a monolithic
//!     radius-R run's nonzero (address, state) set at every sampled
//!     generation (exact match required);
//!   * `--bench` — measure rings/second across window radii and report the
//!     optimum.
//!
//! Usage:
//!   slide <record.jsonl> [index] [window_radius] [options]
//!
//! Options:
//!   --rings N          target cumulative rings         (default 100000)
//!   --secs S           target wall-clock seconds       (overrides --rings)
//!   --launch-radius R  initial launch window radius     (default 64)
//!   --launch-gens G    generations before first hop     (default 70)
//!   --max-window R     auto-grow cap                     (default 4×window)
//!   --pop-band P       population flat-band assertion    (default record max)
//!   --out FILE         write per-hop checkpoint JSONL
//!   --validate R       run the monolithic control at radius R and exit
//!   --bench            run the window-size benchmark and exit
//!   --bench-secs S     per-radius wall-clock for --bench (default 15)

use std::collections::BTreeSet;
use std::io::Write as _;
use std::time::Instant;

use tiling_core::Tiling;
use tiling_core::results::ResultRecord;
use tiling_core::slide::{SlideConfig, Slider, fan_margin, select_component, snapshot_of};

use ca_engine::{CaRule, Engine};

struct Args {
    path: String,
    index: usize,
    window_radius: u32,
    rings: u64,
    secs: Option<f64>,
    launch_radius: u32,
    launch_gens: u64,
    max_window: Option<u32>,
    pop_band: Option<u32>,
    launch_pop_band: Option<u32>,
    select_heading: Option<f64>,
    kappa_radius: u32,
    out: Option<String>,
    csv: Option<String>,
    frames: bool,
    validate: Option<u32>,
    bench: bool,
    bench_secs: f64,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut positional: Vec<String> = Vec::new();
    let mut a = Args {
        path: String::new(),
        index: 0,
        window_radius: 48,
        rings: 100_000,
        secs: None,
        launch_radius: 64,
        launch_gens: 70,
        max_window: None,
        pop_band: None,
        launch_pop_band: None,
        select_heading: None,
        kappa_radius: 128,
        out: None,
        csv: None,
        frames: true,
        validate: None,
        bench: false,
        bench_secs: 15.0,
    };
    let mut i = 0;
    while i < raw.len() {
        let arg = &raw[i];
        let mut val = || {
            i += 1;
            raw.get(i).cloned().expect("flag needs a value")
        };
        match arg.as_str() {
            "--rings" => a.rings = val().parse().unwrap(),
            "--secs" => a.secs = Some(val().parse().unwrap()),
            "--launch-radius" => a.launch_radius = val().parse().unwrap(),
            "--launch-gens" => a.launch_gens = val().parse().unwrap(),
            "--max-window" => a.max_window = Some(val().parse().unwrap()),
            "--pop-band" => a.pop_band = Some(val().parse().unwrap()),
            "--launch-pop-band" => a.launch_pop_band = Some(val().parse().unwrap()),
            "--select-heading" => a.select_heading = Some(val().parse().unwrap()),
            "--kappa-radius" => a.kappa_radius = val().parse().unwrap(),
            "--out" => a.out = Some(val()),
            "--csv" => a.csv = Some(val()),
            "--no-frames" => a.frames = false,
            "--validate" => a.validate = Some(val().parse().unwrap()),
            "--bench" => a.bench = true,
            "--bench-secs" => a.bench_secs = val().parse().unwrap(),
            _ => positional.push(arg.clone()),
        }
        i += 1;
    }
    a.path = positional.first().cloned().expect("record path");
    if let Some(s) = positional.get(1) {
        a.index = s.parse().unwrap();
    }
    if let Some(s) = positional.get(2) {
        a.window_radius = s.parse().unwrap();
    }
    a
}

fn load_record(path: &str, index: usize) -> ResultRecord {
    let text = std::fs::read_to_string(path).expect("read record file");
    let line = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(index)
        .expect("record index");
    serde_json::from_str(line).expect("parse record")
}

fn config(a: &Args, record: &ResultRecord) -> SlideConfig {
    let launch_pop_band = a.launch_pop_band.unwrap_or(record.max_population);
    SlideConfig {
        window_radius: a.window_radius,
        launch_radius: a.launch_radius,
        launch_gens: a.launch_gens,
        max_window_radius: a.max_window.unwrap_or(a.window_radius * 4),
        pop_band: a.pop_band.unwrap_or(launch_pop_band),
        launch_pop_band,
        track_frames: a.frames,
        select_heading: a.select_heading,
    }
}

fn main() {
    let a = parse_args();
    let record = load_record(&a.path, a.index);
    let tiling = Tiling::new(record.family);
    println!(
        "record {}[{}]: family {:?}, root {}, rule states {}, max_pop {}",
        a.path,
        a.index,
        record.family,
        record.root,
        record.table_rule.as_ref().map_or(2, |r| r.states),
        record.max_population,
    );

    if let Some(r) = a.validate {
        validate(&a, &tiling, &record, r);
        return;
    }
    if a.bench {
        bench(&a, &tiling, &record);
        return;
    }
    flight(&a, &tiling, &record);
}

/// Fly to the target, logging hops and writing checkpoints.
fn flight(a: &Args, tiling: &Tiling, record: &ResultRecord) {
    let cfg = config(a, record);
    let mut slider = Slider::launch(tiling, record, &cfg).expect("launch");
    println!(
        "launch: window_radius {}, launch_radius {}, launch_gens {}, pop_band {}, max_window {}",
        cfg.window_radius,
        cfg.launch_radius,
        cfg.launch_gens,
        cfg.pop_band,
        cfg.max_window_radius,
    );
    let target = a.rings;
    let deadline = a.secs;
    println!(
        "target: {}",
        match deadline {
            Some(s) => format!("{s} s"),
            None => format!("{target} rings"),
        }
    );
    // Effective-rings axis: kappa (rings per geometry unit), calibrated
    // once against the monolithic radius-384 arrival. effective = kappa ×
    // Euclidean displacement is the origin-anchored radius (O(1) error by
    // linear repetitivity), on the same axis as the verification figure.
    let kappa = if a.frames {
        let k = kappa_calibration(tiling, record, a.kappa_radius);
        println!(
            "kappa (rings per geometry unit, monolithic r{} calibration): {k:.5}",
            a.kappa_radius
        );
        k
    } else {
        0.0
    };
    println!(
        "\n{:>5} {:>8} {:>10} {:>4} {:>5} {:>4} {:>4} {:>5} {:>4} {:>8} {:>8} {:>8}",
        "hop", "gen", "pathR", "adv", "lclk", "pop", "cmp", "span", "r", "patch_ms", "head", "whead"
    );

    // Stream checkpoints to disk as they are produced (constant memory).
    let mut out_file = a.out.as_ref().map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).expect("create out file"))
    });
    let mut checkpoints_written = 0u64;
    // Per-hop CSV feeding the paper's plots-against-radius.
    let mut csv_file = a.csv.as_ref().map(|p| {
        let mut w =
            std::io::BufWriter::new(std::fs::File::create(p).expect("create csv"));
        writeln!(w, "generation,path_length_rings,euclid_distance,effective_rings,global_x,global_y,heading_running,heading_windowed,local_clock,elapsed_s,hop_ms,patch_ms")
            .unwrap();
        w
    });

    // Straightness bookkeeping: collect windowed headings and
    // (generation, displacement) for the fit.
    let mut headings: Vec<f64> = Vec::new();
    let mut fit_n = 0.0f64;
    let mut fit_sx = 0.0f64;
    let mut fit_sy = 0.0f64;
    let mut fit_sxx = 0.0f64;
    let mut fit_sxy = 0.0f64;
    let mut fit_syy = 0.0f64;

    let start = Instant::now();
    let mut last_hop_time = start;
    let mut patch_secs = 0.0f64;
    let mut grows = 0u64;
    let mut max_residual = 0.0f64;
    let mut max_snap = 0.0f64;
    let mut selection_reported = false;
    let mut last_report = Instant::now();
    let mut reached = false;
    loop {
        match slider.step() {
            Ok(Some(h)) => {
                patch_secs += h.patch_secs;
                if h.grew {
                    grows += 1;
                }
                if !selection_reported
                    && let Some(sel) = slider.selection()
                {
                    println!(
                        "SELECTION at gen {}: kept lane {:.1}° (pop {}) of {} components; dropped {:?}",
                        sel.generation,
                        sel.kept_heading,
                        sel.kept_pop,
                        sel.total_components,
                        sel.dropped
                    );
                    selection_reported = true;
                }
                max_residual = max_residual.max(h.frame_residual);
                max_snap = max_snap.max(h.rotation_snap);
                // Per-hop wall-clock: cumulative and this-hop (engine steps
                // since the last hop + this hop's patch-gen), to expose the
                // O(log R) address-depth cost as the flight goes deep.
                let now = Instant::now();
                let elapsed_s = now.duration_since(start).as_secs_f64();
                let hop_ms = now.duration_since(last_hop_time).as_secs_f64() * 1e3;
                last_hop_time = now;
                if a.frames {
                    let disp = h.euclid_distance;
                    let effective = kappa * disp;
                    if h.windowed_heading_deg.is_finite() {
                        headings.push(h.windowed_heading_deg);
                    }
                    // distance-from-launch vs generation (linear fit).
                    let (x, y) = (h.generation as f64, disp);
                    fit_n += 1.0;
                    fit_sx += x;
                    fit_sy += y;
                    fit_sxx += x * x;
                    fit_sxy += x * y;
                    fit_syy += y * y;
                    if let Some(w) = csv_file.as_mut() {
                        writeln!(
                            w,
                            "{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.3},{:.2},{:.2}",
                            h.generation,
                            h.path_length_rings,
                            disp,
                            effective,
                            h.global[0],
                            h.global[1],
                            h.heading_deg,
                            h.windowed_heading_deg,
                            h.local_clock,
                            elapsed_s,
                            hop_ms,
                            h.patch_secs * 1e3,
                        )
                        .unwrap();
                    }
                }
                if let (Some(w), Some(rec)) = (out_file.as_mut(), slider.last_checkpoint()) {
                    serde_json::to_writer(&mut *w, rec).unwrap();
                    w.write_all(b"\n").unwrap();
                    checkpoints_written += 1;
                }
                let show = h.hop <= 12
                    || h.hop.is_multiple_of(report_stride(target, h.advance.max(1)))
                    || last_report.elapsed().as_secs_f64() > 30.0;
                if show {
                    println!(
                        "{:>5} {:>8} {:>10} {:>4} {:>5.2} {:>4} {:>4} {:>5} {:>4} {:>8.2} {:>8.2} {:>8.2}",
                        h.hop,
                        h.generation,
                        h.path_length_rings,
                        h.advance,
                        h.local_clock,
                        h.population,
                        h.components,
                        h.object_span,
                        h.window_radius,
                        h.patch_secs * 1e3,
                        h.heading_deg,
                        h.windowed_heading_deg,
                    );
                    let _ = std::io::stdout().flush();
                    last_report = Instant::now();
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("\nABORT at gen {}: {e}", slider.generation());
                break;
            }
        }
        let done = match deadline {
            Some(s) => start.elapsed().as_secs_f64() >= s,
            None => slider.path_length_rings() >= target,
        };
        if done {
            reached = true;
            break;
        }
    }

    let wall = start.elapsed().as_secs_f64();
    let rings = slider.path_length_rings();
    let gens = slider.generation();
    println!("\n=== flight summary ===");
    println!("reached target: {reached}");
    println!("total generations: {gens}");
    println!("hops: {} (window grows: {grows})", slider.hops());
    println!(
        "population band: min {} max {} (asserted ≤ {})",
        slider.min_pop(),
        slider.max_pop(),
        cfg.pop_band
    );
    println!("wall clock: {wall:.2} s");
    println!(
        "  patch-gen: {patch_secs:.2} s ({:.0}%), engine+overhead: {:.2} s ({:.0}%)",
        100.0 * patch_secs / wall,
        wall - patch_secs,
        100.0 * (wall - patch_secs) / wall,
    );
    println!("hops/sec: {:.1}", slider.hops() as f64 / wall);

    println!("\n--- distance metrics ---");
    println!(
        "path length (Σ per-hop root-to-root BFS): {rings} rings  [upper bound on origin distance]",
    );

    if a.frames {
        let norm360 = |d: f64| d.rem_euclid(360.0);
        let euclid = slider.displacement();
        let effective = kappa * euclid;
        println!("Euclidean displacement from launch: {euclid:.2} (geometry units)");
        println!(
            "effective rings (kappa × displacement, origin-anchored): {:.0}",
            effective
        );
        if effective > 0.0 {
            println!(
                "gens / effective-ring (ORIGIN clock): {:.3}   [matches the 2.00 verification clock]",
                gens as f64 / effective
            );
        }
        let tortuosity = if euclid > 0.0 {
            slider.euclid_path_length() / euclid
        } else {
            f64::NAN
        };
        println!(
            "tortuosity (Euclidean path length / net displacement): {tortuosity:.4}  [~1.00 ⇒ straight; turn detector]"
        );
        println!(
            "path-length vs effective-ring ratio: {:.3}  [graph-metric inflation of short segments, not a turn]",
            rings as f64 / effective.max(1.0)
        );
        println!("rings/sec (effective): {:.0}", effective / wall);

        println!("\n--- global frame / heading ---");
        println!("final global position: ({:.2}, {:.2})", slider.global()[0], slider.global()[1]);
        println!(
            "final running heading: {:.3}° (CCW from +x, [0,360); known hat A = 226.6°)",
            norm360(slider.heading_deg())
        );
        if !headings.is_empty() {
            let mean = headings.iter().sum::<f64>() / headings.len() as f64;
            let maxdev = headings
                .iter()
                .map(|h| (h - mean).abs())
                .fold(0.0f64, f64::max);
            println!(
                "windowed heading over flight: mean {:.3}°, max deviation {maxdev:.3}° ({} samples)",
                norm360(mean),
                headings.len()
            );
        }
        println!(
            "frame model: max rigid residual {:.2e} (geom units), max rotation-snap {:.4}° (both ~0 ⇒ exact rigid, exact 30° orientation)",
            max_residual,
            max_snap.to_degrees()
        );
        // distance-from-launch vs generation: linear fit disp = slope*gen + b.
        if fit_n > 2.0 {
            let denom = fit_n * fit_sxx - fit_sx * fit_sx;
            let slope = (fit_n * fit_sxy - fit_sx * fit_sy) / denom;
            let intercept = (fit_sy - slope * fit_sx) / fit_n;
            let mean_y = fit_sy / fit_n;
            // R^2 for the linear fit (straightness of distance-vs-time).
            let ss_tot = fit_syy - fit_n * mean_y * mean_y;
            let ss_res = fit_syy - intercept * fit_sy - slope * fit_sxy;
            let r2 = 1.0 - ss_res / ss_tot;
            println!(
                "distance-vs-generation linear fit: displacement = {slope:.5}·gen + {intercept:.3} (geom/gen), R² = {r2:.6}"
            );
        }
    }
    println!("final window root: {}", slider.current_root());

    if let Some(mut w) = out_file {
        w.flush().expect("flush checkpoints");
        println!(
            "wrote {checkpoints_written} checkpoint records to {}",
            a.out.as_ref().unwrap()
        );
    }
    if let Some(mut w) = csv_file {
        w.flush().expect("flush csv");
        println!("wrote per-hop CSV to {}", a.csv.as_ref().unwrap());
    }
}

/// Print roughly one hop line per ~2% of the target.
fn report_stride(target_rings: u64, advance: u32) -> u64 {
    let est_hops = (target_rings / u64::from(advance)).max(1);
    (est_hops / 50).max(1)
}

/// Kappa = graph rings per geometry unit, from a monolithic run: the SLOPE
/// of the leading cell's BFS ring distance against its straight-line
/// geometry displacement, fitted over the flight. Using the slope (not the
/// single-point ratio) cancels the launch offset — the object's effective
/// origin is not exactly cell 0 — so kappa converges at a modest radius
/// instead of needing the memory-heavy radius-384 patch. A pure tiling
/// constant, used to put the frame-composed Euclidean displacement on the
/// same graph-ring radius axis as the verification figure.
fn kappa_calibration(tiling: &Tiling, record: &ResultRecord, radius: u32) -> f64 {
    let (patch, geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .unwrap();
    let dist: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let margin = fan_margin(&patch);
    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = ca_engine::Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());
    let centroid = |c: usize| {
        let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
        let pts = &geom.xy[lo..hi];
        let n = pts.len() as f64;
        let s = pts.iter().fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
        [s[0] / n, s[1] / n]
    };
    let launch = centroid(0);
    // Sample (euclid displacement, BFS ring) of the leading cell each
    // generation once it is clear of the launch region (linear regime).
    let (mut n, mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for _ in 1.. {
        rule.step(&mut engine);
        let front = engine
            .state()
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s != 0)
            .max_by_key(|&(c, _)| dist[c]);
        let Some((c, _)) = front else { break };
        if dist[c] + margin + 2 >= radius {
            break;
        }
        if dist[c] * 3 > radius {
            let p = centroid(c);
            let x = (p[0] - launch[0]).hypot(p[1] - launch[1]); // euclid
            let y = f64::from(dist[c]); // ring
            n += 1.0;
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
        }
    }
    // Slope of ring vs euclid = rings per geometry unit.
    let denom = n * sxx - sx * sx;
    if n >= 2.0 && denom.abs() > 1e-9 {
        (n * sxy - sx * sy) / denom
    } else {
        0.2354 // fallback (~1/tile spacing)
    }
}

/// Cells to zero in the monolithic control to mirror the sliding run's lane
/// selection: everything NOT in the component nearest `hint` (same
/// `select_component` logic the flight uses). Empty if ≤ 1 component.
fn mono_select(
    patch: &tiling_core::Patch,
    state: &[u8],
    centroid: &impl Fn(usize) -> [f64; 2],
    launch: [f64; 2],
    hint: f64,
) -> Vec<u32> {
    use std::collections::HashSet;
    let nz: Vec<u32> = state
        .iter()
        .enumerate()
        .filter(|&(_, &s)| s != 0)
        .map(|(c, _)| c as u32)
        .collect();
    let set: HashSet<u32> = nz.iter().copied().collect();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut comps: Vec<Vec<u32>> = Vec::new();
    for &start in &nz {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut members = vec![start];
        while let Some(c) = stack.pop() {
            for &nb in patch.graph.neighbours(c) {
                if set.contains(&nb) && seen.insert(nb) {
                    stack.push(nb);
                    members.push(nb);
                }
            }
        }
        comps.push(members);
    }
    if comps.len() <= 1 {
        return Vec::new();
    }
    let dirs: Vec<f64> = comps
        .iter()
        .map(|m| {
            let n = m.len() as f64;
            let sum = m.iter().fold([0.0; 2], |a, &c| {
                let p = centroid(c as usize);
                [a[0] + p[0], a[1] + p[1]]
            });
            let cen = [sum[0] / n, sum[1] / n];
            (cen[1] - launch[1])
                .atan2(cen[0] - launch[0])
                .to_degrees()
                .rem_euclid(360.0)
        })
        .collect();
    let sel = select_component(&dirs, hint)
        .expect("monolithic selection matches the sliding run's");
    comps
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != sel)
        .flat_map(|(_, m)| m.iter().copied())
        .collect()
}

/// The soundness control: the sliding run must reproduce a monolithic
/// run's nonzero (address, state) set at every sampled generation.
fn validate(a: &Args, tiling: &Tiling, record: &ResultRecord, radius: u32) {
    const SAMPLE: u64 = 50;
    println!("\n=== validation control: sliding vs monolithic radius {radius} ===");

    // Monolithic reference: snapshot nonzero (address, state) every SAMPLE
    // generations up to boundary contact. When lane selection is in play,
    // apply the SAME selection at the SAME generation here (zero the
    // non-selected components in the monolithic engine — legitimate in the
    // control), so both runs carry only the hero thereafter.
    let t0 = Instant::now();
    let (mono, mono_geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .unwrap();
    let mono_dist: Vec<u32> = mono.cells.iter().map(|c| c.distance).collect();
    let mono_margin = fan_margin(&mono);
    let (mono_strata, mono_rule) = record.replay_setup(&mono);
    let mut me = Engine::with_strata(mono.graph.clone(), mono_strata);
    me.load_state(&record.initial_state(mono.graph.cells()).unwrap());
    let centroid = |c: usize| {
        let (lo, hi) = (mono_geom.offsets[c] as usize, mono_geom.offsets[c + 1] as usize);
        let pts = &mono_geom.xy[lo..hi];
        let n = pts.len() as f64;
        let s = pts.iter().fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
        [s[0] / n, s[1] / n]
    };
    let mono_launch = centroid(0);
    let mut samples: Vec<(u64, BTreeSet<(String, u8)>)> = Vec::new();
    let mut g = 0u64;
    loop {
        // Sample first, THEN apply lane selection — the sliding run's
        // transition hop fires entering gen launch_gens+1, so its snapshot
        // AT launch_gens is still the full launch (all lanes). Zeroing after
        // the sample matches that timing; from launch_gens+1 both carry only
        // the hero.
        if g.is_multiple_of(SAMPLE) {
            samples.push((g, snapshot_of(&mono, me.state()).into_iter().collect()));
        }
        if let Some(hint) = a.select_heading
            && g == a.launch_gens
        {
            let drop = mono_select(&mono, me.state(), &centroid, mono_launch, hint);
            for c in drop {
                me.set_state(c, 0);
            }
            println!("monolithic: applied lane selection (hint {hint:.1}°) at gen {g}");
        }
        let touched = me
            .state()
            .iter()
            .enumerate()
            .any(|(c, &s)| s != 0 && mono_dist[c] + mono_margin >= radius);
        if touched || me.population() == 0 {
            break;
        }
        mono_rule.step(&mut me);
        g += 1;
    }
    println!(
        "monolithic flight: {g} generations to boundary, {} samples (every {SAMPLE} gens), {:.1} s",
        samples.len(),
        t0.elapsed().as_secs_f64(),
    );

    // Sliding run: sample at the same generations.
    let cfg = config(a, record);
    let mut slider = Slider::launch(tiling, record, &cfg).expect("launch");
    println!(
        "sliding: window_radius {}, launch_radius {}, launch_gens {}",
        cfg.window_radius, cfg.launch_radius, cfg.launch_gens
    );
    let mut compared = 0u64;
    let mut mismatches = 0u64;
    for (sg, want) in &samples {
        while slider.generation() < *sg {
            slider.step().expect("slide step");
        }
        let got: BTreeSet<(String, u8)> = slider.snapshot().into_iter().collect();
        if &got != want {
            mismatches += 1;
            if mismatches <= 3 {
                let only_mono: Vec<_> = want.difference(&got).take(4).collect();
                let only_slide: Vec<_> = got.difference(want).take(4).collect();
                eprintln!(
                    "  MISMATCH at gen {sg}: mono-only {only_mono:?}, slide-only {only_slide:?}"
                );
            }
        }
        compared += 1;
    }
    println!(
        "compared {compared} sampled generations (gens 0..{}, every {SAMPLE})",
        samples.last().unwrap().0
    );
    println!("hops during sliding run: {}", slider.hops());
    if mismatches == 0 {
        println!("RESULT: EXACT MATCH at every sampled generation — mechanism sound.");
    } else {
        println!("RESULT: {mismatches} MISMATCH(es) — mechanism BROKEN.");
        std::process::exit(1);
    }
}

/// Window-size benchmark: rings/second across window radii.
fn bench(a: &Args, tiling: &Tiling, record: &ResultRecord) {
    let radii = [16u32, 24, 32, 48, 64];
    println!(
        "\n=== window-size benchmark ({} s per radius) ===",
        a.bench_secs
    );
    println!(
        "{:>4} {:>6} {:>10} {:>9} {:>8} {:>7} {:>7} {:>6}",
        "r", "hops", "rings", "rings/s", "gens", "patch%", "adv", "eff_r"
    );
    let mut best: Option<(u32, f64)> = None;
    for &r in &radii {
        let cfg = SlideConfig {
            window_radius: r,
            launch_radius: a.launch_radius,
            launch_gens: a.launch_gens,
            max_window_radius: a.max_window.unwrap_or(r * 4),
            pop_band: a.pop_band.unwrap_or(record.max_population),
            launch_pop_band: record.max_population,
            track_frames: false,
            select_heading: None,
        };
        let mut slider = match Slider::launch(tiling, record, &cfg) {
            Ok(s) => s,
            Err(e) => {
                println!("{r:>4}  launch failed: {e}");
                continue;
            }
        };
        let start = Instant::now();
        let mut patch_secs = 0.0f64;
        let mut eff_r = r;
        let mut aborted = None;
        loop {
            match slider.step() {
                Ok(Some(h)) => {
                    patch_secs += h.patch_secs;
                    eff_r = eff_r.max(h.window_radius);
                }
                Ok(None) => {}
                Err(e) => {
                    aborted = Some(e);
                    break;
                }
            }
            // Only start the clock check after launch, and stop at deadline.
            if start.elapsed().as_secs_f64() >= a.bench_secs && slider.hops() > 0 {
                break;
            }
        }
        let wall = start.elapsed().as_secs_f64();
        let rings = slider.path_length_rings();
        let rps = rings as f64 / wall;
        let adv = if slider.hops() > 0 {
            rings as f64 / slider.hops() as f64
        } else {
            0.0
        };
        let ok = aborted.is_none();
        println!(
            "{r:>4} {:>6} {:>10} {:>9.0} {:>8} {:>6.0}% {:>7.1} {:>6}{}",
            slider.hops(),
            rings,
            rps,
            slider.generation(),
            100.0 * patch_secs / wall,
            adv,
            eff_r,
            aborted.map_or(String::new(), |e| format!("  (abort: {e})")),
        );
        if ok {
            match best {
                Some((_, b)) if b >= rps => {}
                _ => best = Some((eff_r, rps)),
            }
        }
    }
    if let Some((r, rps)) = best {
        println!("\noptimum: effective window radius {r} at {rps:.0} rings/sec");
    }
}
