#!/usr/bin/env python3
"""Do the anti-hats form rows along the compass directions? (FINDINGS 10.11.54; certificate paper §4.1.)
Reads the radius-512 parametric table, takes a sample of anti-hats, and for each counts the anti-hats
lying within 0.8 units of the line through it in each compass direction 45.5225 + 60k, reporting the
spacings along the line; a direction 17 degrees off a spoke is the control.
Usage: antihat_rows.py [out/traces/param-hatA-r512.tsv]"""
import cmath, math, random, sys
param = sys.argv[1] if len(sys.argv) > 1 else 'out/traces/param-hatA-r512.tsv'
z = cmath.exp(1j * math.pi / 6)
def val(s):
    a, b, c, d = (int(x) for x in s.split(',')); return a + b * z + c * z**2 + d * z**3
anti = []; hats = 0
with open(param) as f:
    next(f)
    for line in f:
        c, a, r, U, V = line.rstrip('\n').split('\t')
        p = val(U) + math.sqrt(3) * val(V)
        if a.startswith('(tile antihat)'): anti.append(p)
        else: hats += 1
print(f"{len(anti)} anti-hats, {hats} hats")
lane = 45.5224878
random.seed(1); sample = random.sample([p for p in anti if abs(p) < 250], 40)   # interior: lines long in every direction
def members(p, deg):
    th = math.radians(deg); d = complex(math.cos(th), math.sin(th))
    return sorted(((q - p) / d).real for q in anti if abs(q - p) > 1 and abs(((q - p) / d).imag) < 0.8)
def report(label, deg):
    counts, nd = [], []
    for p in sample:
        xs = members(p, deg); counts.append(len(xs))
        nd.append(len(set(round(xs[i + 1] - xs[i], 1) for i in range(len(xs) - 1))))
    xs = members(sample[0], deg)
    gaps = sorted(set(round(xs[i + 1] - xs[i], 1) for i in range(len(xs) - 1)))
    c = sorted(counts); n = sorted(nd)
    print(f"{label}: members per line min {c[0]} median {c[len(c)//2]} max {c[-1]}; distinct spacings per line median {n[len(n)//2]} max {n[-1]}; e.g. {gaps[:7]}")
for k in range(6): report(f"spoke {lane + 60 * k:.4f}", lane + 60 * k)
for off in (17, 30):
    report(f"control, {off} deg off a spoke", lane + off)
