//! Independent audit of a generated patch against its own exact geometry.
//!
//! Everything downstream (engine, search, certificates) trusts the
//! transducer's adjacency claims. Loop closure checks the edges the
//! transducer asserts; this audit also checks the ones it might omit,
//! and that the placed polygons form a genuine tiling, using only the
//! polygons:
//!
//! 1. **shapes** — every polygon is congruent (directly or by mirror) to
//!    one of a small set of representative shapes; the number of shapes
//!    must be the family's tile count, and chirality is constant within
//!    a tile class;
//! 2. **planar map** — every directed edge of an interior polygon is
//!    matched by exactly one reversed edge of another polygon (no gaps,
//!    no T-junctions, no doubled edges);
//! 3. **flatness** — the interior angles meeting at every interior vertex
//!    sum to 360° (no branching, no overlap);
//! 4. **faithfulness** — the polygons sharing an edge with an interior
//!    cell are exactly its graph neighbours in the edge patch, and the
//!    polygons sharing a vertex are exactly its neighbours in the vertex
//!    patch: the transducer neither invents nor omits an adjacency.

use std::collections::HashMap;

use crate::{Neighbourhood, Tiling};

type Key = (i64, i64);
fn key(p: [f64; 2]) -> Key {
    ((p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64)
}

/// Outcome of [`audit`]; `ok()` is the verdict.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuditReport {
    pub cells: usize,
    pub interior: usize,
    /// Distinct tile shapes up to isometry.
    pub shapes: usize,
    /// Per tile class: (shape index, +1 same-handed as the representative
    /// or −1 mirror); `None` if a class mixes handedness.
    pub class_chirality: Vec<(u8, Option<(usize, i8)>)>,
    pub unmatched_edges: usize,
    pub doubled_edges: usize,
    pub vertices_checked: usize,
    pub non_flat_vertices: usize,
    pub edge_mismatches: usize,
    pub vertex_mismatches: usize,
    pub examples: Vec<String>,
}

impl AuditReport {
    pub fn ok(&self, expected_shapes: usize) -> bool {
        self.shapes == expected_shapes
            && self.class_chirality.iter().all(|(_, c)| c.is_some())
            && self.unmatched_edges == 0
            && self.doubled_edges == 0
            && self.non_flat_vertices == 0
            && self.edge_mismatches == 0
            && self.vertex_mismatches == 0
    }
}

/// Cyclic match of `a` (optionally reversed, scaled by `sign`) against `b`.
fn cyc(a: &[f64], b: &[f64], sign: f64, rev: bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let m = a.len();
    let mut cand: Vec<f64> = a.iter().map(|x| sign * x).collect();
    if rev {
        cand.reverse();
    }
    (0..m).any(|s| (0..m).all(|i| (cand[(i + s) % m] - b[i]).abs() < 1e-7))
}

