//! Situation-saturation probe for the certificate programme (paper §6):
//! replay a glider record and, at every generation, canonically encode
//! the *situation* — the live cells' states together with the substrate
//! collar within edge-radius k of the support (tile classes, edge
//! gluings, vertex degrees) — as a location-invariant key. The count of
//! distinct keys versus generation is the decisive first experiment: a
//! saturating curve plus a deterministic observed key→next-key map
//! supports a tractable closure certificate; growth to the horizon
//! means the collar captures too much incidental terrain variety or the
//! lane genuinely keeps meeting new k-collars.
//!
//! Encoding: tile types have intrinsic edge numberings, and the
//! neighbour transducer gives, per edge, the neighbouring tile plus the
//! back-edge index (the gluing). An edge-ordered BFS over the collar
//! from an anchor therefore reproduces the labelled collar graph up to
//! isomorphism: per visited cell record (state, class, nedges, vertex
//! degree); per intrinsic edge record the neighbour's visit index and
//! back-edge, or an outside-collar marker. Anchoring on every support
//! cell and taking the lexicographically least byte string makes the
//! key canonical across locations — patch cell indices and addresses
//! never enter the key.
//!
//! A collar of edge-radius k ≥ 4 is safely sufficient to determine one
//! step (cells that can change lie in the vertex-1-ball of the support,
//! i.e. within edge distance 2; their next states need their vertex
//! neighbours' states and their own vertex degrees). Two determinism
//! checks with different standings:
//!
//! - Same-k (key_k(t) → key_k(t+1)): NOT forced even with a perfect
//!   encoding — the next situation's k-collar reaches edge distance
//!   k+2 from the old support, beyond the old k-collar, so identical
//!   k-collars can be continued by different outer terrain. Violations
//!   here measure collar-extension branching, exactly what a closure
//!   certificate must enumerate.
//! - Cross-k (key_kb(t) → key_ks(t+1), kb ≥ ks+2 and kb ≥ 4): forced.
//!   The next support lies within edge distance 2, so its ks-collar
//!   sits inside the old kb-collar, and kb ≥ 4 determines the step.
//!   Any violation here means the encoding or patch radius is wrong.
//!
//! Usage: situations <record.jsonl> [index] [radius] [start_gen]
//!        [ks (comma-separated, default "3,4")] [csv_path] [farthest]
//!
//! `farthest` restricts the keyed support to the vertex-connected
//! component farthest from the root (for records that launch two
//! objects, e.g. spectre s33).

use std::collections::{HashMap, VecDeque};

use ca_engine::{CaRule, Engine};
use substitution_tiling_transducers::address::{EPTileAddress, TileAddress};
use substitution_tiling_transducers::builtin::BuiltinSystem;
use substitution_tiling_transducers::common::{DisplayViaSystem, System};
use tiling_core::results::ResultRecord;
use tiling_core::{Neighbourhood, Patch, Tiling};

/// Neighbour-slot marker: the transducer neighbour is outside the patch.
const OUT: u32 = u32::MAX;

/// Sampling stops when support + collar reach this many edge-rings from
/// the patch boundary: vertex fans (degrees) are only complete ≥ 3
/// rings inside (measured fan bound), +1 slack.
const FAN_GUARD: u32 = 4;

/// Intrinsic-edge-ordered adjacency of one cell, recovered from the
/// neighbour transducer: `(neighbour cell index or OUT, back-edge)`.
type EdgeList = Vec<(u32, u8)>;

