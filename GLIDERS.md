# The Glider Compendium

Human-readable settings and start conditions for every mobile object
discovered (or reproduced) in this project. Each entry gives the exact
substrate, rule, and seed needed to recreate the object from scratch,
plus its measured properties. The machine-readable equivalents are the
JSONL records under `results/` — those are the ground truth; this file
is generated from them (`tiling-core --example record_info <files>`)
plus the measurements reported in the companion paper.

**Shared conventions**

- **Cell numbering**: cells are numbered by deterministic breadth-first
  discovery from the root tile; cell 0 *is* the root tile, and indices
  are stable when the patch radius grows — so a seed defined at radius
  48 lands on the same tiles at radius 384.
- **Neighbourhood**: every object below lives on the **vertex**
  neighbourhood (tiles are neighbours iff their boundaries share at
  least one vertex — the Owens–Stepney "generalised Moore"
  neighbourhood). Vertex degrees: hat/spectre 6–7, P2 8–10, P3 7–11.
- **Rules** are 4-state priority tables: rows are checked top to
  bottom, the first row whose `own`-state and neighbour-count
  conditions all hold decides the next state, and **no match means
  state 0**. `nᵢ` counts neighbours in exactly state *i*. (Evolved
  tables contain duplicate/shadowed rows — genetic cruft; they are
  reproduced verbatim because the record is the rule.)
- **Inert rows** (marked per object below) are measured, not
  eyeballed: `tiling-core --example row_usage <record> 0 384` (looper:
  `0 192 12000`) deletes rows greedily bottom-up, keeping a deletion
  only if a lockstep replay of the whole verified run is identical at
  every generation. This is stronger than "never fires": a row firing
  only dead→dead can still be load-bearing by shadowing a later birth
  row (hat B's row 1 is exactly that, and is *not* inert). Inert rows
  (1-based): hat A [5,7,8]; hat B [2]; hat C [6,7]; spectre [2,4,6];
  mega-looper [2,7]. The paper's atlas tables gray them out.
- **UI rendering**: state 0 dead, state 1 bright ("alive"), states 2–3
  the amber→ember fade. To replay: *Load result…* → pick the JSONL;
  type a larger radius (96–128) to give the object room; do **not**
  touch the B/S rule editor while replaying a table-rule record (it
  would replace the table).
- **Discovery**: all evolved objects come from
  `search --mode evolve-table --family <f> --radius 48 --horizon 2500
  --population 256 --generations 800 --states 4 --pop-cap 64
  --rng-seed <n>` — a GA over random-initialised priority tables,
  fitness = travel × population-flatness under a population cap of 64,
  evaluated over a 13-seed bank at the default root.
- **Verification standard**: replay at radius 48/96/192/384; a glider
  must reach the boundary at *every* radius with flat max-population.
  `verify_candidate <record> 0 384 60000` reproduces the check;
  `heading <record> 0 128 30000 32` reproduces the trajectory numbers.

---

## True gliders (ballistic, compass-locked)

### Hat glider A — `results/hat-tableevolve-r48-s21.jsonl`

The first glider ever seen on a hat tiling. Minimal (1–2 bright cells
per step), moves strictly south-west. Discovered at GA generation 5 of
the very first hat run (`--family hat --rng-seed 21`).

- Substrate: **hat**, vertex neighbourhood, root
  `(tile antihat)(subtile 3 of H0)(subtile 1 of F0):(subtile 4 of F0)`
- Rule (4-state priority table):

  | own | condition | next |
  |---|---|---|
  | 0 | n₂ ≥ 2 | 0 |
  | any | n₁ ≥ 3 (dup. cond.) | 3 |
  | 2 | n₁ ≥ 1 ∧ n₀ ≥ 1 | 1 |
  | 1 | n₂ ≥ 1 (dup. cond.) | 3 |
  | 2 | always | 0 (inert) |
  | 0 | n₁ ≥ 1 | 2 |
  | 2 | n₃ ≥ 2 | 0 (inert) |
  | 2 | always | 0 (inert) |

  (All three inert rows send 2 → 0, which is the fall-through default
  anyway.)

- Seed: cell 0 (the root antihat) = state **1**; cell 1 (edge-neighbour
  hat, `(tile hat)(subtile 2 of H0)(subtile 1 of F0):(subtile 4 of F0)`)
  = state **2**.
- Measured: max population **16**, flat at radius 48/96/192/384/768
  (boundary at generations 89/183/377/763/1533 — **2.0
  generations/ring, constant**). Heading **226.6°** (6-fold compass direction; windowed
  headings 224–234°, one 234° excursion then re-lock). Lane width ≤ 4
  tiles. Startup: emits **two** parallel gliders; one dies at
  generation 42 by hitting the other's tail (the first observed
  glider–glider interaction); the survivor is stable. Ignition: **89%** of 188 generic two-tile seedings (all classes, all edges, both orders) launch a glider; none die. At other sites the pair may instead **deflect non-destructively** (course changes for one or both) and settle at a 60°/120°/180° separation.