/// Audit the patch of `root` at `radius`. Generates the edge patch with
/// geometry and the vertex patch (same BFS, hence the same numbering).
pub fn audit(tiling: &Tiling, root: &str, radius: u32) -> AuditReport {
    let (patch, geom) = tiling
        .generate_patch_with_geometry(root, radius, Neighbourhood::Edge)
        .expect("edge patch");
    let vpatch = tiling
        .generate_patch(root, radius, Neighbourhood::Vertex)
        .expect("vertex patch");
    assert!(patch.seed_artifact_cells.is_empty(), "root has seed artifacts");
    let n = patch.graph.cells() as usize;
    assert_eq!(vpatch.graph.cells() as usize, n);
    let poly = |c: usize| &geom.xy[geom.offsets[c] as usize..geom.offsets[c + 1] as usize];
    let dist = |c: usize| patch.cells[c].distance;
    let interior = |c: usize| dist(c) + 3 <= radius;
    let mut rep = AuditReport {
        cells: n,
        interior: (0..n).filter(|&c| interior(c)).count(),
        ..Default::default()
    };

    // Counter-clockwise listing of a polygon, its orientation sign, and
    // its shape (edge lengths and signed turns).
    let orient: Vec<f64> = (0..n)
        .map(|c| {
            let p = poly(c);
            let area2: f64 = (0..p.len())
                .map(|i| {
                    let (a, b) = (p[i], p[(i + 1) % p.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum();
            if area2 > 0.0 { 1.0 } else { -1.0 }
        })
        .collect();
    let shape = |c: usize| -> (Vec<f64>, Vec<f64>) {
        let mut p: Vec<[f64; 2]> = poly(c).to_vec();
        if orient[c] < 0.0 {
            p.reverse();
        }
        let m = p.len();
        let mut lens = Vec::with_capacity(m);
        let mut turns = Vec::with_capacity(m);
        for i in 0..m {
            let (a, b, d) = (p[i], p[(i + 1) % m], p[(i + 2) % m]);
            lens.push((b[0] - a[0]).hypot(b[1] - a[1]));
            let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
            let (vx, vy) = (d[0] - b[0], d[1] - b[1]);
            turns.push((ux * vy - uy * vx).atan2(ux * vx + uy * vy));
        }
        (lens, turns)
    };
    // 1. shapes. Same-handed: sequences match forward. Mirror, both
    // listed counter-clockwise: reflecting negates the turns and
    // re-listing counter-clockwise reverses and negates again, so the
    // turns match reversed with the same sign.
    let mut reps: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
    let mut class_hand: HashMap<u8, Option<(usize, i8)>> = HashMap::new();
    for c in 0..n {
        let (l, t) = shape(c);
        let mut found = None;
        for (k, (l0, t0)) in reps.iter().enumerate() {
            if cyc(&l, l0, 1.0, false) && cyc(&t, t0, 1.0, false) {
                found = Some((k, 1i8));
                break;
            }
            if cyc(&l, l0, 1.0, true) && cyc(&t, t0, 1.0, true) {
                found = Some((k, -1i8));
                break;
            }
        }
        let found = found.unwrap_or_else(|| {
            reps.push((l, t));
            (reps.len() - 1, 1)
        });
        let cls = patch.cells[c].class as u8;
        match class_hand.get(&cls) {
            None => {
                class_hand.insert(cls, Some(found));
            }
            Some(Some(prev)) if *prev != found => {
                class_hand.insert(cls, None);
            }
            _ => {}
        }
    }
    rep.shapes = reps.len();
    rep.class_chirality = class_hand.into_iter().collect();
    rep.class_chirality.sort();

    // 2. planar map.
    let mut directed: HashMap<(Key, Key), Vec<usize>> = HashMap::new();
    for c in 0..n {
        let p = poly(c);
        for i in 0..p.len() {
            directed
                .entry((key(p[i]), key(p[(i + 1) % p.len()])))
                .or_default()
                .push(c);
        }
    }
    let mut edge_nb: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (&(a, b), cells) in &directed {
        if cells.len() > 1 && cells.iter().any(|&c| interior(c)) {
            rep.doubled_edges += 1;
        }
        for &c in cells {
            if !interior(c) {
                continue;
            }
            match directed.get(&(b, a)) {
                Some(rev) if rev.len() == 1 && rev[0] != c => edge_nb[c].push(rev[0] as u32),
                Some(rev) if rev.len() == 1 => rep.doubled_edges += 1,
                _ => rep.unmatched_edges += 1,
            }
        }
    }

    // 3. flatness.
    let mut angle_at: HashMap<Key, (f64, bool)> = HashMap::new();
    let mut vertex_cells: HashMap<Key, Vec<u32>> = HashMap::new();
    for (c, &o) in orient.iter().enumerate() {
        let p = poly(c);
        let m = p.len();
        for i in 0..m {
            let (a, b, d) = (p[(i + m - 1) % m], p[i], p[(i + 1) % m]);
            let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
            let (vx, vy) = (d[0] - b[0], d[1] - b[1]);
            let turn = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
            let ang = std::f64::consts::PI - o * turn;
            let e = angle_at.entry(key(b)).or_insert((0.0, false));
            e.0 += ang;
            e.1 |= dist(c) + 4 <= radius;
            let vc = vertex_cells.entry(key(b)).or_default();
            if vc.last() != Some(&(c as u32)) {
                vc.push(c as u32);
            }
        }
    }
    for (sum, deep) in angle_at.values() {
        if *deep {
            rep.vertices_checked += 1;
            if (sum - std::f64::consts::TAU).abs() > 1e-6 {
                rep.non_flat_vertices += 1;
            }
        }
    }

    // 4. faithfulness.
    for (c, nb) in edge_nb.iter().enumerate() {
        if !interior(c) {
            continue;
        }
        let mut g = nb.clone();
        g.sort();
        g.dedup();
        let mut t: Vec<u32> = patch.graph.neighbours(c as u32).to_vec();
        t.sort();
        if g != t {
            rep.edge_mismatches += 1;
            if rep.examples.len() < 5 {
                rep.examples.push(format!("cell {c}: geometric edge-neighbours {g:?} vs graph {t:?}"));
            }
        }
        let mut gv: Vec<u32> = poly(c)
            .iter()
            .flat_map(|&v| vertex_cells[&key(v)].iter().copied())
            .filter(|&x| x != c as u32)
            .collect();
        gv.sort();
        gv.dedup();
        let mut tv: Vec<u32> = vpatch.graph.neighbours(c as u32).to_vec();
        tv.sort();
        if gv != tv {
            rep.vertex_mismatches += 1;
            if rep.examples.len() < 5 {
                rep.examples.push(format!("cell {c}: geometric vertex-neighbours {gv:?} vs graph {tv:?}"));
            }
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TilingFamily;

    #[test]
    fn patches_are_tilings_and_graphs_are_faithful() {
        // Expected distinct shapes: one monotile (two chiralities for the
        // hat), two Penrose tiles.
        for (family, shapes) in [
            (TilingFamily::Hat, 1),
            (TilingFamily::Spectre, 1),
            (TilingFamily::PenroseP2, 2),
            (TilingFamily::PenroseP3, 2),
        ] {
            let tiling = Tiling::new(family);
            let rep = audit(&tiling, &tiling.default_root(), 8);
            assert!(rep.ok(shapes), "{family:?}: {rep:?}");
            assert!(rep.interior > 50 && rep.vertices_checked > 50, "{family:?}: too small");
        }
    }

    #[test]
    fn hat_classes_have_fixed_handedness_and_both_occur() {
        let tiling = Tiling::new(TilingFamily::Hat);
        let rep = audit(&tiling, &tiling.default_root(), 8);
        let hands: std::collections::HashSet<i8> =
            rep.class_chirality.iter().map(|(_, c)| c.unwrap().1).collect();
        assert_eq!(hands.len(), 2, "hat tilings carry both chiralities: {rep:?}");
        let spectre = audit(&Tiling::new(TilingFamily::Spectre), &Tiling::new(TilingFamily::Spectre).default_root(), 8);
        let hands: std::collections::HashSet<i8> =
            spectre.class_chirality.iter().map(|(_, c)| c.unwrap().1).collect();
        assert_eq!(hands.len(), 1, "spectre tilings are chiral: {spectre:?}");
    }
}