fn recover_edges(
    c: u32,
    patch: &Patch,
    addr_index: &HashMap<&str, u32>,
    sys: &BuiltinSystem,
) -> EdgeList {
    let cs = &sys.cs_fine;
    let addr = EPTileAddress::parse(&patch.cells[c as usize].address, &sys.tmap)
        .expect("cell address parses");
    let base = addr.base_type().unwrap();
    let nedges = cs.lookup_tile_nedges(base);
    assert!(nedges < 256, "edge count fits a byte");
    let mut out: EdgeList = Vec::with_capacity(nedges);
    for e in 0..nedges {
        let (naddr, back) = addr.neighbour(e, &sys.tr).unwrap();
        assert!(back < 256, "back-edge fits a byte");
        let idx = addr_index
            .get(format!("{}", naddr.display(cs)).as_str())
            .copied()
            .unwrap_or(OUT);
        out.push((idx, back as u8));
    }
    // Wherever the whole neighbourhood is in-patch, the recovered edge
    // set must be exactly the CSR adjacency — validates the address
    // round-trip and the transducer walk against the patch generator.
    if out.iter().all(|&(n, _)| n != OUT) {
        let mut mine: Vec<u32> = out.iter().map(|&(n, _)| n).collect();
        mine.sort_unstable();
        mine.dedup();
        let mut graph = patch.graph.neighbours(c).to_vec();
        graph.sort_unstable();
        assert_eq!(mine, graph, "transducer edges disagree with patch graph at cell {c}");
    }
    out
}

/// Edge-ordered BFS encoding of the k-collar from one anchor. Membership
/// is `stamp[c] == tag && depth[c] <= k` (depth from the current
/// support, so membership is intrinsic to the situation). Returns `None`
/// when the collar is not edge-connected from the anchor — the encoding
/// would silently drop cells.
#[allow(clippy::too_many_arguments)]
fn encode(
    anchor: u32,
    k: u8,
    tag: u32,
    stamp: &[u32],
    depth: &[u8],
    member_count: usize,
    state: &[u8],
    class: &[u16],
    vdeg: &[u8],
    edge_cache: &HashMap<u32, EdgeList>,
) -> Option<Vec<u8>> {
    let mut order: HashMap<u32, u16> = HashMap::with_capacity(member_count);
    let mut seq: Vec<u32> = Vec::with_capacity(member_count);
    seq.push(anchor);
    order.insert(anchor, 0);
    let mut enc: Vec<u8> = Vec::with_capacity(member_count * 48);
    let mut i = 0;
    while i < seq.len() {
        let c = seq[i];
        i += 1;
        let el = &edge_cache[&c];
        enc.push(state[c as usize]);
        enc.extend(class[c as usize].to_be_bytes());
        enc.push(el.len() as u8);
        enc.push(vdeg[c as usize]);
        for &(nb, back) in el {
            let member = nb != OUT
                && stamp[nb as usize] == tag
                && depth[nb as usize] <= k;
            if member {
                let idx = *order.entry(nb).or_insert_with(|| {
                    seq.push(nb);
                    (seq.len() - 1) as u16
                });
                enc.extend(idx.to_be_bytes());
                enc.push(back);
            } else {
                enc.extend([0xFF, 0xFF, 0xFF]);
            }
        }
    }
    (seq.len() == member_count).then_some(enc)
}

/// Distinct-key bookkeeping for one collar radius.
struct KeyTracker {
    k: u8,
    ids: HashMap<Vec<u8>, usize>,
    first_seen: Vec<u64>,
    transitions: HashMap<usize, usize>,
    /// (generation, key, expected next, observed next).
    violations: Vec<(u64, usize, usize, usize)>,
    prev: Option<usize>,
    recurrences: u64,
    disconnected: u64,
}

impl KeyTracker {
    fn new(k: u8) -> Self {
        Self {
            k,
            ids: HashMap::new(),
            first_seen: Vec::new(),
            transitions: HashMap::new(),
            violations: Vec::new(),
            prev: None,
            recurrences: 0,
            disconnected: 0,
        }
    }

    /// Record this generation's key; returns (id, first-seen?).
    fn observe(&mut self, generation: u64, key: Vec<u8>) -> (usize, bool) {
        let next_id = self.ids.len();
        let (id, new) = match self.ids.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => (*e.get(), false),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(next_id);
                self.first_seen.push(generation);
                (next_id, true)
            }
        };
        if !new {
            self.recurrences += 1;
        }
        if let Some(p) = self.prev {
            match self.transitions.get(&p) {
                Some(&expected) if expected != id => {
                    self.violations.push((generation, p, expected, id));
                }
                Some(_) => {}
                None => {
                    self.transitions.insert(p, id);
                }
            }
        }
        self.prev = Some(id);
        (id, new)
    }
}

