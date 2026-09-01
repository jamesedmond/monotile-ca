//! Empirically answers design-brief open question #1: the distribution of
//! neighbour counts on all supported tilings, which determines the
//! dimensions of semi-totalistic rule tables.
//!
//! Usage: neighbour_counts [radius] [edge|vertex]
//!
//! In vertex mode also reports the *fan margin* — the largest edge-BFS
//! distance step across any single vertex-adjacency edge, i.e. how many
//! outer rings of a patch have potentially-incomplete neighbourhoods
//! (pass `boundary_distance = radius − margin` to classification).
//!
//! Only cells with distance ≤ radius − margin are counted — every
//! neighbour of such a cell is present in the patch, so its graph degree
//! is its true neighbour count in the infinite tiling.

use std::collections::BTreeMap;

use tiling_core::{Neighbourhood, Tiling, TilingFamily};

fn main() {
    let radius: u32 = std::env::args()
        .nth(1)
        .map(|a| a.parse().expect("radius must be a number"))
        .unwrap_or(16);
    let nb = match std::env::args().nth(2).as_deref() {
        None | Some("edge") => Neighbourhood::Edge,
        Some("vertex") => Neighbourhood::Vertex,
        Some(other) => panic!("unknown neighbourhood '{other}'"),
    };
    for family in [
        TilingFamily::Hat,
        TilingFamily::Spectre,
        TilingFamily::PenroseP2,
        TilingFamily::PenroseP3,
    ] {
        let tiling = Tiling::new(family);
        let patch =
            tiling.generate_patch(&tiling.default_root(), radius, nb).unwrap();

        // Fan margin: max distance step across any graph edge (1 for
        // edge adjacency by BFS construction).
        let (graph, cells) = (&patch.graph, &patch.cells);
        let margin = (0..graph.cells())
            .flat_map(|c| {
                let dc = cells[c as usize].distance;
                graph
                    .neighbours(c)
                    .iter()
                    .map(move |&n| dc.abs_diff(cells[n as usize].distance))
            })
            .max()
            .unwrap_or(1);

        // degree → count, overall and per base tile name
        let mut overall: BTreeMap<u32, u32> = BTreeMap::new();
        let mut by_base: BTreeMap<&str, BTreeMap<u32, u32>> = BTreeMap::new();
        let mut interior = 0u32;
        for c in 0..patch.graph.cells() {
            let cell = &patch.cells[c as usize];
            // Seed-artifact cells (and their immediate neighbours) carry
            // cone-glued adjacency, not that of a real tiling.
            let near_artifact = patch.seed_artifact_cells.contains(&c)
                || patch
                    .graph
                    .neighbours(c)
                    .iter()
                    .any(|n| patch.seed_artifact_cells.contains(n));
            if cell.distance + margin > radius || near_artifact {
                continue;
            }
            interior += 1;
            let degree = patch.graph.degree(c);
            *overall.entry(degree).or_default() += 1;
            let base = patch.classes[cell.class as usize].base.as_str();
            *by_base.entry(base).or_default().entry(degree).or_default() += 1;
        }

        println!(
            "{family:?} ({nb:?}): {} cells, {} interior (radius {radius}, fan margin {margin})",
            patch.graph.cells(),
            interior
        );
        println!("  neighbour-count distribution (interior cells):");
        for (degree, count) in &overall {
            println!(
                "    {degree:2}: {count:5}  ({:5.1}%)",
                100.0 * f64::from(*count) / f64::from(interior)
            );
        }
        for (base, hist) in &by_base {
            let counts: Vec<String> = hist
                .iter()
                .map(|(d, n)| format!("{d}×{n}"))
                .collect();
            println!("  {base}: {}", counts.join(", "));
        }
        println!();
    }
}
