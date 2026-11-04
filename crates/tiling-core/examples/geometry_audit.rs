//! Independent audit of a generated patch against its own exact geometry
//! (see `tiling_core::audit`): shapes, planar map, flatness, and the
//! transducer's adjacency claims versus the polygons, in both
//! neighbourhoods. FINDINGS 10.11.8.
//!
//! Usage: geometry_audit <hat|spectre|penrosep2|penrosep3> <root|default> [radius]

use tiling_core::audit::audit;
use tiling_core::{Tiling, TilingFamily};

fn main() {
    let mut args = std::env::args().skip(1);
    let (family, shapes) = match args.next().expect("family").as_str() {
        "hat" => (TilingFamily::Hat, 1),
        "spectre" => (TilingFamily::Spectre, 1),
        "penrosep2" => (TilingFamily::PenroseP2, 2),
        "penrosep3" => (TilingFamily::PenroseP3, 2),
        other => panic!("unknown family {other}"),
    };
    let tiling = Tiling::new(family);
    let root = match args.next().as_deref() {
        None | Some("default") => tiling.default_root(),
        Some(r) => r.to_string(),
    };
    let radius: u32 = args.next().map_or(128, |a| a.parse().expect("radius"));
    let r = audit(&tiling, &root, radius);
    println!("{family:?} root {root} radius {radius}: {} cells, {} interior", r.cells, r.interior);
    println!("  shapes: {} distinct up to isometry (expected {shapes})", r.shapes);
    for (cls, c) in &r.class_chirality {
        match c {
            Some((k, h)) => println!("    class {cls}: shape {k}, {}", if *h > 0 { "same-handed" } else { "mirror" }),
            None => println!("    class {cls}: MIXED handedness"),
        }
    }
    println!("  planar map: {} unmatched (gap/T-junction), {} doubled (overlap)", r.unmatched_edges, r.doubled_edges);
    println!("  flatness: {} interior vertices, {} with angle sum != 360°", r.vertices_checked, r.non_flat_vertices);
    println!("  faithfulness: {} edge-neighbour mismatches, {} vertex-neighbour mismatches", r.edge_mismatches, r.vertex_mismatches);
    for e in &r.examples {
        println!("    {e}");
    }
    println!("verdict: {}", if r.ok(shapes) { "PASS" } else { "FAIL" });
}