/// Forced cross-radius transition map (see module docs): the key at
/// collar `big` must determine the next generation's key at collar
/// `small` whenever k_big ≥ k_small + 2 and k_big ≥ 4.
struct CrossMap {
    /// Tracker indices into `ks`.
    big: usize,
    small: usize,
    map: HashMap<usize, usize>,
    /// (generation, big key, expected small next, observed small next).
    violations: Vec<(u64, usize, usize, usize)>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(384, |a| a.parse().expect("radius"));
    let start_gen: u64 = args.next().map_or(60, |a| a.parse().expect("start_gen"));
    let ks: Vec<u8> = args.next().map_or(vec![3, 4], |a| {
        a.split(',').map(|s| s.parse().expect("collar radius")).collect()
    });
    let csv_path = args
        .next()
        .unwrap_or_else(|| "/tmp/situations.csv".into());
    let farthest = args.next().is_some_and(|a| a == "farthest");
    // SITUATIONS_DUMP=<path>: also write one line per (generation, k)
    // with the FNV-64 of the canonical key — cross-run vocabulary
    // membership comparisons (e.g. lane-capture vs canonical glider).
    let mut dump: Option<(String, String)> = std::env::var("SITUATIONS_DUMP")
        .ok()
        .map(|p| (p, String::from("generation,k,key\n")));
    let k_max = *ks.iter().max().expect("at least one collar radius");

    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .nth(index)
        .expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    assert_eq!(
        record.neighbourhood,
        Neighbourhood::Vertex,
        "situation probe assumes vertex dynamics over an edge-metric collar"
    );
    println!(
        "situation probe: {:?} radius {radius} ks {ks:?} start {start_gen}{} — {}",
        record.family,
        if farthest { " (farthest component)" } else { "" },
        record.note
    );

    eprintln!("building patches (vertex + geometry, then edge)...");
    let tiling = Tiling::new(record.family);
    let (patch_v, geom) = tiling
        .generate_patch_with_geometry(&record.root, radius, record.neighbourhood)
        .unwrap();
    let patch_e = tiling.generate(&record.root, radius).unwrap();
    assert!(patch_v.seed_artifact_cells.is_empty());
    // The BFS is neighbourhood-independent: indices and metadata must
    // agree exactly, so the edge graph can drive the collar while the
    // vertex graph drives dynamics and degrees.
    assert_eq!(patch_e.cells, patch_v.cells, "edge/vertex cell numbering must agree");
    let n = patch_e.graph.cells();
    eprintln!("{n} cells");

    let sys = tiling.system();
    let addr_index: HashMap<&str, u32> = patch_e
        .cells
        .iter()
        .enumerate()
        .map(|(i, c)| (c.address.as_str(), i as u32))
        .collect();
    let class: Vec<u16> = patch_e.cells.iter().map(|c| c.class).collect();
    let distance: Vec<u32> = patch_e.cells.iter().map(|c| c.distance).collect();
    let vdeg: Vec<u8> = (0..n)
        .map(|c| {
            let d = patch_v.graph.degree(c);
            assert!(d < 256, "vertex degree fits a byte");
            d as u8
        })
        .collect();
    let centroid: Vec<[f64; 2]> = (0..n as usize)
        .map(|c| {
            let (lo, hi) = (geom.offsets[c] as usize, geom.offsets[c + 1] as usize);
            let m = (hi - lo) as f64;
            let (sx, sy) = geom.xy[lo..hi]
                .iter()
                .fold((0.0, 0.0), |(ax, ay), &[x, y]| (ax + x, ay + y));
            [sx / m, sy / m]
        })
        .collect();
    // Substrate length unit for reporting displacement: mean
    // neighbour-centroid spacing over edge-graph edges.
    let (mut edge_sum, mut edge_n) = (0.0f64, 0u64);
    for c in 0..n {
        let pc = centroid[c as usize];
        for &nb in patch_e.graph.neighbours(c) {
            if nb > c {
                let pn = centroid[nb as usize];
                edge_sum += (pc[0] - pn[0]).hypot(pc[1] - pn[1]);
                edge_n += 1;
            }
        }
    }
    let unit = edge_sum / edge_n as f64;

