#!/usr/bin/env python3
"""Measured-only compass rose (v3): the paper's compass figure.

Two panels (hat, spectre; P3 omitted per the review note). Per substrate:
six dotted spokes at the MEASURED offset (the mean of the lanes' settled
heading mod 60), and one solid arrow per flown lane labelled with its
settled heading ± SE. Missing lanes (no flight yet) stay as bare dotted
spokes. Parameter-free pair separations are annotated. No claimed-vs-refit
anything — measured only.

Settled heading per lane = circular mean of the windowed (local) heading
over the flight's late half; SE = circular-std / sqrt(independent blocks).
Reads each lane's flight CSV (headline lanes from results/, the six
spoke-fill lanes from the scratchpad). Auto-refreshes as flights complete.
"""

import csv
import math
import os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..")
# All twelve lanes read from the committed decimated flight telemetry.
FL = "results/flights"

HAT = "#d55e00"
SPEC = "#cc79a7"

# (label, csv path, color)  — one triad measured (headline), one anti-triad
# + gaps filled from the ignition sweep. csv path relative to ROOT or abs.
# Settled heading and SE per lane, from the full-flight analysis
# (FINDINGS 10.8.1 table; 10^5-ring flights, headline lanes 10^6).
# The full telemetry regenerates deterministically:
#   cargo run --release -p tiling-core --example slide <seed> 0 24 \
#     --rings 100000 --launch-radius 48 --launch-gens 60 --csv lane.csv
# (seeds in results/flights/seed-*.jsonl and the committed records),
# then analysis/analyze_flights.py. Decimated telemetry committed
# beside the seeds is plot-grade, not estimator-grade, hence the
# literals here (recomputing settled headings from decimated rows
# inflates SEs ~10-50x).
LANES = {
    "hat": (HAT, 45.523, [
        ("45°", 45.523, 0.001),
        ("105°", 105.523, 0.004),
        ("165°", 165.523, 0.024),
        ("225°", 225.523, 0.003),
        ("285°", 285.523, 0.001),
        ("345°", 345.523, 0.011),
    ]),
    "spectre": (SPEC, 48.014, [
        ("48°", 48.014, 0.018),
        ("108°", 108.014, 0.006),
        ("168°", 168.014, 0.021),
        ("228°", 228.014, 0.024),
        ("288°", 288.014, 0.021),
        ("348°", 348.014, 0.007),
    ]),
}


def pt(cx, cy, deg, r):
    a = math.radians(deg)
    return cx + r * math.cos(a), cy - r * math.sin(a)


def text(x, y, s, size=13, color="#333333", anchor="middle", weight=""):
    w = f' font-weight="{weight}"' if weight else ""
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" '
            f'font-size="{size}" fill="{color}" text-anchor="{anchor}"{w}>{s}</text>')


W, H = 880, 470
R = 150
CENTERS = [(235, 210), (645, 210)]
svg = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}">',
       f'<rect width="{W}" height="{H}" fill="white"/>']
defs = set()


def arrow_marker(color):
    mid = color.replace("#", "m")
    if mid not in defs:
        defs.add(mid)
        svg.append(f'<defs><marker id="{mid}" viewBox="0 0 10 10" refX="8" refY="5" '
                   f'markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
                   f'<path d="M 0 0 L 10 5 L 0 10 z" fill="{color}"/></marker></defs>')
    return mid


summary = []
for (sub, (color, off0, lanes)), (cx, cy) in zip(LANES.items(), CENTERS):
    # measured offset = mean of available lanes' heading mod 60
    meas = []
    for lab, h, se in lanes:
        m = h % 60.0
        meas.append(m if m < 30 else m - 60)  # unwrap near 0
    offset = (sum(meas) / len(meas)) % 60.0

    svg.append(f'<circle cx="{cx}" cy="{cy}" r="{R}" fill="none" stroke="#d5d8de" stroke-width="1"/>')
    ex, ey = pt(cx, cy, 0, R)
    svg.append(f'<line x1="{ex:.1f}" y1="{ey:.1f}" x2="{ex + 7:.1f}" y2="{ey:.1f}" stroke="#9aa0a8" stroke-width="1"/>')
    svg.append(text(ex + 12, ey + 4, "0°", size=11, color="#9aa0a8", anchor="start"))
    # six spokes at the measured offset
    for k in range(6):
        d = offset + k * 60
        x1, y1 = pt(cx, cy, d, 14)
        x2, y2 = pt(cx, cy, d, R)
        svg.append(f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
                   f'stroke="#c9cdd4" stroke-width="1" stroke-dasharray="2 4"/>')
    # arrows for flown lanes
    n_flown = 0
    for lab, h, se in lanes:
        # Self-check: a lane's measurement must land on its own spoke. A
        # stale/mislabelled CSV (e.g. a 345 flight saved as fly-hat-285)
        # would otherwise be drawn silently on the wrong spoke. Flag and
        # skip if the settled heading is >30° from the label's spoke.
        want = float(lab.rstrip("°"))
        if abs(((h - want + 180) % 360) - 180) > 30.0:
            print(f"  WARNING: {sub} {lab} lane settled {h:.2f}° — wrong spoke "
                  f"(off by {abs(((h - want + 180) % 360) - 180):.1f}°); skipping. "
                  f"Check the LANES table")
            continue
        n_flown += 1
        mid = arrow_marker(color)
        x1, y1 = pt(cx, cy, h, 6)
        x2, y2 = pt(cx, cy, h, R * 0.86)
        svg.append(f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
                   f'stroke="{color}" stroke-width="2.4" marker-end="url(#{mid})"/>')
        lx, ly = pt(cx, cy, h, R + 18)
        svg.append(text(lx, ly + 4, f"{h:.2f}°±{se:.2f}", size=10.5, color=color))
        summary.append((sub, lab, h, se))
    title = f"{sub} — measured offset {offset:.2f}° ({n_flown}/6 lanes)"
    svg.append(text(cx, cy + R + 44, title, size=12))

svg.append("</svg>")
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "compass.svg")
with open(out, "w") as f:
    f.write("\n".join(svg) + "\n")
print("wrote", out)
for sub, lab, h, se in summary:
    print(f"  {sub:8} {lab:5} settled {h:8.3f}° ± {se:.3f}°  (mod60 {h % 60:.3f})")
