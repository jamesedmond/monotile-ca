//! Sliding-window flight tracker (FINDINGS §"Open questions" (h)).
//!
//! Tracks a single glider over unbounded graph distance at constant
//! memory by hopping a small patch *window* along the flight. The loop:
//!
//!   1. replay on the current window until the object's leading edge
//!      nears the window boundary (the *trigger* ring);
//!   2. extract the entire nonzero state (address + state per live cell);
//!   3. re-root a fresh window of the same radius at the object's leading
//!      cell (its canonical address is directly usable as a root — the
//!      addresses are canonical-by-construction, verified);
//!   4. blit the extracted cells into the new window *by address* (every
//!      cell must resolve — asserted);
//!   5. continue. Repeat indefinitely.
//!
//! Soundness rests on three invariants, all asserted:
//!
//! * **Margin invariant** — no active cell is ever within `fan_margin + 2`
//!   rings of the window boundary. `fan_margin` (the max per-step ring
//!   jump; 2 for the monotile vertex neighbourhood) is *computed from each
//!   window*, not hard-coded. Violation ⇒ the window is too small; the hop
//!   is redone with a larger window (auto-grow, capped), else a hard error.
//! * **Full-state extraction** — *every* nonzero cell is extracted, so no
//!   state is ever dropped. The extracted set must be a single graph
//!   component (our gliders leave no live wake) or, at worst, a spatially
//!   localised cluster (the glider's internal phase momentarily splits
//!   into adjacent pieces within a few rings); anything non-local ⇒ hard
//!   error. Normal hops extract at a single-component generation by
//!   construction; forced hops (at the hard limit) tolerate a localised
//!   split and report it.
//! * **Address resolution** — every extracted address resolves to a cell
//!   in the new window (asserted); this doubles as the localisation check.
//!
//! Exactness is proven end-to-end by the monolithic replay control
//! (`examples/slide.rs --validate`): the sliding run reproduces a large
//! monolithic run's nonzero `(address, state)` set at every sampled
//! generation.
//!
//! Graph-distance ring bookkeeping only: per-hop advance is the leading
//! edge's graph distance from the window root at extraction (the object is
//! re-rooted at that cell, so the front resets to ring 0 each hop).
//! Heading / rigid-frame composition across hops is deliberately out of
//! scope for this build.

use std::collections::{HashMap, VecDeque};
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use ca_engine::classify::Outcome;
use ca_engine::{AnyRule, CaRule, Engine};

use crate::results::ResultRecord;
use crate::{Geometry, Neighbourhood, Patch, Stratification, Tiling};

/// Rings of clearance between the margin invariant limit and the patch
/// boundary (`fan_margin + MARGIN_EXTRA` rings are kept live-free).
const MARGIN_EXTRA: u32 = 2;
/// Rings the leading edge may advance past the trigger while waiting for
/// a single-component phase before the hop is forced.
const DEFER_RINGS: u32 = 6;
/// Largest object ring-span accepted as "one localised object". Glider
/// A's is ≤ 5; a wake or shed second glider would be tens of rings away.
const OBJECT_MAX_SPAN: u32 = 24;

/// Per-window safety thresholds derived from the window radius and the
/// window's own fan margin. All in edge-BFS rings from the window root.
#[derive(Clone, Copy, Debug)]
struct Thresholds {
    /// Active cells must never exceed this distance (the margin invariant).
    safe_max: u32,
    /// A hop is forced once the leading edge reaches this distance (one
    /// further step still cannot breach `safe_max`).
    hard_limit: u32,
    /// A hop is sought (at the next single-component generation) once the
    /// leading edge reaches this distance.
    trigger: u32,
}

fn thresholds(radius: u32, fan_margin: u32) -> Thresholds {
    let safe_max = radius.saturating_sub(fan_margin + MARGIN_EXTRA);
    let hard_limit = safe_max.saturating_sub(fan_margin);
    let trigger = hard_limit.saturating_sub(DEFER_RINGS);
    Thresholds {
        safe_max,
        hard_limit,
        trigger,
    }
}

/// The max per-step ring jump across any graph edge of a patch — the
/// vertex-neighbourhood "fan margin" (computed, not hard-coded).
pub fn fan_margin(patch: &Patch) -> u32 {
    let dist: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
    let mut m = 1u32;
    for c in 0..patch.graph.cells() {
        for &n in patch.graph.neighbours(c) {
            m = m.max(dist[c as usize].abs_diff(dist[n as usize]));
        }
    }
    m
}

/// Rigid-transform residual above which the cross-window frame model is
/// judged broken (in geometry units; exact-arithmetic placements round to
/// ~1e-9, so this is ~1000× slack while still catching a wrong model).
const FRAME_RESIDUAL_TOL: f64 = 1e-3;
/// Tolerance (radians) for snapping a per-hop rotation to the tiling's
/// finite orientation set (~0.6°). Exceeding it means the rotation is not
/// one of the tiling's orientations — the frame model is wrong.
const ROTATION_SNAP_TOL: f64 = 1e-2;
/// Number of recent hops the windowed heading averages over.
const HEADING_WINDOW: usize = 20;
/// Minimum gap (degrees) between the nearest and second-nearest launch
/// component's angular distance to the `select_heading` hint. Below this
/// the hint cannot distinguish the lanes — hard error rather than guess.
const SELECT_AMBIGUITY_GAP: f64 = 20.0;
/// The hint must be within this many degrees of *some* launch component,
/// else it matches no lane (underspecified) — hard error.
const SELECT_MAX_MISS: f64 = 45.0;

/// A 2-D rigid transform (rotation + translation): `p ↦ M·p + t`.
#[derive(Clone, Copy, Debug)]
pub struct Affine {
    m00: f64,
    m01: f64,
    m10: f64,
    m11: f64,
    tx: f64,
    ty: f64,
}

impl Affine {
    pub fn identity() -> Self {
        Self { m00: 1.0, m01: 0.0, m10: 0.0, m11: 1.0, tx: 0.0, ty: 0.0 }
    }

    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.m00 * p[0] + self.m01 * p[1] + self.tx,
            self.m10 * p[0] + self.m11 * p[1] + self.ty,
        ]
    }

    /// `self ∘ other`: the transform that applies `other` then `self`.
    pub fn compose(&self, other: &Affine) -> Affine {
        Affine {
            m00: self.m00 * other.m00 + self.m01 * other.m10,
            m01: self.m00 * other.m01 + self.m01 * other.m11,
            m10: self.m10 * other.m00 + self.m11 * other.m10,
            m11: self.m10 * other.m01 + self.m11 * other.m11,
            tx: self.m00 * other.tx + self.m01 * other.ty + self.tx,
            ty: self.m10 * other.tx + self.m11 * other.ty + self.ty,
        }
    }

    /// The transform's rotation angle (radians).
    pub fn angle(&self) -> f64 {
        self.m10.atan2(self.m00)
    }

    /// Coefficients `[m00, m01, m10, m11, tx, ty]` (row-major linear part
    /// then translation), for consumers that need the raw transform.
    pub fn coeffs(&self) -> [f64; 6] {
        [self.m00, self.m01, self.m10, self.m11, self.tx, self.ty]
    }
}

