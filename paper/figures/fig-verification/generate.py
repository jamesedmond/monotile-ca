#!/usr/bin/env python3
"""Verification figure: generations to reach radius r, log-log.
Monolithic boundary arrivals to radius 768, continued by sliding-window
flights (effective radius) to one million rings; the mega-looper foil
terminates at its capture. The grower foil was dropped from this figure
(hidden behind the phoenix at this scale) - it remains in the prose.

A genuine glider is ballistic: doubling the radius doubles the arrival
time, a straight line of slope one on these axes. The verified gliders
form parallel lines (the shared ~2 generations/ring relay clock; the
phoenix at ~1), each labeled with its flat peak population. The foils
show the standard discriminating: the grower arrives on time but its
population explodes (flatness violated); the mega-looper's line simply
ends -- at radius 192 it locks into an exact period-3300 orbit and
never arrives.

Data provenance: FINDINGS.md sections 9.5/9.6/10 (monolithic points;
reproduce with tiling-core --example verify_candidate) and section 10.8
(flights; results/flights/*.csv, regenerable via --example slide).
Writes verification.svg beside this script.
"""

import math
import os

# (label, color, marker, [(radius, generations)...], peak-pop note)
GLIDERS = [
    ("Goucher (P3)", "#0072b2", "circle",
     [(24, 41), (48, 89), (96, 185), (120, 239)], "pop 10"),
    ("phoenix (P3)", "#56b4e9", "tri_down",
     [(48, 48), (96, 96), (192, 191), (384, 385)], "pop 32"),
    ("hat A", "#d55e00", "circle",
     [(48, 89), (96, 183), (192, 377), (384, 763), (768, 1533)], "pop 16"),
    ("hat B", "#e69f00", "square",
     [(48, 89), (96, 183), (192, 377), (384, 763), (768, 1533)], "pop 14"),
    ("hat C", "#009e73", "diamond",
     [(48, 158), (96, 254), (192, 446), (384, 834), (768, 1602)], "pop 32"),
    ("spectre", "#cc79a7", "tri_up",
     [(48, 89), (96, 185), (192, 378), (384, 765), (768, 1534)], "pop 12"),
]
import csv as _csv
_FLIGHT = {
    "hat A": "results/flights/hat-A-flight.csv",
    "hat B": "results/flights/hat-B-flight.csv",
    "hat C": "results/flights/hat-C-flight.csv",
    "spectre": "results/flights/spectre-flight-108.csv",
}
_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..")
def _flight_points(path):
    rows = []
    with open(os.path.join(_ROOT, path)) as f:
        for r in _csv.DictReader(f):
            try:
                rows.append((float(r["effective_rings"]), float(r["generation"])))
            except (ValueError, KeyError):
                pass
    pts = []
    for t in [1536 * 2 ** i for i in range(10)]:
        best = min(rows, key=lambda p: abs(p[0] - t))
        if abs(best[0] - t) / t < 0.5:
            pts.append((best[0], best[1]))
    return pts

GLIDERS = [(lab, c, mk, pts + (_flight_points(_FLIGHT[lab]) if lab in _FLIGHT else []), pop)
           for lab, c, mk, pts, pop in GLIDERS]

GROWER = ("spectre grower", [(48, 46), (96, 94), (192, 191)],
          "pop 50 → 207 → 1165: rejected (not flat)")
LOOPER = ("spectre looper", [(48, 166), (96, 286)],
          "never arrives at 192: exact period 3300")
GREY = "#8a8f98"

W, H = 900, 640
X0, X1 = 100, 830
Y0, Y1 = 550, 60
LX0, LX1 = math.log2(24), math.log2(1_600_000)
LY0, LY1 = 5.0, 21.5  # 32 .. ~3M


def px(x):
    return X0 + (math.log2(x) - LX0) / (LX1 - LX0) * (X1 - X0)


def py(y):
    return Y0 - (math.log2(y) - LY0) / (LY1 - LY0) * (Y0 - Y1)


