#!/usr/bin/env python3
"""Certificate paper: the number of legal R-balls against R, both tilings, log-log, each point
labelled with its count, with an R^2 guide (the growth linear repetitivity predicts asymptotically).
The counts are the leaf counts of the checked exploration graphs (Table of Appendix B).
Usage: fig_atlas_counts.py <out.svg>"""
import math, sys

out = sys.argv[1]
R = list(range(2, 13))
HAT = [118, 195, 287, 394, 544, 719, 897, 1081, 1282, 1504, 1733]
SPEC = [89, 165, 255, 357, 473, 620, 788, 991, 1205, 1430, 1684]

W, H = 380, 300
x0, x1, y0, y1 = 52, 322, 20, 250          # plot box (SVG y down)
lr = (math.log(2), math.log(12)); lc = (math.log(80), math.log(2000))
def sx(r): return x0 + (math.log(r) - lr[0]) / (lr[1] - lr[0]) * (x1 - x0)
def sy(c): return y1 - (math.log(c) - lc[0]) / (lc[1] - lc[0]) * (y1 - y0)
el = []
def text(x, y, t, size=7, anchor="middle", **kw):
    s = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in kw.items())
    el.append(f'<text x="{x:.1f}" y="{y:.1f}" font-size="{size}" text-anchor="{anchor}" font-family="Helvetica, Arial, sans-serif" {s}>{t}</text>')
def line(a, b, c, d, **kw):
    s = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in kw.items())
    el.append(f'<line x1="{a:.1f}" y1="{b:.1f}" x2="{c:.1f}" y2="{d:.1f}" {s}/>')

# axes and grid
el.append(f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}" fill="none" stroke="#999" stroke-width="0.6"/>')
for r in R:
    line(sx(r), y1, sx(r), y1 + 3, stroke="#999", stroke_width=0.6)
    text(sx(r), y1 + 12, str(r), 6.5, fill="#444")
for c in (100, 200, 500, 1000, 2000):
    line(x0 - 3, sy(c), x1, sy(c), stroke="#e6e6e6", stroke_width=0.6)
    text(x0 - 6, sy(c) + 2.5, f"{c:,}", 6.5, "end", fill="#444")
text((x0 + x1) / 2, y1 + 26, "atlas radius R", 7.5, font_weight="bold")
el.append(f'<text transform="translate(12,{(y0 + y1) / 2:.0f}) rotate(-90)" font-size="7.5" font-weight="bold" text-anchor="middle" font-family="Helvetica, Arial, sans-serif">legal R-balls</text>')
# R^2 guide through the spectre's R = 12 point
c12 = SPEC[-1]
r_lo = 12 * math.sqrt(80 / c12)   # where the guide enters the plot box from below
line(sx(r_lo), sy(80), sx(12), sy(c12), stroke="#333", stroke_width=0.8, stroke_dasharray="3 3")
text(sx(4.2) + 4, sy(c12 * (4.2 / 12) ** 2) + 9, "slope 2 (R²)", 6.5, "start", fill="#333")
# series
for vals, col, name, dy in ((HAT, "#d55e00", "hat", -6), (SPEC, "#0072b2", "spectre", 9)):
    pts = " ".join(f"{sx(r):.1f},{sy(c):.1f}" for r, c in zip(R, vals))
    el.append(f'<polyline points="{pts}" fill="none" stroke="{col}" stroke-width="1.2"/>')
    for r, c in zip(R, vals):
        el.append(f'<circle cx="{sx(r):.1f}" cy="{sy(c):.1f}" r="2.2" fill="{col}" stroke="white" stroke-width="0.6"/>')
        if r in (2, 5, 9, 12):
            text(sx(r) + (-4 if name == "hat" else 4), sy(c) + dy, f"{c:,}", 6, "end" if name == "hat" else "start", fill=col)
    if name == "hat": text(sx(3.6), sy(vals[2]) - 16, name, 7.5, "middle", fill=col, font_weight="bold")
    else: text(sx(8.5), sy(vals[6]) + 18, name, 7.5, "middle", fill=col, font_weight="bold")
svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">\n<rect width="{W}" height="{H}" fill="white"/>\n' + "\n".join(el) + "\n</svg>\n"
open(out, "w").write(svg)
for name, v in (("hat", HAT), ("spectre", SPEC)):
    e = [math.log(v[i + 1] / v[i]) / math.log(R[i + 1] / R[i]) for i in range(len(R) - 1)]
    print(name, "local exponents", [round(x, 2) for x in e])