    let (strata, rule) = record.replay_setup(&patch_v);
    let mut engine = Engine::with_strata(patch_v.graph.clone(), strata);
    engine.load_state(&record.initial_state(n).unwrap());

    let mut edge_cache: HashMap<u32, EdgeList> = HashMap::new();
    // Stamped scratch arrays (reset-free across generations).
    let mut stamp: Vec<u32> = vec![u32::MAX; n as usize];
    let mut depth: Vec<u8> = vec![0; n as usize];
    let mut comp_stamp: Vec<u32> = vec![u32::MAX; n as usize];
    let mut trackers: Vec<KeyTracker> = ks.iter().map(|&k| KeyTracker::new(k)).collect();
    let mut crossmaps: Vec<CrossMap> = Vec::new();
    for (bi, &kb) in ks.iter().enumerate() {
        for (si, &ksm) in ks.iter().enumerate() {
            if kb >= ksm + 2 && kb >= 4 {
                crossmaps.push(CrossMap {
                    big: bi,
                    small: si,
                    map: HashMap::new(),
                    violations: Vec::new(),
                });
            }
        }
    }
    let mut prev_ids: Vec<Option<usize>> = vec![None; ks.len()];
    let mut csv_rows: Vec<String> = Vec::new();
    let mut sampled: u64 = 0;
    let mut prev_centroid: Option<[f64; 2]> = None;
    let (mut disp_sum, mut disp_min, mut disp_max) = (0.0f64, f64::MAX, 0.0f64);
    let mut net_start: Option<[f64; 2]> = None;
    let mut net_end = [0.0f64, 0.0f64];
    let (mut pop_min, mut pop_max) = (usize::MAX, 0usize);
    let mut max_components = 0usize;
    let mut multi_component_gens = 0u64;
    let stop_reason;

