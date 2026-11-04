// Independent cross-check of the crate's transducers against the paper's recursive Algorithm 1
// (arXiv:2512.16595 §4.1, p.10-11), reimplemented here from the pseudocode using only the public
// combinatorial data (Layer::subtiles / Layer::adjacency / Layer::nsubedges).
use std::collections::{HashMap, HashSet, VecDeque};
use stt_new::address::{EdgeAddrSymbol as E, FiniteTileAddress, TileAddrSymbol as TS, TileAddress};
use stt_new::combinatorial::CombSystem;
use stt_new::common::{LayerIndex, Subedge, System, TileIndex};

#[derive(Debug)]
struct Overflow;

// Algorithm 1. `addr[0]` = Initial{base_type, edge}, `addr[j]` = Supertile{parent_type, subtile} for j>=1,
// with addr[j] living in layer parent^j(layer0). Returns the neighbour's address of the same length.
fn alg1(cs: &CombSystem, addr: &[E], layer0: LayerIndex) -> Result<Vec<E>, Overflow> {
    let E::Initial { base_type: _t0, edge: e } = addr[0] else { panic!("bad addr[0]") };
    if addr.len() < 2 { return Err(Overflow); }
    let E::Supertile { parent_type: mut t1, subtile: i1 } = addr[1] else { panic!("bad addr[1]") };
    let l0 = cs.lookup_layer(layer0);
    let mut tail: Vec<E> = addr[2..].to_vec();
    let mut lookup = *l0.adjacency[&t1].get(&Subedge::Int { subtile: i1, edge: e })
        .unwrap_or_else(|| panic!("adjacency map of {} lacks I_{{{},{}}}", cs.lookup_tile_name(t1), i1, e));
    loop {
        match lookup {
            Subedge::Int { subtile: i1p, edge: ep } => {
                let t0p = l0.subtiles[&t1][i1p];
                let mut out = vec![E::Initial { base_type: t0p, edge: ep }, E::Supertile { parent_type: t1, subtile: i1p }];
                out.extend(tail);
                return Ok(out);
            }
            Subedge::Ext { edge: u, subedge: v } => {
                let mut super_addr = vec![E::Initial { base_type: t1, edge: u }];
                super_addr.extend(tail.iter().copied());
                let new_super = alg1(cs, &super_addr, l0.parent)?;
                let E::Initial { base_type: t1p, edge: up } = new_super[0] else { panic!() };
                t1 = t1p;
                tail = new_super[1..].to_vec();
                lookup = *l0.adjacency[&t1p].get(&Subedge::Ext { edge: up, subedge: -v })
                    .unwrap_or_else(|| panic!("adjacency map of {} lacks E_{{{},{}}}", cs.lookup_tile_name(t1p), up, -v));
            }
        }
    }
}

// Level of the lowest common supertile of two equal-length addresses: smallest j>=1 with equal parent
// type at j and identical symbols above j.
fn common_level(a: &[E], b: &[E]) -> Option<usize> {
    let n = a.len();
    (1..n).find(|&j| {
        let (E::Supertile { parent_type: pa, .. }, E::Supertile { parent_type: pb, .. }) = (a[j], b[j]) else { return false };
        pa == pb && a[j + 1..] == b[j + 1..]
    })
}

fn to_tile_syms(addr: &[E]) -> (Vec<TS>, usize) {
    let E::Initial { edge, .. } = addr[0] else { panic!() };
    (addr.iter().map(|s| match *s {
        E::Initial { base_type, .. } => TS::Initial { base_type },
        E::Supertile { parent_type, subtile } => TS::Supertile { parent_type, subtile },
    }).collect(), edge)
}

fn check_adjacency_maps(cs: &CombSystem) -> usize {
    // Every adjacency map is a fixed-point-free involution covering every I_{i,e} of every subtile and
    // every E_{u,v} of every outline edge (paper §2.2, p.3).
    let mut problems = 0;
    for layer in &cs.layers {
        for (&t, map) in &layer.adjacency {
            let mut expected: HashSet<Subedge> = HashSet::new();
            for (i, &sub) in layer.subtiles[&t].iter().enumerate() {
                for e in 0..cs.lookup_tile_nedges(sub) { expected.insert(Subedge::Int { subtile: i, edge: e }); }
            }
            for (u, &n) in layer.nsubedges[&t].iter().enumerate() {
                for k in 0..n { expected.insert(Subedge::Ext { edge: u, subedge: -(n as isize) + 1 + 2 * k as isize }); }
            }
            let keys: HashSet<Subedge> = map.keys().copied().collect();
            if keys != expected { problems += 1; eprintln!("layer {} tile {}: key set mismatch", layer.name, cs.lookup_tile_name(t)); }
            for (k, v) in map {
                if k == v || map.get(v) != Some(k) { problems += 1; eprintln!("layer {} tile {}: not a fixed-point-free involution at {:?}", layer.name, cs.lookup_tile_name(t), k); }
            }
        }
    }
    problems
}