/// Best rigid transform `S` with `a_i ≈ S(b_i)` (2-D Kabsch/Procrustes,
/// rotation only — no scale, no reflection), plus the RMS residual and the
/// rotation angle. The monotile placements are exact, so for a correct
/// frame model the residual is ~1e-9 (f64 rounding); a large residual means
/// the two windows are not related by a rigid motion.
pub fn derive_rigid(a: &[[f64; 2]], b: &[[f64; 2]]) -> (Affine, f64, f64) {
    let n = a.len() as f64;
    let abar = a.iter().fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
    let bbar = b.iter().fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
    let abar = [abar[0] / n, abar[1] / n];
    let bbar = [bbar[0] / n, bbar[1] / n];
    let (mut sxx, mut sxy, mut syx, mut syy) = (0.0, 0.0, 0.0, 0.0);
    for (pa, pb) in a.iter().zip(b.iter()) {
        let (ax, ay) = (pa[0] - abar[0], pa[1] - abar[1]);
        let (bx, by) = (pb[0] - bbar[0], pb[1] - bbar[1]);
        sxx += ax * bx;
        sxy += ax * by;
        syx += ay * bx;
        syy += ay * by;
    }
    // 2-D optimal rotation: θ = atan2(Σ(a'_y b'_x − a'_x b'_y), Σ(a'·b')).
    let theta = (syx - sxy).atan2(sxx + syy);
    let (s, c) = theta.sin_cos();
    let aff = Affine {
        m00: c,
        m01: -s,
        m10: s,
        m11: c,
        tx: abar[0] - (c * bbar[0] - s * bbar[1]),
        ty: abar[1] - (s * bbar[0] + c * bbar[1]),
    };
    let mut sse = 0.0;
    for (pa, pb) in a.iter().zip(b.iter()) {
        let q = aff.apply(*pb);
        sse += (q[0] - pa[0]).powi(2) + (q[1] - pa[1]).powi(2);
    }
    (aff, (sse / n).sqrt(), theta)
}

/// Angular distance (degrees, 0..180) between two headings.
fn ang_dist(a: f64, b: f64) -> f64 {
    let x = (a - b).rem_euclid(360.0);
    x.min(360.0 - x)
}

/// Choose the component whose centroid heading is nearest `hint`. `dirs`
/// is one heading (degrees) per component. Errors if the hint matches no
/// lane (`SELECT_MAX_MISS`) or cannot distinguish the two nearest lanes
/// (`SELECT_AMBIGUITY_GAP`) — never guesses. Shared by the flight and the
/// monolithic validation control so both select identically.
pub fn select_component(dirs: &[f64], hint: f64) -> Result<usize, String> {
    let mut order: Vec<(usize, f64)> = dirs
        .iter()
        .enumerate()
        .map(|(i, &d)| (i, ang_dist(d, hint)))
        .collect();
    order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let (nearest_i, nearest_d) = order[0];
    if nearest_d > SELECT_MAX_MISS {
        return Err(format!(
            "hint {hint:.1}° matches no lane (nearest lane {nearest_d:.1}° away)"
        ));
    }
    if let Some(&(_, second_d)) = order.get(1)
        && second_d - nearest_d < SELECT_AMBIGUITY_GAP
    {
        return Err(format!(
            "hint {hint:.1}° ambiguous: nearest two lanes at {nearest_d:.1}° and {second_d:.1}° distance"
        ));
    }
    Ok(nearest_i)
}

/// Per-cell polygon centroids from a patch geometry (index-aligned).
fn centroids(geom: &crate::Geometry) -> Vec<[f64; 2]> {
    (0..geom.offsets.len() - 1)
        .map(|c| {
            let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
            let pts = &geom.xy[lo..hi];
            let n = pts.len() as f64;
            let s = pts.iter().fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
            [s[0] / n, s[1] / n]
        })
        .collect()
}

/// Configuration for a sliding flight.
#[derive(Clone, Debug)]
pub struct SlideConfig {
    /// Radius of the (small) flight window used for every hop.
    pub window_radius: u32,
    /// Radius of the initial launch window (must hold all launch activity
    /// through `launch_gens`; the monotile launch emits two gliders, one
    /// of which dies ~gen 42, so `launch_gens` should be ≳ 60).
    pub launch_radius: u32,
    /// Generations run on the launch window before the first hop, by which
    /// point the flight must be a single object.
    pub launch_gens: u64,
    /// Auto-grow cap: a hop whose object does not fit the window grows the
    /// window (×3/2) up to this radius before erroring.
    pub max_window_radius: u32,
    /// Population cap during the flight (after selection). The hero's flat
    /// band; exceeding it signals a grower/explosion — hard error.
    pub pop_band: u32,
    /// Population cap during the launch phase (before the first hop), which
    /// holds every object (e.g. hat C's three-glider shower peaks at 32).
    pub launch_pop_band: u32,
    /// Track the global frame: generate geometry per window, compose the
    /// exact rigid transform between consecutive windows, and report the
    /// object's global position and heading. Adds patch-gen cost (geometry).
    pub track_frames: bool,
    /// Lane selection for a multi-object launch: at the launch→flight
    /// transition, if the state has several graph components, keep only the
    /// one whose centroid direction from the launch root (degrees CCW from
    /// +x, launch-window geometry) is nearest this hint and drop the rest
    /// (sound by the light-cone argument — diverging lanes never re-reach
    /// the hero). Requires `track_frames`. `None` ⇒ keep all (the launch
    /// must already be a single localised object).
    pub select_heading: Option<f64>,
}

/// What a lane selection kept and dropped (provenance for the first
/// checkpoint and the log).
#[derive(Clone, Debug)]
pub struct Selection {
    pub kept_heading: f64,
    pub kept_pop: u32,
    /// Dropped components as (centroid heading°, population).
    pub dropped: Vec<(f64, u32)>,
    pub total_components: usize,
    pub generation: u64,
}

/// One hop's log line.
#[derive(Clone, Debug)]
pub struct HopLog {
    pub hop: u64,
    /// Cumulative total generation (from the original seed) at the hop.
    pub generation: u64,
    /// Cumulative **path length** in graph rings: the sum of per-hop
    /// root-to-root BFS distances. This is an *upper bound* on the origin
    /// graph distance (triangle inequality), not the distance from launch —
    /// use `euclid_distance`/effective rings for that.
    pub path_length_rings: u64,
    /// This hop's exact root-to-root advance (previous window root's BFS
    /// distance from the new root — the graph distance the root moved).
    pub advance: u32,
    /// This hop's local clock: generations this hop / `advance` rings. A
    /// per-hop constant-speed instrument (stable ⇒ constant speed).
    pub local_clock: f64,
    pub population: u32,
    /// Graph-connected components of the extracted nonzero set.
    pub components: usize,
    /// Ring-span of the extracted object (max − min distance).
    pub object_span: u32,
    /// Radius actually used for the new window (post any auto-grow).
    pub window_radius: u32,
    /// Whether the window had to grow this hop.
    pub grew: bool,
    /// Wall-clock spent generating the new window.
    pub patch_secs: f64,
    /// Object global position in the launch frame (0,0 if frames untracked).
    pub global: [f64; 2],
    /// Straight-line Euclidean distance from the launch point in geometry
    /// units (0 if frames untracked) — the primary global distance metric.
    pub euclid_distance: f64,
    /// Running heading: atan2 of cumulative displacement from the launch
    /// point, degrees CCW from +x (NaN until there is displacement).
    pub heading_deg: f64,
    /// Heading over the last `HEADING_WINDOW` hops (a mid-flight turn shows
    /// here immediately; NaN before enough history).
    pub windowed_heading_deg: f64,
    /// This hop's frame-composition rigid residual (geometry units) and the
    /// deviation of its rotation from the nearest tiling orientation (rad);
    /// both ~0 when frames are tracked, both 0 when they are not.
    pub frame_residual: f64,
    pub rotation_snap: f64,
}

