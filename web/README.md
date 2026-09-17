# monotile-ca-web

Web frontend for running cellular automata on hat/spectre aperiodic tilings.
Plain TypeScript + WebGL2, bundled with vite. The simulation engine is a Rust
crate compiled to WASM with wasm-bindgen.

## Build the WASM engine first

From the **repo root**:

```sh
wasm-pack build crates/ui-wasm --target web --release -d ../../web/src/wasm
```

This overwrites the placeholder `src/wasm/ui_wasm.js` / `ui_wasm.d.ts` and adds
`ui_wasm_bg.wasm`. Until then the app builds fine but shows a
"WASM module not built" message at runtime.

## Run

```sh
cd web
npm install
npm run dev        # dev server
```

Other scripts:

```sh
npm run typecheck  # tsc --noEmit
npm run build      # production build into dist/
npm run preview    # serve the production build
```

## Usage

- **family / radius / Regenerate** — build a new patch (generation can take
  seconds for large radii; a loading overlay is shown).
- **B/S checkboxes + presets** — life-like rule as birth/survival neighbour-count
  bitmasks; the current rule is shown as e.g. `B3/S2,3`.
- **Back / Step / Play / speed** — run the automaton (generations per second).
  Back recomputes the previous generation from snapshot history (kept every 64
  generations plus at every edit/rule change), so it is exact and effectively
  instant; edits fork the timeline like an undo history.
- **Randomize** — seeded random fill (percent slider) within a BFS distance of
  the root tile.
- **Canvas** — drag to pan, wheel to zoom (cursor-centred), click a tile to
  toggle it, hover for cell id / class / distance / address.
- **Load result… / Reload @ radius** — replay a search `ResultRecord`
  (.json/.jsonl; a record selector appears for multi-record files). "Reload @
  radius" replays the selected record on a patch of the radius currently in
  the radius box — cell indices are BFS-stable as radius grows, so the seed
  lands on the same tiles with more room before the dead boundary interferes.
  Class-stratified records (per-tile-class rule tables, e.g. evolutionary
  champions in `results/*-evolve-*.jsonl`) replay with their full per-class
  rule; the B/S editor shows the class-0 table for reference, and toggling a
  box deliberately collapses the rule to uniform.

  **Start with `results/gallery.jsonl`** — a curated tour (the record selector
  pages through them): the hat walker, the spectre directed filament, the
  per-class and k-state evolved champions (the k=4 one shows dying-phase
  amber→ember colours), and two long-period small-seed oscillators. Load it,
  pick a record, zoom toward the active cluster, and play. Regenerate with
  `cargo run --release -p tiling-core --example gallery`.

## Deploy (offlattice.org/monotile/)

The site is built with `base: '/monotile/'` — the playground lands at
`offlattice.org/monotile/`, the essay at `offlattice.org/monotile/essay/`
(the citable home), kiosk at `/monotile/kiosk.html`.

```sh
npm run build
rsync -a --delete dist/ ~/Code/offlattice/site/monotile/
# the paper, in preprint mode, at offlattice.org/monotile/paper.pdf (linked from every essay page)
(cd ../paper && sed 's/^%\\preprinttrue/\\preprinttrue/' paper.tex > paper-preprint.tex && pdflatex -interaction=nonstopmode paper-preprint.tex >/dev/null && pdflatex -interaction=nonstopmode paper-preprint.tex >/dev/null && cp paper-preprint.pdf ~/Code/offlattice/site/monotile/paper.pdf)
cd ~/Code/offlattice
npx wrangler pages deploy site --project-name offlattice --branch staging  # preview
npx wrangler pages deploy site --project-name offlattice                   # PRODUCTION
```

Staging serves at https://staging.offlattice.pages.dev/monotile/essay/
(plus a per-deploy hash URL); production only moves when deploying
without `--branch`. Before a production deploy, run the launch sweep:
`grep -rn TODO essay paper/` from the repo root (footer/colophon links,
clone instructions, paper URL TODOs).