fn name(cs: &CombSystem, s: &E) -> String {
    match *s {
        E::Initial { base_type, edge } => format!("({},e{})", cs.lookup_tile_name(base_type), edge),
        E::Supertile { parent_type, subtile } => format!("({}<-{})", subtile, cs.lookup_tile_name(parent_type)),
    }
}
fn show(cs: &CombSystem, a: &[E]) -> String { a.iter().map(|s| name(cs, s)).collect::<Vec<_>>().join("") }

// Algorithm 1 with a printed trace (same logic as alg1).
fn alg1_trace(cs: &CombSystem, addr: &[E], layer0: LayerIndex, depth: usize) -> Result<Vec<E>, Overflow> {
    let ind = "  ".repeat(depth);
    let E::Initial { base_type: _t0, edge: e } = addr[0] else { panic!() };
    if addr.len() < 2 { println!("{ind}address exhausted -> overflow"); return Err(Overflow); }
    let E::Supertile { parent_type: mut t1, subtile: i1 } = addr[1] else { panic!() };
    let l0 = cs.lookup_layer(layer0);
    let mut tail: Vec<E> = addr[2..].to_vec();
    let mut lookup = l0.adjacency[&t1][&Subedge::Int { subtile: i1, edge: e }];
    println!("{ind}layer {}: I_{{{},{}}} in map of {} -> {:?}", l0.name, i1, e, cs.lookup_tile_name(t1), lookup);
    loop {
        match lookup {
            Subedge::Int { subtile: i1p, edge: ep } => {
                let t0p = l0.subtiles[&t1][i1p];
                let mut out = vec![E::Initial { base_type: t0p, edge: ep }, E::Supertile { parent_type: t1, subtile: i1p }];
                out.extend(tail);
                println!("{ind}  -> interior: neighbour is subtile {} ({}) of {}, edge {}", i1p, cs.lookup_tile_name(t0p), cs.lookup_tile_name(t1), ep);
                return Ok(out);
            }
            Subedge::Ext { edge: u, subedge: v } => {
                println!("{ind}  -> on outline of {} (edge {}, sub-edge {}); climb to find neighbour of {} across edge {}", cs.lookup_tile_name(t1), u, v, cs.lookup_tile_name(t1), u);
                let mut super_addr = vec![E::Initial { base_type: t1, edge: u }];
                super_addr.extend(tail.iter().copied());
                let new_super = alg1_trace(cs, &super_addr, l0.parent, depth + 1)?;
                let E::Initial { base_type: t1p, edge: up } = new_super[0] else { panic!() };
                t1 = t1p;
                tail = new_super[1..].to_vec();
                lookup = l0.adjacency[&t1p][&Subedge::Ext { edge: up, subedge: -v }];
                println!("{ind}  back in layer {}: E_{{{},{}}} in map of {} -> {:?}", l0.name, up, -v, cs.lookup_tile_name(t1p), lookup);
            }
        }
    }
}