### Hat glider B — `results/hat-tableevolve-r48-s22.jsonl`

Almost certainly the **same glider phenotype as A in a different
genome** (identical heading 226.6°, speed, and windowed-heading
sequence), but visually distinct in dress: 4–5 bright cells that
oscillate side-to-side across the movement axis. Same two-glider
startup with one survivor. From `--family hat --rng-seed 22`.

- Substrate: hat, vertex, same root as A.
- Rule:

  | own | condition | next |
  |---|---|---|
  | 0 | n₁ ≥ 2 ∧ n₀ ≥ 1 | 0 |
  | 0 | n₀ ≥ 2 ∧ n₂ ≥ 2 | 1 (inert) |
  | 0 | n₀ ≥ 2 ∧ n₃ ≥ 1 | 1 |
  | 1 | n₃ ≥ 1 | 3 |

  (The tersest glider rule found so far: three load-bearing rows. The
  inert row waits for state 2, which neither the seed nor any row
  produces — it can never fire. Row 1 (dead→dead) *is* load-bearing:
  it blocks row 3 births, so it is not inert. There is no survival row
  for 1: a bright cell lasts one generation — wing if a 3 stands
  beside it, dead otherwise — so the anatomy is reborn ahead each
  step.)
- Seed: cell 0 (root antihat) = state **3**; cell 1 (hat neighbour,
  address as in A) = state **1**.
- Measured: max population **14**, flat; boundary at
  89/183/377/763/1533 — identical clock to A. Heading 226.6°. Ignition: 88% generic (165/188); same merge-or-deflect pair behaviour as A.

### Hat glider C (triple) — `results/hat-tableevolve-r48-s23.jsonl`

The showpiece: starts as a **wanderer** that meanders near the origin
for ~85 generations, then decays into two, then **three separate
gliders** — two heading ≈345° (NNW), one ≈105° (ESE); splits of
≈ 2·60°. Direct evidence that wanderers are excited bound states of
gliders. From `--family hat --rng-seed 23`.

- Substrate: hat, vertex, same root as A.
- Rule:

  | own | condition | next |
  |---|---|---|
  | any | n₂ ≥ 1 ∧ n₁ ≥ 3 | 3 |
  | 1 | n₂ ≥ 1 | 2 |
  | 3 | n₃ ≥ 3 | 3 |
  | 0 | n₃ ≥ 2 | 1 |
  | any | n₂ ≥ 1 | 1 |
  | 1 | n₀ ≥ 3 ∧ n₂ ≥ 2 | 2 (inert: shadowed by row 2) |
  | 0 | n₃ ≥ 3 | 3 (inert: shadowed by row 4) |

- Seed: cell 0 (root antihat) = state **2**; cell 1 (hat neighbour) =
  state **1**.
- Measured: max population **32** (all three gliders together), flat;
  boundary at 158/254/446/834/1602 — the law gen = 62 + 2r, fitted at
  ≤ 384, predicts the radius-768 arrival to 0.25%.
  Settled headings 345.8°, 103.1°, 104.8°; the cleanest glider (object
  2) holds its lane to 1.3 tiles. Ignition: 24% generic (45/188; most other seedings read as growers/showers at radius 64 — consistent with the wanderer-intermediate launch).

### Spectre glider (pair) — `results/spectre-tableevolve-r48-s33.jsonl`

