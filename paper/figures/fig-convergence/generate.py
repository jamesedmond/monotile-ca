#!/usr/bin/env python3
"""Linear-y convergence variant (v3): SIGNED residual of the running
(net) heading against claimed and refit fans, log-x / linear-y. The
running heading is smooth (1/r convergence), so no log-scale spikes:
claimed-fan curves flatten at the +0.477 deg hat floor, refit curves
converge to zero. Reads the five final flight CSVs from results/."""
import csv, math, os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..")
FLIGHTS = [
    ("results/flights/hat-A-flight.csv", "hat A", "#d55e00", 46.0, 45.523),
    ("results/flights/hat-B-flight.csv", "hat B", "#e69f00", 46.0, 45.523),
    ("results/flights/hat-C-flight.csv", "hat C (346°)", "#009e73", 46.0, 45.523),
    ("results/flights/spectre-flight-108.csv", "spectre 108°", "#cc79a7", 48.0, 48.014),
    ("results/flights/spectre-flight-348.csv", "spectre 348°", "#8c5d80", 48.0, 48.014),
]

def sresid(h, off):
    m = (h - off) % 60.0
    return m - 60.0 if m > 30.0 else m

def load(path, cl, rf):
    xs, rc, rr = [], [], []
    with open(os.path.join(ROOT, path)) as f:
        for row in csv.DictReader(f):
            try:
                e = float(row["effective_rings"]); h = float(row["heading_running"])
            except (ValueError, KeyError):
                continue
            if e < 48: continue
            xs.append(e); rc.append(sresid(h, cl)); rr.append(sresid(h, rf))
    return xs, rc, rr

W, H = 980, 600
X0, X1, Y0, Y1 = 100, 700, 540, 60
LX0, LX1 = math.log10(48), math.log10(1200000)
YMIN, YMAX = -0.9, 1.6
px = lambda x: X0 + (math.log10(x) - LX0) / (LX1 - LX0) * (X1 - X0)
py = lambda y: Y0 - (y - YMIN) / (YMAX - YMIN) * (Y0 - Y1)

def text(x, y, s, size=15, color="#333333", anchor="start", style=""):
    return (f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" '
            f'font-size="{size}" fill="{color}" text-anchor="{anchor}" {style}>{s}</text>')

s = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}">',
     f'<rect width="{W}" height="{H}" fill="white"/>']
# hat claimed floor
# 1/r decay envelopes (launch-transient scale)
env = 120.0  # deg·rings, hugging the largest transient (hat C)
pts_hi = [(x, min(env / x, 10)) for x in [48*1.2**k for k in range(60)] if x < 1200000]
for sgn in (1, -1):
    d = " ".join(f"{'M' if i == 0 else 'L'} {px(x):.1f} {py(max(min(sgn*y, YMAX), YMIN)):.1f}" for i, (x, y) in enumerate(pts_hi))
    s.append(f'<path d="{d}" stroke="#c9cdd4" stroke-width="1" stroke-dasharray="2 4" fill="none"/>')
s.append(text(px(1400), py(0.17), "∝ ±1/r", size=12, color="#9aa0a8"))
for yv in [-0.5, 0.0, 0.5, 1.0, 1.5]:
    y = py(yv)
    w = 1.4 if yv == 0.0 else 1.0
    col = "#b6bac2" if yv == 0.0 else "#e4e7eb"
    s.append(f'<line x1="{X0}" y1="{y:.1f}" x2="{X1}" y2="{y:.1f}" stroke="{col}" stroke-width="{w}"/>')
    s.append(text(X0 - 10, y + 5, f"{yv:+.1f}°" if yv else "0°", anchor="end"))
for xv in [48, 384, 3000, 30000, 300000, 1000000]:
    x = px(xv)
    s.append(f'<line x1="{x:.1f}" y1="{Y0}" x2="{x:.1f}" y2="{Y0 + 6}" stroke="#333333" stroke-width="1"/>')
    lab = "1M" if xv >= 1000000 else (f"{xv // 1000}k" if xv >= 1000 else str(xv))
    s.append(text(x, Y0 + 24, lab, anchor="middle"))
s.append(f'<line x1="{X0}" y1="{Y0}" x2="{X1}" y2="{Y0}" stroke="#333333" stroke-width="1.2"/>')
s.append(f'<line x1="{X0}" y1="{Y0}" x2="{X0}" y2="{Y1}" stroke="#333333" stroke-width="1.2"/>')
s.append(text((X0 + X1) / 2, Y0 + 52, "effective radius (rings, frame displacement × κ)", size=16, anchor="middle"))
s.append(text(0, 0, "running-heading residual (signed)", size=16, anchor="middle",
              style=f'transform="translate(32,{(Y0 + Y1) / 2:.0f}) rotate(-90)"'))

def sample(xs, ys, n=140):
    out, lo, hi = [], math.log10(xs[0]), math.log10(xs[-1])
    step = (hi - lo) / n; nextl = lo
    for x, y in zip(xs, ys):
        if math.log10(x) >= nextl:
            out.append((x, y)); nextl += step
    return out

legend = []
for path, lab, color, cl, rf in FLIGHTS:
    xs, rc, rr = load(path, cl, rf)
    if not xs: continue
    pts = sample(xs, rr)
    d = " ".join(f"{'M' if i == 0 else 'L'} {px(x):.1f} {py(max(min(y, YMAX), YMIN)):.1f}" for i, (x, y) in enumerate(pts))
    s.append(f'<path d="{d}" stroke="{color}" stroke-width="1.8" fill="none" opacity="0.9"/>')
    legend.append((lab, color))

LX, LY = 715, 70
s.append(f'<rect x="{LX}" y="{LY}" width="250" height="{46 + len(legend) * 20}" rx="6" fill="white" stroke="#d5d8de"/>')
s.append(text(LX + 12, LY + 20, "residual vs measured fan", size=12))
s.append(text(LX + 12, LY + 36, "(hat 45.523°+60k, spectre 48.014°+60k)", size=10.5, color="#6d737c"))
y = LY + 58
for lab, color in legend:
    s.append(f'<line x1="{LX + 12}" y1="{y}" x2="{LX + 40}" y2="{y}" stroke="{color}" stroke-width="2.2"/>')
    s.append(text(LX + 48, y + 4, lab, size=12.5))
    y += 20
s.append(text((X0 + X1) / 2, Y1 - 18,
              "Launch transients decay as 1/r onto the fan; no floor: the lanes sit on the measured fan exactly (SE 0.003–0.011°).",
              size=12.5, color="#6d737c", anchor="middle"))
s.append("</svg>")
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "convergence.svg")
open(out, "w").write("\n".join(s) + "\n")
print("wrote", out)
