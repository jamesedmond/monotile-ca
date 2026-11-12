//! certify — legal ball atlas, its validation, the situation closure and its cocycle.
//!
//!   certify types <hat|spectre>
//!   certify dfa <hat|spectre>
//!   certify atlas <family> <R> [--out FILE] [--factor F] [--verify] [--full-keys]
//!   certify validate <family> <atlas.json> [--radius N] [--stride S] [--root ADDR]
//!   certify closure <record.jsonl> <atlas.json> [--k 4] [--start 60] [--radius 384]
//!                   [--index 0] [--out FILE]
//!   certify census | convoy | ball-key | relabel | cocycle | stats | sccs | trace | param | situation-dump | check-graph | debug-key | dfa-dump
//!     (arguments documented at each arm of `main`)
//!
//! The row-structure tooling of a follow-up paper (address enumeration, return words, pose
//! checks, corridor tables) is kept in a separate, not yet published crate that uses this
//! one as a library.
use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Instant;

use certify::atlas::{ExploreOpts, explore, type_name};
use certify::ball::{Atlas, EdgeCtx};
use certify::closure::{StepRule, run_closure};
use certify::ctx::Ctx;
use certify::dfa::dfa_facts;
use certify::patchctx::{Component, PatchCtx, replay_flight};
use certify::validate::{check_patch, check_witnesses};
use tiling_core::results::ResultRecord;
use tiling_core::{Tiling, TilingFamily};

fn family(s: &str) -> TilingFamily {
    match s {
        "hat" => TilingFamily::Hat,
        "spectre" => TilingFamily::Spectre,
        "penrosep2" | "p2" => TilingFamily::PenroseP2,
        "penrosep3" | "p3" => TilingFamily::PenroseP3,
        other => panic!("unknown family {other}"),
    }
}

fn family_name(f: TilingFamily) -> &'static str {
    match f {
        TilingFamily::Hat => "hat",
        TilingFamily::Spectre => "spectre",
        TilingFamily::PenroseP2 => "penrosep2",
        TilingFamily::PenroseP3 => "penrosep3",
    }
}

struct Args {
    positional: Vec<String>,
    flags: HashMap<String, String>,
    switches: HashSet<String>,
}

fn parse_args() -> Args {
    let mut positional = Vec::new();
    let mut flags = HashMap::new();
    let mut switches = HashSet::new();
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        if let Some(name) = a.strip_prefix("--") {
            if let Some((n, v)) = name.split_once('=') {
                flags.insert(n.to_string(), v.to_string());
                continue;
            }
            match it.peek() {
                Some(v) if !v.starts_with("--") => {
                    flags.insert(name.to_string(), it.next().unwrap());
                }
                _ => {
                    switches.insert(name.to_string());
                }
            }
        } else {
            positional.push(a);
        }
    }
    Args {
        positional,
        flags,
        switches,
    }
}

impl Args {
    fn flag<T: std::str::FromStr>(&self, name: &str, default: T) -> T
    where
        T::Err: std::fmt::Debug,
    {
        self.flags
            .get(name)
            .map(|v| v.parse().expect("flag value"))
            .unwrap_or(default)
    }
}

fn load_atlas(path: &str) -> Atlas {
    let text = fs::read_to_string(path).expect("read atlas");
    serde_json::from_str(&text).expect("parse atlas")
}

