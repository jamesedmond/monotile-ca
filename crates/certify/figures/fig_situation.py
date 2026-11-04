#!/usr/bin/env python3
"""Paper 2, Figure 2: a situation and two of its legal extensions.
(a) the anatomy of a situation: support cells filled by state, the k-ring collar shaded, the centre
    cell outlined heavily with its radius rho stated;
(b) the same situation embedded in two legal balls of the atlas (left, right): after one step of
    the rule the successor pattern is the same in both (filled by state; the old support dashed),
    but the successor's collar (shaded) reaches tiles on which the two balls differ, in arrangement
    or only in class label; those tiles are hatched and, inside the successor's collar, labelled with
    their class, so the two balls lead to two different successor situations.
Usage: fig_situation.py <situation-dump.json> <out.svg> [scale=9] [ring_a=6] [ring_b=8] [--no-anatomy]
The JSON comes from `certify situation-dump <cert> <record> --state S --balls b1,b2 --out ...`.
Greyscale-safe: state fills are luminance-ordered (deep, medium, pale); differences are hatched and
labelled, not coloured."""
import json, sys

src, out = sys.argv[1:3]
SC = float(sys.argv[3]) if len(sys.argv) > 3 else 9.0
RING_A = int(sys.argv[4]) if len(sys.argv) > 4 else 6
RING_B = int(sys.argv[5]) if len(sys.argv) > 5 else 8
NO_ANATOMY = '--no-anatomy' in sys.argv   # certificate paper: the two extensions only (the anatomy has its own figure)
STATE_FILL = {1: '#d55e00', 2: '#5fb878', 3: '#b7cde8'}      # as the earlier paper's figures
COLLAR = '#e4e4e4'; COLLAR1 = '#efe6c8'; EDGE = '#444'; DIFF = '#8b1a1a'

d = json.load(open(src))
A, B = d['balls']
k, rho, st = d['k'], d['rho'], d['state']

def ckey(c):
    xs = [p[0] for p in c['xy']]; ys = [p[1] for p in c['xy']]
    return (round(sum(xs) / len(xs), 1), round(sum(ys) / len(ys), 1))

ka = {ckey(c): c for c in A['cells']}; kb = {ckey(c): c for c in B['cells']}
def differs(c, other):
    """True if the tile at c's position is absent or of another class in the other ball."""
    o = other.get(ckey(c))
    return o is None or o['cls'] != c['cls']

svg = []
def poly(c, fill, ox, oy, stroke=EDGE, sw=0.4, dash=None, extra=''):
    pts = ' '.join(f'{(x + ox) * SC:.1f},{-(y + oy) * SC:.1f}' for x, y in c['xy'])
    da = f' stroke-dasharray="{dash}"' if dash else ''
    return f'<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{da}{extra}/>'
FS = 2.8                                   # font scale: the drawing is ~1500 units wide and prints at 482 pt
def text(x, y, s, size=8, anchor='middle', weight='normal', fill='#111', halo=True):
    h = f' stroke="white" stroke-width="{2.5 * FS:.1f}" paint-order="stroke"' if halo else ''
    size = size * FS
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" font-size="{size:.1f}" text-anchor="{anchor}" font-weight="{weight}" fill="{fill}"{h}>{s}</text>'
def bbox(cells):
    xs = [x for c in cells for x, _ in c['xy']]; ys = [y for c in cells for _, y in c['xy']]
    return min(xs), max(xs), min(ys), max(ys)
def cen(c):
    xs = [p[0] for p in c['xy']]; ys = [p[1] for p in c['xy']]
    return sum(xs) / len(xs), sum(ys) / len(ys)

defs = ('<defs><pattern id="hatch" patternUnits="userSpaceOnUse" width="4" height="4" patternTransform="rotate(45)">'
        '<line x1="0" y1="0" x2="0" y2="4" stroke="#8b1a1a" stroke-width="1"/></pattern></defs>')

# ---------- (a) anatomy ----------
cells_a = [c for c in A['cells'] if c['d'] <= RING_A]
x0, x1, y0, y1 = bbox(cells_a); ox, oy = -x0, -y1
pa = []
for c in sorted(cells_a, key=lambda c: (c['sup'], c['col'])):
    fill = STATE_FILL[c['s0']] if c['sup'] else (COLLAR if c['col'] else 'white')
    pa.append(poly(c, fill, ox, oy))
