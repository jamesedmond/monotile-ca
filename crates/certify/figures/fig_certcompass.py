#!/usr/bin/env python3
"""Certificate paper, the compass figure: the rigorous heading intervals of Theorem 2 against the
flight-measured lanes, both tilings, both spectre lane families.

(a) Hat rose: six spokes at hat A's count-weighted return heading, 45.5225 + 60k; arrows for the
    six flight-measured lanes of the earlier paper (net-displacement heading of each sliding-window
    flight; the uncertainty is its difference from the second half's heading); small markers for the three gliders from outside the
    census (Section 4) on their spokes.
(b) Spectre rose: the glider family's six spokes at 48.0138 + 60k (solid), the net heading of the
    million-generation lane-108 flight, and the second family's at 57.1700 + 60k (dashed), the net
    heading of its million-generation flight; with the earlier paper's six measured lanes.
All headings are from the committed flights in results/flights/ (no ten-million-generation data).
(c) Zoom strips, one per lane family: the rigorous interval of each certificate (mod 60, every spoke
    carries the same interval since the keys are rotation-blind), the exact value, and every measured
    heading reduced mod 60 with its standard error.

Usage: fig_certcompass.py <out.svg>   (run from the repository root; reads results/flights/*.csv)
"""
import csv, math, os, sys

out = sys.argv[1]
ROOT = os.getcwd()

HAT_EXACT = 45.522493     # hat A, count-weighted return heading over 10^6 generations (certify trace), mod 60
SPEC1_EXACT = 48.0138     # spectre glider, net displacement of the public 10^6-generation lane-108 flight, mod 60
SPEC2_EXACT = 57.1700     # second family, net displacement of the public 10^6-generation lane-177 flight, mod 60

# Theorem 2 intervals, mod 60 (Table 2 quotes them mod 30; the hat's and the spectre glider's lanes
# lie in the first half-turn of each spoke pair).
HAT_INT = (45.4895, 45.6084)          # hat A, hat B, captured, hat C (both), wake, s33-on-hat
WORM_INT = (45.4895, 45.6086)         # Trevorrow's worm
SPEC1_INT = (47.7448, 48.1373)        # spectre glider
SPEC2_INT = (56.9589, 57.5125)        # second family

FLIGHTS = {
    "hat": [
        ("45°", "results/flights/fly-hat-45.csv"),
        ("105°", "results/flights/fly-hat-105.csv"),
        ("165°", "results/flights/fly-hat-165.csv"),
        ("225°", "results/flights/hat-A-flight.csv"),
        ("285°", "results/flights/fly-hat-285.csv"),
        ("345°", "results/flights/hat-C-flight.csv"),
    ],
    "spectre": [
        ("48°", "results/flights/fly-spec-48.csv"),
        ("108°", "results/flights/spectre-flight-108.csv"),
        ("168°", "results/flights/fly-spec-168.csv"),
        ("228°", "results/flights/fly-spec-228.csv"),
        ("288°", "results/flights/fly-spec-288.csv"),
        ("348°", "results/flights/spectre-flight-348.csv"),
    ],
}
# the second spectre family: the 10^6-generation flight of lane 177 (results/flights/spectre-second-lane177-1M.csv)
SPEC2_FLIGHTS = [("177°", 177.16995, 0.0001)]
# gliders from outside the census (Section 4): patch-scale fits, radius 384
OUTSIDE = [("worm", 225.5237), ("worm", 45.5189), ("wake", 345.5240), ("s33", 225.5121)]


def settled(path):
    """Net-displacement heading of the whole flight (the statistic of the companion paper's Table 7),
    with the heading of the second half of the flight as an internal check; the quoted uncertainty is
    their difference. The windowed-heading column of the decimated telemetry is not used: its late-half
    mean is biased by up to 0.2 degrees on some lanes (the window aliases the corridor's rhythm)."""
    p = os.path.join(ROOT, path)
    rows = list(csv.DictReader(open(p)))
    xy = [(float(r["global_x"]), float(r["global_y"])) for r in rows]
    full = math.degrees(math.atan2(xy[-1][1], xy[-1][0])) % 360
    mid = xy[len(xy) // 2]
    half = math.degrees(math.atan2(xy[-1][1] - mid[1], xy[-1][0] - mid[0])) % 360
    d = (full - half + 180) % 360 - 180
    return full, abs(d)


measured = {k: [(lab, *settled(p)) for lab, p in v] for k, v in FLIGHTS.items()}

# ---------------------------------------------------------------- SVG
W, H = 380, 440   # designed at the Complex Systems text width (25.5 pc), so labels keep their size
el = []
def line(x1, y1, x2, y2, **kw):
    s = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in kw.items())
    el.append(f'<line x1="{x1:.2f}" y1="{y1:.2f}" x2="{x2:.2f}" y2="{y2:.2f}" {s}/>')
def text(x, y, t, size=9, anchor="middle", **kw):
    s = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in kw.items())
    el.append(f'<text x="{x:.2f}" y="{y:.2f}" font-size="{size}" text-anchor="{anchor}" font-family="Helvetica, Arial, sans-serif" {s}>{t}</text>')
def circle(x, y, r, **kw):
    s = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in kw.items())
    el.append(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r}" {s}/>')
def rect(x, y, w, h, **kw):
    s = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in kw.items())
    el.append(f'<rect x="{x:.2f}" y="{y:.2f}" width="{w:.2f}" height="{h:.2f}" {s}/>')

