#!/bin/sh
# Atlas assets: per monotile glider, (a) a close-up frame, (b) an
# oblique space-time worldtube panel, (c) the full-lifetime trail at
# radius 384. Close-ups and trails are pure Rust (render_frames /
# render_trail); the tube panels are captured from the live essay
# viewer (dev server on localhost:5173 + playwright in web/ required;
# see capture-tubes.mjs — copy it into web/ and run with
# FIG_OUT=../paper/figures/fig-atlas).
# Rule tables in paper.tex are transcribed verbatim from GLIDERS.md.
set -e
D=paper/figures/fig-atlas
cargo run --release -p tiling-core --example render_frames results/hat-tableevolve-r48-s21.jsonl 0 96 100 8 $D/s21-closeup far plain square
cargo run --release -p tiling-core --example render_frames results/hat-tableevolve-r48-s22.jsonl 0 96 100 8 $D/s22-closeup far plain square
cargo run --release -p tiling-core --example render_frames results/hat-tableevolve-r48-s23.jsonl 0 96 110 8 $D/s23-closeup far plain square
cargo run --release -p tiling-core --example render_frames results/spectre-tableevolve-r48-s33.jsonl 0 96 100 8 $D/s33-closeup far plain square
for s in s21 s22 s23 s32 s33; do mv $D/$s-closeup-g*.svg $D/$s-closeup.svg; rm -f $D/$s-closeup-strip.svg; done
cargo run --release -p tiling-core --example render_trail results/hat-tableevolve-r48-s21.jsonl 0 384 $D/s21-trail.svg
cargo run --release -p tiling-core --example render_trail results/hat-tableevolve-r48-s22.jsonl 0 384 $D/s22-trail.svg
cargo run --release -p tiling-core --example render_trail results/hat-tableevolve-r48-s23.jsonl 0 384 $D/s23-trail.svg
cargo run --release -p tiling-core --example render_trail results/spectre-tableevolve-r48-s33.jsonl 0 384 $D/s33-trail.svg
# The looper: long horizon (never reaches the boundary; the full
# period-3300 orbit needs ~4,000 generations), trail-fit zoom.
cargo run --release -p tiling-core --example render_frames results/spectre-tableevolve-r48-s32.jsonl 0 96 100 8 $D/s32-closeup far plain square
mv $D/s32-closeup-g100.svg $D/s32-closeup.svg; rm -f $D/s32-closeup-strip.svg
cargo run --release -p tiling-core --example render_trail results/spectre-tableevolve-r48-s32.jsonl 0 384 $D/s32-trail.svg 4800 trail
cp $D/capture-tubes.mjs web/atlas-capture-tubes.mjs
cd web && FIG_OUT=../$D node atlas-capture-tubes.mjs && rm atlas-capture-tubes.mjs && cd ..
cd web && for s in s21 s22 s23 s32 s33; do node rasterize.mjs ../$D/$s-closeup.svg ../$D/$s-trail.svg; done