centre = next(c for c in A['cells'] if c['i'] == A['centre'])
pa.append(poly(centre, 'none', ox, oy, stroke='#111', sw=2.2))
cx, cy = cen(centre)
pa.append(text((cx + ox) * SC, -(cy + oy) * SC - 22, 'centre', 8, weight='bold'))
nsup = sum(1 for c in A['cells'] if c['sup']); ncol = sum(1 for c in A['cells'] if c['col'] and not c['sup'])
wa, ha = (x1 - x0) * SC, (y1 - y0) * SC
# (the support/collar counts are stated in the LaTeX caption, not drawn: in-figure text is unreadable at page width)

# ---------- (b) two extensions ----------
panels = []
for ball, other, tag in ((A, kb, 'left'), (B, ka, 'right')):
    cells = [c for c in ball['cells'] if c['d'] <= RING_B]
    bx0, bx1, by0, by1 = bbox(cells); box_, boy_ = -bx0, -by1
    pb = []
    for c in sorted(cells, key=lambda c: (c['col1'], c['s1'] != 0)):
        base = STATE_FILL[c['s1']] if c['s1'] else (COLLAR1 if c['col1'] else 'white')
        pb.append(poly(c, base, box_, boy_))
        if differs(c, other) and not c['s1']:
            pb.append(poly(c, 'url(#hatch)', box_, boy_, stroke=DIFF, sw=1.4 if c['col1'] else 0.6))
    for c in cells:                                   # the old support, dashed
        if c['sup']:
            pb.append(poly(c, 'none', box_, boy_, stroke='#111', sw=1.1, dash='2.5,2'))
    for c in cells:                                   # class labels on differing tiles inside the successor's collar
        if differs(c, other) and c['col1'] and not c['s1']:
            cx, cy = cen(c)
            pb.append(poly(c, 'none', box_, boy_, stroke=DIFF, sw=2.4))
            pb.append(text((cx + box_) * SC, -(cy + boy_) * SC - 26, c['cls'], 10, weight='bold', fill=DIFF))
    ndiff = sum(1 for c in ball['cells'] if differs(c, other)); ndiff1 = sum(1 for c in ball['cells'] if differs(c, other) and c['col1'])
    title = f"ball {ball['ball']} → situation {ball['successor']}"
    panels.append((pb, (bx1 - bx0) * SC, (by1 - by0) * SC, title, ndiff, ndiff1))

wb = sum(p[1] for p in panels) + 30; hb = max(p[2] for p in panels)
if NO_ANATOMY:
    W = wb + 24; H = hb + 120
    svg.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">')
    svg.append('<rect width="100%" height="100%" fill="white"/>' + defs)
    x = 12.0
    svg.append('<g transform="translate(12,0)">' + text(0, 30, f'situation {st} in two legal 9-balls, after one step', 10, 'start', 'bold', halo=False) + '</g>')
    ytop = 52
else:
    W = max(wa, wb) + 24; H = ha + hb + 150
    svg.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">')
    svg.append('<rect width="100%" height="100%" fill="white"/>' + defs)
    svg.append('<g transform="translate(12,40)">' + text(0, -12, f'(a)  situation {st} of the hat A certificate', 10, 'start', 'bold', halo=False) + ''.join(pa) + '</g>')
    x = 12.0; ytop = ha + 110
    svg.append(f'<g transform="translate(12,{ytop - 20:.0f})">' + text(0, -12, '(b)  the same situation in two legal 9-balls, after one step', 10, 'start', 'bold', halo=False) + '</g>')
for pb, w, h, title, ndiff, ndiff1 in panels:
    svg.append(f'<g transform="translate({x:.0f},{ytop + 24:.0f})">' + text(0, -6, title, 9, 'start', 'bold', halo=False)
               + f'<g transform="translate(0,12)">' + ''.join(pb) + '</g>'
               + '</g>')
    x += w + 30
svg.append('</svg>')
open(out, 'w').write('\n'.join(svg))
print(f'{out}: state {st}, balls {A["ball"]}->{A["successor"]} and {B["ball"]}->{B["successor"]}; differing tiles {panels[0][4]}/{panels[1][4]}, in successor collar {panels[0][5]}/{panels[1][5]}')
