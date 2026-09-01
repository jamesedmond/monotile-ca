#!/bin/sh
# Portrait gallery: one frame per monotile glider, composed 2x2.
# Frames via render_frames (white ground, Figure-1 palette). All four
# are close-ups of a single glider in flight: the multi-glider spread
# of the s23/s33 launches is a seed artifact, while the glider body is
# the rule-invariant object (focus "far" crops to one object).
# Run from the repository root.
set -e
D=paper/figures/fig-portraits
cargo run --release -p tiling-core --example render_frames results/hat-tableevolve-r48-s21.jsonl 0 96 100 8 $D/s21 far plain
cargo run --release -p tiling-core --example render_frames results/hat-tableevolve-r48-s22.jsonl 0 96 100 8 $D/s22 far plain
cargo run --release -p tiling-core --example render_frames results/hat-tableevolve-r48-s23.jsonl 0 96 110 8 $D/s23 far plain
cargo run --release -p tiling-core --example render_frames results/spectre-tableevolve-r48-s33.jsonl 0 96 100 8 $D/s33 far plain
python3 $D/compose.py
