#!/bin/sh
# Figure 1: spectre glider s33, five consecutive generations (100-104),
# white ground, cropped to the far glider of the launched pair.
# Regenerates s33-strip.svg (the figure) plus the per-generation panels.
# Run from the repository root. Decisions: FINDINGS §10.1/GLIDERS.md
# (subject), WRITEUP-PLAN "Figure 1 DECIDED" (principles: consecutive
# steps, saturation+luminance state ladder, plain ground).
set -e
cargo run --release -p tiling-core --example render_frames \
  results/spectre-tableevolve-r48-s33.jsonl 0 96 \
  100,101,102,103,104 5 \
  paper/figures/fig1-glider-strip/s33 far plain