    let mut generation: u64 = 0;
    loop {
        let state = engine.state();
        let support: Vec<u32> =
            (0..n).filter(|&c| state[c as usize] != 0).collect();
        if support.is_empty() {
            stop_reason = format!("population died at generation {generation}");
            break;
        }
        let maxd = support.iter().map(|&c| distance[c as usize]).max().unwrap();
        if maxd + k_max as u32 + FAN_GUARD > radius {
            stop_reason = format!(
                "terrain margin reached at generation {generation} (support distance {maxd})"
            );
            break;
        }

        if generation >= start_gen {
            // Vertex-connected components of the support: s21 should be
            // a single object post-launch; `farthest` picks one of two.
            let tag = generation as u32;
            let mut components: Vec<Vec<u32>> = Vec::new();
            for &start in &support {
                if comp_stamp[start as usize] == tag {
                    continue;
                }
                comp_stamp[start as usize] = tag;
                let mut queue = VecDeque::from([start]);
                let mut comp = Vec::new();
                while let Some(c) = queue.pop_front() {
                    comp.push(c);
                    for &nb in patch_v.graph.neighbours(c) {
                        if state[nb as usize] != 0 && comp_stamp[nb as usize] != tag {
                            comp_stamp[nb as usize] = tag;
                            queue.push_back(nb);
                        }
                    }
                }
                components.push(comp);
            }
            max_components = max_components.max(components.len());
            if components.len() > 1 {
                multi_component_gens += 1;
            }
            // Object selection for two-glider records: farthest from the
            // root at the first sampled generation, then the component
            // nearest the previously keyed centroid — a raw "farthest"
            // rule flip-flops between gliders as their phase-dependent
            // extents alternate, splicing two objects into one key
            // stream (caught by the forced-map check).
            let keyed_support: Vec<u32> = if farthest {
                let comp_centroid = |comp: &[u32]| {
                    let m = comp.len() as f64;
                    let (sx, sy) = comp.iter().fold((0.0, 0.0), |(ax, ay), &c| {
                        (ax + centroid[c as usize][0], ay + centroid[c as usize][1])
                    });
                    [sx / m, sy / m]
                };
                let chosen = match prev_centroid {
                    Some(pc) => components
                        .iter()
                        .min_by(|a, b| {
                            let d = |comp: &[u32]| {
                                let cc = comp_centroid(comp);
                                (cc[0] - pc[0]).hypot(cc[1] - pc[1])
                            };
                            d(a).partial_cmp(&d(b)).unwrap()
                        })
                        .unwrap(),
                    None => components
                        .iter()
                        .max_by_key(|comp| {
                            comp.iter().map(|&c| distance[c as usize]).max().unwrap()
                        })
                        .unwrap(),
                };
                chosen.clone()
            } else {
                support.clone()
            };
            pop_min = pop_min.min(keyed_support.len());
            pop_max = pop_max.max(keyed_support.len());

            // Multi-source edge-BFS collar to depth k_max.
            let mut collar: Vec<u32> = Vec::new();
            let mut queue: VecDeque<u32> = VecDeque::new();
            for &c in &keyed_support {
                stamp[c as usize] = tag;
                depth[c as usize] = 0;
                collar.push(c);
                queue.push_back(c);
            }
            while let Some(c) = queue.pop_front() {
                if depth[c as usize] as usize >= k_max as usize {
                    continue;
                }
                let d = depth[c as usize] + 1;
                for &nb in patch_e.graph.neighbours(c) {
                    if stamp[nb as usize] != tag {
                        stamp[nb as usize] = tag;
                        depth[nb as usize] = d;
                        collar.push(nb);
                        queue.push_back(nb);
                    }
                }
            }
            assert!(collar.len() < u16::MAX as usize, "collar fits u16 indices");
            for &c in &collar {
                edge_cache
                    .entry(c)
                    .or_insert_with(|| recover_edges(c, &patch_e, &addr_index, sys));
            }

            // Canonical key per collar radius: lexicographic min over
            // support anchors.
            let mut cur_ids: Vec<Option<usize>> = Vec::with_capacity(trackers.len());
            let mut row = format!("{generation},{}", keyed_support.len());
            for tr in trackers.iter_mut() {
                let k = tr.k;
                let members = collar
                    .iter()
                    .filter(|&&c| depth[c as usize] <= k)
                    .count();
                let key = keyed_support
                    .iter()
                    .filter_map(|&a| {
                        encode(
                            a, k, tag, &stamp, &depth, members, state, &class,
                            &vdeg, &edge_cache,
                        )
                    })
                    .min();
                match key {
                    Some(key) => {
                        if let Some((_, buf)) = dump.as_mut() {
                            let mut h = 0xcbf2_9ce4_8422_2325u64;
                            for &b in &key {
                                h ^= u64::from(b);
                                h = h.wrapping_mul(0x0000_0100_0000_01b3);
                            }
                            buf.push_str(&format!("{generation},{k},{h:016x}\n"));
                        }
                        let (id, new) = tr.observe(generation, key);
                        cur_ids.push(Some(id));
                        row.push_str(&format!(
                            ",{},{}",
                            tr.ids.len(),
                            u8::from(new)
                        ));
                    }
                    None => {
                        // Collar not edge-connected: break the transition
                        // chain rather than key a partial encoding.
                        tr.disconnected += 1;
                        tr.prev = None;
                        cur_ids.push(None);
                        row.push_str(&format!(",{},", tr.ids.len()));
                    }
                }
            }
            for cm in crossmaps.iter_mut() {
                if let (Some(b), Some(s)) = (prev_ids[cm.big], cur_ids[cm.small]) {
                    match cm.map.get(&b) {
                        Some(&expected) if expected != s => {
                            cm.violations.push((generation, b, expected, s));
                        }
                        Some(_) => {}
                        None => {
                            cm.map.insert(b, s);
                        }
                    }
                }
            }
            prev_ids = cur_ids;

            // Support centroid displacement (report-grade, not part of
            // any key).
            let m = keyed_support.len() as f64;
            let (sx, sy) = keyed_support.iter().fold((0.0, 0.0), |(ax, ay), &c| {
                (ax + centroid[c as usize][0], ay + centroid[c as usize][1])
            });
            let ctr = [sx / m, sy / m];
            let disp = prev_centroid
                .map(|p| (ctr[0] - p[0]).hypot(ctr[1] - p[1]))
                .unwrap_or(0.0);
            if prev_centroid.is_some() {
                disp_sum += disp;
                disp_min = disp_min.min(disp);
                disp_max = disp_max.max(disp);
            }
            prev_centroid = Some(ctr);
            net_start.get_or_insert(ctr);
            net_end = ctr;
            row.push_str(&format!(",{disp:.3}"));
            csv_rows.push(row);
            sampled += 1;
        }

        if generation.is_multiple_of(100) {
            eprintln!(
                "generation {generation}: pop {}, max distance {maxd}",
                support.len()
            );
        }
        rule.step(&mut engine);
        generation += 1;
    }