The first glider on a spectre tiling. The seed emits a **pair** of
maximally-minimal gliders (exactly one bright cell per step, like
Goucher's) separating at **119.06° ≈ 2·60°**. From
`--family spectre --rng-seed 33`.

- Substrate: **spectre**, vertex neighbourhood, root
  `(tile spectre)(subtile 0 of Delta):(subtile 2 of Delta)`
- Rule:

  | own | condition | next |
  |---|---|---|
  | 0 | n₀ ≥ 3 ∧ n₁ ≥ 1 | 3 |
  | 0 | always | 0 (inert: restates the default) |
  | 3 | n₃ ≥ 3 | 0 |
  | 0 | n₀ ≥ 3 ∧ n₁ ≥ 1 | 0 (inert: shadowed) |
  | 3 | n₁ ≥ 1 | 1 |
  | 0 | always | 0 (inert: shadowed) |

  (Note the early `own 0 | always → 0` row: rows below it can never
  fire for ground cells — the *only* live ground transition is the
  first row. A three-state relay: 1 excites a wing 3, the wing becomes
  the next 1.)
- Seed: cell 0 (root spectre) = state **1**; cell 1 (spectre
  edge-neighbour,
  `(tile spectre)(subtile 0 of Phi)(subtile 6 of Phi)(subtile 3 of Delta):(subtile 2 of Delta)`)
  = state **3**.
- Measured: max population **12** (both gliders), flat; boundary at
  89/185/378/765/1534 — 2.0 generations/ring. Headings 107.7° and 348.6°
  (6-fold locked at 0.36° mean residual — the tightest compass lock
  measured). Lane width ≤ 1.9 tiles. Ignition: 71% generic (81/114); none die — and stereotyped: nearly every site launches a 120°-separated pair (one site gave 180°).

### Goucher glider (reproduction) — `results/penrose-goucher-glider.jsonl`

The first published aperiodic-tiling glider (Goucher 2012), reproduced
as the project's detection control — with **the paper's Table 1 row 2
corrected** (printed next-state 3 is a typo; it must be 1, per the
paper's own prose and the Ready reference kernel; as printed, the seed
dies at generation 4).

- Substrate: **Penrose P3** (rhombs), vertex neighbourhood, root
  `(tile thick):(subtile 1 of thick)`, recorded radius 96.
- Rule (states: 0 ground, 1 head, 2 tail, 3 wing):

  | own | condition | next |
  |---|---|---|
  | 0 | n₁ ≥ 1 ∧ n₂ ≥ 1 | 3 |
  | 0 | n₁ ≥ 1 ∧ n₃ ≥ 2 | **1** (paper misprints 3) |
  | 1 | n₃ ≥ 1 | 2 |
  | 1 | always | 1 |
  | 2 | always | 3 |

- Seed: cell 0 (root thick rhomb) = state **1** (head); cell 1 (thin
  rhomb edge-neighbour, `(tile thin)(subtile 0 of thick):(subtile 1 of
  thick)`) = state **2** (tail). Any head+tail pair across any shared
  edge works — it launches along the de Bruijn ribbon crossing that
  edge.
- Measured: max population **10**, flat; ~2.0 generations/ring;
  boundary at generation 41/89/185 for radius 24/48/96 (and 239 at
  radius 120). Heading 89.9°, locked to the 10-fold pentagrid fan at
  **0.08°**; deviates < 1 tile from a straight line — the rail-rider
  benchmark. On P2 the same rule produces loopers (see below). Ignition: 100% generic (40/40) — any head+tail pair on any edge launches, as ribbon theory predicts.

### The phoenix (pair) — `results/penrosep3-tableevolve-r48-s11.jsonl`

Blind-GA discovery, **2× faster than Goucher's glider**: states 1 and 2
have no survival rows at all, so every live cell dies each generation
and the pattern is reborn ahead of itself. Emits two gliders separating
at **142.9° ≈ 4·36°**, each aimed at a decagon corner, each trailing a
"pulsing exhaust" of short-lived cells. From
`--family p3 --rng-seed 11`.

- Substrate: Penrose P3, vertex, root `(tile thick):(subtile 1 of thick)`
- Rule:

  | own | condition | next |
  |---|---|---|
  | 3 | n₂ ≥ 1 | 1 |
  | 0 | n₂ ≥ 3 | 0 |
  | 0 | n₃ ≥ 1 ∧ n₂ ≥ 1 | 3 |
  | 0 | n₃ ≥ 1 ∧ n₂ ≥ 1 | 3 (inert: dup. of row 3) |
  | 0 | n₃ ≥ 2 (dup. cond.) | 1 |
  | 0 | n₃ ≥ 2 (dup. cond.) | 2 (inert: shadowed by row 5) |
  | 0 | n₃ ≥ 1 ∧ n₀ ≥ 3 | 2 |
  | 0 | n₃ ≥ 1 ∧ n₀ ≥ 3 | 2 (inert: dup. of row 7) |

- Seed: cell 0 (root thick) = state **2**; cell 1 (thin neighbour) =
  state **3**.
- Measured: max population **32** (both gliders + exhaust), flat;
  boundary at 48/96/191/385 — **~1 ring/generation, constant** (the
  fastest object known here). Headings 53.7° / 270.8°, 10-fold locked
  at 0.45°; lane ≤ 1.6 tiles.

---

## Wanderers (super-diffusive Lévy walkers on the rail fan)

All three P3 wanderers share the transport law: displacement ∝ t^α
with α ≈ 0.6–0.85 (31× faster than diffusion over the measured span,
well short of ballistic), realised as ballistic runs locked to
pentagrid directions with stochastic re-orientation. Cohesive, ash-free
clumps; the flat-population *immortal* corner that §8 found missing on
the monotiles in Generations rules.

### The dressed particle — `results/penrosep3-tableevolve-r48.jsonl`

The original level-2 discovery (pre-scrambler-fix run, effective RNG
stream 5). A persistent state-2 core travelling inside a live halo of
3s; annihilates on 2–2 contact.

- Substrate: Penrose P3, vertex, root `(tile thick):(subtile 1 of thick)`
- Rule:

  | own | condition | next |
  |---|---|---|
  | 2 | n₂ ≥ 1 | 0 |
  | 0 | n₃ ≥ 1 ∧ n₂ ≥ 1 | 1 |
  | 3 | n₂ ≥ 1 ∧ n₁ ≥ 3 | 2 |
  | 2 | n₂ ≥ 3 (dup. cond.) | 1 |
  | any | n₂ ≥ 1 | 3 |
  | any | n₂ ≥ 1 | 3 (dup.) |
  | 2 | n₀ ≥ 2 (dup. cond.) | 2 |
  | any | n₂ ≥ 1 ∧ n₁ ≥ 3 | 2 (shadowed) |

- Seed: cell 0 (root thick) = state **2**; cell 1 (thin neighbour) =
  state **3**.
- Measured: max population **19**, flat to radius 384 (boundary at
  179/595/1433/4699 generations — α ≈ 0.6–0.84). Slight persistent
  drift within the wander.

### Wanderer "energetic" — `results/penrosep3-tableevolve-r48-s12.jsonl`

Same transport law, twitchier temperament: its pentagrid lock is only
visible at window 8 (re-orients roughly twice as often as s15). From
`--family p3 --rng-seed 12`.

- Rule:

  | own | condition | next |
  |---|---|---|
  | 0 | n₁ ≥ 1 ∧ n₀ ≥ 2 | 2 |
  | any | n₁ ≥ 2 ∧ n₀ ≥ 2 | 0 |
  | 2 | n₁ ≥ 1 ∧ n₂ ≥ 1 | 1 |
  | 0 | n₁ ≥ 2 | 2 |
  | 0 | n₃ ≥ 2 (dup. cond.) | 3 |
  | any | n₃ ≥ 1 ∧ n₀ ≥ 2 | 1 |
  | any | n₁ ≥ 1 ∧ n₃ ≥ 3 | 1 |
  | 2 | n₃ ≥ 2 | 3 |

- Seed: cell 0 = state **3**; cell 1 (thin neighbour) = state **3**.
- Measured: max population **32**, flat; boundary at
  175/607/1415/4634; run segments 10-fold locked at 1.41° (window 8).

### Wanderer "calm" — `results/penrosep3-tableevolve-r48-s15.jsonl`

The sedate one: long ballistic runs (lock already clean at window 16).
From `--family p3 --rng-seed 15`.

- Rule:

  | own | condition | next |
  |---|---|---|
  | 3 | n₀ ≥ 3 | 3 |
  | 2 | n₁ ≥ 3 (dup. cond.) | 3 |
  | 0 | n₃ ≥ 2 | 0 |
  | 0 | n₃ ≥ 1 | 1 |
  | any | n₃ ≥ 1 | 2 |
  | 1 | n₁ ≥ 3 (dup. cond.) | 3 |
  | 0 | n₃ ≥ 1 | 1 (shadowed) |

- Seed: cell 0 = state **1**; cell 1 (thin neighbour) = state **3**.
- Measured: max population **17**, flat; boundary at
  183/599/1437/4703; segments 10-fold locked at 1.32° (window 16).

---

## Loopers (bounded orbits of glider mechanisms)

### Goucher looper — `results/penrose-goucher-looper.jsonl`

The same corrected Goucher rule on **Penrose P2** (kites and darts),
where the glider path closes into loops instead of running straight.
This record orbits at exact period **40** (the cartwheel loop); other
anchors give period 20 (the shortest loop) and, at radius 64, period
200 — matching the published sequence 40, 200, 1240
(P(n+2) = 5·P(n+1) + 6·P(n)).

- Substrate: Penrose P2, vertex, root `(tile dart):(subtile 1 of dart)`,
  radius 32.
- Rule: identical to the Goucher glider above.
- Seed: cell 0 (root dart) = state **1** (head); cell 1 (kite
  edge-neighbour, `(tile kite)(subtile 0 of kite)(subtile 0 of
  kite)(subtile 0 of dart):(subtile 1 of dart)`) = state **2** (tail).
- Measured: max population 8; `Periodic { period: 40 }`, Brent-exact.

### Spectre mega-looper — `results/spectre-tableevolve-r48-s32.jsonl`

The great impostor: a pair of oscillating wanderers that passes the
radius-48 and radius-96 checks, then at radius 192 goes exactly
periodic — full-state period **3300**, which is **not one orbit** but
the beat of **two independent captures** (decomposed 2026-08-15 after
a user UI observation at radius 128 spotted the first loop):

- **wanderer 1** is trapped almost at once: locks at **generation
  92** into a **period-60** loop (207-tile track, rings 17–40) — the
  tight hexagon visible near the seed in any replay;
- **wanderer 2** meanders for ~1,000 generations, reaching ring
  **189**, then locks at **generation 992** into a **period-550**
  circuit (1,509-tile closed track spanning rings 107–189 — ring 189
  is *on* the circuit, not transient);
- full state periodic from generation 992 with period
  **lcm(60, 550) = 3300**; Brent's `detected_by: 7395` is detection
  lag, not lock. Transient-only track: 2,818 tiles.

At radius ≤ 128 wanderer 2 exits the patch instead (boundary at
166/286 gens for r48/96) and only the period-60 loop remains. A
possible monotile analogue of the P2 loopers. From
`--family spectre --rng-seed 32`. Reproduce the decomposition:
`cargo run --release -p tiling-core --example cycle_components
results/spectre-tableevolve-r48-s32.jsonl 0 192 3300`.

- Substrate: spectre, vertex, root
  `(tile spectre)(subtile 0 of Delta):(subtile 2 of Delta)`
- Rule:

  | own | condition | next |
  |---|---|---|
  | 0 | n₁ ≥ 1 ∧ n₃ ≥ 1 | 3 |
  | 0 | n₁ ≥ 1 ∧ n₃ ≥ 1 | 3 (inert: dup. of row 1) |
  | 0 | n₀ ≥ 2 ∧ n₃ ≥ 1 | 1 |
  | 1 | n₀ ≥ 2 | 2 |
  | 0 | n₂ ≥ 3 | 2 |
  | 1 | always | 1 |
  | 3 | n₃ ≥ 1 | 1 (inert: row 8 converts the same cells) |
  | 3 | n₀ ≥ 1 | 1 |

- Seed: cell 0 (root spectre) = state **3**; cell 1 (spectre
  edge-neighbour, address as in the spectre glider) = state **1**.
- Measured: max population 29/30/31 at radius 48/96/192; boundary at
  166/286 generations for 48/96; at 192,
  `Periodic { period: 3300, detected_by: 7395 }` = the two-loop beat
  above.
