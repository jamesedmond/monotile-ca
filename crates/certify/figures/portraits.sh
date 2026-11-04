#!/bin/bash
# Certificate paper: portraits of the ten certified objects (Figures "objects" and "outside").
# One settled frame per object from its committed record, cropped by render_frames, then composed
# at one scale by fig_portraits.py. Run from the repository root after `cargo build --release
# -p tiling-core --example render_frames`.
set -e
R=./target/release/examples/render_frames; O=out/portraits; F=${1:-out/portraits/figures}   # final SVGs go to $1 (the paper directory) or out/
mkdir -p "$F"
mkdir -p $O
sed -n 9p results/certificates/census-seeds.jsonl > $O/spectre-second.jsonl   # the second spectre family's census seed
$R results/hat-tableevolve-r48-s21.jsonl 0 96 100 6 $O/hatA far
$R results/hat-lanechange.jsonl          0 96 150 6 $O/captured far
$R results/hat-tableevolve-r48-s22.jsonl 0 96 100 6 $O/hatB far
$R results/hat-tableevolve-r48-s23.jsonl 0 96 110 6 $O/hatC-c1285 cell:1285:12   # the 345-degree object
$R results/hat-tableevolve-r48-s23.jsonl 0 96 110 6 $O/hatC-c411  cell:411:12    # the 105-degree glider
$R results/spectre-tableevolve-r48-s33.jsonl 0 96 100 6 $O/spectre far
$R $O/spectre-second.jsonl               0 96 150 6 $O/spectre2 far
$R results/hat-worms-trevorrow-soup2.jsonl 0 192 300 6 $O/worm far
$R results/hat-wake-gabbay-soup8.jsonl   0 96 150 6 $O/wake far
$R results/hat-s33-soup14.jsonl          0 96 150 6 $O/s33hat far
python3 crates/certify/figures/fig_portraits.py $F/fig-objects.svg 4 \
 "hat A (s21)|generation 100=$O/hatA-g100.svg" \
 "the captured object (s21)|generation 150=$O/captured-g150.svg" \
 "hat B (s22)|generation 100=$O/hatB-g100.svg" \
 "hat C, the 345° object (s23)|generation 110=$O/hatC-c1285-g110.svg" \
 "hat C, the 105° glider (s23)|generation 110=$O/hatC-c411-g110.svg" \
 "the spectre glider (s33)|generation 100=$O/spectre-g100.svg" \
 "spectre, second family (s33)|generation 150=$O/spectre2-g150.svg"
python3 crates/certify/figures/fig_portraits.py $F/fig-outside.svg 3 \
 "Trevorrow's worm (Worms)|generation 300=$O/worm-g300.svg" \
 "Gabbay's wake traveller|generation 150=$O/wake-g150.svg" \
 "s33 on the hat|generation 150=$O/s33hat-g150.svg"
rsvg-convert -f pdf -o $F/fig-objects.pdf $F/fig-objects.svg
rsvg-convert -f pdf -o $F/fig-outside.pdf $F/fig-outside.svg