/// Why a flight aborted. All are hard soundness stops, not warnings.
#[derive(Clone, Debug)]
pub enum SlideError {
    /// The margin invariant was breached — the window is too small and
    /// could not be grown within the cap. Redo with a larger window.
    MarginInvariant {
        generation: u64,
        max_dist: u32,
        safe_max: u32,
        window_radius: u32,
    },
    /// A window big enough to hold the object (with room to fly) could not
    /// be built within `max_window_radius`.
    WindowTooSmall {
        window_radius: u32,
        object_span: u32,
    },
    /// An extracted cell's address did not resolve in the new window (its
    /// canonical address was absent) — placement would be unsound.
    Unresolved {
        address: String,
        window_radius: u32,
    },
    /// The extracted nonzero set is not one localised object (a wake or a
    /// second object appeared) — outside the clean single-glider regime.
    NotLocalised { generation: u64, object_span: u32 },
    /// The object went extinct (all cells dead) — nothing to track.
    Extinct { generation: u64 },
    /// Population left the flat band — a grower/explosion, not a glider.
    PopulationEscaped {
        generation: u64,
        population: u32,
        band: u32,
    },
    /// Patch generation failed (bad root address).
    Patch(String),
    /// Cross-window frame composition failed: either too few shared tiles
    /// to fit a transform, a large rigid residual (the windows are not
    /// related by a rigid motion), or a rotation that is not one of the
    /// tiling's finite orientations. Any of these means the frame model is
    /// wrong — stop rather than accumulate a bad global position.
    FrameModel {
        generation: u64,
        detail: String,
    },
}

impl fmt::Display for SlideError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SlideError::MarginInvariant {
                generation,
                max_dist,
                safe_max,
                window_radius,
            } => write!(
                f,
                "margin invariant breached at gen {generation}: active cell at ring {max_dist} > safe_max {safe_max} (window radius {window_radius}); use a larger window"
            ),
            SlideError::WindowTooSmall {
                window_radius,
                object_span,
            } => write!(
                f,
                "object (ring-span {object_span}) does not fit a radius-{window_radius} window with room to fly (grow cap reached)"
            ),
            SlideError::Unresolved {
                address,
                window_radius,
            } => write!(
                f,
                "extracted cell address did not resolve in the radius-{window_radius} window: {address}"
            ),
            SlideError::NotLocalised {
                generation,
                object_span,
            } => write!(
                f,
                "nonzero set at gen {generation} spans {object_span} rings (> {OBJECT_MAX_SPAN}): not one localised object (wake/second object?)"
            ),
            SlideError::Extinct { generation } => {
                write!(f, "object went extinct at gen {generation}")
            }
            SlideError::PopulationEscaped {
                generation,
                population,
                band,
            } => write!(
                f,
                "population {population} left the flat band (≤ {band}) at gen {generation}: grower, not glider"
            ),
            SlideError::Patch(e) => write!(f, "patch generation failed: {e}"),
            SlideError::FrameModel { generation, detail } => write!(
                f,
                "cross-window frame model failed at gen {generation}: {detail}"
            ),
        }
    }
}

impl std::error::Error for SlideError {}

/// A single-object flight tracked through a sliding window.
pub struct Slider<'a> {
    tiling: &'a Tiling,
    /// The source record (family, neighbourhood, rule, display fields).
    source: ResultRecord,
    rule: AnyRule,
    neighbourhood: Neighbourhood,
    scheme: Stratification,
    window_radius: u32,
    max_window_radius: u32,
    launch_gens: u64,
    pop_band: u32,
    launch_pop_band: u32,
    track_frames: bool,
    select_heading: Option<f64>,
    selection: Option<Selection>,

    // Current window.
    patch: Patch,
    engine: Engine,
    dist: Vec<u32>,
    addr: Vec<String>,
    thresh: Thresholds,
    /// Per-cell centroids of the current window (empty unless tracking frames).
    centroids: Vec<[f64; 2]>,
    /// Full geometry of the current window, retained when tracking frames
    /// (already generated for the centroids) so render consumers need not
    /// regenerate the patch.
    geometry: Option<Geometry>,

    // Bookkeeping (cumulative from the original seed).
    generation: u64,
    /// Graph path length: sum of per-hop root-to-root distances (an upper
    /// bound on origin distance, not the distance from launch).
    path_length_rings: u64,
    /// Total generation at the previous hop (for the per-hop local clock).
    prev_hop_generation: u64,
    hops: u64,
    launched: bool,

    // Global frame (valid only when `track_frames`).
    /// Maps current-window coordinates into the launch frame.
    frame: Affine,
    launch_point: [f64; 2],
    /// Object global position in the launch frame (the current window root).
    global: [f64; 2],
    /// Previous hop's global position (for the Euclidean path length).
    prev_global: [f64; 2],
    /// Sum of per-hop Euclidean segment lengths (geometry units); with the
    /// net displacement this gives the geometric tortuosity.
    euclid_path_length: f64,
    /// Recent (path_length, global) history for the windowed heading.
    positions: VecDeque<(u64, [f64; 2])>,
    max_frame_residual: f64,
    max_rotation_snap: f64,

    // Provenance and stats. Only the most recent checkpoint is retained
    // (the caller streams it to disk), so memory stays constant regardless
    // of flight length.
    last_checkpoint: Option<ResultRecord>,
    min_pop: u32,
    max_pop: u32,
}