def marker(kind, x, y, color, size=5.5, fill=True):
    f = color if fill else "#ffffff"
    if kind == "circle":
        return f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{size}" fill="{f}" stroke="{color}" stroke-width="1.6"/>'
    if kind == "square":
        s = size * 0.9
        return f'<rect x="{x - s:.1f}" y="{y - s:.1f}" width="{2 * s:.1f}" height="{2 * s:.1f}" fill="{f}" stroke="{color}" stroke-width="1.6"/>'
    if kind == "diamond":
        s = size * 1.2
        pts = f"{x:.1f},{y - s:.1f} {x + s:.1f},{y:.1f} {x:.1f},{y + s:.1f} {x - s:.1f},{y:.1f}"
        return f'<polygon points="{pts}" fill="{f}" stroke="{color}" stroke-width="1.6"/>'
    if kind == "tri_up":
        s = size * 1.25
        pts = f"{x:.1f},{y - s:.1f} {x + s:.1f},{y + s * 0.8:.1f} {x - s:.1f},{y + s * 0.8:.1f}"
        return f'<polygon points="{pts}" fill="{f}" stroke="{color}" stroke-width="1.6"/>'
    if kind == "tri_down":
        s = size * 1.25
        pts = f"{x:.1f},{y + s:.1f} {x + s:.1f},{y - s * 0.8:.1f} {x - s:.1f},{y - s * 0.8:.1f}"
        return f'<polygon points="{pts}" fill="{f}" stroke="{color}" stroke-width="1.6"/>'
    if kind == "cross":
        s = size
        return (f'<path d="M {x - s:.1f} {y - s:.1f} L {x + s:.1f} {y + s:.1f} '
                f'M {x - s:.1f} {y + s:.1f} L {x + s:.1f} {y - s:.1f}" '
                f'stroke="{color}" stroke-width="2.2" fill="none"/>')
    if kind == "plus":
        s = size * 1.2
        return (f'<path d="M {x - s:.1f} {y:.1f} L {x + s:.1f} {y:.1f} '
                f'M {x:.1f} {y - s:.1f} L {x:.1f} {y + s:.1f}" '
                f'stroke="{color}" stroke-width="2.2" fill="none"/>')
    raise ValueError(kind)


def text(x, y, s, size=15, color="#333333", anchor="start", style=""):
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" '
            f'font-size="{size}" fill="{color}" text-anchor="{anchor}" {style}>{s}</text>')


svg = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}">',
       f'<rect width="{W}" height="{H}" fill="white"/>']

# Gridlines + ticks.
for yv in [32, 256, 2048, 16384, 131072, 1048576]:
    y = py(yv)
    svg.append(f'<line x1="{X0}" y1="{y:.1f}" x2="{X1}" y2="{y:.1f}" stroke="#eceef1" stroke-width="1"/>')
    lab = f"{yv // 1024}k" if 1024 <= yv < 1048576 else ("1M" if yv >= 1048576 else str(yv))
    svg.append(text(X0 - 10, y + 5, lab, anchor="end"))
for xv in [24, 96, 384, 1536, 6144, 24576, 98304, 393216, 1000000]:
    x = px(xv)
    svg.append(f'<line x1="{x:.1f}" y1="{Y0}" x2="{x:.1f}" y2="{Y0 + 6}" stroke="#333333" stroke-width="1"/>')
    lab = "1M" if xv >= 1000000 else (f"{round(xv/1000)}k" if xv >= 1000 else str(xv))
    svg.append(text(x, Y0 + 24, lab, anchor="middle"))
svg.append(f'<line x1="{X0}" y1="{Y0}" x2="{X1}" y2="{Y0}" stroke="#333333" stroke-width="1.2"/>')
svg.append(f'<line x1="{X0}" y1="{Y0}" x2="{X0}" y2="{Y1}" stroke="#333333" stroke-width="1.2"/>')
svg.append(text((X0 + X1) / 2, Y0 + 52, "radius r (rings; effective radius for flights)", size=17, anchor="middle"))
svg.append(text(0, 0, "generations to reach radius r", size=17, anchor="middle",
                style=f'transform="translate(38,{(Y0 + Y1) / 2:.0f}) rotate(-90)"'))