fn run(name_: &str, sys: &stt_new::builtin::BuiltinSystem, max_len: usize, max_checks: usize) {
    let cs = &sys.cs_fine;
    let tr = &sys.tr;
    println!("== {name_}: adjacency-map structural problems = {}", check_adjacency_maps(cs));
    let base = cs.base_layer;
    // queue: (address, layer of its top symbol, expected output if the transducer already accepted)
    let mut queue: VecDeque<(Vec<E>, LayerIndex, Option<(Vec<E>, usize)>)> = VecDeque::new();
    for &t in &cs.lookup_layer(base).tiles {
        for e in 0..cs.lookup_tile_nedges(t) {
            queue.push_back((vec![E::Initial { base_type: t, edge: e }], base, None));
        }
    }
    let (mut checked, mut succ, mut open, mut early, mut confirmed, mut contradictions, mut unconfirmed, mut other) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut max_level = 0usize; let mut max_extra_depth = 0usize; let mut traced = false;
    let mut early_by_len: std::collections::BTreeMap<usize, usize> = Default::default();
    while let Some((addr, top_layer, expected)) = queue.pop_front() {
        let top_type = match *addr.last().unwrap() { E::Initial { base_type, .. } => base_type, E::Supertile { parent_type, .. } => parent_type };
        let parent_layer = cs.lookup_layer(top_layer).parent;
        let pl = cs.lookup_layer(parent_layer);
        let mut labels: Vec<(TileIndex, usize)> = Vec::new();
        for &p in &pl.tiles {
            for (i, &sub) in cs.lookup_layer(top_layer).subtiles[&p].iter().enumerate() {
                if sub == top_type { labels.push((p, i)); }
            }
        }
        labels.sort();
        for (p, i) in labels {
            let mut a = addr.clone();
            let sym = E::Supertile { parent_type: p, subtile: i };
            a.push(sym);
            checked += 1;
            let r1 = alg1(cs, &a, base);
            let (tsyms, edge) = to_tile_syms(&a);
            let r2 = FiniteTileAddress::from(tsyms).neighbour_with_accept_point(edge, tr);
            let out2: Option<(Vec<E>, usize)> = r2.as_ref().ok().map(|(out_tiles, out_edge, accept)| {
                let v: Vec<E> = out_tiles.iter().enumerate().map(|(j, s)| match s {
                    TS::Initial { base_type } => { assert_eq!(j, 0); E::Initial { base_type, edge: *out_edge } }
                    TS::Supertile { parent_type, subtile } => E::Supertile { parent_type, subtile },
                }).collect();
                (v, accept.unwrap())
            });
            match expected {
                Some((ref b, m)) => {
                    // transducer already accepted on the prefix: it must keep accepting at m and copy the symbol
                    let mut bx = b.clone(); bx.push(sym);
                    match &out2 { Some((v, acc)) if *v == bx && *acc == m => {}, _ => { other += 1; if other <= 3 { eprintln!("identity-tail violation on {}", show(cs, &a)); } } }
                    match r1 {
                        Ok(out1) => { if out1 == bx { confirmed += 1; max_extra_depth = max_extra_depth.max(a.len() - 1 - m); } else { contradictions += 1; if contradictions <= 5 { eprintln!("CONTRADICTION {}: transducer {} vs Algorithm 1 {}", show(cs, &a), show(cs, &bx), show(cs, &out1)); } } }
                        Err(Overflow) => { if a.len() < max_len && checked < max_checks { queue.push_back((a, parent_layer, Some((bx, m)))); } else { unconfirmed += 1; } }
                    }
                }
                None => match (r1, out2) {
                    (Ok(out1), Some((v, acc))) => {
                        succ += 1;
                        let lvl = common_level(&a, &out1);
                        if v != out1 || Some(acc) != lvl { other += 1; if other <= 5 { eprintln!("MISMATCH on {}: alg1={} tr={} accept={} level={:?}", show(cs, &a), show(cs, &out1), show(cs, &v), acc, lvl); } }
                        if let Some(l) = lvl { max_level = max_level.max(l); }
                    }
                    (Err(Overflow), None) => { open += 1; if a.len() < max_len && checked < max_checks { queue.push_back((a, parent_layer, None)); } }
                    (Err(Overflow), Some((v, acc))) => {
                        early += 1; *early_by_len.entry(a.len()).or_default() += 1;
                        if !traced { traced = true; println!("--- example of early acceptance: {} ; transducer accepts at symbol index {} with neighbour {}", show(cs, &a), acc, show(cs, &v)); println!("    Algorithm 1 trace:"); let _ = alg1_trace(cs, &a, base, 2); }
                        if a.len() < max_len && checked < max_checks { queue.push_back((a, parent_layer, Some((v, acc)))); } else { unconfirmed += 1; }
                    }
                    (Ok(out1), None) => { other += 1; if other <= 5 { eprintln!("TRANSDUCER FAILED where Algorithm 1 succeeded: {} -> {}", show(cs, &a), show(cs, &out1)); } }
                },
            }
        }
    }
    println!("   addresses checked={checked}; Alg1 and transducer agree (neighbour, accept level)={succ}; both undetermined (extended)={open}");
    println!("   early acceptances (transducer accepts, Alg1 overflows)={early} by length {:?}", early_by_len);
    println!("   ... confirmed by Alg1 on a continuation={confirmed} (max extra symbols needed={max_extra_depth}); contradictions={contradictions}; continuations still open at max_len={unconfirmed}");
    println!("   other mismatches (output/level/identity-tail/transducer-failure)={other}; max common-supertile level seen={max_level}");
}

fn main() {
    let hat = stt_new::builtin::hat_htpf(stt_new::builtin::HatTilingType::Hats);
    let sp = stt_new::builtin::spectre();
    run("hat-htpf (HHTPFFF refinement)", &hat, 12, 6_000_000);
    run("spectre (9-type refinement)", &sp, 11, 6_000_000);
    // also: accepting states have empty pending output
    for (name, sys) in [("hat", &hat), ("spectre", &sp)] {
        let (tr, descs) = sys.cs_fine.transducer_from_recogniser_with_descs(&sys.recogniser_fine).unwrap();
        let ok = tr.accept.iter().all(|&s| descs[s].0.iter().all(|(_, p)| p.is_empty()));
        println!("{name}: every accepting state has zero pending output = {ok}");
    }
}