impl<'a> Slider<'a> {
    /// Build the launch window at the record's root and load its seed.
    pub fn launch(
        tiling: &'a Tiling,
        record: &ResultRecord,
        cfg: &SlideConfig,
    ) -> Result<Self, SlideError> {
        if cfg.select_heading.is_some() && !cfg.track_frames {
            return Err(SlideError::FrameModel {
                generation: 0,
                detail: "lane selection needs track_frames (component directions come from geometry)".into(),
            });
        }
        let (patch, geometry, centroids) = if cfg.track_frames {
            let (patch, geom) = tiling
                .generate_patch_with_geometry(
                    &record.root,
                    cfg.launch_radius,
                    record.neighbourhood,
                )
                .map_err(|e| SlideError::Patch(e.to_string()))?;
            let c = centroids(&geom);
            (patch, Some(geom), c)
        } else {
            let patch = tiling
                .generate_patch(&record.root, cfg.launch_radius, record.neighbourhood)
                .map_err(|e| SlideError::Patch(e.to_string()))?;
            (patch, None, Vec::new())
        };
        let (strata, rule) = record.replay_setup(&patch);
        let mut engine = Engine::with_strata(patch.graph.clone(), strata);
        let seed = record
            .initial_state(patch.graph.cells())
            .map_err(|e| SlideError::Patch(e.to_string()))?;
        engine.load_state(&seed);

        let dist: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
        let addr: Vec<String> = patch.cells.iter().map(|c| c.address.clone()).collect();
        let thresh = thresholds(cfg.launch_radius, fan_margin(&patch));
        let pop = engine.population();
        // Launch point = the launch root (cell 0) in its own frame.
        let launch_point = centroids.first().copied().unwrap_or([0.0, 0.0]);

        Ok(Self {
            tiling,
            source: record.clone(),
            rule,
            neighbourhood: record.neighbourhood,
            scheme: record.scheme(),
            window_radius: cfg.window_radius,
            max_window_radius: cfg.max_window_radius,
            launch_gens: cfg.launch_gens,
            pop_band: cfg.pop_band,
            launch_pop_band: cfg.launch_pop_band,
            track_frames: cfg.track_frames,
            select_heading: cfg.select_heading,
            selection: None,
            patch,
            engine,
            dist,
            addr,
            thresh,
            centroids,
            geometry,
            generation: 0,
            path_length_rings: 0,
            prev_hop_generation: 0,
            hops: 0,
            launched: false,
            frame: Affine::identity(),
            launch_point,
            global: launch_point,
            prev_global: launch_point,
            euclid_path_length: 0.0,
            positions: VecDeque::new(),
            max_frame_residual: 0.0,
            max_rotation_snap: 0.0,
            last_checkpoint: None,
            min_pop: pop,
            max_pop: pop,
        })
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn population(&self) -> u32 {
        self.engine.population()
    }
    /// Graph path length (sum of per-hop root-to-root distances) in rings —
    /// an upper bound on origin distance, not the distance from launch.
    pub fn path_length_rings(&self) -> u64 {
        self.path_length_rings
    }
    /// Sum of per-hop Euclidean segment lengths (geometry units).
    pub fn euclid_path_length(&self) -> f64 {
        self.euclid_path_length
    }
    pub fn hops(&self) -> u64 {
        self.hops
    }
    /// Object global position in the launch frame (only meaningful when
    /// frames are tracked).
    pub fn global(&self) -> [f64; 2] {
        self.global
    }
    /// Straight-line distance from the launch point in geometry units
    /// (tortuosity-free; 0 when frames are untracked).
    pub fn displacement(&self) -> f64 {
        let d = [self.global[0] - self.launch_point[0], self.global[1] - self.launch_point[1]];
        d[0].hypot(d[1])
    }
    /// Running heading (degrees CCW from +x) of the cumulative displacement.
    pub fn heading_deg(&self) -> f64 {
        let d = [self.global[0] - self.launch_point[0], self.global[1] - self.launch_point[1]];
        if d[0].hypot(d[1]) < 1e-9 {
            f64::NAN
        } else {
            d[1].atan2(d[0]).to_degrees()
        }
    }
    /// Heading over the last `HEADING_WINDOW` hops.
    pub fn windowed_heading_deg(&self) -> f64 {
        if self.positions.len() < 2 {
            return f64::NAN;
        }
        let (_, old) = self.positions.front().unwrap();
        let d = [self.global[0] - old[0], self.global[1] - old[1]];
        if d[0].hypot(d[1]) < 1e-9 {
            f64::NAN
        } else {
            d[1].atan2(d[0]).to_degrees()
        }
    }
    /// The lane selection made at the launch transition, if any.
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }
    /// Length of the retained heading history (bounded by `HEADING_WINDOW`).
    pub fn history_len(&self) -> usize {
        self.positions.len()
    }
    /// Cells in the current window — the dominant retained allocation, which
    /// must stay O(window), not grow with flight length.
    pub fn window_cells(&self) -> u32 {
        self.patch.graph.cells()
    }
    pub fn max_frame_residual(&self) -> f64 {
        self.max_frame_residual
    }
    pub fn max_rotation_snap(&self) -> f64 {
        self.max_rotation_snap
    }
    pub fn min_pop(&self) -> u32 {
        self.min_pop
    }
    pub fn max_pop(&self) -> u32 {
        self.max_pop
    }
    pub fn window_radius(&self) -> u32 {
        self.window_radius
    }
    pub fn current_root(&self) -> &str {
        &self.patch.root
    }
    /// Radius of the current window's patch (the launch window before the
    /// first hop, the flight window after, possibly auto-grown).
    pub fn current_radius(&self) -> u32 {
        self.patch.radius
    }
    /// Per-cell states of the current window, index-aligned with the
    /// window patch's cells.
    pub fn window_state(&self) -> &[u8] {
        self.engine.state()
    }
    /// The transform mapping current-window coordinates into the launch
    /// frame (identity unless `track_frames`).
    pub fn frame(&self) -> &Affine {
        &self.frame
    }
    /// The current window's patch (graph, metadata, root, radius).
    pub fn window_patch(&self) -> &Patch {
        &self.patch
    }
    /// The current window's geometry (present when tracking frames).
    pub fn window_geometry(&self) -> Option<&Geometry> {
        self.geometry.as_ref()
    }
    /// The checkpoint record produced by the most recent hop (a fully
    /// replayable `ResultRecord`: new root + rule + the extracted object as
    /// `initial_states`). Stream this to disk after each hop to build the
    /// provenance chain at constant memory.
    pub fn last_checkpoint(&self) -> Option<&ResultRecord> {
        self.last_checkpoint.as_ref()
    }