    // --- summary ---
    println!("\n{stop_reason}");
    println!(
        "sampled {sampled} generations ({start_gen}..{}), support size {pop_min}–{pop_max}",
        start_gen + sampled.saturating_sub(1)
    );
    println!(
        "support components: max {max_components}, {multi_component_gens} sampled generations with >1"
    );
    let net = net_start
        .map(|s| (net_end[0] - s[0]).hypot(net_end[1] - s[1]))
        .unwrap_or(0.0);
    println!(
        "displacement: net {:.1} substrate units ({:.1} raw, unit {unit:.2}); per-generation mean {:.3}, min {:.3}, max {:.3} raw",
        net / unit,
        net,
        disp_sum / (sampled.saturating_sub(1).max(1)) as f64,
        if disp_min == f64::MAX { 0.0 } else { disp_min },
        disp_max
    );
    println!("edge lists recovered+validated for {} cells", edge_cache.len());
    for tr in &trackers {
        let last_new = tr.first_seen.last().copied().unwrap_or(0);
        println!(
            "\nk={}: {} distinct situations, last new at generation {last_new}, {} recurrences, {} disconnected",
            tr.k,
            tr.ids.len(),
            tr.recurrences,
            tr.disconnected
        );
        if tr.violations.is_empty() {
            println!(
                "  same-k transition map deterministic ({} transitions over {} keys)",
                tr.transitions.len(),
                tr.ids.len()
            );
        } else {
            let mut branch_keys: Vec<usize> =
                tr.violations.iter().map(|&(_, key, _, _)| key).collect();
            branch_keys.sort_unstable();
            branch_keys.dedup();
            println!(
                "  same-k branchings: {} (at {} distinct keys — collar-extension \
                 branching if the forced maps below hold):",
                tr.violations.len(),
                branch_keys.len()
            );
            for &(g, key, expected, got) in tr.violations.iter().take(20) {
                println!(
                    "    generation {g}: key {key} -> {got} (previously -> {expected})"
                );
            }
        }
        println!(
            "  first-seen generations: {}",
            tr.first_seen
                .iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
    }

    if !crossmaps.is_empty() {
        println!(
            "\nforced cross-radius maps (key_kb(t) → key_ks(t+1); any violation here \
             means the encoding or patch radius is insufficient):"
        );
        let mut sound = true;
        for cm in &crossmaps {
            let (kb, ksm) = (ks[cm.big], ks[cm.small]);
            if cm.violations.is_empty() {
                println!(
                    "  k{kb} → next k{ksm}: DETERMINISTIC ({} keys mapped)",
                    cm.map.len()
                );
            } else {
                sound = false;
                println!(
                    "  k{kb} → next k{ksm}: {} VIOLATIONS:",
                    cm.violations.len()
                );
                for &(g, b, expected, got) in cm.violations.iter().take(10) {
                    println!(
                        "    generation {g}: key {b} -> {got} (previously -> {expected})"
                    );
                }
            }
        }
        println!(
            "encoding soundness verdict: {}",
            if sound { "PASS" } else { "FAIL — investigate before trusting counts" }
        );
    }

    let mut csv = String::from("gen,pop");
    for &k in &ks {
        csv.push_str(&format!(",distinct_k{k},new_k{k}"));
    }
    csv.push_str(",disp\n");
    for row in &csv_rows {
        csv.push_str(row);
        csv.push('\n');
    }
    std::fs::write(&csv_path, csv).expect("write csv");
    println!("\nper-generation curve written to {csv_path}");
    if let Some((p, buf)) = dump {
        std::fs::write(&p, buf).expect("write key dump");
        println!("key dump written to {p}");
    }
}