# Slope-one guides: gen = 2r and gen = r, labels floated just below
# their lines (the guides coincide with data bundles, so above-line
# labels would strike through them).
for k, lab, fac in [(2.0, "2 generations / ring", 2.4), (1.0, "1 generation / ring", 2.2)]:
    xa = max(24, 32 / k)
    xb = min(1_000_000, 2_960_000 / k)
    svg.append(f'<line x1="{px(xa):.1f}" y1="{py(k * xa):.1f}" x2="{px(xb):.1f}" y2="{py(k * xb):.1f}" '
               f'stroke="#c9cdd4" stroke-width="1" stroke-dasharray="2 4"/>')
    xl = 2000 if k == 2.0 else 24000
    dy = -20 if k == 2.0 else 18
    svg.append(text(px(xl), py(k * xl) + dy, lab, size=12.5, color="#9aa0a8", anchor="middle"))

# Foils first (under the colored lines). Both are grey and dashed, so
# the point marker carries the distinction: plus = grower, x = looper
# (the x matches the looper's never-arrives endmark).
for (label, pts, note), mk in [(LOOPER, "cross")]:
    path = " ".join(f"{'M' if i == 0 else 'L'} {px(r):.1f} {py(g):.1f}" for i, (r, g) in enumerate(pts))
    svg.append(f'<path d="{path}" stroke="{GREY}" stroke-width="1.8" stroke-dasharray="6 4" fill="none"/>')
    for r, g in pts:
        svg.append(marker(mk, px(r), py(g), GREY))
# Looper: dashed continuation to a cross at the top edge over x=192
# (it never arrives there; the legend carries the explanation).
r, g = LOOPER[1][-1]
svg.append(f'<path d="M {px(r):.1f} {py(g):.1f} L {px(192):.1f} {Y1 + 14:.1f}" '
           f'stroke="{GREY}" stroke-width="1.2" stroke-dasharray="2 5" fill="none"/>')
svg.append(marker("cross", px(192), Y1 + 10, GREY, size=7))

# Glider lines.
for label, color, mk, pts, pop in GLIDERS:
    path = " ".join(f"{'M' if i == 0 else 'L'} {px(r):.1f} {py(g):.1f}" for i, (r, g) in enumerate(pts))
    svg.append(f'<path d="{path}" stroke="{color}" stroke-width="2.2" fill="none"/>')
    hollow = label == "hat B"  # coincides with hat A; hollow markers keep both visible
    for r, g in pts:
        svg.append(marker(mk, px(r), py(g), color, fill=not hollow))

# Legend box (bottom right; the plot is empty there — the phoenix line
# passes just above its top edge).
LEG_X, LEG_Y, LEG_W = 556, 352, 272
ROW = 20
rows = [(label, color, mk, label == "hat B", False, f"{label} — {pop}, flat")
        for label, color, mk, _, pop in GLIDERS]
foil_rows = [
    ("looper", GREY, "cross", False, True, "looper — period 3300, never arrives"),
]
LEG_H = 14 + len(rows) * ROW + 8 + len(foil_rows) * ROW + 8
svg.append(f'<rect x="{LEG_X}" y="{LEG_Y}" width="{LEG_W}" height="{LEG_H}" rx="6" '
           f'fill="white" stroke="#d5d8de" stroke-width="1"/>')
y = LEG_Y + 14
for _, color, mk, hollow, dashed, lab in rows + [None] + foil_rows if False else []:
    pass
entries = rows + ["gap"] + foil_rows
for e in entries:
    if e == "gap":
        y += 8
        continue
    _, color, mk, hollow, dashed, lab = e
    dash = ' stroke-dasharray="6 4"' if dashed else ""
    svg.append(f'<line x1="{LEG_X + 12}" y1="{y:.1f}" x2="{LEG_X + 46}" y2="{y:.1f}" '
               f'stroke="{color}" stroke-width="2.2"{dash}/>')
    svg.append(marker(mk, LEG_X + 29, y, color, size=4.8, fill=not hollow))
    svg.append(text(LEG_X + 54, y + 4.5, lab, size=13, color="#333333"))
    y += ROW

svg.append("</svg>")
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "verification.svg")
with open(out, "w") as f:
    f.write("\n".join(svg) + "\n")
print("wrote", out)