    /// Max edge-BFS distance from the window root over all nonzero cells.
    fn max_active_dist(&self) -> u32 {
        self.engine
            .state()
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s != 0)
            .map(|(c, _)| self.dist[c])
            .max()
            .unwrap_or(0)
    }

    /// Indices of the current window's nonzero cells.
    fn nonzero(&self) -> Vec<u32> {
        self.engine
            .state()
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s != 0)
            .map(|(c, _)| c as u32)
            .collect()
    }

    /// Graph-connected components of `cells` as membership lists.
    fn components_members(&self, cells: &[u32]) -> Vec<Vec<u32>> {
        use std::collections::HashSet;
        let set: HashSet<u32> = cells.iter().copied().collect();
        let mut seen: HashSet<u32> = HashSet::new();
        let mut comps: Vec<Vec<u32>> = Vec::new();
        for &start in cells {
            if !seen.insert(start) {
                continue;
            }
            let mut stack = vec![start];
            let mut members = vec![start];
            while let Some(c) = stack.pop() {
                for &nb in self.patch.graph.neighbours(c) {
                    if set.contains(&nb) && seen.insert(nb) {
                        stack.push(nb);
                        members.push(nb);
                    }
                }
            }
            comps.push(members);
        }
        comps
    }

    /// Centroid direction (degrees CCW from +x, launch frame) of a set of
    /// cells, from the launch point. Requires geometry (`self.centroids`).
    fn direction_of(&self, members: &[u32]) -> f64 {
        let n = members.len() as f64;
        let sum = members.iter().fold([0.0; 2], |acc, &c| {
            let p = self.centroids[c as usize];
            [acc[0] + p[0], acc[1] + p[1]]
        });
        let cen = [sum[0] / n, sum[1] / n];
        (cen[1] - self.launch_point[1])
            .atan2(cen[0] - self.launch_point[0])
            .to_degrees()
            .rem_euclid(360.0)
    }

    /// Number of graph-connected components of `cells` in the window graph.
    fn components(&self, cells: &[u32]) -> usize {
        use std::collections::HashSet;
        let set: HashSet<u32> = cells.iter().copied().collect();
        let mut seen: HashSet<u32> = HashSet::new();
        let mut n = 0;
        for &start in cells {
            if !seen.insert(start) {
                continue;
            }
            n += 1;
            let mut stack = vec![start];
            while let Some(c) = stack.pop() {
                for &nb in self.patch.graph.neighbours(c) {
                    if set.contains(&nb) && seen.insert(nb) {
                        stack.push(nb);
                    }
                }
            }
        }
        n
    }

    /// Advance one total generation, hopping first if the object nears the
    /// window edge (or on the launch→flight transition). Returns the hop
    /// log if a hop happened this generation.
    pub fn step(&mut self) -> Result<Option<HopLog>, SlideError> {
        let mut hop = None;

        if !self.launched {
            if self.generation >= self.launch_gens {
                // Transition: re-root the (now single) object into the
                // flight window. Tolerates a momentary localised split.
                hop = Some(self.hop()?);
                self.launched = true;
            }
        } else {
            let md = self.max_active_dist();
            if md >= self.thresh.trigger {
                let nz = self.nonzero();
                let force = md >= self.thresh.hard_limit;
                if force || self.components(&nz) == 1 {
                    hop = Some(self.hop()?);
                }
                // else: defer — step once more and re-check next generation.
            }
        }

        // One generation on the current window.
        self.rule.step(&mut self.engine);
        self.generation += 1;

        let pop = self.engine.population();
        if pop == 0 {
            return Err(SlideError::Extinct {
                generation: self.generation,
            });
        }
        self.min_pop = self.min_pop.min(pop);
        self.max_pop = self.max_pop.max(pop);
        // The launch phase holds every object (the whole shower); after the
        // transition (and any selection) only the hero remains.
        let band = if self.launched {
            self.pop_band
        } else {
            self.launch_pop_band
        };
        if pop > band {
            return Err(SlideError::PopulationEscaped {
                generation: self.generation,
                population: pop,
                band,
            });
        }

        let md = self.max_active_dist();
        if md > self.thresh.safe_max {
            return Err(SlideError::MarginInvariant {
                generation: self.generation,
                max_dist: md,
                safe_max: self.thresh.safe_max,
                window_radius: self.patch.radius,
            });
        }
        Ok(hop)
    }

    /// Extract the whole nonzero state, re-root at the leading cell, and
    /// blit it into a fresh flight window (auto-growing the radius if the
    /// object does not fit with room to fly). Generation is unchanged.
    fn hop(&mut self) -> Result<HopLog, SlideError> {
        let nz_all = self.nonzero();
        if nz_all.is_empty() {
            return Err(SlideError::Extinct {
                generation: self.generation,
            });
        }

        // Lane selection at the launch→flight transition: if several
        // diverging components are present and a heading hint is set, keep
        // only the component nearest the hint and drop the rest (sound by
        // the light-cone argument — the discarded lanes recede and never
        // re-reach the hero). Only at the transition; later within-hero
        // splits are localised, not diverging lanes.
        let nz = if let (false, Some(hint)) = (self.launched, self.select_heading) {
            let comps = self.components_members(&nz_all);
            if comps.len() > 1 {
                let dirs: Vec<f64> =
                    comps.iter().map(|m| self.direction_of(m)).collect();
                let sel = select_component(&dirs, hint).map_err(|detail| {
                    SlideError::FrameModel {
                        generation: self.generation,
                        detail,
                    }
                })?;
                let dropped: Vec<(f64, u32)> = comps
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| i != sel)
                    .map(|(i, m)| (dirs[i], m.len() as u32))
                    .collect();
                self.selection = Some(Selection {
                    kept_heading: dirs[sel],
                    kept_pop: comps[sel].len() as u32,
                    dropped,
                    total_components: comps.len(),
                    generation: self.generation,
                });
                comps[sel].clone()
            } else {
                nz_all
            }
        } else {
            nz_all
        };

        // Object localisation: a single object (whole or momentarily split
        // into adjacent pieces) has a small ring-span; a wake/second object
        // would be far away.
        let rmin = nz.iter().map(|&c| self.dist[c as usize]).min().unwrap();
        let rmax = nz.iter().map(|&c| self.dist[c as usize]).max().unwrap();
        let span = rmax - rmin;
        if span > OBJECT_MAX_SPAN {
            return Err(SlideError::NotLocalised {
                generation: self.generation,
                object_span: span,
            });
        }
        let components = self.components(&nz);

        // Re-root at the leading cell (max distance from the current root).
        let front = *nz
            .iter()
            .max_by_key(|&&c| self.dist[c as usize])
            .unwrap();
        let front_advance = self.dist[front as usize];
        let new_root = self.addr[front as usize].clone();
        let old_root = self.patch.root.clone();
        let extracted: Vec<(String, u8)> = nz
            .iter()
            .map(|&c| (self.addr[c as usize].clone(), self.engine.state()[c as usize]))
            .collect();
        let population = nz.len() as u32;

        // Build the new window, growing until the object fits with room to
        // fly (its footprint plus a fan margin must clear the trigger).
        // (Hop timing is diagnostics only; std::time is unavailable on
        // wasm32, where the browser panel reports 0.)
        #[cfg(not(target_arch = "wasm32"))]
        let t0 = Instant::now();
        let mut radius = self.window_radius;
        let mut grew = false;
        loop {
            let (patch, geom) = if self.track_frames {
                let (p, g) = self
                    .tiling
                    .generate_patch_with_geometry(&new_root, radius, self.neighbourhood)
                    .map_err(|e| SlideError::Patch(e.to_string()))?;
                (p, Some(g))
            } else {
                let p = self
                    .tiling
                    .generate_patch(&new_root, radius, self.neighbourhood)
                    .map_err(|e| SlideError::Patch(e.to_string()))?;
                (p, None)
            };
            let idx: HashMap<&str, u32> = patch
                .cells
                .iter()
                .enumerate()
                .map(|(i, c)| (c.address.as_str(), i as u32))
                .collect();

            // Resolve every extracted cell (all-or-nothing).
            let mut resolved: Vec<u32> = Vec::with_capacity(extracted.len());
            let mut missing: Option<String> = None;
            for (a, _) in &extracted {
                match idx.get(a.as_str()) {
                    Some(&i) => resolved.push(i),
                    None => {
                        missing = Some(a.clone());
                        break;
                    }
                }
            }

            let fits = missing.is_none();
            let fm = fan_margin(&patch);
            let th = thresholds(radius, fm);
            let footprint = resolved
                .iter()
                .map(|&i| patch.cells[i as usize].distance)
                .max()
                .unwrap_or(0);
            // Room to fly: front (now ring 0) must be able to reach the
            // trigger with the whole footprint clear of the boundary.
            let room = th.trigger > 0 && footprint + fm < th.trigger;

            if fits && room {
                #[cfg(not(target_arch = "wasm32"))]
                let patch_secs = t0.elapsed().as_secs_f64();
                #[cfg(target_arch = "wasm32")]
                let patch_secs = 0.0;

                // Exact root-to-root advance: the previous window root
                // resolved in the new window gives the graph distance the
                // root moved. On the launch hop the old root is beyond the
                // (small) flight window; fall back to the equivalent
                // distance measured in the old window.
                let advance = idx
                    .get(old_root.as_str())
                    .map(|&i| patch.cells[i as usize].distance)
                    .unwrap_or(front_advance);

                let strata = patch.strata(self.scheme).0;
                let dist: Vec<u32> = patch.cells.iter().map(|c| c.distance).collect();
                let addr: Vec<String> =
                    patch.cells.iter().map(|c| c.address.clone()).collect();
                let mut state = vec![0u8; patch.graph.cells() as usize];
                for (&i, (_, s)) in resolved.iter().zip(extracted.iter()) {
                    state[i as usize] = *s;
                }
                let mut engine = Engine::with_strata(patch.graph.clone(), strata);
                engine.load_state(&state);

                // Cross-window frame composition (exact rigid transform).
                let mut euclid_distance = 0.0;
                let (new_centroids, frame_residual, rotation_snap) =
                    if let Some(geom) = &geom {
                        let new_c = centroids(geom);
                        let (s_transform, residual, snap) =
                            self.frame_step(&addr, &new_c)?;
                        self.frame = self.frame.compose(&s_transform);
                        self.global = self.frame.apply(new_c[0]);
                        let seg = (self.global[0] - self.prev_global[0])
                            .hypot(self.global[1] - self.prev_global[1]);
                        self.euclid_path_length += seg;
                        self.prev_global = self.global;
                        euclid_distance = self.displacement();
                        self.max_frame_residual = self.max_frame_residual.max(residual);
                        self.max_rotation_snap = self.max_rotation_snap.max(snap);
                        (new_c, residual, snap)
                    } else {
                        (Vec::new(), 0.0, 0.0)
                    };

                self.hops += 1;
                self.path_length_rings += u64::from(advance);
                let delta_gens = self.generation - self.prev_hop_generation;
                let local_clock = if advance > 0 {
                    delta_gens as f64 / f64::from(advance)
                } else {
                    f64::NAN
                };
                self.prev_hop_generation = self.generation;
                if self.track_frames {
                    self.positions.push_back((self.path_length_rings, self.global));
                    while self.positions.len() > HEADING_WINDOW {
                        self.positions.pop_front();
                    }
                }
                let initial_states: Vec<u8> =
                    extracted.iter().map(|(_, s)| *s).collect();
                self.last_checkpoint = Some(ResultRecord {
                    family: self.source.family,
                    root: patch.root.clone(),
                    radius,
                    rule: self.source.rule,
                    stratification: self.source.stratification,
                    tables: self.source.tables.clone(),
                    table_rule: self.source.table_rule.clone(),
                    neighbourhood: self.neighbourhood,
                    initial_cells: resolved,
                    initial_states,
                    generations: 0,
                    outcome: Outcome::Active,
                    max_population: population,
                    note: {
                        let mut note = format!(
                            "slide hop {} gen {} path-rings {}",
                            self.hops, self.generation, self.path_length_rings
                        );
                        // Selection provenance on the transition checkpoint.
                        if !self.launched
                            && let Some(sel) = &self.selection
                        {
                            note.push_str(&format!(
                                "; selected lane {:.1}° (pop {}) of {} components, dropped {:?}",
                                sel.kept_heading,
                                sel.kept_pop,
                                sel.total_components,
                                sel.dropped
                            ));
                        }
                        note
                    },
                });

                self.patch = patch;
                self.geometry = geom;
                self.engine = engine;
                self.dist = dist;
                self.addr = addr;
                self.centroids = new_centroids;
                self.thresh = th;

                return Ok(HopLog {
                    hop: self.hops,
                    generation: self.generation,
                    path_length_rings: self.path_length_rings,
                    advance,
                    local_clock,
                    population,
                    components,
                    object_span: span,
                    window_radius: radius,
                    grew,
                    patch_secs,
                    global: self.global,
                    euclid_distance,
                    heading_deg: self.heading_deg(),
                    windowed_heading_deg: self.windowed_heading_deg(),
                    frame_residual,
                    rotation_snap,
                });
            }

            // Grow and retry from the same extracted state.
            if radius >= self.max_window_radius {
                return match missing {
                    Some(address) => Err(SlideError::Unresolved {
                        address,
                        window_radius: radius,
                    }),
                    None => Err(SlideError::WindowTooSmall {
                        window_radius: radius,
                        object_span: span,
                    }),
                };
            }
            radius = ((radius * 3) / 2 + 1).min(self.max_window_radius);
            grew = true;
        }
    }

    /// Derive the exact rigid transform mapping the *new* window's
    /// coordinates onto the *previous* window's, from tiles shared by
    /// canonical address. Returns the transform, its RMS residual, and the
    /// deviation of its rotation from the nearest tiling orientation
    /// (multiple of 30°). Errors if too few shared tiles, a large residual
    /// (windows not related by a rigid motion), or a rotation off the
    /// finite orientation set — any means the frame model is wrong.
    fn frame_step(
        &self,
        new_addr: &[String],
        new_centroids: &[[f64; 2]],
    ) -> Result<(Affine, f64, f64), SlideError> {
        let prev: HashMap<&str, [f64; 2]> = self
            .addr
            .iter()
            .zip(self.centroids.iter())
            .map(|(a, &c)| (a.as_str(), c))
            .collect();
        let mut a = Vec::new(); // previous-window coordinates
        let mut b = Vec::new(); // new-window coordinates
        for (addr, &c) in new_addr.iter().zip(new_centroids.iter()) {
            if let Some(&pc) = prev.get(addr.as_str()) {
                a.push(pc);
                b.push(c);
            }
        }
        if a.len() < 8 {
            return Err(SlideError::FrameModel {
                generation: self.generation,
                detail: format!("only {} shared tiles between windows", a.len()),
            });
        }
        let (transform, residual, theta) = derive_rigid(&a, &b);
        if residual > FRAME_RESIDUAL_TOL {
            return Err(SlideError::FrameModel {
                generation: self.generation,
                detail: format!("rigid residual {residual:.3e} exceeds {FRAME_RESIDUAL_TOL:.0e} over {} tiles", a.len()),
            });
        }
        // Snap the rotation to the tiling's finite orientation set (30°).
        let quantum = std::f64::consts::FRAC_PI_6;
        let snapped = (theta / quantum).round() * quantum;
        let snap = (theta - snapped).abs();
        if snap > ROTATION_SNAP_TOL {
            return Err(SlideError::FrameModel {
                generation: self.generation,
                detail: format!(
                    "rotation {:.4}° is {:.4}° off the nearest 30° orientation",
                    theta.to_degrees(),
                    snap.to_degrees()
                ),
            });
        }
        Ok((transform, residual, snap))
    }

    /// The current window's live cells as `(address, state)` pairs — the
    /// unit the monolithic-replay control compares against.
    pub fn snapshot(&self) -> Vec<(String, u8)> {
        self.engine
            .state()
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s != 0)
            .map(|(c, &s)| (self.addr[c].clone(), s))
            .collect()
    }
}