def pol(cx, cy, r, deg):
    a = math.radians(deg)
    return cx + r * math.cos(a), cy - r * math.sin(a)

def rose(cx, cy, R, title, families, arrows, extra):
    circle(cx, cy, R, fill="none", stroke="#999", stroke_width=0.6)
    for k in range(12):
        x, y = pol(cx, cy, R, 30 * k); x2, y2 = pol(cx, cy, R + 4, 30 * k)
        line(x, y, x2, y2, stroke="#999", stroke_width=0.6)
        if k % 3 == 0:
            tx, ty = pol(cx, cy, R + 11, 30 * k)
            text(tx, ty + 2.5, f"{30*k}°", 6.5, fill="#666")
    for exact, dash in families:
        for k in range(6):
            x, y = pol(cx, cy, R, exact + 60 * k)
            kw = dict(stroke="#333", stroke_width=0.9)
            if dash: kw["stroke_dasharray"] = "4 3"
            line(cx, cy, x, y, **kw)
    for lab, h, se in arrows:
        x, y = pol(cx, cy, R * 0.86, h)
        line(cx, cy, x, y, stroke="#d55e00", stroke_width=2.2, stroke_linecap="round")
        rr = R * (0.30 if lab.endswith("177°") else 0.62)
        tx, ty = pol(cx, cy, rr, h + 11)
        text(tx, ty + 2.5, f"{h:.2f}°", 6, fill="#d55e00")
    for lab, h in extra:
        x, y = pol(cx, cy, R * 0.93, h)
        circle(x, y, 2.6, fill="white", stroke="#0072b2", stroke_width=1.2)
    text(cx, cy + R + 22, title, 8.5, font_weight="bold")

rose(98, 100, 74, "(a) hat", [(HAT_EXACT, False)], measured["hat"], OUTSIDE)
rose(282, 100, 74, "(b) spectre", [(SPEC1_EXACT, False), (SPEC2_EXACT, True)],
     measured["spectre"] + SPEC2_FLIGHTS, [])
# legend, one row
ly = 218
for lx, kind, lab in [(14, "exact", "lane heading (long flight)"), (104, "dash", "second spectre family"),
                      (200, "meas", "measured lane (flight)"), (296, "out", "outside glider (§4)")]:
    if kind == "exact": line(lx, ly, lx + 14, ly, stroke="#333", stroke_width=0.9)
    elif kind == "dash": line(lx, ly, lx + 14, ly, stroke="#333", stroke_width=0.9, stroke_dasharray="3 2")
    elif kind == "meas": line(lx, ly, lx + 14, ly, stroke="#d55e00", stroke_width=2.0)
    else: circle(lx + 7, ly, 2.6, fill="white", stroke="#0072b2", stroke_width=1.2)
    text(lx + 18, ly + 2.5, lab, 6.5, "start")

# ---------------------------------------------------------------- zoom strips
def strip(y, title, exact, intervals, points, lo, hi, outside=()):
    x0, x1 = 70, 368
    sx = lambda d: x0 + (d - lo) / (hi - lo) * (x1 - x0)
    text(x0 - 6, y + 3, title, 7, "end", font_weight="bold")
    line(x0, y, x1, y, stroke="#999", stroke_width=0.6)
    for t in [lo + i * 0.1 for i in range(int(round((hi - lo) / 0.1)) + 1)]:
        line(sx(t), y - 3, sx(t), y + 3, stroke="#999", stroke_width=0.6)
        text(sx(t), y + 11, f"{t:.1f}", 6, fill="#666")
    for i, (name, (a, b)) in enumerate(intervals):
        yy = y - 9 - 6 * i
        rect(sx(a), yy - 2.2, sx(b) - sx(a), 4.4, fill="#cfe3f3", stroke="#0072b2", stroke_width=0.6)
        text(sx(b) + 3, yy + 2.2, name, 6, "start", fill="#0072b2")
    line(sx(exact), y - 20, sx(exact), y + 5, stroke="#333", stroke_width=1.0)
    text(sx(exact), y - 23, f"long flight {exact:.4f}°", 6.5, fill="#333")
    for lab, h, se in points:
        hm = h % 60
        line(sx(hm - se), y, sx(hm + se), y, stroke="#d55e00", stroke_width=1.2)
        circle(sx(hm), y, 2.2, fill="#d55e00", stroke="none")
    for lab, h in outside:
        circle(sx(h % 60), y + 7, 2.6, fill="white", stroke="#0072b2", stroke_width=1.1)

text(190, 250, "(c) intervals of Theorem 2, long-flight headings, measured headings, mod 60°", 8.5, font_weight="bold")
strip(296, "hat", HAT_EXACT, [("A, B, C, wake, s33", HAT_INT), ("worm", WORM_INT)], measured["hat"], 45.2, 45.9, OUTSIDE)
strip(356, "spectre, glider", SPEC1_EXACT, [("glider", SPEC1_INT)], measured["spectre"], 47.6, 48.3)
strip(416, "spectre, 2nd", SPEC2_EXACT, [("second family", SPEC2_INT)], SPEC2_FLIGHTS, 56.8, 57.6)

svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">\n<rect width="{W}" height="{H}" fill="white"/>\n' + "\n".join(el) + "\n</svg>\n"
open(out, "w").write(svg)
for k, v in measured.items():
    for lab, h, se in v:
        print(f"{k} {lab}: {h:.4f} ± {se:.4f}  (mod 60: {h % 60:.4f})")
