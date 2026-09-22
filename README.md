# monotile-ca — cellular automata on the hat and spectre tilings

The toolchain, search drivers, and complete result set behind the paper
**_Gliders on Aperiodic Monotilings: Cellular Automata on the Hat and
Spectre_** (James Edmond, 2026) and its companion interactive essay:

- **Essay** (every figure live, every record replayable in the browser):
  https://offlattice.org/monotile/essay/
- **Paper**: [arXiv:2609.22579](https://arxiv.org/abs/2609.22579) (nlin.CG), "Gliders on Aperiodic Monotilings: Cellular Automata on the Hat and Spectre"
- **Archived release**: v1.0.0, [doi:10.5281/zenodo.22836248](https://doi.org/10.5281/zenodo.22836248) (all versions: [doi:10.5281/zenodo.22836247](https://doi.org/10.5281/zenodo.22836247))

The hat and spectre monotiles (discovered 2023) tile the plane only
aperiodically. This project ran the first cellular-automaton searches on
them and found verified gliders on both — plus the search methodology
(structural controls, cross-radius verification, anti-gaming filters)
that makes such claims trustworthy on a board with no translational
symmetry.

## What is here

```
crates/tiling-core   patch generation: hat/spectre/Penrose patches as CSR
                     adjacency graphs from Tatham's substitution-tiling
                     transducers; exact-geometry vertex adjacency; the
                     sliding-window flight tracker; verification examples
crates/ca-engine     the automaton: pure no_std graph CA (Generations and
                     priority-table rules, uniform or class-stratified),
                     identical semantics native and in WebAssembly
crates/search        headless rule-space search: exhaustive sweeps, seed
                     sweeps, and the genetic search over table rules
crates/ui-wasm       wasm-bindgen shim exposing the engine to the browser
web/                 the interactive essay and research playground
results/             every committed result record (JSONL), including
                     results/flights/ — seeds, decimated telemetry, and
                     final checkpoints of the million-ring flights
GLIDERS.md           the object compendium: exact rule, seed, and measured
                     properties of every mobile object
analysis/            flight telemetry analysis (headings, compass refit)
paper/figures/       one directory per paper figure; each generate.sh
                     regenerates that figure from the committed records
```

A run is a small record: *(tiling family, root address, radius,
neighbourhood, rule, seed)*. Patches regenerate deterministically from
their identifiers, cell numbering is stable as radius grows, and the
browser runs the same engine compiled to WebAssembly — so every result
here replays exactly, headless or live.

## Build and test

Rust ≥ 1.96 (edition 2024). From the repository root:

```sh
cargo build --release
cargo test                 # full workspace test suite
```

## Verify a glider

Replay the first hat glider's committed record on a radius-384 patch
(the record was discovered at radius 48; growing the board is how a
candidate earns the name):

```sh
cargo run --release -p tiling-core --example verify_candidate \
  results/hat-tableevolve-r48-s21.jsonl 0 384 60000
```

The other gliders verify the same way — see `GLIDERS.md` for every
object's record file and parameters, regenerable via:

```sh
cargo run --release -p tiling-core --example record_info results/*.jsonl
```

## Fly one (unbounded)

The sliding-window tracker re-roots a small patch under the glider as it
flies — constant memory, arbitrary distance:

```sh
cargo run --release -p tiling-core --example slide \
  results/hat-tableevolve-r48-s21.jsonl 0 24 --launch-radius 48 --launch-gens 60
```

## Search

```sh
cargo run --release -p search -- --family hat --radius 32 --horizon 2048 --out-dir out/
cargo run --release -p search -- --mode evolve-table --family hat   # the GA that found the gliders
```

## The browser UI

Build the WebAssembly package first, then the standard Vite flow:

```sh
wasm-pack build crates/ui-wasm --target web --release -d ../../web/src/wasm
cd web && npm install && npm run dev    # then open the printed URL
```

`web/README.md` documents the playground; the essay sources are under
`web/essay/`.

## A note on code comments

Comments citing `FINDINGS §N` refer to the project's internal lab log,
which is not part of this snapshot; the corresponding results and
their controls appear in the paper.

## Citing

See `CITATION.cff`. Please cite the paper for the results and this
repository (or its archived Zenodo release) for the software and
records.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or
[Apache License 2.0](LICENSE-APACHE), at your option. The result
records, the object compendium, and the essay text are © 2026 James
Edmond and may be reused under the same terms.

## Contributing

This tree is a snapshot, synced from a private research repository in which
the work is done; it is published so that every record in the paper can be
regenerated and checked. Bug reports, reproduction problems and corrections
are welcome as issues. Pull requests are welcome too, but a fix will normally
be ported into the source tree and re-synced rather than merged here directly.
