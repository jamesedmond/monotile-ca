# monotile-ca — cellular automata on the hat and spectre tilings

The toolchain, search drivers, and complete result set behind the paper
**_Gliders on Aperiodic Monotilings: Cellular Automata on the Hat and
Spectre_** (James Edmond, 2026) and its companion interactive essay:

- **Essay** (every figure live, every record replayable in the browser):
  https://offlattice.org/monotile/essay/
- **Glider census** (thirty-two certified travelling objects, each flying in a live viewer):
  https://offlattice.org/monotile/census/
- **Paper**: [arXiv:2609.22579](https://arxiv.org/abs/2609.22579) (nlin.CG), "Gliders on Aperiodic Monotilings: Cellular Automata on the Hat and Spectre"
- **Certificates paper**: "Certificates of Immortality for Gliders on the Hat and Spectre Tilings" (James Edmond, 2026; arXiv identifier to be added on posting) — the gliders proved immortal on every tiling of the hull, with exact heading and speed bounds; see [Certificates of immortality](#certificates-of-immortality) below
- **Archived release**: v1.1.0, [doi:10.5281/zenodo.23963284](https://doi.org/10.5281/zenodo.23963284) (all versions: [doi:10.5281/zenodo.22836247](https://doi.org/10.5281/zenodo.22836247)); v1.0.0, the state of the first paper: [doi:10.5281/zenodo.22836248](https://doi.org/10.5281/zenodo.22836248)

The hat and spectre monotiles (discovered 2023) tile the plane only
aperiodically. This project ran the first systematic cellular-automaton
searches on them and found verified gliders on both — plus the search
methodology (structural controls, cross-radius verification, anti-gaming
filters) that makes such claims trustworthy on a board with no
translational symmetry — and then proved the gliders immortal by finite
computation, the certificates of the second paper.

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
crates/certify       certificates of immortality: the legal-ball atlas by
                     transducer exploration, the situation closure, the
                     exact cocycle (heading and speed bounds), keyed flights,
                     the census; crates/certify/check/ holds the independent
                     Python checker and the transducer cross-check
crates/ui-wasm       wasm-bindgen shim exposing the engine to the browser
web/                 the interactive essay, the glider census pages and the
                     research playground
results/             every committed result record (JSONL), including
                     results/flights/ — seeds, decimated telemetry, and
                     final checkpoints of the million-ring flights;
                     results/certificates/ — the ten certificates, the
                     atlas and geometry audit logs, the census tables, the
                     census objects' records and the table of commands
                     that rebuild their certificates;
                     results/rules/ — rules from outside our search, as data
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

## Certificates of immortality

A certificate is a finite set of *situations* (a glider's pattern with
four rings of surrounding tiles) closed under the rule on every legal
environment the tiling can present. Its existence proves the glider
immortal on every tiling of the hull, and its exact cocycle bounds the
heading and speed (Theorems 1 and 2 of the certificates paper). The ten
certificates are in `results/certificates/`; `GLIDERS.md` tabulates them.

Regenerate the hat's radius-9 atlas of legal balls (a few minutes), then
read a certificate's numbers and check it independently:

```sh
cargo run --release -p certify -- atlas hat 9 --verify --out out/atlas-hat-r9.json
cargo run --release -p certify -- cocycle results/certificates/hat-A-s21-k4-R9.json
python3 crates/certify/check/check_certificate.py   results/certificates/hat-A-s21-k4-R9.json out/atlas-hat-r9.json   results/hat-tableevolve-r48-s21.jsonl
```

Rebuild a certificate from its record (the closure, from the keyed flight
on a radius-384 patch), fly it with every generation keyed against the
certificate, and compare two certificates:

```sh
cargo run --release -p certify -- closure results/hat-tableevolve-r48-s21.jsonl out/atlas-hat-r9.json   --radius 384 --start 60 --k 4 --out out/hat-A.json
cargo run --release -p certify -- trace results/hat-tableevolve-r48-s21.jsonl   results/certificates/hat-A-s21-k4-R9.json --gens 20000 --out out/hat-A-trace.tsv
cargo run --release -p certify -- stats results/certificates/hat-A-s21-k4-R9.json results/certificates/hat-B-s22-k4-R10.json
cargo run --release -p certify -- relabel results/certificates/hat-A-s21-k4-R9.json   results/certificates/hat-B-s22-k4-R10.json --map 1:3,2:1,3:0     # hat B is hat A in another alphabet
```

The spectre and the radius-10/11 certificates need the matching atlas
(`atlas spectre 9`, `atlas hat 10`, `atlas hat 11`, `atlas spectre 11`);
the stratified Worms certificate takes `--strata 1:1` on the checker.

The census of the certificates paper found 32 certified travelling objects
(https://offlattice.org/monotile/census/). Their certificates are not
stored. `results/certificates/census-certificates.tsv` lists, for each
object, the `certify closure` command that rebuilds its certificate from a
record in this repository, and what the certificate contains: situation
count, heading window, speed bound and signature. A rebuild takes about a
minute once the two radius-11 atlases exist. The driver generates the
atlases if they are missing (a few minutes each), runs all 32 commands and
compares each result with the table:

```sh
cargo build --release -p certify
python3 crates/certify/census/rebuild_certificates.py            # --check also runs the independent checker on each
```

Rules from outside our search run as data: `results/rules/*.json` through
`cargo run --release -p tiling-core --example table_soups`, and
Trevorrow's HatLife Worms through `--example worms_hatlife`, which checks
the transcription against a direct implementation of HatLife's rule.
`crates/certify/check/transducer-crosscheck` is the stand-alone cross-check
of the transducer against its reference algorithm (Appendix C of the paper).

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

See `CITATION.cff`. Please cite the papers for the results — the first
for the gliders and the search, the second for the certificates — and
this repository (or its archived Zenodo release) for the software and
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
