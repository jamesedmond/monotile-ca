//! Parametric placement (FINDINGS 10.11.19): the hat family Tile(a, b)
//! shares one combinatorial hierarchy, and every tile position is
//! a·U + b·V with U, V ∈ ℤ[ζ12] — the sums of the unit-length and
//! √3-length edge directions along any path from the root. The library
//! places tiles for the hat proper (a, b) = (1, √3); here the a-part and
//! b-part are propagated separately along shared edges, so that the same
//! tiling can be evaluated in any member of the family, in particular in
//! the exactly self-similar (golden) realisation.
//!
//! Conventions: edge e of a tile runs from vertex e to vertex e+1 in the
//! prototile's default pose; placements are rotations by multiples of
//! 30° (index r) followed by a translation; a shared edge is traversed in
//! opposite directions by the two tiles.

use std::collections::VecDeque;

use crate::ball::EdgeCtx;
use crate::ctx::Ctx;
use crate::exact::Zeta;
use crate::patchctx::PatchCtx;

/// A prototile edge: direction index m (the edge is a·ζ^m or b·ζ^m with
/// ζ = e^{iπ/6}) and whether it is a b-edge (length √3).
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub dir: u32,
    pub long: bool,
}

pub struct Proto {
    pub edges: Vec<Edge>,
    /// Parametric vertex positions relative to vertex 0: (U, V).
    pub verts: Vec<(Zeta, Zeta)>,
    /// Vertex 0 of the default placement (exact).
    pub v0: Zeta,
}

fn direction_index(v: [f64; 2]) -> u32 {
    let ang = v[1].atan2(v[0]).to_degrees().rem_euclid(360.0);
    let m = (ang / 30.0).round() as i64;
    assert!((ang - 30.0 * m as f64).abs() < 1e-6, "edge direction {ang} is not a multiple of 30°");
    (m.rem_euclid(12)) as u32
}

pub fn proto(ctx: &Ctx, t: substitution_tiling_transducers::common::TileIndex) -> Proto {
    let p = ctx.od.place_default(t);
    let pts: Vec<[f64; 2]> = ctx.od.tile_vertices(&p).map(|v| v.to_float().unwrap()).collect();
    let n = pts.len();
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let e = [b[0] - a[0], b[1] - a[1]];
        let l2 = e[0] * e[0] + e[1] * e[1];
        let long = if (l2 - 1.0).abs() < 1e-6 {
            false
        } else if (l2 - 3.0).abs() < 1e-6 {
            true
        } else {
            panic!("edge length² {l2} is neither 1 nor 3");
        };
        edges.push(Edge { dir: direction_index(e), long });
    }
    let mut verts = Vec::with_capacity(n);
    let (mut u, mut v) = (Zeta::ZERO, Zeta::ZERO);
    for e in &edges {
        verts.push((u, v));
        let z = Zeta::power(e.dir);
        if e.long {
            v = v.add(z);
        } else {
            u = u.add(z);
        }
    }
    let v0 = Zeta::from_float(pts[0]).expect("vertex 0 exact");
    Proto { edges, verts, v0 }
}

/// Evaluate a parametric vector at edge lengths (a, b): a·U + b·V.
pub fn eval(u: Zeta, v: Zeta, a: f64, b: f64) -> [f64; 2] {
    let fu = u.to_float();
    let fv = v.to_float();
    [a * fu[0] + b * fv[0], a * fu[1] + b * fv[1]]
}

pub struct Placed {
    pub rot: u32,
    pub u: Zeta,
    pub v: Zeta,
}

/// Parametric placements of every cell of `pctx`, BFS from cell 0 (which
/// gets rotation 0 and translation (0, 0): its vertex 0 sits at the
/// prototile's default vertex 0). Returns per cell (rot, U, V) meaning
/// vertex i of the cell is at v0 + ζ^rot·(U_i, V_i) + (U, V) in the family
/// coordinates; the control checks the (1, √3) evaluation against the
/// library's polygons for every cell.
pub fn place_all(ctx: &Ctx, pctx: &PatchCtx) -> (Vec<Placed>, f64) {
    let protos: std::collections::HashMap<_, Proto> = ctx.base_types.iter().map(|&t| (t, proto(ctx, t))).collect();
    let n = pctx.cells() as usize;
    let mut out: Vec<Option<Placed>> = (0..n).map(|_| None).collect();
    out[0] = Some(Placed { rot: 0, u: Zeta::ZERO, v: Zeta::ZERO });
    let mut queue = VecDeque::from([0u32]);
    let mut worst = 0.0f64;
    let s3 = 3f64.sqrt();
    let root0 = pctx.polygon(0)[0];
    while let Some(c) = queue.pop_front() {
        let (rc, uc, vc) = {
            let p = out[c as usize].as_ref().unwrap();
            (p.rot, p.u, p.v)
        };
        let pt = &protos[&pctx.base_type(c)];
        // control: every vertex of c at (1, √3), relative to the root's
        // vertex 0, against the library polygon
        let poly = pctx.polygon(c);
        let tr = eval(uc, vc, 1.0, s3);
        for (i, &(ui, vi)) in pt.verts.iter().enumerate() {
            let rel = eval(Zeta::power(rc).mul(ui), Zeta::power(rc).mul(vi), 1.0, s3);
            let mine = [tr[0] + rel[0] + root0[0], tr[1] + rel[1] + root0[1]];
            worst = worst.max((mine[0] - poly[i][0]).hypot(mine[1] - poly[i][1]));
        }
        for e in 0..pctx.nedges(c) {
            let Some((d, back)) = pctx.edge(c, e) else { continue };
            if out[d as usize].is_some() {
                continue;
            }
            let pd = &protos[&pctx.base_type(d)];
            let ea = pt.edges[e];
            let eb = pd.edges[back as usize];
            assert_eq!(ea.long, eb.long, "shared edge types differ");
            // ζ^{rd}·ζ^{eb.dir} = −ζ^{rc}·ζ^{ea.dir}
            let rd = (rc + ea.dir + 6 + 12 - eb.dir) % 12;
            // A's vertex e+1 = B's vertex back:  T_A + R_A pv_A[e+1] = T_B + R_B pv_B[back]
            let na = pt.verts.len();
            let (ua1, va1) = pt.verts[(e + 1) % na];
            let (ub, vb) = pd.verts[back as usize];
            let ra = Zeta::power(rc);
            let rb = Zeta::power(rd);
            let ud = uc.add(ra.mul(ua1)).sub(rb.mul(ub));
            let vd = vc.add(ra.mul(va1)).sub(rb.mul(vb));
            out[d as usize] = Some(Placed { rot: rd, u: ud, v: vd });
            queue.push_back(d);
        }
    }
    (out.into_iter().map(|p| p.expect("all cells placed")).collect(), worst)
}
