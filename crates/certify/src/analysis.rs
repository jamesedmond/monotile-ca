//! Certificate-graph analysis: strongly connected components (the
//! objects), and the cocycle statistics (orientation lock, exact and
//! float cycle-mean bounds, heading window) on any subgraph.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::ball::rotate;
use crate::closure::{Closure, Transition};

/// Tarjan's SCCs over the live states of a closure (the death state, if
/// any, is excluded). Components are returned in Tarjan order.
pub fn sccs(cl: &Closure) -> Vec<Vec<usize>> {
    let n = cl.states.len();
    let dead = cl.dead;
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for t in &cl.transitions {
        if Some(t.to) != dead && Some(t.from) != dead {
            succ[t.from].push(t.to);
        }
    }
    struct St<'a> {
        succ: &'a [Vec<usize>],
        index: Vec<usize>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        out: Vec<Vec<usize>>,
        counter: usize,
    }
    fn visit(s: &mut St, v: usize) {
        s.index[v] = s.counter;
        s.low[v] = s.counter;
        s.counter += 1;
        s.stack.push(v);
        s.on[v] = true;
        for i in 0..s.succ[v].len() {
            let w = s.succ[v][i];
            if s.index[w] == usize::MAX {
                visit(s, w);
                s.low[v] = s.low[v].min(s.low[w]);
            } else if s.on[w] {
                s.low[v] = s.low[v].min(s.index[w]);
            }
        }
        if s.low[v] == s.index[v] {
            let mut comp = Vec::new();
            while let Some(w) = s.stack.pop() {
                s.on[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            comp.sort_unstable();
            s.out.push(comp);
        }
    }
    let mut st = St {
        succ: &succ,
        index: vec![usize::MAX; n],
        low: vec![0; n],
        on: vec![false; n],
        stack: Vec::new(),
        out: Vec::new(),
        counter: 0,
    };
    for v in 0..n {
        if Some(v) != dead && st.index[v] == usize::MAX {
            visit(&mut st, v);
        }
    }
    st.out
}

/// 128-bit signature of a set of keys (order-independent).
pub fn signature(keys: &[Vec<u8>]) -> String {
    let mut sorted: Vec<&Vec<u8>> = keys.iter().collect();
    sorted.sort();
    let mut h1 = DefaultHasher::new();
    let mut h2 = DefaultHasher::new();
    0x5eedu16.hash(&mut h2);
    for k in sorted {
        k.hash(&mut h1);
        k.hash(&mut h2);
    }
    format!("{:016x}{:016x}", h1.finish(), h2.finish())
}

/// Class-blind key: class bytes zeroed in every cell.
pub fn blind(key: &[u8]) -> Vec<u8> {
    let mut out = key.to_vec();
    let mut p = 0;
    while p < out.len() {
        out[p + 1] = 0;
        out[p + 2] = 0;
        let ne = out[p + 3] as usize;
        p += 4 + 3 * ne;
    }
    out
}

#[derive(Debug, Clone)]
pub struct Cocycle {
    pub states: usize,
    pub edges: usize,
    pub rotation_violations: usize,
    pub unreached: usize,
    /// None if the reference cycle has zero displacement (looper).
    pub heading: Option<f64>,
    pub cycle_len: usize,
    pub cycle_disp: f64,
    pub along: (f64, f64),
    pub perp: (f64, f64),
    pub window: (f64, f64),
}

fn karp(n: usize, edges: &[(usize, usize)], w: &[f64]) -> f64 {
    let inf = f64::INFINITY;
    let mut dist = vec![vec![inf; n]; n + 1];
    dist[0][0] = 0.0;
    for k in 1..=n {
        for (i, &(u, v)) in edges.iter().enumerate() {
            let cand = dist[k - 1][u] + w[i];
            if cand < dist[k][v] {
                dist[k][v] = cand;
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
}

/// Cocycle statistics on the subgraph induced by `states` (any subset,
/// e.g. one SCC): edges with both ends inside.
pub fn cocycle_on(cl: &Closure, states: &[usize]) -> Cocycle {
    let local: HashMap<usize, usize> = states.iter().enumerate().map(|(i, &s)| (s, i)).collect();
    let n = states.len();
    let ts: Vec<&Transition> = cl
        .transitions
        .iter()
        .filter(|t| local.contains_key(&t.from) && local.contains_key(&t.to))
        .collect();
    let edges: Vec<(usize, usize)> = ts.iter().map(|t| (local[&t.from], local[&t.to])).collect();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, &(u, _)) in edges.iter().enumerate() {
        succ[u].push(i);
    }
    let mut theta: Vec<Option<u8>> = vec![None; n];
    theta[0] = Some(0);
    let mut q = VecDeque::from([0usize]);
    let mut violations = 0;
    while let Some(s) = q.pop_front() {
        for &i in &succ[s] {
            let v = edges[i].1;
            let th = (theta[s].unwrap() + ts[i].rot) % 12;
            match theta[v] {
                Some(x) if x != th => violations += 1,
                Some(_) => {}
                None => {
                    theta[v] = Some(th);
                    q.push_back(v);
                }
            }
        }
    }
    let unreached = theta.iter().filter(|t| t.is_none()).count();
    let vecs: Vec<[f64; 2]> = edges
        .iter()
        .zip(&ts)
        .map(|(&(u, _), t)| rotate(t.offset, theta[u].unwrap_or(0)))
        .collect();
    let mut base = Cocycle {
        states: n,
        edges: edges.len(),
        rotation_violations: violations,
        unreached,
        heading: None,
        cycle_len: 0,
        cycle_disp: 0.0,
        along: (0.0, 0.0),
        perp: (0.0, 0.0),
        window: (0.0, 0.0),
    };
    if edges.is_empty() || succ[0].is_empty() {
        return base;
    }
    let mut seen = vec![None; n];
    let mut walk: Vec<usize> = Vec::new();
    let mut s = 0usize;
    let cyc = loop {
        if let Some(pos) = seen[s] {
            break walk[pos..].to_vec();
        }
        seen[s] = Some(walk.len());
        let Some(&i) = succ[s].first() else { return base };
        walk.push(i);
        s = edges[i].1;
    };
    let mut sum = [0.0f64; 2];
    for &i in &cyc {
        sum[0] += vecs[i][0];
        sum[1] += vecs[i][1];
    }
    let norm = sum[0].hypot(sum[1]);
    base.cycle_len = cyc.len();
    base.cycle_disp = norm;
    if norm < 1e-9 {
        return base;
    }
    let d = [sum[0] / norm, sum[1] / norm];
    let dp = [-d[1], d[0]];
    let par: Vec<f64> = vecs.iter().map(|v| v[0] * d[0] + v[1] * d[1]).collect();
    let perp: Vec<f64> = vecs.iter().map(|v| v[0] * dp[0] + v[1] * dp[1]).collect();
    let neg = |v: &[f64]| -> Vec<f64> { v.iter().map(|x| -x).collect() };
    let a_min = karp(n, &edges, &par);
    let a_max = -karp(n, &edges, &neg(&par));
    let p_min = karp(n, &edges, &perp);
    let p_max = -karp(n, &edges, &neg(&perp));
    let heading = d[1].atan2(d[0]).to_degrees().rem_euclid(360.0);
    let lo = p_min.min(0.0).atan2(a_min).to_degrees();
    let hi = p_max.max(0.0).atan2(a_min).to_degrees();
    base.heading = Some(heading);
    base.along = (a_min, a_max);
    base.perp = (p_min, p_max);
    base.window = (heading + lo, heading + hi);
    base
}

/// One object of the census: an SCC, identified by its key set.
pub struct Object {
    pub signature: String,
    pub keys: Vec<Vec<u8>>,
    pub blind_states: usize,
    pub population: (usize, usize),
    pub terminal: bool,
    /// Signatures of the SCCs this one has transitions into.
    pub successors: Vec<String>,
    pub cocycle: Cocycle,
    /// (file, bucket-level component count if known) occurrences.
    pub files: Vec<String>,
}

/// Decompose a closure into objects; `project` re-keys states at a
/// smaller collar radius first (for comparing runs at different k).
pub fn objects_of(cl: &mut Closure, file: &str, project: Option<u8>) -> Vec<Object> {
    for s in cl.states.iter_mut() {
        s.sit.restore();
    }
    let comps = sccs(cl);
    let comp_of: HashMap<usize, usize> = comps
        .iter()
        .enumerate()
        .flat_map(|(ci, c)| c.iter().map(move |&s| (s, ci)))
        .collect();
    let key_of = |s: usize| -> Vec<u8> {
        match project {
            Some(k) => cl.states[s].sit.project(k),
            None => cl.states[s].sit.key.clone(),
        }
    };
    let sig_of: Vec<String> = comps
        .iter()
        .map(|c| {
            let keys: Vec<Vec<u8>> = c.iter().map(|&s| key_of(s)).collect::<HashSet<_>>().into_iter().collect();
            signature(&keys)
        })
        .collect();
    comps
        .iter()
        .enumerate()
        .map(|(ci, c)| {
            let keyset: HashSet<Vec<u8>> = c.iter().map(|&s| key_of(s)).collect();
            let keys: Vec<Vec<u8>> = keyset.into_iter().collect();
            let blind_states = keys.iter().map(|k| blind(k)).collect::<HashSet<_>>().len();
            let pops: Vec<usize> = c.iter().map(|&s| cl.states[s].population).collect();
            let mut succs: Vec<String> = cl
                .transitions
                .iter()
                .filter(|t| comp_of.get(&t.from) == Some(&ci) && Some(t.to) != cl.dead)
                .filter_map(|t| comp_of.get(&t.to).copied())
                .filter(|&cj| cj != ci)
                .map(|cj| sig_of[cj].clone())
                .collect();
            succs.sort();
            succs.dedup();
            Object {
                signature: sig_of[ci].clone(),
                blind_states,
                population: (*pops.iter().min().unwrap(), *pops.iter().max().unwrap()),
                terminal: succs.is_empty(),
                successors: succs,
                cocycle: cocycle_on(cl, c),
                files: vec![file.to_string()],
                keys,
            }
        })
        .collect()
}
