#!/usr/bin/env python3
"""Compose the four glider portraits into one labeled 2x2 gallery.
Panels come from render_frames (see generate.sh); each keeps its own
scale (close-ups for the single gliders, wide shots for the multi-
glider records — the separation is part of the portrait). Writes
portraits.svg beside this script."""

import os
import re

DIR = os.path.dirname(os.path.abspath(__file__))
PANELS = [
    ("s21-g100.svg", "(a) hat A — one glider, generation 100"),
    ("s22-g100.svg", "(b) hat B — one glider, generation 100"),
    ("s23-g110.svg", "(c) hat C — one glider of the triple, generation 110"),
    ("s33-g100.svg", "(d) spectre — one glider of the pair, generation 100"),
]

CELL_W, CELL_H = 460, 360
GAP, LABEL_H = 26, 34

cells = []
for fname, label in PANELS:
    with open(os.path.join(DIR, fname)) as f:
        svg = f.read()
    m = re.search(r'viewBox="([^"]+)"', svg)
    viewbox = m.group(1)
    body = svg[svg.index(">", svg.index("<svg")) + 1: svg.rindex("</svg>")]
    cells.append((viewbox, body, label))

W = 2 * CELL_W + GAP
H = 2 * (CELL_H + LABEL_H) + GAP
out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}">',
       f'<rect width="{W}" height="{H}" fill="white"/>']
for i, (viewbox, body, label) in enumerate(cells):
    col, row = i % 2, i // 2
    x = col * (CELL_W + GAP)
    y = row * (CELL_H + LABEL_H + GAP)
    out.append(f'<svg x="{x}" y="{y}" width="{CELL_W}" height="{CELL_H}" '
               f'viewBox="{viewbox}" preserveAspectRatio="xMidYMid meet">')
    out.append(body)
    out.append('</svg>')
    out.append(f'<text x="{x + CELL_W / 2}" y="{y + CELL_H + 23}" '
               f'font-family="Helvetica, Arial, sans-serif" font-size="17" '
               f'fill="#333333" text-anchor="middle">{label}</text>')
out.append("</svg>")
with open(os.path.join(DIR, "portraits.svg"), "w") as f:
    f.write("\n".join(out) + "\n")
print("wrote portraits.svg")