/// Live `(address, state)` pairs of an arbitrary engine state over a
/// patch — for building the monolithic reference snapshots.
pub fn snapshot_of(patch: &Patch, state: &[u8]) -> Vec<(String, u8)> {
    state
        .iter()
        .enumerate()
        .filter(|&(_, &s)| s != 0)
        .map(|(c, &s)| (patch.cells[c].address.clone(), s))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn glider_a() -> ResultRecord {
        load("../../results/hat-tableevolve-r48-s21.jsonl")
    }

    fn load(path: &str) -> ResultRecord {
        let text = std::fs::read_to_string(path).expect("record");
        serde_json::from_str(text.lines().next().unwrap()).unwrap()
    }

    /// The core soundness proof, in a debug build (integer overflow panics
    /// here): a sliding flight through many hops reproduces a monolithic
    /// run's nonzero (address, state) set at every sampled generation.
    /// This exercises the whole hop machinery — address round-trip,
    /// re-rooting, blit, margin invariant — at real (progressively deeper)
    /// hop roots.
    #[test]
    fn sliding_matches_monolithic_debug() {
        let record = glider_a();
        let tiling = Tiling::new(record.family);

        // Monolithic reference at radius 96: snapshot nonzero
        // (address, state) every 25 gens to boundary contact. (Radius is
        // kept modest so this runs in a debug build in a few seconds; the
        // release control in examples/slide.rs --validate goes to 384.)
        let mono_radius = 96;
        let mono = tiling
            .generate_patch(&record.root, mono_radius, record.neighbourhood)
            .unwrap();
        let mono_dist: Vec<u32> = mono.cells.iter().map(|c| c.distance).collect();
        let mono_margin = fan_margin(&mono);
        let (mono_strata, mono_rule) = record.replay_setup(&mono);
        let mut me = Engine::with_strata(mono.graph.clone(), mono_strata);
        me.load_state(&record.initial_state(mono.graph.cells()).unwrap());
        let mut samples: Vec<(u64, BTreeSet<(String, u8)>)> = Vec::new();
        let mut g = 0u64;
        loop {
            if g.is_multiple_of(25) {
                samples.push((g, snapshot_of(&mono, me.state()).into_iter().collect()));
            }
            let touched = me.state().iter().enumerate().any(|(c, &s)| {
                s != 0 && mono_dist[c] + mono_margin >= mono_radius
            });
            if touched || me.population() == 0 {
                break;
            }
            mono_rule.step(&mut me);
            g += 1;
        }
        let last = samples.last().unwrap().0;
        assert!(last >= 150, "monolithic flight too short: {last}");

        // Sliding run at a small window; sample at the same generations.
        let cfg = SlideConfig {
            window_radius: 24,
            launch_radius: 48,
            launch_gens: 60,
            max_window_radius: 96,
            pop_band: record.max_population,
            launch_pop_band: record.max_population,
            select_heading: None,
            track_frames: false,
        };
        let mut slider = Slider::launch(&tiling, &record, &cfg).unwrap();
        let mut sample_iter = samples.iter();
        let mut next = sample_iter.next();
        let mut compared = 0;
        while let Some(&(sg, ref want)) = next {
            if sg > last {
                break;
            }
            while slider.generation() < sg {
                slider.step().expect("slide step");
            }
            let got: BTreeSet<(String, u8)> = slider.snapshot().into_iter().collect();
            assert_eq!(&got, want, "mismatch at generation {sg}");
            compared += 1;
            next = sample_iter.next();
        }
        assert!(compared >= 6, "too few comparisons: {compared}");
        assert!(slider.hops() >= 3, "too few hops: {}", slider.hops());
    }

    /// Deep-root overflow guard: generating a window at the deepest root a
    /// flight reaches must not overflow the transducer's exact arithmetic
    /// (panics in this debug build, wraps silently in release). Uses the
    /// deepest checkpoint root from a short flight, or the root supplied in
    /// the `SLIDE_DEEP_ROOT` env var (the demo's deepest root).
    #[test]
    fn deep_root_no_overflow_debug() {
        let record = glider_a();
        let tiling = Tiling::new(record.family);
        let root = match std::env::var("SLIDE_DEEP_ROOT") {
            Ok(r) if !r.is_empty() => r,
            _ => {
                let cfg = SlideConfig {
                    window_radius: 24,
                    launch_radius: 48,
                    launch_gens: 60,
                    max_window_radius: 96,
                    pop_band: record.max_population,
                    launch_pop_band: record.max_population,
                    select_heading: None,
                    track_frames: false,
                };
                let mut slider = Slider::launch(&tiling, &record, &cfg).unwrap();
                for _ in 0..200 {
                    if slider.step().is_err() {
                        break;
                    }
                }
                slider.current_root().to_string()
            }
        };
        // The window generation itself is the guard (no panic == pass).
        let patch = tiling
            .generate_patch(&root, 32, record.neighbourhood)
            .expect("deep root generates");
        assert!(
            patch.seed_artifact_cells.is_empty(),
            "deep root produced seed artifacts: {:?}",
            patch.seed_artifact_cells
        );
        assert!(patch.graph.cells() > 0);
    }

    /// Frame composition is exact: the object's global position, obtained by
    /// composing the per-hop rigid transforms, matches its true geometry
    /// position in a monolithic patch (same root ⇒ same place_default frame)
    /// at every hop, to f64 precision. Also confirms the rigid residual is
    /// ~0 and rotations are exact 30° multiples.
    #[test]
    fn frame_composition_matches_monolithic_geometry_debug() {
        let record = glider_a();
        let tiling = Tiling::new(record.family);

        // Monolithic geometry: canonical position (centroid) per address.
        let (mono, geom) = tiling
            .generate_patch_with_geometry(&record.root, 96, record.neighbourhood)
            .unwrap();
        let mono_c = centroids(&geom);
        let mono_pos: HashMap<&str, [f64; 2]> = mono
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| (c.address.as_str(), mono_c[i]))
            .collect();

        let cfg = SlideConfig {
            window_radius: 24,
            launch_radius: 48,
            launch_gens: 60,
            max_window_radius: 96,
            pop_band: record.max_population,
            launch_pop_band: record.max_population,
            select_heading: None,
            track_frames: true,
        };
        let mut slider = Slider::launch(&tiling, &record, &cfg).unwrap();
        let mut checked = 0;
        // A few hops (their roots stay within the radius-96 monolithic patch).
        while slider.hops() < 4 {
            if let Some(h) = slider.step().expect("slide step") {
                let want = mono_pos
                    .get(slider.current_root())
                    .copied()
                    .expect("hop root present in monolithic patch");
                let got = h.global;
                let err = (got[0] - want[0]).hypot(got[1] - want[1]);
                assert!(
                    err < 1e-6,
                    "hop {} global {:?} vs monolithic {:?}: error {err:.3e}",
                    h.hop,
                    got,
                    want
                );
                assert!(h.frame_residual < FRAME_RESIDUAL_TOL);
                assert!(
                    h.rotation_snap < 1e-6,
                    "rotation {:.3e} rad off the nearest 30° multiple",
                    h.rotation_snap
                );
                checked += 1;
            }
        }
        assert!(checked >= 4, "too few hops checked: {checked}");
    }

    /// Within-window BFS control: a root-to-root distance measured in the
    /// small flight window (r24) must equal the same distance in a larger
    /// window (r48) containing the pair — otherwise a geodesic exits the
    /// small window and the per-hop advance would be inflated. Confirms the
    /// r24 window is large enough that its BFS distances are the true
    /// infinite-tiling distances for the pairs the flight actually uses.
    #[test]
    fn within_window_bfs_agrees_across_radii_debug() {
        let record = glider_a();
        let tiling = Tiling::new(record.family);
        let cfg = SlideConfig {
            window_radius: 24,
            launch_radius: 48,
            launch_gens: 60,
            max_window_radius: 96,
            pop_band: record.max_population,
            launch_pop_band: record.max_population,
            select_heading: None,
            track_frames: false,
        };
        let mut slider = Slider::launch(&tiling, &record, &cfg).unwrap();
        // Sequence of window roots as the flight hops.
        let mut roots: Vec<String> = Vec::new();
        while roots.len() < 6 {
            if slider.step().expect("step").is_some() {
                roots.push(slider.current_root().to_string());
            }
        }
        let mut checked = 0;
        for pair in roots.windows(2) {
            let (old, new) = (&pair[0], &pair[1]);
            let dist_in = |radius: u32| -> Option<u32> {
                let p = tiling
                    .generate_patch(new, radius, record.neighbourhood)
                    .unwrap();
                p.cells
                    .iter()
                    .find(|c| c.address == *old)
                    .map(|c| c.distance)
            };
            let d24 = dist_in(24).expect("old root within r24 window");
            let d48 = dist_in(48).expect("old root within r48 window");
            assert_eq!(d24, d48, "r24 vs r48 root-to-root distance disagree");
            checked += 1;
        }
        assert!(checked >= 4, "too few pairs checked: {checked}");
    }

    /// Selection determinism: applying the same heading hint to the sliding
    /// run and to the monolithic control (zeroing the non-selected lanes at
    /// the transition generation) keeps them exact-equal thereafter. Uses
    /// the spectre pair (s33), lane ~108°.
    #[test]
    fn selection_matches_monolithic_debug() {
        let record = load("../../results/spectre-tableevolve-r48-s33.jsonl");
        let tiling = Tiling::new(record.family);
        let hint = 108.0;
        let launch_gens = 60u64;
        let mono_radius = 96u32;

        // Monolithic + geometry; apply the same selection at launch_gens.
        let (mono, geom) = tiling
            .generate_patch_with_geometry(&record.root, mono_radius, record.neighbourhood)
            .unwrap();
        let mono_c = centroids(&geom);
        let mono_dist: Vec<u32> = mono.cells.iter().map(|c| c.distance).collect();
        let mono_margin = fan_margin(&mono);
        let launch = mono_c[0];
        let (strata, rule) = record.replay_setup(&mono);
        let mut me = Engine::with_strata(mono.graph.clone(), strata);
        me.load_state(&record.initial_state(mono.graph.cells()).unwrap());
        let mut samples: Vec<(u64, BTreeSet<(String, u8)>)> = Vec::new();
        let mut g = 0u64;
        loop {
            // Sample before selecting (mirrors the sliding run's transition,
            // which fires entering launch_gens+1).
            if g.is_multiple_of(25) {
                samples.push((g, snapshot_of(&mono, me.state()).into_iter().collect()));
            }
            if g == launch_gens {
                // Component membership, directions, select, zero the rest.
                let nz: Vec<u32> = me
                    .state()
                    .iter()
                    .enumerate()
                    .filter(|&(_, &s)| s != 0)
                    .map(|(c, _)| c as u32)
                    .collect();
                let set: std::collections::HashSet<u32> = nz.iter().copied().collect();
                let mut seen = std::collections::HashSet::new();
                let mut comps: Vec<Vec<u32>> = Vec::new();
                for &s0 in &nz {
                    if !seen.insert(s0) {
                        continue;
                    }
                    let mut st = vec![s0];
                    let mut m = vec![s0];
                    while let Some(c) = st.pop() {
                        for &nb in mono.graph.neighbours(c) {
                            if set.contains(&nb) && seen.insert(nb) {
                                st.push(nb);
                                m.push(nb);
                            }
                        }
                    }
                    comps.push(m);
                }
                assert!(comps.len() >= 2, "expected a multi-lane launch");
                let dirs: Vec<f64> = comps
                    .iter()
                    .map(|m| {
                        let n = m.len() as f64;
                        let sum = m.iter().fold([0.0; 2], |a, &c| {
                            [a[0] + mono_c[c as usize][0], a[1] + mono_c[c as usize][1]]
                        });
                        ((sum[1] / n) - launch[1])
                            .atan2((sum[0] / n) - launch[0])
                            .to_degrees()
                            .rem_euclid(360.0)
                    })
                    .collect();
                let sel = select_component(&dirs, hint).unwrap();
                for (i, m) in comps.iter().enumerate() {
                    if i != sel {
                        for &c in m {
                            me.set_state(c, 0);
                        }
                    }
                }
            }
            let touched = me
                .state()
                .iter()
                .enumerate()
                .any(|(c, &s)| s != 0 && mono_dist[c] + mono_margin >= mono_radius);
            if touched || me.population() == 0 {
                break;
            }
            rule.step(&mut me);
            g += 1;
        }
        assert!(samples.last().unwrap().0 >= 150);

        // Sliding run with the same hint.
        let cfg = SlideConfig {
            window_radius: 24,
            launch_radius: 48,
            launch_gens,
            max_window_radius: 96,
            pop_band: record.max_population,
            launch_pop_band: record.max_population,
            select_heading: Some(hint),
            track_frames: true,
        };
        let mut slider = Slider::launch(&tiling, &record, &cfg).unwrap();
        let mut checked = 0;
        for (sg, want) in &samples {
            while slider.generation() < *sg {
                slider.step().expect("slide step");
            }
            let got: BTreeSet<(String, u8)> = slider.snapshot().into_iter().collect();
            assert_eq!(&got, want, "selection mismatch at gen {sg}");
            checked += 1;
        }
        assert!(slider.selection().is_some(), "no selection recorded");
        assert!(checked >= 6);
    }

    /// Constant-memory regression guard: over many hops the Slider's
    /// retained logical state must not grow with flight length — the window
    /// cell count stays O(window) (it only changes on an auto-grow) and the
    /// heading history stays capped. This catches a logical accumulator
    /// leak; the flat measured RSS curve (reported separately) covers
    /// allocator-level growth. Frames off keeps it fast — the retained-state
    /// shape is identical either way (per-hop geometry is transient, not
    /// retained).
    #[test]
    fn flight_retains_bounded_state_debug() {
        let record = glider_a();
        let tiling = Tiling::new(record.family);
        let cfg = SlideConfig {
            window_radius: 24,
            launch_radius: 48,
            launch_gens: 60,
            max_window_radius: 96,
            pop_band: record.max_population,
            launch_pop_band: record.max_population,
            select_heading: None,
            track_frames: false,
        };
        let mut slider = Slider::launch(&tiling, &record, &cfg).unwrap();
        let mut cells_band = (u32::MAX, 0u32);
        let mut hops = 0;
        // A modest hop count: debug patch-gen is ~0.5 s/hop, and the retained
        // state is either constant from hop 1 (a leak would show immediately)
        // or it is not — more hops do not add signal. The flat RSS curve
        // (reported) covers the long-run allocator behaviour.
        while hops < 18 {
            if slider.step().expect("step").is_some() {
                hops += 1;
                assert!(
                    slider.history_len() <= HEADING_WINDOW,
                    "heading history grew to {}",
                    slider.history_len()
                );
                let wc = slider.window_cells();
                cells_band = (cells_band.0.min(wc), cells_band.1.max(wc));
            }
        }
        // Window cell count is O(window), not growing with flight length: the
        // late windows are no bigger than the early ones (radius is fixed;
        // only an auto-grow could change it, and none is expected here).
        assert!(
            cells_band.1 <= cells_band.0 * 2,
            "window cells grew from {} to {} — retained state scales with flight length",
            cells_band.0,
            cells_band.1
        );
    }
}