fn main() {
    let args = parse_args();
    let cmd = args.positional.first().map(String::as_str).unwrap_or("");
    match cmd {
        "types" => {
            // The refined combinatorial system's tile table, as the atlas dumps index it
            // (path symbols are [type index, slot]), plus the class table:
            //   certify types <family>
            let tiling = Tiling::new(family(&args.positional[1]));
            let ctx = Ctx::new(&tiling);
            let cs = ctx.cs;
            println!("# base layer {}", cs.base_layer.0);
            println!("# tile\tindex\tname\tnedges\tlayers");
            for (i, t) in cs.tiles.iter().enumerate() {
                let layers: Vec<String> = cs
                    .layers
                    .iter()
                    .filter(|l| l.tiles.contains(&substitution_tiling_transducers::common::TileIndex(i)))
                    .map(|l| format!("{}:{}", l.index.0, l.name))
                    .collect();
                println!("tile\t{i}\t{}\t{}\t{}", t.name, t.nedges, layers.join(","));
            }
            println!("# class\tindex\tparent\tsubtile\tbase");
            for (i, class) in tiling.classes().iter().enumerate() {
                println!("class\t{i}\t{}\t{}\t{}", class.parent, class.subtile, ctx.tile_name(ctx.base_of_class[i]));
            }
        }
        "dfa" => {
            let tiling = Tiling::new(family(&args.positional[1]));
            let f = dfa_facts(&tiling.system().tr);
            println!(
                "{}: {} states ({} accepting), {} transitions; pending transitions that emit: {}; accepting transitions not copying one symbol: {}; accepting -> non-accepting: {}; burst lengths at acceptance: {:?}",
                args.positional[1],
                f.states,
                f.accepting,
                f.transitions,
                f.pending_emit_transitions,
                f.accepting_noncopy_transitions,
                f.accepting_leaves_accept,
                f.burst_lengths
            );
        }
        "dfa-dump" => {
            // What the library's transducer emits for short finite prefixes.
            use substitution_tiling_transducers::address::{
                FiniteTileAddress, TileAddrSymbol, TileAddress,
            };
            use substitution_tiling_transducers::common::DisplayViaSystem;
            let tiling = Tiling::new(family(&args.positional[1]));
            let ctx = Ctx::new(&tiling);
            let cs = ctx.cs;
            for &ty in &ctx.base_types {
                let init = vec![TileAddrSymbol::Initial { base_type: ty }];
                println!("type {} ({} edges):", ctx.tile_name(ty), ctx.nedges(ty));
                for e in 0..ctx.nedges(ty) {
                    let a = FiniteTileAddress::from(init.clone());
                    let (out, edge) = a.neighbour_partial(e, ctx.tr);
                    println!(
                        "  prefix [type] edge {e}: partial output '{}' back {:?}",
                        out.display(cs),
                        edge.ok()
                    );
                }
                for &(p, s) in ctx.legal_parents(cs.base_layer, ty) {
                    let mut syms = init.clone();
                    syms.push(TileAddrSymbol::Supertile {
                        parent_type: p,
                        subtile: s,
                    });
                    let a = FiniteTileAddress::from(syms);
                    println!("  prefix {}:", a.display(cs));
                    for e in 0..ctx.nedges(ty) {
                        let (out, edge) = a.neighbour_partial(e, ctx.tr);
                        let acc = a.neighbour_with_accept_point(e, ctx.tr).ok().map(|t| t.2);
                        println!(
                            "    edge {e}: '{}' back {:?} accept {:?}",
                            out.display(cs),
                            edge.ok(),
                            acc
                        );
                    }
                }
            }
        }
        "atlas" => {
            let fam = family(&args.positional[1]);
            let r: u32 = args.positional[2].parse().expect("radius");
            let out = args
                .flags
                .get("out")
                .cloned()
                .unwrap_or_else(|| format!("out/atlas-{}-r{r}.json", family_name(fam)));
            let dump = args.switches.contains("dump");
            let opts = ExploreOpts {
                region_factor: args.flag("factor", 1.0),
                verify: args.switches.contains("verify"),
                full_keys: args.switches.contains("full-keys") || dump,
                log_every: args.flag("log", 0u64),
                max_nodes: args.flag("max-nodes", 0u64),
                dump,
            };
            let tiling = Tiling::new(fam);
            let ctx = Ctx::new(&tiling);
            println!(
                "{} R={r}: d_tile {:.3}, d_nb {:.3}, region radius {:.2}",
                family_name(fam),
                ctx.d_tile,
                ctx.d_nb,
                opts.region_factor * (f64::from(r) + 1.0) * ctx.d_nb
            );
            let mut atlas = Atlas {
                family: family_name(fam).to_string(),
                radius: r,
                region_factor: opts.region_factor,
                balls: Vec::new(),
            };
            for &ty in &ctx.base_types {
                let t0 = Instant::now();
                let (balls, stats, graph) = match explore(&ctx, ty, r, &opts) {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("exploration failed: {e}");
                        std::process::exit(1);
                    }
                };
                if let Some(g) = graph {
                    let gpath = format!(
                        "{}.graph-{}.json",
                        out.trim_end_matches(".json"),
                        type_name(&ctx, ty.0)
                    );
                    if let Some(dir) = std::path::Path::new(&gpath).parent() {
                        fs::create_dir_all(dir).ok();
                    }
                    fs::write(&gpath, serde_json::to_string(&g).unwrap()).expect("write graph");
                    println!("  graph: {} nodes written to {gpath}", g.nodes.len());
                }
                println!(
                    "  type {}: {} balls (nodes {}, leaves {}, max level {}, max tiles {}, max live runs {}, max stack {}) in {:.1}s",
                    type_name(&ctx, ty.0),
                    balls.len(),
                    stats.nodes,
                    stats.leaves,
                    stats.max_level,
                    stats.max_tiles,
                    stats.max_live_runs,
                    stats.max_stack,
                    t0.elapsed().as_secs_f64()
                );
                atlas.balls.extend(balls);
            }
            println!("total {} balls at R={r}", atlas.balls.len());
            let sizes: Vec<usize> = atlas.balls.iter().map(|b| b.tiles.len()).collect();
            println!(
                "ball sizes: min {} max {} mean {:.1}",
                sizes.iter().min().unwrap(),
                sizes.iter().max().unwrap(),
                sizes.iter().sum::<usize>() as f64 / sizes.len() as f64
            );
            if let Some(dir) = std::path::Path::new(&out).parent() {
                fs::create_dir_all(dir).ok();
            }
            fs::write(&out, serde_json::to_string(&atlas).unwrap()).expect("write atlas");
            println!("written {out}");
        }
        "debug-key" => {
            // Compare the exploration's incremental key with the recomputed
            // key for one path: certify debug-key <family> <R> <root type index> <p:s,p:s,...>
            use certify::atlas::{Config, Region};
            use substitution_tiling_transducers::common::TileIndex;
            let fam = family(&args.positional[1]);
            let r: u32 = args.positional[2].parse().unwrap();
            let ty = TileIndex(args.positional[3].parse().unwrap());
            let path: Vec<(usize, usize)> = args
                .positional
                .get(4)
                .map(|s| {
                    s.split(',')
                        .filter(|x| !x.is_empty())
                        .map(|x| {
                            let (p, q) = x.split_once(':').unwrap();
                            (p.parse().unwrap(), q.parse().unwrap())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let tiling = Tiling::new(fam);
            let ctx = Ctx::new(&tiling);
            let region = Region {
                centre: ctx.centroid(&ctx.od.place_default(ty)),
                radius: (f64::from(r) + 1.0) * ctx.d_nb,
                d_tile: ctx.d_tile,
                end: None,
            };
            let mut cfg = Config::root(&ctx, ty, &region, true);
            for &(p, s) in &path {
                cfg.feed(&ctx, &region, (TileIndex(p), s));
            }
            let a = cfg.key(&ctx);
            let b = certify::validate::recompute_key(&ctx, ty, &path, r, 1.0);
            println!("exploration key {} bytes, recomputed {} bytes, equal: {}", a.len(), b.len(), a == b);
            if let Some(gfile) = args.flags.get("graph") {
                let text = fs::read_to_string(gfile).expect("read graph");
                let graph: certify::atlas::ExplorationGraph = serde_json::from_str(&text).expect("parse graph");
                if let Some((id, node)) = graph.nodes.iter().enumerate().find(|(_, n)| n.path == path) {
                    let stored: Vec<u8> = (0..node.key.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&node.key[i..i + 2], 16).unwrap())
                        .collect();
                    let first_diff = |x: &[u8], y: &[u8]| x.iter().zip(y).position(|(p, q)| p != q);
                    println!(
                        "graph node {id}: stored {} bytes; stored == exploration: {} (first diff {:?}); stored == recomputed: {} (first diff {:?})",
                        stored.len(),
                        stored == a,
                        first_diff(&stored, &a),
                        stored == b,
                        first_diff(&stored, &b)
                    );
                } else {
                    println!("no graph node with this path");
                }
            }
            let decode = |key: &[u8]| -> Vec<String> {
                let mut out = Vec::new();
                out.push(format!("layer {} top {}", u16::from_be_bytes([key[0], key[1]]), u16::from_be_bytes([key[2], key[3]])));
                let mut p = 4;
                let mut t = 0;
                while p < key.len() {
                    let ty = u16::from_be_bytes([key[p], key[p + 1]]);
                    let class = u16::from_be_bytes([key[p + 2], key[p + 3]]);
                    let ns = key[p + 4] as usize;
                    p += 5;
                    let mut slots = Vec::new();
                    for _ in 0..ns {
                        match key[p] {
                            0 => {
                                slots.push("out".to_string());
                                p += 1;
                            }
                            1 => {
                                slots.push(format!("pend({})", u16::from_be_bytes([key[p + 1], key[p + 2]])));
                                p += 3;
                            }
                            2 => {
                                slots.push(format!("res({},{})", u16::from_be_bytes([key[p + 1], key[p + 2]]), key[p + 3]));
                                p += 4;
                            }
                            3 => {
                                slots.push(format!(
                                    "live({},{};{})",
                                    u16::from_be_bytes([key[p + 1], key[p + 2]]),
                                    key[p + 3],
                                    u16::from_be_bytes([key[p + 4], key[p + 5]])
                                ));
                                p += 6;
                            }
                            x => panic!("bad tag {x}"),
                        }
                    }
                    let nc = key[p] as usize;
                    p += 1;
                    let mut ch = Vec::new();
                    for _ in 0..nc {
                        ch.push(u16::from_be_bytes([key[p], key[p + 1]]));
                        p += 2;
                    }
                    out.push(format!("t{t}: ty {ty} class {class} slots [{}] copy {:?}", slots.join(" "), ch));
                    t += 1;
                }
                out
            };
            let da = decode(&a);
            let db = decode(&b);
            let dc: Vec<String> = args
                .flags
                .get("graph")
                .and_then(|gfile| {
                    let text = fs::read_to_string(gfile).ok()?;
                    let graph: certify::atlas::ExplorationGraph = serde_json::from_str(&text).ok()?;
                    let node = graph.nodes.iter().find(|n| n.path == path)?;
                    let stored: Vec<u8> = (0..node.key.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&node.key[i..i + 2], 16).unwrap())
                        .collect();
                    Some(decode(&stored))
                })
                .unwrap_or_default();
            for i in 0..da.len().max(db.len()).max(dc.len()) {
                let x = da.get(i).map(String::as_str).unwrap_or("-");
                let y = db.get(i).map(String::as_str).unwrap_or("-");
                let z = dc.get(i).map(String::as_str).unwrap_or("-");
                if x != y || (!dc.is_empty() && z != x) {
                    println!("DIFF line {i}:\n  exploration: {x}\n  recomputed:  {y}\n  stored:      {z}");
                }
            }
            println!("{} lines exploration, {} lines recomputed", da.len(), db.len());
        }
        "check-graph" => {
            // Faithfulness of an exploration graph: every node re-derived
            // from its path by the library's driver.
            let fam = family(&args.positional[1]);
            let text = fs::read_to_string(&args.positional[2]).expect("read graph");
            let graph: certify::atlas::ExplorationGraph = serde_json::from_str(&text).expect("parse graph");
            assert_eq!(graph.family, family_name(fam));
            let tiling = Tiling::new(fam);
            let ctx = Ctx::new(&tiling);
            let check_edges = !args.switches.contains("no-edges");
            let t0 = Instant::now();
            let rep = certify::validate::check_graph(&ctx, &graph, check_edges, true);
            println!(
                "graph {} (type {}, R={}): {} nodes checked, {} node mismatches, {} leaf errors, {} ball mismatches, {} nodes with wrong child symbol sets, {} edges checked, {} edge mismatches ({:.0}s)",
                args.positional[2],
                type_name(&ctx, graph.root_ty),
                graph.radius,
                rep.nodes,
                rep.node_mismatches.len(),
                rep.leaf_errors.len(),
                rep.ball_mismatches.len(),
                rep.missing_children.len(),
                rep.edges,
                rep.edge_mismatches.len(),
                t0.elapsed().as_secs_f64()
            );
            for id in rep.node_mismatches.iter().take(5) {
                println!("  NODE MISMATCH {id}: path {:?}", graph.nodes[*id].path);
            }
            for id in rep.leaf_errors.iter().take(5) {
                println!("  LEAF ERROR {id}: path {:?} leaf {}", graph.nodes[*id].path, graph.nodes[*id].leaf);
            }
            for (a, b) in rep.edge_mismatches.iter().take(5) {
                println!("  EDGE MISMATCH {a} -> {b}");
            }
            let ok = rep.node_mismatches.is_empty()
                && rep.leaf_errors.is_empty()
                && rep.ball_mismatches.is_empty()
                && rep.missing_children.is_empty()
                && rep.edge_mismatches.is_empty();
            println!("graph verdict: {}", if ok { "FAITHFUL" } else { "NOT FAITHFUL" });
        }
        "validate" => {
            let fam = family(&args.positional[1]);
            let atlas = load_atlas(&args.positional[2]);
            assert_eq!(atlas.family, family_name(fam));
            let tiling = Tiling::new(fam);
            let ctx = Ctx::new(&tiling);
            let t0 = Instant::now();
            let w = check_witnesses(&atlas, &ctx);
            println!(
                "witness check (library driver): {}/{} balls reproduce exactly ({:.1}s)",
                w.ok,
                w.checked,
                t0.elapsed().as_secs_f64()
            );
            for (i, e) in w.failures.iter().take(10) {
                println!("  FAIL ball {i}: {e}");
            }
            let radius: u32 = args.flag("radius", 64);
            let stride: usize = args.flag("stride", 1);
            let root = args
                .flags
                .get("root")
                .cloned()
                .unwrap_or_else(|| tiling.default_root());
            let t0 = Instant::now();
            let pctx = PatchCtx::new(&ctx, &root, radius);
            println!(
                "patch radius {radius}: {} cells ({:.1}s)",
                pctx.cells(),
                t0.elapsed().as_secs_f64()
            );
            let t0 = Instant::now();
            let p = check_patch(&atlas, &pctx, stride);
            println!(
                "patch check: {}/{} balls found in atlas, {} distinct in patch (atlas {}), {} vertex-adjacency mismatches ({:.1}s)",
                p.found,
                p.checked,
                p.distinct_in_patch,
                atlas.balls.len(),
                p.vadj_mismatch.len(),
                t0.elapsed().as_secs_f64()
            );
            for c in p.missing.iter().take(10) {
                println!("  MISSING: cell {c} (distance {})", pctx.distance(*c));
            }
            for c in p.vadj_mismatch.iter().take(10) {
                println!("  VADJ MISMATCH: cell {c}");
            }
            let verdict = w.failures.is_empty() && p.missing.is_empty() && p.vadj_mismatch.is_empty();
            println!("validation verdict: {}", if verdict { "PASS" } else { "FAIL" });
        }
        "closure" => {
            let path = &args.positional[1];
            let atlas = load_atlas(&args.positional[2]);
            let index: usize = args.flag("index", 0);
            let k: u8 = args.flag("k", 4);
            let start_gen: u64 = args.flag("start", 60);
            let radius: u32 = args.flag("radius", 384);
            let text = fs::read_to_string(path).expect("read record");
            let line = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .nth(index)
                .expect("record index");
            let record: ResultRecord = serde_json::from_str(line).expect("parse record");
            assert_eq!(atlas.family, family_name(record.family), "atlas family");
            let tiling = Tiling::new(record.family);
            let ctx = Ctx::new(&tiling);
            // Control: step with another record's rule (`--rule-from path[:index]`).
            let rule_record: ResultRecord = match args.flags.get("rule-from") {
                Some(spec) => {
                    let (p, i) = spec.rsplit_once(':').unwrap_or((spec, "0"));
                    let t = fs::read_to_string(p).expect("read rule record");
                    let l = t
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .nth(i.parse().expect("index"))
                        .expect("rule record index");
                    let rr: ResultRecord = serde_json::from_str(l).expect("parse rule record");
                    println!("CONTROL: stepping with the rule of {spec} ({})", rr.note);
                    rr
                }
                None => record.clone(),
            };
            let rule = StepRule::from_any(&rule_record.ruleset()).expect("rule");
            println!(
                "closure: {:?} k={k} R={} atlas {} balls, record {} ({})",
                record.family,
                atlas.radius,
                atlas.balls.len(),
                path,
                record.note
            );

            let t0 = Instant::now();
            let pctx = PatchCtx::new(&ctx, &record.root, radius);
            println!("patch radius {radius}: {} cells ({:.1}s)", pctx.cells(), t0.elapsed().as_secs_f64());
            let component = match args.flags.get("component").map(String::as_str) {
                None | Some("all") => Component::All,
                Some("farthest") => Component::Farthest,
                Some(s) => Component::Index(s.parse().expect("component index")),
            };
            let t0 = Instant::now();
            let flight = replay_flight(&pctx, &record, start_gen, k, atlas.radius, component);
            println!(
                "flight ({component:?}): {} isolated generations from {start_gen} ({} skipped as not isolated); {} ({:.1}s)",
                flight.gens.len(),
                flight.skipped_not_isolated,
                flight.stop_reason,
                t0.elapsed().as_secs_f64()
            );
            let mut seeds: Vec<(certify::situation::Situation, String)> = Vec::new();
            let mut seen: HashSet<Vec<u8>> = HashSet::new();
            let mut last_new = 0u64;
            let mut max_rho = 0;
            let mut multi = 0;
            let (mut pop_min, mut pop_max) = (usize::MAX, 0usize);
            let seed_gen: Option<u64> = args.flags.get("seed-gen").map(|v| v.parse().expect("seed-gen"));
            for g in &flight.gens {
                max_rho = max_rho.max(g.rho);
                if g.components > 1 {
                    multi += 1;
                }
                pop_min = pop_min.min(g.support.len());
                pop_max = pop_max.max(g.support.len());
                if seen.insert(g.sit.key.clone()) {
                    last_new = g.generation;
                    if seed_gen.is_none_or(|sg| sg == g.generation) {
                        seeds.push((g.sit.clone(), format!("observed at generation {}", g.generation)));
                    }
                }
            }
            println!(
                "observed: {} distinct situations (last new at generation {last_new}), population {pop_min}-{pop_max}, max rho {max_rho}, {multi} generations with >1 component; seeding from {}",
                seen.len(),
                match seed_gen {
                    Some(g) => format!("the single situation at generation {g}"),
                    None => "all observed situations".to_string(),
                }
            );
            assert!(!seeds.is_empty(), "no seed situation");
            // Atlas completeness on the flight's own terrain.
            let aidx = atlas.index();
            let mut ball_missing = 0;
            let mut ball_checked = 0;
            for g in &flight.gens {
                if let Some(bk) = &g.ball_key {
                    ball_checked += 1;
                    if !aidx.contains_key(bk.as_slice()) {
                        ball_missing += 1;
                    }
                }
            }
            println!(
                "flight terrain: {}/{ball_checked} centre balls found in atlas",
                ball_checked - ball_missing
            );

            let t0 = Instant::now();
            let base_of_class: Vec<usize> = ctx.base_of_class.iter().map(|t| t.0).collect();
            let stratum_of_class: Vec<u8> = tiling
                .classes()
                .iter()
                .enumerate()
                .map(|(i, c)| rule_record.scheme().stratum_of(i, c))
                .collect();
            let mut cl = run_closure(&atlas, &base_of_class, &stratum_of_class, &rule, &seeds, k, true);
            cl.rule_note = format!("{} [{}]", rule_record.note, path);
            if let (Some(first), Some(last)) = (flight.gens.first(), flight.gens.last()) {
                cl.entry = Some(certify::closure::Entry {
                    record: path.clone(),
                    index,
                    root: record.root.clone(),
                    radius,
                    start_gen,
                    component: format!("{component:?}"),
                    entry_generation: first.generation,
                    entry_state: cl.index[&first.sit.key],
                    last_generation: last.generation,
                });
            }
            println!(
                "closure: {} states, {} transitions, dead state: {:?}, errors: {} ({:.1}s)",
                cl.states.len(),
                cl.transitions.len(),
                cl.dead,
                cl.errors.len(),
                t0.elapsed().as_secs_f64()
            );
            for e in cl.errors.iter().take(20) {
                println!("  ERROR: {e}");
            }
            let deaths: Vec<&certify::closure::Transition> = cl
                .transitions
                .iter()
                .filter(|t| Some(t.to) == cl.dead)
                .collect();
            println!("death transitions: {}", deaths.len());
            for t in deaths.iter().take(20) {
                println!(
                    "  DEATH: state {} (origin: {}) on ball {} (witness {:?})",
                    t.from, cl.states[t.from].origin, t.ball, atlas.balls[t.ball].witness
                );
            }
            let n_states = cl.states.len();
            let mut branching = 0;
            let mut max_succ = 0;
            let mut balls_used: HashSet<usize> = HashSet::new();
            for t in &cl.transitions {
                balls_used.insert(t.ball);
            }
            let mut pops: Vec<usize> = Vec::new();
            for s in 0..n_states {
                let succ = cl.successors(s);
                if succ.len() > 1 {
                    branching += 1;
                }
                max_succ = max_succ.max(succ.len());
                if Some(s) != cl.dead {
                    pops.push(cl.states[s].population);
                }
            }
            let new_states = n_states - seeds.len() - usize::from(cl.dead.is_some());
            println!(
                "states beyond observed: {new_states}; states with >1 distinct successor: {branching} (max {max_succ}); distinct balls used: {}; population range {}-{}; max rho {}",
                balls_used.len(),
                pops.iter().min().unwrap_or(&0),
                pops.iter().max().unwrap_or(&0),
                cl.states.iter().map(|s| s.rho).max().unwrap_or(0)
            );
            // Control: every observed transition is a closure transition
            // (only meaningful when every observed situation is a state).
            let trans_set: HashSet<(usize, usize)> =
                cl.transitions.iter().map(|t| (t.from, t.to)).collect();
            let mut obs_ok = 0;
            let mut obs_bad = Vec::new();
            let mut obs_unknown = 0;
            let by_pair_id: HashMap<(usize, usize), usize> =
                cl.transitions.iter().enumerate().map(|(i, t)| ((t.from, t.to), i)).collect();
            let mut observed_ids: Vec<usize> = Vec::new();
            for w in flight.gens.windows(2) {
                if w[1].generation != w[0].generation + 1 {
                    continue;
                }
                match (cl.index.get(&w[0].sit.key), cl.index.get(&w[1].sit.key)) {
                    (Some(&a), Some(&b)) => {
                        if trans_set.contains(&(a, b)) {
                            obs_ok += 1;
                            observed_ids.push(by_pair_id[&(a, b)]);
                        } else {
                            obs_bad.push(w[0].generation);
                        }
                    }
                    _ => obs_unknown += 1,
                }
            }
            cl.observed = observed_ids;
            println!(
                "control: {obs_ok}/{} observed transitions are closure transitions ({obs_unknown} observed situations outside the closed set)",
                obs_ok + obs_bad.len()
            );
            // Frequencies: which certificate edges the observed flight
            // used, how often, and the frequency-weighted displacement
            // composed from the transitions' exact motions, against the
            // net displacement read off the patch geometry — an
            // end-to-end check of the frame composition.
            if args.switches.contains("freq") && obs_bad.is_empty() {
                let n = cl.states.len();
                let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
                for (i, t) in cl.transitions.iter().enumerate() {
                    succ[t.from].push(i);
                }
                let mut theta: Vec<Option<u8>> = vec![None; n];
                let s0 = cl.index[&flight.gens[0].sit.key];
                theta[s0] = Some(0);
                let mut q = std::collections::VecDeque::from([s0]);
                while let Some(s) = q.pop_front() {
                    for &i in &succ[s] {
                        let t = &cl.transitions[i];
                        if Some(t.to) == cl.dead {
                            continue;
                        }
                        if theta[t.to].is_none() {
                            theta[t.to] = Some((theta[s].unwrap() + t.rot) % 12);
                            q.push_back(t.to);
                        }
                    }
                }
                let by_pair: HashMap<(usize, usize), usize> =
                    cl.transitions.iter().enumerate().map(|(i, t)| ((t.from, t.to), i)).collect();
                let mut counts: HashMap<usize, usize> = HashMap::new();
                let mut sum = [0.0f64; 2];
                let mut used_gens = 0usize;
                for w in flight.gens.windows(2) {
                    if w[1].generation != w[0].generation + 1 {
                        continue;
                    }
                    let a = cl.index[&w[0].sit.key];
                    let b = cl.index[&w[1].sit.key];
                    let i = by_pair[&(a, b)];
                    *counts.entry(i).or_default() += 1;
                    let v = certify::ball::rotate(cl.transitions[i].offset, theta[a].unwrap_or(0));
                    sum[0] += v[0];
                    sum[1] += v[1];
                    used_gens += 1;
                }
                // Net displacement from patch geometry, in the frame of the
                // first observed centre tile (rotate by its placement).
                let first = &flight.gens[0];
                let last = flight.gens.iter().rev().find(|g| g.generation == first.generation + used_gens as u64).unwrap();
                let (rot0, off0) = pctx.placement(first.centre_cell);
                let (_, offl) = pctx.placement(last.centre_cell);
                let d = [offl[0] - off0[0], offl[1] - off0[1]];
                let d_local = certify::ball::rotate(d, (12 - rot0) % 12);
                let h_comp = sum[1].atan2(sum[0]).to_degrees().rem_euclid(360.0);
                let h_patch = d_local[1].atan2(d_local[0]).to_degrees().rem_euclid(360.0);
                // Absolute heading in the patch frame, from support centroids
                // (comparable with the paper's lane angles).
                let c0 = pctx.centroids[first.centre_cell as usize];
                let c1 = pctx.centroids[last.centre_cell as usize];
                let h_abs = (c1[1] - c0[1]).atan2(c1[0] - c0[0]).to_degrees().rem_euclid(360.0);
                println!(
                    "absolute patch-frame heading over the observed flight: {h_abs:.4}° (mod 60: {:.4}°, mod 30: {:.4}°); population {}-{}",
                    h_abs.rem_euclid(60.0),
                    h_abs.rem_euclid(30.0),
                    flight.gens.iter().map(|g| g.support.len()).min().unwrap(),
                    flight.gens.iter().map(|g| g.support.len()).max().unwrap()
                );
                let mut freq: Vec<usize> = counts.values().copied().collect();
                freq.sort_unstable();
                println!(
                    "frequencies over {used_gens} observed generations: {} of {} certificate edges used (counts min {} max {}); composed displacement heading {h_comp:.6}° vs patch-geometry heading {h_patch:.6}° (diff {:.2e}°), |composed − patch| = {:.2e} units",
                    counts.len(),
                    cl.transitions.len(),
                    freq.first().unwrap_or(&0),
                    freq.last().unwrap_or(&0),
                    (h_comp - h_patch).abs(),
                    (sum[0] - d_local[0]).hypot(sum[1] - d_local[1])
                );
            }
            for g in obs_bad.iter().take(10) {
                println!("  OBSERVED TRANSITION MISSING at generation {g}");
            }
            if let Some(other) = args.flags.get("compare") {
                let text = fs::read_to_string(other).expect("read comparison closure");
                let o: certify::closure::Closure = serde_json::from_str(&text).expect("parse");
                let mine: HashSet<&[u8]> = cl.states.iter().map(|s| s.sit.key.as_slice()).collect();
                let theirs: HashSet<&[u8]> = o.states.iter().map(|s| s.sit.key.as_slice()).collect();
                println!(
                    "compare with {other}: this {} states, other {} states, common {}, only here {}, only there {}",
                    mine.len(),
                    theirs.len(),
                    mine.intersection(&theirs).count(),
                    mine.difference(&theirs).count(),
                    theirs.difference(&mine).count()
                );
            }
            let verdict = cl.errors.is_empty()
                && deaths.is_empty()
                && obs_bad.is_empty()
                && ball_missing == 0;
            println!(
                "closure verdict: {}",
                if verdict {
                    "CLOSED, no death — certificate holds for this rule/lane at this k, R"
                } else {
                    "NOT CERTIFIED (see errors/deaths/controls above)"
                }
            );
            if let Some(out) = args.flags.get("out") {
                if let Some(dir) = std::path::Path::new(out).parent() {
                    fs::create_dir_all(dir).ok();
                }
                fs::write(out, serde_json::to_string(&cl).unwrap()).expect("write closure");
                println!("written {out}");
            }
        }
        "census" => {
            // Object census for one rule: seed the record's two-cell pattern
            // at one generic site per tile class, every shared edge, both
            // orders (the §10.3 ignition sweep); let each launch settle; close
            // every isolated component; bucket by closed set.
            //   certify census <record.jsonl> <atlas.json> [--radius 384] [--settle 150]
            //          [--track 300] [--k 4] [--max-states 6000] [--known DIR] [--out FILE]
            use certify::patchctx::replay_flight_from;
            let path = &args.positional[1];
            let atlas = load_atlas(&args.positional[2]);
            let radius: u32 = args.flag("radius", 384);
            let settle: u64 = args.flag("settle", 150);
            let track: u64 = args.flag("track", 300);
            let k: u8 = args.flag("k", 4);
            let max_states: usize = args.flag("max-states", 6000);
            // Broader census (FINDINGS 10.11.9): anchors per class, all
            // ordered state pairs, random soups, deterministic subsample.
            let anchors_per_class: usize = args.flag("anchors", 1);
            let states_mode: String = args.flags.get("states").cloned().unwrap_or_else(|| "record".into());
            let soups: usize = args.flag("soups", 0);
            let soup_cells: usize = args.flag("soup-cells", 6);
            let soup_radius: u32 = args.flag("soup-radius", 2);
            let rng_seed: u64 = args.flag("rng", 1);
            let max_seedings: usize = args.flag("max-seedings", usize::MAX);
            // Shower screen: follow every fragment of a grower/shower for
            // `screen_delta` generations after settling; fragments whose
            // heading is off every vertex direction (mod 60, ±2°) are then
            // tracked and closed like ordinary components.
            let screen_delta: u64 = args.flag("screen-delta", 0);
            let vertices: Vec<f64> = args
                .flags
                .get("vertices")
                .map(|v| v.split(',').map(|x| x.parse().expect("vertex angle")).collect())
                .unwrap_or_default();
            let max_pop: usize = args.flag("max-pop", 64);
            let max_comps: usize = args.flag("max-comps", 6);
            let text = fs::read_to_string(path).expect("read record");
            let line = text.lines().find(|l| !l.trim().is_empty()).expect("record");
            let record: ResultRecord = serde_json::from_str(line).expect("parse record");
            // --seeds FILE: take every seeding from the records in FILE
            // (their initial cells/states on this record's root and rule)
            // instead of generating pairs and soups — the late-settling
            // pass for showers (FINDINGS 10.11.9).
            let seed_records: Vec<ResultRecord> = match args.flags.get("seeds") {
                Some(f) => fs::read_to_string(f)
                    .expect("read seeds")
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| serde_json::from_str(l).expect("parse seed record"))
                    .collect(),
                None => Vec::new(),
            };
            assert!(
                !seed_records.is_empty() || record.initial_cells.len() == 2,
                "two-cell seed record expected (or --seeds FILE)"
            );
            let s0 = record.initial_states.first().copied().unwrap_or(1);
            let s1 = record.initial_states.get(1).copied().unwrap_or(1);
            let tiling = Tiling::new(record.family);
            let ctx = Ctx::new(&tiling);
            let rule = StepRule::from_any(&record.ruleset()).expect("rule");
            let base_of_class: Vec<usize> = ctx.base_of_class.iter().map(|t| t.0).collect();
            let t0 = Instant::now();
            let pctx = PatchCtx::new(&ctx, &record.root, radius);
            let n = pctx.cells();
            println!(
                "census: {:?} rule {} — seeds ({s0},{s1}) both orders; patch radius {radius} ({} cells, {:.0}s); settle {settle}, track ≤ {track}, k={k}, R={}",
                record.family,
                record.note,
                n,
                t0.elapsed().as_secs_f64(),
                atlas.radius
            );
            let (strata, any_rule) = record.replay_setup(&pctx.patch_v);
            // Known certificates, to name buckets.
            let mut known: Vec<(String, HashSet<Vec<u8>>)> = Vec::new();
            for dir in args.flags.get("known").map(|d| d.split(',').collect::<Vec<_>>()).unwrap_or_default() {
                for entry in fs::read_dir(dir).expect("known dir").flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e == "json") {
                        let text = fs::read_to_string(&p).unwrap();
                        if let Ok(c) = serde_json::from_str::<certify::closure::Closure>(&text)
                            && c.k == k
                        {
                            let keys: HashSet<Vec<u8>> = c
                                .states
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| Some(*i) != c.dead)
                                .map(|(_, s)| s.sit.key.clone())
                                .collect();
                            known.push((p.file_name().unwrap().to_string_lossy().to_string(), keys));
                        }
                    }
                }
                println!("known certificates at k={k}: {}", known.len());
            }
            // Seedings: (label with 5 tab fields: kind, class, anchor, partner, states; cells).
            let band = (radius / 4)..=(radius / 2);
            let mut seedings: Vec<(String, Vec<(u32, u8)>)> = Vec::new();
            for (i, r) in seed_records.iter().enumerate() {
                assert_eq!(r.root, record.root, "seed record {i} has a different root");
                let cells: Vec<(u32, u8)> = r
                    .initial_cells
                    .iter()
                    .enumerate()
                    .map(|(j, &c)| (c, r.initial_states.get(j).copied().unwrap_or(1)))
                    .collect();
                seedings.push((format!("rec{i}\t-\t{}\t-\t{}", cells[0].0, r.note.replace('\t', " ")), cells));
            }
            let state_pairs: Vec<(u8, u8)> = if states_mode == "all" {
                (1..=3u8).flat_map(|a| (1..=3u8).map(move |b| (a, b))).collect()
            } else {
                vec![(s0, s1), (s1, s0)]
            };
            for class in (0..pctx.patch_e.classes.len() as u16).filter(|_| seed_records.is_empty()) {
                let members: Vec<u32> = (0..n)
                    .filter(|&c| {
                        pctx.patch_e.cells[c as usize].class == class
                            && band.contains(&pctx.patch_e.cells[c as usize].distance)
                    })
                    .collect();
                if members.is_empty() {
                    continue;
                }
                let step = (members.len() / anchors_per_class).max(1);
                for &anchor in members.iter().step_by(step).take(anchors_per_class) {
                    for &nb in pctx.patch_e.graph.neighbours(anchor) {
                        for &(sa, sb) in &state_pairs {
                            seedings.push((
                                format!("pair\t{class}\t{anchor}\t{nb}\t{sa}{sb}"),
                                vec![(anchor, sa), (nb, sb)],
                            ));
                        }
                    }
                }
            }
            // Random soups: `soup_cells` distinct cells within `soup_radius`
            // edge-steps of a random band cell, states uniform in 1..=3.
            let mut rng = rng_seed ^ 0x9E37_79B9_7F4A_7C15;
            let mut next = move || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                rng
            };
            let band_cells: Vec<u32> = (0..n)
                .filter(|&c| band.contains(&pctx.patch_e.cells[c as usize].distance))
                .collect();
            for si in (0..soups).filter(|_| seed_records.is_empty()) {
                let anchor = band_cells[(next() % band_cells.len() as u64) as usize];
                let mut ball = vec![anchor];
                let mut seen: HashSet<u32> = HashSet::from([anchor]);
                let mut frontier = vec![anchor];
                for _ in 0..soup_radius {
                    let mut nf = Vec::new();
                    for &c in &frontier {
                        for &d in pctx.patch_e.graph.neighbours(c) {
                            if seen.insert(d) {
                                ball.push(d);
                                nf.push(d);
                            }
                        }
                    }
                    frontier = nf;
                }
                let m = soup_cells.min(ball.len());
                let mut cells: Vec<(u32, u8)> = Vec::with_capacity(m);
                for i in 0..m {
                    let j = i + (next() % (ball.len() - i) as u64) as usize;
                    ball.swap(i, j);
                    cells.push((ball[i], 1 + (next() % 3) as u8));
                }
                let desc: String = cells.iter().map(|(c, s)| format!("{c}:{s}")).collect::<Vec<_>>().join(",");
                seedings.push((format!("soup{si}\t-\t{anchor}\t-\t{desc}"), cells));
            }
            if seedings.len() > max_seedings {
                let mut idx: Vec<usize> = (0..seedings.len()).collect();
                for i in 0..max_seedings {
                    let j = i + (next() % (idx.len() - i) as u64) as usize;
                    idx.swap(i, j);
                }
                let mut keep: Vec<usize> = idx[..max_seedings].to_vec();
                keep.sort_unstable();
                seedings = keep.into_iter().map(|i| seedings[i].clone()).collect();
            }
            println!("{} seedings", seedings.len());
            // Buckets: closed-set signature -> (bucket id).
            let mut buckets: Vec<(HashSet<Vec<u8>>, usize, usize, String)> = Vec::new(); // (keys, states, hits, name)
            let mut outcomes: HashMap<&'static str, usize> = HashMap::new();
            let mut rows: Vec<String> = Vec::new();
            let t_all = Instant::now();
            let mut bucket_headings: Vec<Vec<f64>> = Vec::new();
            for (si, (label, cells)) in seedings.iter().enumerate() {
                let mut initial = vec![0u8; n as usize];
                for &(c, st) in cells {
                    initial[c as usize] = st;
                }
                // First pass: run to `settle`, count live cells and components.
                let Some((pop, ncomp)) = certify::patchctx::probe_at(
                    &pctx,
                    &any_rule,
                    strata.clone(),
                    &initial,
                    settle,
                    u32::from(k) + atlas.radius + 4,
                ) else {
                    *outcomes.entry("died or hit margin").or_default() += 1;
                    rows.push(format!("{si}\t{label}\tdied-or-margin"));
                    continue;
                };
                let mut comp_indices: Vec<usize> = (0..ncomp).collect();
                let mut labels: Vec<String> = Vec::new();
                if pop > max_pop || ncomp > max_comps {
                    *outcomes.entry("grower/shower").or_default() += 1;
                    if screen_delta == 0 {
                        rows.push(format!("{si}\t{label}\tgrower ({pop} cells, {ncomp} comps)"));
                        continue;
                    }
                    let Some(frags) = certify::patchctx::screen_components(
                        &pctx,
                        &any_rule,
                        strata.clone(),
                        &initial,
                        settle,
                        screen_delta,
                        u32::from(k) + atlas.radius + 4,
                    ) else {
                        rows.push(format!("{si}\t{label}\tgrower ({pop} cells, {ncomp} comps): screen hit margin"));
                        continue;
                    };
                    let on_vertex = |h: f64| vertices.iter().any(|v| { let d = (h - v).rem_euclid(60.0); d.min(60.0 - d) <= 2.0 });
                    let (mut on, mut other) = (0usize, 0usize);
                    let mut off: Vec<String> = Vec::new();
                    comp_indices.clear();
                    for f in &frags {
                        if f.heading.is_nan() || f.disp < 0.5 * screen_delta as f64 {
                            other += 1;
                        } else if on_vertex(f.heading) {
                            on += 1;
                        } else {
                            off.push(format!("{}@{:.1}°/{:.0}u", f.index, f.heading, f.disp));
                            comp_indices.push(f.index);
                        }
                    }
                    labels.push(format!(
                        "shower({pop} cells): frags {} on-vertex {on} off-vertex {} [{}] other {other}",
                        frags.len(),
                        off.len(),
                        off.join(",")
                    ));
                    *outcomes.entry("shower fragments on-vertex").or_default() += on;
                    *outcomes.entry("shower fragments off-vertex").or_default() += off.len();
                    if comp_indices.is_empty() {
                        rows.push(format!("{si}\t{label}\t{}", labels.join(" ")));
                        continue;
                    }
                }
                for ci in comp_indices {
                    let flight = replay_flight_from(
                        &pctx,
                        &any_rule,
                        strata.clone(),
                        &initial,
                        settle,
                        k,
                        atlas.radius,
                        Component::Index(ci),
                        track,
                    );
                    if flight.gens.len() < 50 {
                        labels.push(format!("c{ci}:short({})", flight.gens.len()));
                        continue;
                    }
                    // Replay heading of the tracked centre (root frame).
                    let p0 = pctx.centroids[flight.gens[0].centre_cell as usize];
                    let p1 = pctx.centroids[flight.gens.last().unwrap().centre_cell as usize];
                    let disp = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
                    let heading = (p1[1] - p0[1]).atan2(p1[0] - p0[0]).to_degrees().rem_euclid(360.0);
                    let motion = if disp < 5.0 { "stationary".to_string() } else { format!("{heading:.1}°/{disp:.0}u") };
                    let mut seeds: Vec<(certify::situation::Situation, String)> = Vec::new();
                    let mut seen: HashSet<Vec<u8>> = HashSet::new();
                    for g in &flight.gens {
                        if seen.insert(g.sit.key.clone()) {
                            seeds.push((g.sit.clone(), format!("seeding {si} component {ci} gen {}", g.generation)));
                        }
                    }
                    if flight.gens.iter().any(|g| g.rho + u32::from(k) + 3 > atlas.radius) {
                        labels.push(format!("c{ci}:too-big(rho {})@{motion}", flight.gens.iter().map(|g| g.rho).max().unwrap()));
                        continue;
                    }
                    // Shortcut: flight keys all inside an existing bucket or a
                    // known certificate ⇒ the closure stays inside that closed,
                    // death-free set; count it there without re-closing.
                    let flight_keys: Vec<&Vec<u8>> = seeds.iter().map(|(s, _)| &s.key).collect();
                    if let Some(bid) = buckets.iter().position(|(ks, _, _, _)| flight_keys.iter().all(|k| ks.contains(*k))) {
                        buckets[bid].2 += 1;
                        if disp >= 5.0 {
                            bucket_headings[bid].push(heading);
                        }
                        labels.push(format!("c{ci}:bucket{bid}@{motion}"));
                        continue;
                    }
                    if let Some((nm, ks)) = known.iter().find(|(_, ks)| flight_keys.iter().all(|k| ks.contains(*k))) {
                        buckets.push((ks.clone(), ks.len(), 1, nm.clone()));
                        bucket_headings.push(if disp >= 5.0 { vec![heading] } else { Vec::new() });
                        labels.push(format!("c{ci}:bucket{}@{motion}", buckets.len() - 1));
                        continue;
                    }
                    let stratum_of_class: Vec<u8> = tiling
                        .classes()
                        .iter()
                        .enumerate()
                        .map(|(i, c)| record.scheme().stratum_of(i, c))
                        .collect();
                    let cl = run_closure(&atlas, &base_of_class, &stratum_of_class, &rule, &seeds, k, false);
                    let dead_t = cl.transitions.iter().any(|t| Some(t.to) == cl.dead);
                    if !cl.errors.is_empty() || dead_t || cl.states.len() > max_states {
                        labels.push(format!(
                            "c{ci}:NOT-CERTIFIED({} states, {} errors, {})@{motion}",
                            cl.states.len(),
                            cl.errors.len(),
                            if dead_t { "deaths" } else { "no deaths" }
                        ));
                        continue;
                    }
                    let keys: HashSet<Vec<u8>> = cl
                        .states
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| Some(*i) != cl.dead)
                        .map(|(_, s)| s.sit.key.clone())
                        .collect();
                    let bid = match buckets.iter().position(|(ks, _, _, _)| *ks == keys) {
                        Some(i) => {
                            buckets[i].2 += 1;
                            i
                        }
                        None => {
                            let name = known
                                .iter()
                                .find(|(_, ks)| *ks == keys)
                                .map(|(nm, _)| nm.clone())
                                .unwrap_or_else(|| format!("new object ({} states)", keys.len()));
                            buckets.push((keys.clone(), keys.len(), 1, name));
                            bucket_headings.push(Vec::new());
                            if let Some(out) = args.flags.get("out") {
                                let p = format!("{}.bucket{}.json", out.trim_end_matches(".json"), buckets.len() - 1);
                                if let Some(dir) = std::path::Path::new(&p).parent() {
                                    fs::create_dir_all(dir).ok();
                                }
                                fs::write(&p, serde_json::to_string(&cl).unwrap()).ok();
                            }
                            buckets.len() - 1
                        }
                    };
                    if disp >= 5.0 {
                        bucket_headings[bid].push(heading);
                    }
                    labels.push(format!("c{ci}:bucket{bid}@{motion}"));
                }
                *outcomes.entry("launched").or_default() += 1;
                rows.push(format!("{si}\t{label}\t{}", labels.join(" ")));
                if si % 10 == 9 {
                    eprintln!("  census: {}/{} seedings, {} buckets, {:.0}s", si + 1, seedings.len(), buckets.len(), t_all.elapsed().as_secs_f64());
                }
            }
            println!("\noutcomes: {:?}", outcomes);
            println!("buckets (closed sets at k={k}):");
            for (i, (_, size, hits, name)) in buckets.iter().enumerate() {
                let hs = &bucket_headings[i];
                let motion = if hs.is_empty() {
                    "stationary".to_string()
                } else {
                    // Circular mean mod 60 and spread, from replay headings.
                    let (sx, sy) = hs.iter().fold((0.0, 0.0), |(x, y), h| {
                        let r = (h * 6.0).to_radians();
                        (x + r.cos(), y + r.sin())
                    });
                    let mean60 = (sy.atan2(sx).to_degrees() / 6.0).rem_euclid(60.0);
                    let spread = hs
                        .iter()
                        .map(|h| { let d = (h - mean60).rem_euclid(60.0); d.min(60.0 - d) })
                        .fold(0.0f64, f64::max);
                    format!("{} mobile, heading {mean60:.2}° mod 60 (max dev {spread:.2}°)", hs.len())
                };
                println!("  bucket {i}: {size} states, reached by {hits} components — {name}; {motion}");
            }
            if let Some(out) = args.flags.get("out") {
                if let Some(dir) = std::path::Path::new(out).parent() {
                    fs::create_dir_all(dir).ok();
                }
                let mut table = String::from("seeding\tkind\tclass\tanchor\tpartner\tstates\toutcome\n");
                for r in &rows {
                    table.push_str(r);
                    table.push('\n');
                }
                fs::write(out, table).expect("write census table");
                println!("written {out}");
            }
        }
        "ball-key" => {
            // Ball keys of chosen cells, and every cell of the patch whose radius-R ball shares
            // them (an existence count for a local configuration such as a loop circuit's ground):
            //   certify ball-key <family> '<root>' <radius> <R> [--cells C1,C2,...] [--keys FILE]
            //                    [--out-keys FILE] [--list]
            // Each target is reported with its classed key and its class-blind key (classes
            // zeroed: types, degrees and gluings only); --out-keys/--keys carry targets between
            // patches, since keys are canonical.
            let fam = family(&args.positional[1]);
            let root = &args.positional[2];
            let radius: u32 = args.positional[3].parse().expect("radius");
            let r: u32 = args.positional[4].parse().expect("R");
            let tiling = Tiling::new(fam);
            let ctx = Ctx::new(&tiling);
            let pctx = PatchCtx::try_new(&ctx, root, radius).expect("root has seed artifacts");
            let blind = |b: &certify::ball::Ball| -> Vec<u8> {
                let mut t = b.tiles.clone();
                for x in &mut t {
                    x.class = 0;
                }
                certify::ball::ball_key(&t)
            };
            let hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect() };
            let unhex = |s: &str| -> Vec<u8> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect() };
            let mut targets: Vec<(String, Vec<u8>, Vec<u8>)> = Vec::new();
            if let Some(cs) = args.flags.get("cells") {
                for c in cs.split(',') {
                    let c: u32 = c.trim().parse().expect("cell");
                    let b = certify::ball::build_ball(&pctx, c, r, Vec::new()).unwrap_or_else(|e| panic!("{e}"));
                    targets.push((format!("cell {c} of {root} (ring {})", pctx.distance(c)), b.key.clone(), blind(&b)));
                }
            }
            if let Some(f) = args.flags.get("keys") {
                for line in fs::read_to_string(f).expect("read keys").lines().filter(|l| !l.trim().is_empty()) {
                    let mut it = line.split('\t');
                    let label = it.next().unwrap().to_string();
                    let k = unhex(it.next().expect("key"));
                    let kb = unhex(it.next().expect("blind key"));
                    targets.push((label, k, kb));
                }
            }
            if let Some(f) = args.flags.get("out-keys") {
                let s: String = targets.iter().map(|(l, k, kb)| format!("{l}\t{}\t{}\n", hex(k), hex(kb))).collect();
                fs::write(f, s).expect("write keys");
            }
            let mut hits: Vec<(Vec<(u32, u32)>, Vec<(u32, u32)>)> = vec![(Vec::new(), Vec::new()); targets.len()];
            let t0 = Instant::now();
            let mut scanned = 0usize;
            for c in 0..pctx.cells() {
                if pctx.distance(c) + r + 3 > radius {
                    continue;
                }
                let Ok(b) = certify::ball::build_ball(&pctx, c, r, Vec::new()) else { continue };
                scanned += 1;
                let kb = blind(&b);
                for (i, (_, k, kbt)) in targets.iter().enumerate() {
                    if &b.key == k {
                        hits[i].0.push((c, pctx.distance(c)));
                    }
                    if &kb == kbt {
                        hits[i].1.push((c, pctx.distance(c)));
                    }
                }
            }
            println!("{scanned} cells of {} scanned at R = {r} in {:.0}s ({} cells per ball)", pctx.cells(), t0.elapsed().as_secs_f64(),
                certify::ball::build_ball(&pctx, 0, r, Vec::new()).map(|b| b.tiles.len()).unwrap_or(0));
            // sites: matching cells grouped by graph distance <= 3 (a symmetric junction has
            // several equivalent centre tiles with identical balls)
            let sites = |cells: &[(u32, u32)]| -> usize {
                let set: std::collections::HashSet<u32> = cells.iter().map(|&(c, _)| c).collect();
                let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
                let mut n = 0;
                for &(c, _) in cells {
                    if seen.contains(&c) {
                        continue;
                    }
                    n += 1;
                    let mut frontier = vec![(c, 0u32)];
                    seen.insert(c);
                    while let Some((x, d)) = frontier.pop() {
                        if d == 3 {
                            continue;
                        }
                        for e in 0..pctx.nedges(x) {
                            if let Some((y, _)) = pctx.edge(x, e)
                                && !seen.contains(&y)
                            {
                                seen.insert(y);
                                if set.contains(&y) {
                                    frontier.push((y, 0));
                                } else {
                                    frontier.push((y, d + 1));
                                }
                            }
                        }
                    }
                }
                n
            };
            for (i, (l, _, _)) in targets.iter().enumerate() {
                println!(
                    "{l}: {} cells share the classed key ({} sites), {} the class-blind key ({} sites)",
                    hits[i].0.len(),
                    sites(&hits[i].0),
                    hits[i].1.len(),
                    sites(&hits[i].1)
                );
                if args.switches.contains("list") {
                    println!("  classed (cell, ring): {:?}", hits[i].0);
                    println!("  blind   (cell, ring): {:?}", hits[i].1);
                }
            }
        }
        "relabel" => {
            // Are one certificate's situations another's with the states renamed? Re-encode every
            // situation of A under the map (s:t renames state s to t; t = 0 deletes the cell) and look
            // the key up in B; then the same for A's transitions.
            //   certify relabel <certA.json> <certB.json> --map 1:3,2:1,3:0
            let mut a: certify::closure::Closure = serde_json::from_str(&fs::read_to_string(&args.positional[1]).expect("read A")).expect("parse A");
            let mut b: certify::closure::Closure = serde_json::from_str(&fs::read_to_string(&args.positional[2]).expect("read B")).expect("parse B");
            for s in a.states.iter_mut() {
                s.sit.restore();
            }
            for s in b.states.iter_mut() {
                s.sit.restore();
            }
            let mut map: Vec<u8> = (0..=8).collect();
            if let Some(m) = args.flags.get("map") {
                for pair in m.split(',') {
                    let (x, y) = pair.split_once(':').expect("s:t");
                    map[x.trim().parse::<usize>().unwrap()] = y.trim().parse().unwrap();
                }
            }
            let bkeys: HashMap<&[u8], usize> = b.states.iter().enumerate().map(|(i, s)| (s.sit.key.as_slice(), i)).collect();
            let amap: Vec<Option<usize>> = a.states.iter().map(|s| s.sit.relabel(&map).and_then(|k| bkeys.get(k.as_slice()).copied())).collect();
            let hit = amap.iter().filter(|m| m.is_some()).count();
            let distinct: HashSet<usize> = amap.iter().flatten().copied().collect();
            let vanish = a.states.iter().filter(|s| s.sit.relabel(&map).is_none()).count();
            let miss: Vec<usize> = (0..a.states.len()).filter(|&i| amap[i].is_none() && a.states[i].sit.relabel(&map).is_some()).collect();
            println!(
                "{} situations of A under the map {:?}: {hit} land on situations of B ({} distinct of B's {}), {vanish} vanish (no cell left), {} miss{}",
                a.states.len(),
                &map[..4],
                distinct.len(),
                b.states.len(),
                miss.len(),
                if miss.len() <= 24 { format!(": {miss:?}") } else { String::new() }
            );
            let bt: HashSet<(usize, usize)> = b.transitions.iter().map(|t| (t.from, t.to)).collect();
            let (mut tok, mut tmiss) = (0, 0);
            for t in &a.transitions {
                match (amap[t.from], amap[t.to]) {
                    (Some(f), Some(g)) if bt.contains(&(f, g)) => tok += 1,
                    _ => tmiss += 1,
                }
            }
            println!("transitions of A under the map: {tok} are transitions of B, {tmiss} are not (B has {})", b.transitions.len());
        }
        "param" => {
            // Parametric placement (FINDINGS 10.11.19): rot, U, V per cell.
            //   certify param <hat|spectre> '<root>' <radius> [--out FILE]
            let tiling = Tiling::new(family(&args.positional[1]));
            let ctx = Ctx::new(&tiling);
            let radius: u32 = args.positional[3].parse().expect("radius");
            let pctx = PatchCtx::new(&ctx, &args.positional[2], radius);
            let (placed, worst) = certify::param::place_all(&ctx, &pctx);
            eprintln!("{} cells; control: worst vertex deviation at (1, √3) = {worst:.3e}", placed.len());
            let mut out = String::from("cell\taddress\trot\tU\tV\n");
            for (c, p) in placed.iter().enumerate() {
                out.push_str(&format!("{c}\t{}\t{}\t{},{},{},{}\t{},{},{},{}\n", pctx.patch_e.cells[c].address, p.rot, p.u.0[0], p.u.0[1], p.u.0[2], p.u.0[3], p.v.0[0], p.v.0[1], p.v.0[2], p.v.0[3]));
            }
            match args.flags.get("out") {
                Some(f) => fs::write(f, out).expect("write"),
                None => print!("{out}"),
            }
        }
        "situation-dump" => {
            // Figure data (paper 2, Figure 2): a certificate situation embedded in chosen legal balls,
            // stepped once, with polygons, classes, states and collar membership as JSON.
            //   certify situation-dump <cert.json> <record.jsonl> --state S --balls b1,b2 [--radius 14] [--dump-radius 10] --out FILE
            let cl: certify::closure::Closure =
                serde_json::from_str(&fs::read_to_string(&args.positional[1]).expect("read certificate")).expect("parse");
            let text = fs::read_to_string(&args.positional[2]).expect("read record");
            let record: ResultRecord = serde_json::from_str(text.lines().find(|l| !l.trim().is_empty()).unwrap()).expect("parse record");
            let state: usize = args.flag("state", 0);
            let balls: Vec<usize> = args.flags.get("balls").expect("--balls b1,b2").split(',').map(|x| x.trim().parse().expect("ball index")).collect();
            let json = certify::figdump::situation_dump(&cl, &record, state, &balls, args.flag("radius", 14), args.flag("dump-radius", 10))
                .unwrap_or_else(|e| panic!("{e}"));
            match args.flags.get("out") {
                Some(p) => {
                    fs::write(p, &json).expect("write");
                    eprintln!("situation-dump: wrote {p}");
                }
                None => println!("{json}"),
            }
        }
        "trace" => {
            // Instrumented flight (FINDINGS 10.11.11): fly a record in
            // re-rooted windows, key every generation as a certificate state,
            // identify each transition by the observed rigid motion of the
            // centre tile, and check the certificate's exact edge vectors
            // against the tiling's own geometry over the whole flight.
            //   certify trace <record.jsonl> <certificate.json> [--index 0] [--gens N]
            //          [--window 48] [--launch-radius 64] [--launch-gens 60] [--k 4] [--out FILE]
            use ca_engine::CaRule;
            use certify::exact::Zeta;
            use certify::situation::encode;
            let path = &args.positional[1];
            let mut cl: certify::closure::Closure =
                serde_json::from_str(&fs::read_to_string(&args.positional[2]).expect("read certificate")).expect("parse");
            let index: usize = args.flag("index", 0);
            let gens_target: u64 = args.flag("gens", 100_000);
            let window: u32 = args.flag("window", 48);
            let launch_radius: u32 = args.flag("launch-radius", 64);
            let launch_gens: u64 = args.flag("launch-gens", 60);
            let k: u8 = args.flag("k", cl.k);
            let select_heading: Option<f64> = args.flags.get("select-heading").map(|v| v.parse().expect("heading"));
            // --addr FILE: the centre tile's canonical address every keyed
            // generation (its whole ancestor chain), for the hierarchy analysis.
            let mut addr_out: Option<std::io::BufWriter<fs::File>> = args
                .flags
                .get("addr")
                .map(|f| std::io::BufWriter::new(fs::File::create(f).expect("create addr file")));
            let text = fs::read_to_string(path).expect("read record");
            let line = text.lines().filter(|l| !l.trim().is_empty()).nth(index).expect("record index");
            let record: ResultRecord = serde_json::from_str(line).expect("parse record");
            let tiling = Tiling::new(record.family);
            let ctx = Ctx::new(&tiling);
            // Certificate: keys → states; live transitions; orientation potential; exact vectors.
            for st in cl.states.iter_mut() {
                st.sit.restore();
            }
            let key_index: HashMap<Vec<u8>, usize> = cl.states.iter().enumerate().map(|(i, s)| (s.sit.key.clone(), i)).collect();
            let n = cl.states.len();
            let edges: Vec<certify::closure::Transition> = cl.transitions.iter().filter(|t| Some(t.to) != cl.dead).copied().collect();
            let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
            for (i, t) in edges.iter().enumerate() {
                succ[t.from].push(i);
            }
            let mut theta: Vec<Option<u8>> = vec![None; n];
            theta[0] = Some(0);
            let mut queue = std::collections::VecDeque::from([0usize]);
            while let Some(s) = queue.pop_front() {
                let th = theta[s].unwrap();
                for &i in &succ[s] {
                    let t = &edges[i];
                    if theta[t.to].is_none() {
                        theta[t.to] = Some((th + t.rot) % 12);
                        queue.push_back(t.to);
                    }
                }
            }
            let zvec: Vec<Option<Zeta>> = edges
                .iter()
                .map(|t| Zeta::from_float(t.offset).map(|o| Zeta::power(u32::from(theta[t.from].unwrap_or(0))).mul(o)))
                .collect();
            let fvec: Vec<[f64; 2]> = edges.iter().map(|t| certify::ball::rotate(t.offset, theta[t.from].unwrap_or(0))).collect();
            // Rigid window frame → global frame: p_global = rotate(p, rot) + off.
            let mut frame: (u8, [f64; 2]) = (0, [0.0, 0.0]);
            let to_global = |frame: (u8, [f64; 2]), p: [f64; 2]| {
                let r = certify::ball::rotate(p, frame.0);
                [r[0] + frame.1[0], r[1] + frame.1[1]]
            };
            // Launch window.
            let t_start = Instant::now();
            let mut pctx = PatchCtx::new(&ctx, &record.root, launch_radius);
            let (strata, rule) = record.replay_setup(&pctx.patch_v);
            let mut engine = ca_engine::Engine::with_strata(pctx.patch_v.graph.clone(), strata);
            engine.load_state(&record.initial_state(pctx.cells()).unwrap());
            for _ in 0..launch_gens {
                rule.step(&mut engine);
            }
            // Multi-object launches: keep only the component whose direction
            // from the seed is nearest --select-heading (the paper's pairs).
            if let Some(h) = select_heading {
                let state = engine.state().to_vec();
                let support: Vec<u32> = (0..pctx.cells()).filter(|&c| state[c as usize] != 0).collect();
                let comps = certify::patchctx::components_of(&pctx, &state, &support);
                let seed = {
                    let n = record.initial_cells.len() as f64;
                    let (sx, sy) = record.initial_cells.iter().fold((0.0, 0.0), |(a, b), &c| (a + pctx.centroids[c as usize][0], b + pctx.centroids[c as usize][1]));
                    [sx / n, sy / n]
                };
                let dir = |comp: &Vec<u32>| {
                    let n = comp.len() as f64;
                    let (sx, sy) = comp.iter().fold((0.0, 0.0), |(a, b), &c| (a + pctx.centroids[c as usize][0], b + pctx.centroids[c as usize][1]));
                    let ang = (sy / n - seed[1]).atan2(sx / n - seed[0]).to_degrees();
                    let d = (ang - h).rem_euclid(360.0);
                    d.min(360.0 - d)
                };
                let best = comps.iter().min_by(|a, b| dir(a).partial_cmp(&dir(b)).unwrap()).expect("components");
                let mut only = vec![0u8; pctx.cells() as usize];
                for &c in best {
                    only[c as usize] = state[c as usize];
                }
                println!("launch: {} components at generation {launch_gens}; keeping {} cells nearest heading {h}°", comps.len(), best.len());
                engine.load_state(&only);
            }
            let mut rule = rule;
            let mut generation = launch_gens;
            let mut hops = 0u64;
            let mut counts: Vec<u64> = vec![0; edges.len()];
            let mut seq: Vec<u16> = Vec::with_capacity(gens_target as usize);
            let (mut off_cert, mut mismatches, mut examples) = (0u64, 0u64, Vec::<String>::new());
            let mut prev: Option<(usize, u8, [f64; 2])> = None; // state, centre rot, centre offset (window frame)
            let mut start_global: Option<[f64; 2]> = None;
            let mut end_global = [0.0f64; 2];
            let mut exact_sum = Zeta::ZERO;
            let mut float_sum = [0.0f64; 2];
            let mut exact_ok = true;
            let mut max_pop = 0usize;
            let (mut t_hop, mut t_key) = (0.0f64, 0.0f64);
            // Global orientation offset: rot_global − theta[state] must be constant
            // along the flight (orientation lock against the geometry).
            let mut c0: Option<u8> = None;
            let mut lock_violations = 0u64;
            while generation < launch_gens + gens_target {
                let state = engine.state();
                let support: Vec<u32> = (0..pctx.cells()).filter(|&c| state[c as usize] != 0).collect();
                assert!(!support.is_empty(), "died at generation {generation}");
                max_pop = max_pop.max(support.len());
                let maxd = support.iter().map(|&c| pctx.distance(c)).max().unwrap();
                if maxd + u32::from(k) + 6 > pctx.radius {
                    let t_h0 = Instant::now();
                    // Re-root at the centre of the support (its cell of minimal eccentricity).
                    let state_of = |c: u32| state[c as usize];
                    let (sit, order) = encode(&pctx, &support, &state_of, k).expect("keyable at hop");
                    let centre = order[sit.centre().0];
                    let root = pctx.patch_e.cells[centre as usize].address.clone();
                    let (rot_c, off_c) = pctx.placement(centre);
                    let old_state: Vec<(String, u8)> = support.iter().map(|&c| (pctx.patch_e.cells[c as usize].address.clone(), state[c as usize])).collect();
                    pctx = PatchCtx::new(&ctx, &root, window);
                    let (rot_r, off_r) = pctx.placement(0);
                    // New-window point p → old-window: rotate(p − off_r, rot_c − rot_r) + off_c.
                    let dr = (12 + rot_c - rot_r) % 12;
                    let m_off = {
                        let ro = certify::ball::rotate(off_r, dr);
                        [off_c[0] - ro[0], off_c[1] - ro[1]]
                    };
                    // frame ∘ M: rot adds, offset = rotate(m_off, frame.rot) + frame.off.
                    let fo = certify::ball::rotate(m_off, frame.0);
                    frame = ((frame.0 + dr) % 12, [fo[0] + frame.1[0], fo[1] + frame.1[1]]);
                    let addr_index: HashMap<&str, u32> = pctx.patch_e.cells.iter().enumerate().map(|(i, c)| (c.address.as_str(), i as u32)).collect();
                    let mut new_state = vec![0u8; pctx.cells() as usize];
                    for (a, s) in &old_state {
                        let c = *addr_index.get(a.as_str()).unwrap_or_else(|| panic!("live cell {a} not in the new window"));
                        new_state[c as usize] = *s;
                    }
                    let (strata, r2) = record.replay_setup(&pctx.patch_v);
                    rule = r2;
                    engine = ca_engine::Engine::with_strata(pctx.patch_v.graph.clone(), strata);
                    engine.load_state(&new_state);
                    // Carry the previous centre placement into the new frame:
                    // p_new = rotate(p_old − m_off, −dr), rot_new = rot_old − dr.
                    if let Some((ps, prot, poff)) = prev {
                        let q = certify::ball::rotate([poff[0] - m_off[0], poff[1] - m_off[1]], (12 - dr) % 12);
                        prev = Some((ps, (12 + prot - dr) % 12, q));
                    }
                    hops += 1;
                    t_hop += t_h0.elapsed().as_secs_f64();
                    continue;
                }
                // Key this generation.
                let t_k0 = Instant::now();
                let state_of = |c: u32| state[c as usize];
                let keyed = encode(&pctx, &support, &state_of, k);
                t_key += t_k0.elapsed().as_secs_f64();
                let Some((sit, order)) = keyed else {
                    off_cert += 1;
                    prev = None;
                    rule.step(&mut engine);
                    generation += 1;
                    continue;
                };
                let centre = order[sit.centre().0];
                let (rot, off) = pctx.placement(centre);
                let g = to_global(frame, off);
                if start_global.is_none() {
                    start_global = Some(g);
                }
                end_global = g;
                match key_index.get(&sit.key) {
                    None => {
                        off_cert += 1;
                        prev = None;
                    }
                    Some(&s) => {
                        seq.push(s as u16);
                        if let Some(w) = addr_out.as_mut() {
                            use std::io::Write as _;
                            writeln!(w, "{generation}\t{s}\t{}", pctx.patch_e.cells[centre as usize].address).expect("write addr");
                        }
                        let rot_global = (frame.0 + rot) % 12;
                        let c = (12 + rot_global - theta[s].unwrap_or(0)) % 12;
                        match c0 {
                            None => c0 = Some(c),
                            Some(x) if x != c => lock_violations += 1,
                            _ => {}
                        }
                        if let Some((ps, prot, poff)) = prev {
                            // Observed motion of the centre tile in the previous centre's frame.
                            let r = (12 + rot - prot) % 12;
                            let d = [off[0] - poff[0], off[1] - poff[1]];
                            let o = certify::ball::rotate(d, (12 - prot) % 12);
                            let found = succ[ps].iter().copied().find(|&i| {
                                let t = &edges[i];
                                t.to == s && t.rot == r && (t.offset[0] - o[0]).abs() < 1e-6 && (t.offset[1] - o[1]).abs() < 1e-6
                            });
                            match found {
                                Some(i) => {
                                    counts[i] += 1;
                                    float_sum[0] += fvec[i][0];
                                    float_sum[1] += fvec[i][1];
                                    match zvec[i] {
                                        Some(z) => exact_sum = exact_sum.add(z),
                                        None => exact_ok = false,
                                    }
                                }
                                None => {
                                    mismatches += 1;
                                    if examples.len() < 5 {
                                        examples.push(format!("gen {generation}: {ps} -> {s} motion rot {r} off ({:.3},{:.3}) not a certificate transition", o[0], o[1]));
                                    }
                                }
                            }
                        }
                        prev = Some((s, rot, off));
                    }
                }
                rule.step(&mut engine);
                generation += 1;
                if generation.is_multiple_of(100_000) {
                    eprintln!("  trace: gen {generation}, hops {hops}, off-certificate {off_cert}, mismatches {mismatches}, {:.0}s", t_start.elapsed().as_secs_f64());
                }
            }
            let used = counts.iter().filter(|&&c| c > 0).count();
            let total: u64 = counts.iter().sum();
            let sg = start_global.unwrap();
            let geo = [end_global[0] - sg[0], end_global[1] - sg[1]];
            let head_geo = geo[1].atan2(geo[0]).to_degrees().rem_euclid(360.0);
            // Rotate the certificate sum into the global frame.
            let c0v = c0.unwrap_or(0);
            let float_sum = certify::ball::rotate(float_sum, c0v);
            let exact_sum = Zeta::power(u32::from(c0v)).mul(exact_sum);
            let head_cyc = float_sum[1].atan2(float_sum[0]).to_degrees().rem_euclid(360.0);
            let ex = exact_sum.to_float();
            println!(
                "trace {}#{index} with {}: {} generations keyed from {launch_gens}, {hops} hops (window {window}), max population {max_pop}, {:.0}s",
                path, args.positional[2], seq.len(), t_start.elapsed().as_secs_f64()
            );
            println!("  off-certificate generations: {off_cert}; transitions not in the certificate: {mismatches}; transitions used: {used}/{} ({total} traversals); orientation offset {c0v}×30°, lock violations {lock_violations}; time: hops {t_hop:.0}s, keying {t_key:.0}s", edges.len());
            for e in &examples {
                println!("    {e}");
            }
            println!("  geometric displacement of the centre tile: ({:.4}, {:.4}), heading {head_geo:.6}°", geo[0], geo[1]);
            println!("  sum of certificate edge vectors:            ({:.4}, {:.4}), heading {head_cyc:.6}°  (difference from geometry: {:.3e}, {:.3e})", float_sum[0], float_sum[1], float_sum[0] - geo[0], float_sum[1] - geo[1]);
            if exact_ok {
                let z = exact_sum.0; // rotated into the global frame
                println!("  exact sum in Z[ζ12]: {:?} = ({:.4}, {:.4})", z, ex[0], ex[1]);
                // Hat: total = m·A + n·B with A = 16 + 46ζ², B = 10 + 28ζ² (§10.11.3), det = −12.
                if cl.family == "hat" || record.family == TilingFamily::Hat {
                    let (t0, t2) = (z[0] as f64, z[2] as f64);
                    let m = (28.0 * t0 - 10.0 * t2) / -12.0;
                    let nn = (16.0 * t2 - 46.0 * t0) / -12.0;
                    let phi_inv = (5f64.sqrt() - 1.0) / 2.0;
                    println!(
                        "  cycle lattice: total = m·A + n·B with m = {m:.2}, n = {nn:.2} (ζ and ζ³ components {} {}); n/m = {:.6} vs 1/φ = {phi_inv:.6} (diff {:+.2e})",
                        z[1], z[3], nn / m, nn / m - phi_inv
                    );
                }
            }
            if let Some(out) = args.flags.get("out") {
                let mut t = String::from("transition\tfrom\tto\trot\toffset_x\toffset_y\tcount\n");
                for (i, e) in edges.iter().enumerate() {
                    if counts[i] > 0 {
                        t.push_str(&format!("{i}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{}\n", e.from, e.to, e.rot, e.offset[0], e.offset[1], counts[i]));
                    }
                }
                fs::write(out, t).expect("write counts");
                let seq_path = format!("{}.states.u16", out.trim_end_matches(".tsv"));
                let bytes: Vec<u8> = seq.iter().flat_map(|s| s.to_le_bytes()).collect();
                fs::write(&seq_path, bytes).expect("write states");
                println!("  written {out} and {seq_path}");
            }
        }
        "convoy" => {
            // Decompose every state of a convoy object into its vertex-
            // connected pieces (inside the state's own atlas ball) and ask
            // whether each piece is a pattern of a single-glider
            // certificate (keys at collar radius --k, default 1). Answers
            // "gliders in formation" vs "joined object" (FINDINGS 10.11.10).
            //   certify convoy <convoy.json> <atlas.json> <single.json>... [--scc HEX] [--k 1]
            let mut cl: certify::closure::Closure =
                serde_json::from_str(&fs::read_to_string(&args.positional[1]).expect("read convoy")).expect("parse");
            let atlas = load_atlas(&args.positional[2]);
            let kk: u8 = args.flag("k", 1);
            // --blind: compare pattern keys with tile classes stripped.
            let blind = args.switches.contains("blind");
            let norm = |k: Vec<u8>| -> Vec<u8> { if blind { certify::analysis::blind(&k) } else { k } };
            for st in cl.states.iter_mut() {
                st.sit.restore();
            }
            // Single-glider repertoires at collar kk, named by file.
            let mut singles: Vec<(String, HashSet<Vec<u8>>)> = Vec::new();
            for path in &args.positional[3..] {
                let mut sc: certify::closure::Closure =
                    serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|e| panic!("read single {path}: {e}"))).expect("parse single");
                let mut keys = HashSet::new();
                for (i, st) in sc.states.iter_mut().enumerate() {
                    if Some(i) == sc.dead {
                        continue;
                    }
                    st.sit.restore();
                    keys.insert(norm(st.sit.project(kk)));
                }
                let name = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().to_string();
                singles.push((name, keys));
            }
            // Which states: one SCC (by signature prefix) or all live states.
            let mut states: Vec<usize> = (0..cl.states.len()).filter(|&i| Some(i) != cl.dead).collect();
            if let Some(pre) = args.flags.get("scc") {
                let comps = certify::analysis::sccs(&cl);
                let chosen = comps
                    .iter()
                    .find(|c| {
                        let keys: Vec<Vec<u8>> = c.iter().map(|&i| cl.states[i].sit.key.clone()).collect();
                        certify::analysis::signature(&keys).starts_with(pre.as_str())
                    })
                    .expect("no SCC with that signature prefix");
                states = chosen.clone();
            }
            // A transition out of each state gives a ball the state embeds in.
            let mut ball_of: Vec<Option<usize>> = vec![None; cl.states.len()];
            for t in &cl.transitions {
                if ball_of[t.from].is_none() {
                    ball_of[t.from] = Some(t.ball);
                }
            }
            let name_of = |key: &Vec<u8>| -> Option<&str> {
                singles.iter().find(|(_, ks)| ks.contains(key)).map(|(n, _)| n.as_str())
            };
            let mut tally: HashMap<String, usize> = HashMap::new();
            let mut examples: HashMap<String, usize> = HashMap::new();
            let mut pops: Vec<usize> = Vec::new();
            for &si in &states {
                let st = &cl.states[si];
                let Some(bi) = ball_of[si] else {
                    *tally.entry("no transition (unembedded)".into()).or_default() += 1;
                    continue;
                };
                let ball = &atlas.balls[bi];
                let Some(map) = st.sit.embed(st.centre, ball) else {
                    *tally.entry("embed failed".into()).or_default() += 1;
                    continue;
                };
                let live: Vec<usize> = st.sit.support();
                pops.push(live.len());
                let live_ball: Vec<u32> = live.iter().map(|&i| u32::from(map[i])).collect();
                let state_of_ball = |b: u32| -> u8 {
                    live.iter().find(|&&i| u32::from(map[i]) == b).map_or(0, |&i| st.sit.cells[i].state)
                };
                // Vertex-connected pieces of the live set inside the ball.
                let inset: HashSet<u32> = live_ball.iter().copied().collect();
                let mut seen: HashSet<u32> = HashSet::new();
                let mut pieces: Vec<Vec<u32>> = Vec::new();
                for &c in &live_ball {
                    if !seen.insert(c) {
                        continue;
                    }
                    let mut piece = vec![c];
                    let mut stack = vec![c];
                    while let Some(x) = stack.pop() {
                        for y in ball.vertex_neighbours(x) {
                            if inset.contains(&y) && seen.insert(y) {
                                piece.push(y);
                                stack.push(y);
                            }
                        }
                    }
                    piece.sort_unstable();
                    pieces.push(piece);
                }
                let mut names: Vec<String> = Vec::new();
                let mut unknown = 0usize;
                for piece in &pieces {
                    let key = certify::situation::encode(ball, piece, &state_of_ball, kk).map(|(s, _)| norm(s.key));
                    match key.as_ref().and_then(name_of) {
                        Some(n) => names.push(n.to_string()),
                        None => {
                            unknown += 1;
                            names.push(format!("?{}", piece.len()));
                        }
                    }
                }
                // A single unknown connected piece: is it two known gliders touching?
                // A single unknown connected piece: two known gliders touching,
                // or one known glider plus an unknown remainder (a passenger or
                // an altered partner)? Search all 2-partitions; prefer a fully
                // known split, else the split with the smallest remainder.
                let mut split_ok: Option<String> = None;
                let mut partial: Option<(String, usize)> = None;
                if pieces.len() == 1 && unknown == 1 && live.len() <= 18 {
                    let piece = &pieces[0];
                    let m = piece.len();
                    for mask in 1u32..(1 << (m - 1)) {
                        let a: Vec<u32> = (0..m).filter(|&i| mask & (1 << i) != 0).map(|i| piece[i]).collect();
                        let b: Vec<u32> = (0..m).filter(|&i| mask & (1 << i) == 0).map(|i| piece[i]).collect();
                        let ka = certify::situation::encode(ball, &a, &state_of_ball, kk).map(|(s, _)| norm(s.key));
                        let kb = certify::situation::encode(ball, &b, &state_of_ball, kk).map(|(s, _)| norm(s.key));
                        let na = ka.as_ref().and_then(name_of);
                        let nb = kb.as_ref().and_then(name_of);
                        match (na, nb) {
                            (Some(na), Some(nb)) => {
                                split_ok = Some(format!("{na}+{nb} touching"));
                                break;
                            }
                            (Some(na), None) if partial.as_ref().is_none_or(|p| b.len() < p.1) => {
                                partial = Some((na.to_string(), b.len()));
                            }
                            (None, Some(nb)) if partial.as_ref().is_none_or(|p| a.len() < p.1) => {
                                partial = Some((nb.to_string(), a.len()));
                            }
                            _ => {}
                        }
                    }
                }
                names.sort();
                let label = match (pieces.len(), unknown, &split_ok, &partial) {
                    (_, 0, _, _) => format!("{} separate: {}", pieces.len(), names.join(" + ")),
                    (1, 1, Some(sp), _) => sp.clone(),
                    (1, 1, None, Some((n, rem))) => format!("{n} + ?{rem} attached ({} cells)", live.len()),
                    (1, 1, None, None) => format!("1 fused piece, no known part ({} cells)", live.len()),
                    _ => format!("{} pieces, {unknown} unknown: {}", pieces.len(), names.join(" + ")),
                };
                *tally.entry(label.clone()).or_default() += 1;
                examples.entry(label).or_insert(si);
            }
            println!(
                "convoy {}: {} states analysed, population {}–{}; pieces judged against {} single certificates at k={kk}",
                args.positional[1],
                states.len(),
                pops.iter().min().unwrap_or(&0),
                pops.iter().max().unwrap_or(&0),
                singles.len()
            );
            let mut rows: Vec<(&String, &usize)> = tally.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1));
            for (label, n) in rows {
                println!("  {n:5}  {label}   (e.g. state {})", examples.get(label).copied().unwrap_or(0));
            }
        }
        "sccs" => {
            // Object census at the level of strongly connected components:
            //   certify sccs [--project K] [--min N] <closure.json>...
            let project: Option<u8> = args.flags.get("project").map(|v| v.parse().unwrap());
            let min: usize = args.flag("min", 1);
            let mut objects: Vec<certify::analysis::Object> = Vec::new();
            for f in args.positional.iter().skip(1) {
                let text = fs::read_to_string(f).expect("read closure");
                let mut cl: certify::closure::Closure = serde_json::from_str(&text).expect("parse");
                let short = f.rsplit('/').next().unwrap_or(f).to_string();
                for o in certify::analysis::objects_of(&mut cl, &short, project) {
                    if let Some(e) = objects.iter_mut().find(|x| x.signature == o.signature) {
                        e.files.push(short.clone());
                    } else {
                        objects.push(o);
                    }
                }
            }
            objects.sort_by_key(|o| std::cmp::Reverse(o.keys.len()));
            println!(
                "{} distinct objects across {} files{}",
                objects.len(),
                args.positional.len() - 1,
                project.map(|k| format!(" (keys projected to k={k})")).unwrap_or_default()
            );
            for o in objects.iter().filter(|o| o.keys.len() >= min) {
                let c = &o.cocycle;
                // Bounds rounded outward, as in `cocycle`.
                let down = |x: f64| (x * 1e4).floor() / 1e4;
                let up = |x: f64| (x * 1e4).ceil() / 1e4;
                let motion = match c.heading {
                    Some(h) => format!(
                        "window [{:.4}°, {:.4}°] (mod 30 [{:.4}°, {:.4}°], width {:.4}°; reference cycle {:.4}°), speed ≥ {:.4}, lock {}",
                        down(c.window.0),
                        up(c.window.1),
                        down(c.window.0.rem_euclid(30.0)),
                        up(c.window.1.rem_euclid(30.0)),
                        up(c.window.1) - down(c.window.0),
                        h,
                        down(c.along.0),
                        if c.rotation_violations == 0 && c.unreached == 0 { "yes" } else { "NO" }
                    ),
                    None => format!("LOOPER/OSCILLATOR (reference cycle length {} has zero displacement)", c.cycle_len),
                };
                println!(
                    "  {} | {} states ({} class-blind), pop {}-{}, {} | {} | in: {}",
                    &o.signature[..12],
                    o.keys.len(),
                    o.blind_states,
                    o.population.0,
                    o.population.1,
                    if o.terminal { "settled".to_string() } else { format!("converts to {}", o.successors.iter().map(|s| &s[..12]).collect::<Vec<_>>().join(",")) },
                    motion,
                    o.files.join(" ")
                );
            }
        }
        "cocycle" => {
            // Theorem 2 machinery: orientation lock and ballistic progress.
            let text = fs::read_to_string(&args.positional[1]).expect("read closure");
            let cl: certify::closure::Closure = serde_json::from_str(&text).expect("parse");
            let n = cl.states.len();
            let dead = cl.dead;
            let edges: Vec<&certify::closure::Transition> =
                cl.transitions.iter().filter(|t| Some(t.to) != dead).collect();
            assert!(!edges.is_empty());
            // Orientation potential theta (mod 12) by BFS over directed edges.
            let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
            for (i, t) in edges.iter().enumerate() {
                succ[t.from].push(i);
            }
            let mut theta: Vec<Option<u8>> = vec![None; n];
            theta[0] = Some(0);
            let mut queue = std::collections::VecDeque::from([0usize]);
            let mut rot_violations = 0usize;
            while let Some(s) = queue.pop_front() {
                let th = theta[s].unwrap();
                for &i in &succ[s] {
                    let t = edges[i];
                    let th2 = (th + t.rot) % 12;
                    match theta[t.to] {
                        Some(x) if x != th2 => rot_violations += 1,
                        Some(_) => {}
                        None => {
                            theta[t.to] = Some(th2);
                            queue.push_back(t.to);
                        }
                    }
                }
            }
            let unreached = (0..n).filter(|&s| Some(s) != dead && theta[s].is_none()).count();
            // Global-frame edge vectors.
            let vecs: Vec<[f64; 2]> = edges
                .iter()
                .map(|t| certify::ball::rotate(t.offset, theta[t.from].unwrap_or(0)))
                .collect();
            // Direction candidate from one closed walk (first-successor walk).
            let mut visited = vec![None; n];
            let mut s = 0usize;
            let mut walk: Vec<usize> = Vec::new();
            let cycle_sum = loop {
                if let Some(pos) = visited[s] {
                    let mut sum = [0.0f64; 2];
                    for &i in &walk[pos..] {
                        sum[0] += vecs[i][0];
                        sum[1] += vecs[i][1];
                    }
                    break (sum, walk.len() - pos);
                }
                visited[s] = Some(walk.len());
                let i = succ[s][0];
                walk.push(i);
                s = edges[i].to;
            };
            let (cs, clen) = cycle_sum;
            let norm = cs[0].hypot(cs[1]);
            if norm < 1e-9 {
                println!(
                    "{}: {} states, {} live transitions; orientation potential: {} rotation violations, {} unreached; reference cycle (length {clen}) has ZERO displacement — a looper or oscillator, not a glider",
                    args.positional[1], n, edges.len(), rot_violations, unreached
                );
                return;
            }
            let d = [cs[0] / norm, cs[1] / norm];
            let dperp = [-d[1], d[0]];
            let heading = d[1].atan2(d[0]).to_degrees().rem_euclid(360.0);
            // Karp minimum cycle mean for a weight function.
            let karp = |w: &dyn Fn(usize) -> f64| -> f64 {
                let inf = f64::INFINITY;
                let mut dist = vec![vec![inf; n]; n + 1];
                dist[0][0] = 0.0;
                for k in 1..=n {
                    for (i, t) in edges.iter().enumerate() {
                        let cand = dist[k - 1][t.from] + w(i);
                        if cand < dist[k][t.to] {
                            dist[k][t.to] = cand;
                        }
                    }
                }
                let mut best = inf;
                for (v, &dn) in dist[n].iter().enumerate() {
                    if dn == inf {
                        continue;
                    }
                    let mut worst = f64::NEG_INFINITY;
                    for (k, row) in dist.iter().enumerate().take(n) {
                        if row[v] < inf {
                            worst = worst.max((dn - row[v]) / (n - k) as f64);
                        }
                    }
                    best = best.min(worst);
                }
                best
            };
            let mu_par = karp(&|i| vecs[i][0] * d[0] + vecs[i][1] * d[1]);
            let mu_par_max = -karp(&|i| -(vecs[i][0] * d[0] + vecs[i][1] * d[1]));
            let mu_perp_min = karp(&|i| vecs[i][0] * dperp[0] + vecs[i][1] * dperp[1]);
            let mu_perp_max = -karp(&|i| -(vecs[i][0] * dperp[0] + vecs[i][1] * dperp[1]));
            println!(
                "{}: {} states, {} live transitions",
                args.positional[1],
                n,
                edges.len()
            );
            println!(
                "orientation potential: {} rotation violations, {} unreached states — {}",
                rot_violations,
                unreached,
                if rot_violations == 0 && unreached == 0 {
                    "ORIENTATION LOCKED (no cycle rotates)"
                } else {
                    "NOT LOCKED"
                }
            );
            println!(
                "reference cycle: length {clen}, displacement {:.6} units, heading {heading:.6}° (mod 30: {:.6}°)",
                norm,
                heading.rem_euclid(30.0)
            );
            println!(
                "cycle means along heading: min {mu_par:.6}, max {mu_par_max:.6} units/generation — {}",
                if mu_par > 0.0 { "EVERY CYCLE ADVANCES" } else { "some cycle does not advance" }
            );
            println!(
                "cycle means perpendicular: min {mu_perp_min:.9}, max {mu_perp_max:.9} — {}",
                if mu_perp_min.abs() < 1e-7 && mu_perp_max.abs() < 1e-7 {
                    "ALL CYCLES COLLINEAR: the heading is exact"
                } else {
                    "cycles are not collinear (lane drifts within a corridor)"
                }
            );
            // Every cycle's direction lies within this window about the
            // reference heading (conservative: perpendicular extremes over
            // the smallest along-speed); so does every long trajectory's.
            // Printed to four decimals rounded outward (lower ends and the
            // speed down, upper ends up), so every printed figure is itself a
            // valid bound; the width is that of the printed window.
            if mu_par > 0.0 {
                let lo = mu_perp_min.min(0.0).atan2(mu_par).to_degrees();
                let hi = mu_perp_max.max(0.0).atan2(mu_par).to_degrees();
                let down = |x: f64| (x * 1e4).floor() / 1e4;
                let up = |x: f64| (x * 1e4).ceil() / 1e4;
                let (wlo, whi) = (down(heading + lo), up(heading + hi));
                println!(
                    "rigorous heading window: [{:.4}°, {:.4}°] (mod 30: [{:.4}°, {:.4}°]), width {:.4}°; speed ≥ {:.4} units/generation along it",
                    wlo,
                    whi,
                    down((heading + lo).rem_euclid(30.0)),
                    up((heading + hi).rem_euclid(30.0)),
                    whi - wlo,
                    down(mu_par)
                );
            }
            // Exact version: translations as elements of Z[ζ12], rotations
            // as powers of ζ, components in (Z + Z√3)/2, Karp with exact
            // comparisons.
            if args.switches.contains("exact") {
                use certify::exact::{Half, Zeta, karp_exact};
                let zvec: Vec<Zeta> = edges
                    .iter()
                    .map(|t| {
                        let off = Zeta::from_float(t.offset)
                            .unwrap_or_else(|| panic!("offset {:?} is not a ring element", t.offset));
                        Zeta::power(u32::from(theta[t.from].unwrap_or(0))).mul(off)
                    })
                    .collect();
                let mut dz = Zeta::ZERO;
                for &i in &walk[walk.len() - clen..] {
                    dz = dz.add(zvec[i]);
                }
                let dconj = dz.conj();
                let along: Vec<Half> = zvec.iter().map(|z| dconj.mul(*z).re()).collect();
                let perp: Vec<Half> = zvec.iter().map(|z| dconj.mul(*z).im()).collect();
                let e2: Vec<(usize, usize)> = edges.iter().map(|t| (t.from, t.to)).collect();
                let neg = |v: &[Half]| -> Vec<Half> { v.iter().map(|h| h.neg()).collect() };
                let (amin, amin_d) = karp_exact(n, &e2, &along);
                let (amax_n, amax_d) = karp_exact(n, &e2, &neg(&along));
                let (pmin, pmin_d) = karp_exact(n, &e2, &perp);
                let (pmax_n, pmax_d) = karp_exact(n, &e2, &neg(&perp));
                let dnorm2 = dconj.mul(dz).re(); // |d|² exactly
                let fmt = |h: Half, den: i128| format!("({} + {}√3)/{}", h.p, h.q, 2 * den);
                println!(
                    "exact: reference displacement d = {:?} (|d|² = {}), components in units of |d|:",
                    dz.0,
                    fmt(dnorm2, 1)
                );
                println!(
                    "  along·|d|  min {} = {:.9}, max {} = {:.9}",
                    fmt(amin, amin_d),
                    amin.to_float() / amin_d as f64,
                    fmt(amax_n.neg(), amax_d),
                    -amax_n.to_float() / amax_d as f64
                );
                println!(
                    "  perp·|d|   min {} = {:.9}, max {} = {:.9}",
                    fmt(pmin, pmin_d),
                    pmin.to_float() / pmin_d as f64,
                    fmt(pmax_n.neg(), pmax_d),
                    -pmax_n.to_float() / pmax_d as f64
                );
                // Distinct reduced edge vectors u_e = v_e + P(from) − P(to)
                // with P from a BFS arborescence: every cycle displacement is a
                // sum of these over its non-tree edges.
                let mut pot: Vec<Option<Zeta>> = vec![None; n];
                pot[0] = Some(Zeta::ZERO);
                let mut q = std::collections::VecDeque::from([0usize]);
                let mut tree = vec![false; edges.len()];
                while let Some(s) = q.pop_front() {
                    for &i in &succ[s] {
                        let v = edges[i].to;
                        if pot[v].is_none() {
                            pot[v] = Some(pot[s].unwrap().add(zvec[i]));
                            tree[i] = true;
                            q.push_back(v);
                        }
                    }
                }
                let mut reduced: Vec<Zeta> = Vec::new();
                for (i, t) in edges.iter().enumerate() {
                    if tree[i] {
                        continue;
                    }
                    let u = zvec[i].add(pot[t.from].unwrap()).sub(pot[t.to].unwrap());
                    if !reduced.contains(&u) {
                        reduced.push(u);
                    }
                }
                let collinear = reduced.iter().all(|u| dconj.mul(*u).im() == Half::ZERO);
                println!(
                    "  {} non-tree edges carry {} distinct reduced displacement vectors; all collinear with d: {}",
                    edges.len() - tree.iter().filter(|&&t| t).count(),
                    reduced.len(),
                    collinear
                );
                for u in reduced.iter().take(12) {
                    let f = u.to_float();
                    println!(
                        "    u = {:?}  ({:.4}, {:.4}), angle from d {:.6}°",
                        u.0,
                        f[0],
                        f[1],
                        f[1].atan2(f[0]).to_degrees() - dz.to_float()[1].atan2(dz.to_float()[0]).to_degrees()
                    );
                }
                // Empirical frequencies of the reduced vectors along the
                // observed flight (the ergodic average, measured), and the
                // heading of the frequency-weighted sum.
                if !cl.observed.is_empty() {
                    // Map transition id -> index into `edges` (live transitions).
                    let live_ids: HashMap<usize, usize> = cl
                        .transitions
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| Some(t.to) != dead)
                        .enumerate()
                        .map(|(ei, (ti, _))| (ti, ei))
                        .collect();
                    let mut counts: HashMap<Zeta, usize> = HashMap::new();
                    let mut total_disp = Zeta::ZERO;
                    let mut steps = 0usize;
                    for &tid in &cl.observed {
                        let Some(&ei) = live_ids.get(&tid) else { continue };
                        steps += 1;
                        total_disp = total_disp.add(zvec[ei]);
                        if !tree[ei] {
                            let u = zvec[ei].add(pot[edges[ei].from].unwrap()).sub(pot[edges[ei].to].unwrap());
                            *counts.entry(u).or_default() += 1;
                        }
                    }
                    let tf = total_disp.to_float();
                    println!(
                        "  observed flight: {steps} transitions, net displacement ({:.3}, {:.3}) heading {:.6}°; reduced-vector counts:",
                        tf[0],
                        tf[1],
                        tf[1].atan2(tf[0]).to_degrees().rem_euclid(360.0)
                    );
                    let mut cv: Vec<(Zeta, usize)> = counts.into_iter().collect();
                    cv.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
                    for (u, c) in &cv {
                        println!("    {:?}: {c}", u.0);
                    }
                    // Generator counts when the nonzero reduced vectors are
                    // {A, B, A+B} (the hat case): A = d, B = the other
                    // generator; a walk's displacement is (#A)·A + (#B)·B.
                    let nonzero: Vec<Zeta> = reduced.iter().copied().filter(|u| *u != Zeta::ZERO).collect();
                    if nonzero.len() == 3 && nonzero.contains(&dz) {
                        let others: Vec<Zeta> = nonzero.iter().copied().filter(|u| *u != dz).collect();
                        let (x, y) = (others[0], others[1]);
                        let (bvec, sum) = if x.add(dz) == y { (x, y) } else if y.add(dz) == x { (y, x) } else { (x, y) };
                        let count = |u: Zeta| cv.iter().find(|(v, _)| *v == u).map_or(0, |(_, c)| *c);
                        let na = count(dz) + count(sum);
                        let nb = count(bvec) + count(sum);
                        println!(
                            "    generators A = {:?}, B = {:?}: A traversed {na} times, B {nb} times, B/A = {:.6} (1/φ = 0.618034)",
                            dz.0,
                            bvec.0,
                            nb as f64 / na as f64
                        );
                    }
                }
            }
        }
        "stats" => {
            // Structure of a certificate graph: class-blind quotient,
            // branching, strong connectivity, ball usage. With a second
            // closure file: overlap of class-labelled and class-blind sets.
            let text = fs::read_to_string(&args.positional[1]).expect("read closure");
            let mut cl: certify::closure::Closure = serde_json::from_str(&text).expect("parse");
            for s in cl.states.iter_mut() {
                s.sit.restore();
            }
            let n = cl.states.len();
            let dead = cl.dead;
            let live: Vec<usize> = (0..n).filter(|&s| Some(s) != dead).collect();
            // Class-blind key: zero the class bytes of every cell.
            let blind = |key: &[u8]| -> Vec<u8> {
                let mut out = key.to_vec();
                let mut p = 0;
                while p < out.len() {
                    out[p + 1] = 0;
                    out[p + 2] = 0;
                    let ne = out[p + 3] as usize;
                    p += 4 + 3 * ne;
                }
                out
            };
            let blind_keys: HashSet<Vec<u8>> =
                live.iter().map(|&s| blind(&cl.states[s].sit.key)).collect();
            // Pattern-only key: states of cells only (the "phase" of the glider).
            let pattern_key = |sit: &certify::situation::Situation| -> Vec<u8> {
                // canonical: the pattern alone re-encoded by BFS from the anchor over
                // support cells' induced graph is ambiguous; use the full class-blind
                // key restricted to the 0-collar instead — approximate by the
                // multiset of (state, nedges) plus the class-blind 1-collar? Keep it
                // simple: population and state histogram.
                let mut h = [0u32; 4];
                for c in &sit.cells {
                    if c.state != 0 {
                        h[c.state as usize % 4] += 1;
                    }
                }
                h.iter().flat_map(|x| x.to_be_bytes()).collect()
            };
            let hist: HashSet<Vec<u8>> = live.iter().map(|&s| pattern_key(&cl.states[s].sit)).collect();
            let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
            let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
            for t in &cl.transitions {
                succ[t.from].push(t.to);
                pred[t.to].push(t.from);
            }
            for v in succ.iter_mut().chain(pred.iter_mut()) {
                v.sort_unstable();
                v.dedup();
            }
            let branching: Vec<usize> = live.iter().copied().filter(|&s| succ[s].len() > 1).collect();
            let mut branch_hist: HashMap<usize, usize> = HashMap::new();
            for &s in &branching {
                *branch_hist.entry(succ[s].len()).or_default() += 1;
            }
            let sources = live.iter().filter(|&&s| pred[s].is_empty()).count();
            // Tarjan SCC over live states.
            let mut index = vec![usize::MAX; n];
            let mut low = vec![0usize; n];
            let mut on = vec![false; n];
            let mut stack: Vec<usize> = Vec::new();
            let mut sccs: Vec<Vec<usize>> = Vec::new();
            let mut counter = 0usize;
            #[allow(clippy::too_many_arguments)]
            fn tarjan(
                v: usize,
                succ: &[Vec<usize>],
                dead: Option<usize>,
                index: &mut [usize],
                low: &mut [usize],
                on: &mut [bool],
                stack: &mut Vec<usize>,
                sccs: &mut Vec<Vec<usize>>,
                counter: &mut usize,
            ) {
                index[v] = *counter;
                low[v] = *counter;
                *counter += 1;
                stack.push(v);
                on[v] = true;
                for &w in &succ[v] {
                    if Some(w) == dead {
                        continue;
                    }
                    if index[w] == usize::MAX {
                        tarjan(w, succ, dead, index, low, on, stack, sccs, counter);
                        low[v] = low[v].min(low[w]);
                    } else if on[w] {
                        low[v] = low[v].min(index[w]);
                    }
                }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        on[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(comp);
                }
            }
            for &s in &live {
                if index[s] == usize::MAX {
                    tarjan(s, &succ, dead, &mut index, &mut low, &mut on, &mut stack, &mut sccs, &mut counter);
                }
            }
            let mut scc_sizes: Vec<usize> = sccs.iter().map(Vec::len).collect();
            scc_sizes.sort_unstable_by(|a, b| b.cmp(a));
            let observed = live.iter().filter(|&&s| cl.states[s].origin.starts_with("observed")).count();
            let pops: Vec<usize> = live.iter().map(|&s| cl.states[s].population).collect();
            println!(
                "{}: k={} R={} family {} rule {}",
                args.positional[1], cl.k, cl.r, cl.family, cl.rule_note
            );
            println!(
                "states {} (observed {}, discovered {}), dead state {:?}, transitions {} over {} distinct balls",
                live.len(),
                observed,
                live.len() - observed,
                dead,
                cl.transitions.len(),
                cl.balls.len()
            );
            println!(
                "class-blind quotient: {} distinct states; state-histogram classes: {}; population {}-{}",
                blind_keys.len(),
                hist.len(),
                pops.iter().min().unwrap(),
                pops.iter().max().unwrap()
            );
            let mut bh: Vec<(usize, usize)> = branch_hist.into_iter().collect();
            bh.sort_unstable();
            println!(
                "branching states {} (successor-count histogram {:?}); states with no predecessor {}",
                branching.len(),
                bh,
                sources
            );
            println!(
                "strongly connected components: {} (sizes {:?}{})",
                sccs.len(),
                &scc_sizes[..scc_sizes.len().min(8)],
                if scc_sizes.len() > 8 { ", ..." } else { "" }
            );
            let trivial = scc_sizes.iter().filter(|&&s| s == 1).count();
            println!("  singleton components (transient states): {trivial}");
            // Radius invariance: project this closure's states to a smaller
            // collar radius and compare with a closure computed at that radius.
            if let Some(k2) = args.flags.get("project") {
                let k2: u8 = k2.parse().expect("project radius");
                let other = args.positional.get(2).expect("closure at the smaller radius");
                let text = fs::read_to_string(other).expect("read second closure");
                let o: certify::closure::Closure = serde_json::from_str(&text).expect("parse");
                assert!(args.switches.contains("both") || o.k == k2, "second closure is not at radius {k2}");
                let projected: HashSet<Vec<u8>> = live.iter().map(|&s| cl.states[s].sit.project(k2)).collect();
                // --both: project the second closure too (compare two objects
                // at a common smaller radius, e.g. k=0 for pattern sets).
                let both = args.switches.contains("both");
                let theirs: HashSet<Vec<u8>> = o
                    .states
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| Some(*i) != o.dead)
                    .map(|(_, s)| {
                        if both {
                            let mut sit = s.sit.clone();
                            sit.restore();
                            sit.project(k2)
                        } else {
                            s.sit.key.clone()
                        }
                    })
                    .collect();
                println!(
                    "projection to k={k2}: {} states project to {} distinct; closure at k={k2} has {}; shared {}, only projected {}, only there {} — {}",
                    live.len(),
                    projected.len(),
                    theirs.len(),
                    projected.intersection(&theirs).count(),
                    projected.difference(&theirs).count(),
                    theirs.difference(&projected).count(),
                    if projected == theirs { "IDENTICAL" } else { "DIFFER" }
                );
                return;
            }
            if let Some(other) = args.positional.get(2) {
                let text = fs::read_to_string(other).expect("read second closure");
                let o: certify::closure::Closure = serde_json::from_str(&text).expect("parse");
                let mine: HashSet<&[u8]> = live.iter().map(|&s| cl.states[s].sit.key.as_slice()).collect();
                let theirs: HashSet<&[u8]> = o
                    .states
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| Some(*i) != o.dead)
                    .map(|(_, s)| s.sit.key.as_slice())
                    .collect();
                let theirs_blind: HashSet<Vec<u8>> = theirs.iter().map(|k| blind(k)).collect();
                println!(
                    "vs {other}: class-labelled states {} / {}, shared {}; class-blind {} / {}, shared {}",
                    mine.len(),
                    theirs.len(),
                    mine.intersection(&theirs).count(),
                    blind_keys.len(),
                    theirs_blind.len(),
                    blind_keys.intersection(&theirs_blind).count()
                );
            }
        }
        _ => {
            eprintln!("usage: certify types|dfa|dfa-dump|atlas|debug-key|check-graph|validate|closure|census|ball-key|relabel|param|situation-dump|trace|convoy|sccs|cocycle|stats ... (see source header)");
            std::process::exit(2);
        }
    }
}
