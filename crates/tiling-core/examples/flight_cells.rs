//! Replay a record and write the live cells of every generation with their
//! addresses and states, for the essay viewer that redraws a flight in any
//! member of the hat family (essay 2 notes, entry 7).
//!
//! Usage: flight_cells <record.jsonl> [index] [radius] [gens] [out.tsv]
//! Output: tab-separated `gen cell state address` for every live cell.

use std::fmt::Write as _;

use ca_engine::{CaRule, Engine};
use tiling_core::Tiling;
use tiling_core::results::ResultRecord;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("record file");
    let index: usize = args.next().map_or(0, |a| a.parse().expect("index"));
    let radius: u32 = args.next().map_or(512, |a| a.parse().expect("radius"));
    let gens: u64 = args.next().map_or(1024, |a| a.parse().expect("gens"));
    let out = args.next().unwrap_or_else(|| "flight-cells.tsv".into());
    let text = std::fs::read_to_string(&path).expect("read record file");
    let line = text.lines().filter(|l| !l.trim().is_empty()).nth(index).expect("record index");
    let record: ResultRecord = serde_json::from_str(line).expect("parse record");
    let tiling = Tiling::new(record.family);
    let patch = tiling.generate_patch(&record.root, radius, record.neighbourhood).unwrap();
    assert!(patch.seed_artifact_cells.is_empty());
    let (strata, rule) = record.replay_setup(&patch);
    let mut engine = Engine::with_strata(patch.graph.clone(), strata);
    engine.load_state(&record.initial_state(patch.graph.cells()).unwrap());
    let mut s = String::from("gen\tcell\tstate\taddress\n");
    let mut maxdist = 0u32;
    for g in 0..=gens {
        for (c, &st) in engine.state().iter().enumerate() {
            if st != 0 {
                maxdist = maxdist.max(patch.cells[c].distance);
                writeln!(s, "{g}\t{c}\t{st}\t{}", patch.cells[c].address).unwrap();
            }
        }
        if g < gens {
            rule.step(&mut engine);
        }
    }
    std::fs::write(&out, s).expect("write");
    eprintln!("wrote {out}: {} cells, {gens} generations, farthest live cell at {maxdist} rings (patch radius {radius})", patch.graph.cells());
}
