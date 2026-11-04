//! Data for the extensions figure of the certificate paper: a certificate situation embedded in chosen legal
//! balls of the atlas, stepped once by the rule, with the successor identified in the certificate.
//! Writes JSON consumed by `crates/certify/figures/fig_situation.py`: per ball, every cell within
//! `dump_radius` rings of the centre with its polygon, class, ring distance, state before and after
//! the step, and whether it lies in the situation's support or collar.
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;

use ca_engine::CaRule;
use substitution_tiling_transducers::address::{EPTileAddress, TileAddrSymbol};
use substitution_tiling_transducers::common::{DisplayViaSystem, System, TileIndex};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

use crate::ball::EdgeCtx;
use crate::closure::Closure;
use crate::ctx::Ctx;
use crate::patchctx::canonical_patch;
use crate::situation::encode;
use crate::walk::Walker;

pub fn situation_dump(
    cert: &Closure,
    record: &ResultRecord,
    state: usize,
    balls: &[usize],
    radius: u32,
    dump_radius: u32,
) -> Result<String, String> {
    let tiling = Tiling::new(record.family);
    let ctx = Ctx::new(&tiling);
    let sys = tiling.system();
    let cs = &sys.cs_fine;
    let base_layer = cs.lookup_layer(cs.base_layer);
    let meta_layer = {
        let mut l = cs.lookup_layer(base_layer.parent);
        for _ in 0..=cs.layers.len() {
            if l.parent == l.index {
                break;
            }
            l = cs.lookup_layer(l.parent);
        }
        l
    };
    let meta_types: Vec<TileIndex> = {
        let mut v: Vec<TileIndex> = meta_layer.subtiles.keys().copied().collect();
        v.sort_by_key(|t| cs.lookup_tile_name(*t).to_string());
        v
    };
    let parents_of = |t: TileIndex| -> Vec<(TileIndex, usize)> {
        let mut out = Vec::new();
        for &p in &meta_types {
            for (s, &c) in meta_layer.subtiles[&p].iter().enumerate() {
                if c == t {
                    out.push((p, s));
                }
            }
        }
        out
    };
    let subtiles = |t: TileIndex| meta_layer.subtiles[&t].clone();
    let display = |a: EPTileAddress| format!("{}", a.display(cs));
    let walker = Walker::new(cert, state, 1, radius);
    let k = u32::from(cert.k);
    let mut out = String::new();
    write!(
        out,
        "{{\"state\":{state},\"k\":{k},\"rho\":{},\"population\":{},\"balls\":[",
        cert.states[state].rho, cert.states[state].population
    )
    .unwrap();
    for (bi, &b) in balls.iter().enumerate() {
        let bref = cert.balls.iter().find(|r| r.index == b).ok_or_else(|| format!("ball {b} is not used by the certificate"))?;
        let mut context: Vec<TileAddrSymbol> = vec![TileAddrSymbol::Initial { base_type: TileIndex(bref.root_ty) }];
        context.extend(bref.witness.iter().map(|&(p, s)| TileAddrSymbol::Supertile { parent_type: TileIndex(p), subtile: s }));
        let pctx = canonical_patch(&ctx, &subtiles, &display, &parents_of, &context, radius)
            .ok_or_else(|| format!("ball {b}: witness patch has seed artifacts"))?;
        let map = walker.embed(&pctx, 0).ok_or_else(|| format!("ball {b}: situation does not embed at the centre"))?;
        let ncells = pctx.cells() as usize;
        let mut s0 = vec![0u8; ncells];
        for (i, c) in walker.sit.cells.iter().enumerate() {
            if c.state != 0 {
                s0[map[i] as usize] = c.state;
            }
        }
        let support: Vec<u32> = (0..pctx.cells()).filter(|&c| s0[c as usize] != 0).collect();
        // collar: cells within k edge-rings of a support
        let collar_of = |support: &[u32]| -> HashSet<u32> {
            let mut collar: HashSet<u32> = HashSet::new();
            let mut dist: HashMap<u32, u32> = HashMap::new();
            let mut q = VecDeque::new();
            for &c in support {
                dist.insert(c, 0);
                q.push_back(c);
            }
            while let Some(x) = q.pop_front() {
                let dx = dist[&x];
                collar.insert(x);
                if dx == k {
                    continue;
                }
                for e in 0..pctx.nedges(x) {
                    if let Some((y, _)) = pctx.edge(x, e)
                        && let std::collections::hash_map::Entry::Vacant(v) = dist.entry(y)
                    {
                        v.insert(dx + 1);
                        q.push_back(y);
                    }
                }
            }
            collar
        };
        let collar = collar_of(&support);
        // one step of the rule on the exact vertex adjacency
        let (strata, rule) = record.replay_setup(&pctx.patch_v);
        let mut engine = ca_engine::Engine::with_strata(pctx.patch_v.graph.clone(), strata);
        engine.load_state(&s0);
        rule.step(&mut engine);
        let s1: Vec<u8> = engine.state().to_vec();
        let support1: Vec<u32> = (0..pctx.cells()).filter(|&c| s1[c as usize] != 0).collect();
        let collar1 = collar_of(&support1);
        let state_of = |c: u32| s1[c as usize];
        let (succ_id, succ_centre) = match encode(&pctx, &support1, &state_of, cert.k) {
            Some((s, order)) => (walker.key_index.get(&s.key).map(|&i| i as i64).unwrap_or(-1), order[s.centre().0] as i64),
            None => (-1, -1),
        };
        let listed: Vec<usize> = cert.transitions.iter().filter(|t| t.from == state && t.ball == b).map(|t| t.to).collect();
        if bi > 0 {
            out.push(',');
        }
        let witness: Vec<String> = bref.witness.iter().map(|&(p, s)| format!("[{p},{s}]")).collect();
        write!(
            out,
            "{{\"ball\":{b},\"witness\":[{}],\"centre\":{},\"successor\":{succ_id},\"successor_centre\":{succ_centre},\"listed\":{listed:?},\"cells\":[",
            witness.join(","), map[walker.sit_centre]
        )
        .unwrap();
        let mut first = true;
        for c in 0..pctx.cells() {
            if pctx.distance(c) > dump_radius {
                continue;
            }
            let class = &pctx.patch_e.classes[pctx.patch_e.cells[c as usize].class as usize];
            let poly: Vec<String> = pctx.polygon(c).iter().map(|&[x, y]| format!("[{x:.3},{y:.3}]")).collect();
            if !first {
                out.push(',');
            }
            first = false;
            write!(
                out,
                "{{\"i\":{c},\"d\":{},\"cls\":\"{}.{}\",\"base\":\"{}\",\"s0\":{},\"s1\":{},\"sup\":{},\"col\":{},\"col1\":{},\"xy\":[{}]}}",
                pctx.distance(c),
                class.parent,
                class.subtile,
                class.base,
                s0[c as usize],
                s1[c as usize],
                s0[c as usize] != 0,
                collar.contains(&c),
                collar1.contains(&c),
                poly.join(",")
            )
            .unwrap();
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    Ok(out)
}
