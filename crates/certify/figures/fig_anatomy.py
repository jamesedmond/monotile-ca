#!/usr/bin/env python3
"""Certificate paper: the vocabulary of Definition 1 drawn on one situation.
(a) situation 160 of hat A's certificate: the support (non-quiescent cells, filled by state), the
    k = 4 collar shaded by ring from the support (ring 1 darkest), the centre cell outlined, the tiles
    beyond the collar drawn white (not part of the situation).
(b) the same situation rotated by 120 degrees: the key records neither coordinates nor orientation,
    so it is the same situation.
Rings are edge-adjacency distances, computed here from the polygons (two tiles are edge-adjacent when
they share an edge, i.e. at least two vertices).
Usage: fig_anatomy.py <situation-dump.json> <out.svg> [scale=9] [ring_shown=6] [--no-rotated]
The JSON is the one behind the extensions figure: `certify situation-dump <cert> <record> --state S --balls ...`."""
import json, math, sys, collections

src, out = sys.argv[1:3]
SC = float(sys.argv[3]) if len(sys.argv) > 3 else 9.0
RING = int(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] != '--no-rotated' else 6
NO_ROT = '--no-rotated' in sys.argv
STATE_FILL = {1: '#d55e00', 2: '#5fb878', 3: '#b7cde8'}
RING_FILL = {1: '#9a9a9a', 2: '#b4b4b4', 3: '#cdcdcd', 4: '#e4e4e4'}
EDGE = '#444'

d = json.load(open(src))
A = d['balls'][0]
k, rho, st = d['k'], d['rho'], d['state']
cells = [c for c in A['cells'] if c['d'] <= RING]

# edge adjacency from the polygons
def vkey(p): return (round(p[0], 4), round(p[1], 4))
byv = collections.defaultdict(set)
for i, c in enumerate(cells):
    for p in c['xy']: byv[vkey(p)].add(i)
adj = collections.defaultdict(collections.Counter)
for s in byv.values():
    for i in s:
        for j in s:
            if i != j: adj[i][j] += 1
nbr = {i: [j for j, n in adj[i].items() if n >= 2] for i in range(len(cells))}
ring = {i: 0 for i, c in enumerate(cells) if c['sup']}
frontier = list(ring)
r = 0
while frontier:
    r += 1; nxt = []
    for i in frontier:
        for j in nbr[i]:
            if j not in ring: ring[j] = r; nxt.append(j)
    frontier = nxt
col_ok = all((ring.get(i, 99) <= k) == bool(c['col']) for i, c in enumerate(cells) if not c['sup'])

def rot(xy, deg):
    a = math.radians(deg); ca, sa = math.cos(a), math.sin(a)
    return [(x * ca - y * sa, x * sa + y * ca) for x, y in xy]
def poly(pts, fill, ox, oy, stroke=EDGE, sw=0.4):
    s = ' '.join(f'{(x + ox) * SC:.1f},{-(y + oy) * SC:.1f}' for x, y in pts)
    return f'<polygon points="{s}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>'
FS = 2.6
def text(x, y, s, size=8, anchor='middle', weight='normal', fill='#111', halo=True):
    h = f' stroke="white" stroke-width="{2.5 * FS:.1f}" paint-order="stroke"' if halo else ''
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" font-size="{size * FS:.1f}" text-anchor="{anchor}" font-weight="{weight}" fill="{fill}"{h}>{s}</text>'
def bbox(polys):
    xs = [x for p in polys for x, _ in p]; ys = [y for p in polys for _, y in p]
    return min(xs), max(xs), min(ys), max(ys)
def cen(pts):
    return sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts)

def panel(deg):
    polys = [rot(c['xy'], deg) for c in cells]
    x0, x1, y0, y1 = bbox(polys); ox, oy = -x0, -y1
    out = []
    order = sorted(range(len(cells)), key=lambda i: (cells[i]['sup'], -ring.get(i, 99)))
    for i in order:
        c = cells[i]
        if c['sup']: fill = STATE_FILL[c['s0']]
        elif ring.get(i, 99) <= k: fill = RING_FILL[ring[i]]
        else: fill = 'white'
        out.append(poly(polys[i], fill, ox, oy, sw=0.5 if ring.get(i, 99) <= k or c['sup'] else 0.3,
                        stroke=EDGE if ring.get(i, 99) <= k or c['sup'] else '#bbb'))
    ci = next(i for i, c in enumerate(cells) if c['i'] == A['centre'])
    out.append(poly(polys[ci], 'none', ox, oy, stroke='#111', sw=2.4))
    return out, (x1 - x0) * SC, (y1 - y0) * SC, ox, oy, polys, ci

pa, wa, ha, oxa, oya, polys_a, ci = panel(0)
pb, wb, hb, oxb, oyb, polys_b, cib = panel(120)
SB = 0.55   # the rotated copy is drawn smaller
cx, cy = cen(polys_a[ci])
pa.append(text((cx + oxa) * SC, -(cy + oya) * SC - 20, 'centre', 8, weight='bold'))

# legend (drawn to the right of panel a)
lg = []
lx, ly = wa + 40, 30
def swatch(y, fill, label, stroke=EDGE):
    lg.append(f'<rect x="{lx:.0f}" y="{y:.0f}" width="34" height="22" fill="{fill}" stroke="{stroke}" stroke-width="0.8"/>')
    lg.append(text(lx + 44, y + 17, label, 8, 'start', halo=False))
swatch(ly, STATE_FILL[1], 'support, state 1'); swatch(ly + 30, STATE_FILL[2], 'support, state 2'); swatch(ly + 60, STATE_FILL[3], 'support, state 3')
lg.append(text(lx, ly + 110, 'the pattern is the states on the support', 7.5, 'start', halo=False))
for i in range(1, 5):
    swatch(ly + 110 + 30 * i, RING_FILL[i], f'collar, ring {i}')
lg.append(text(lx, ly + 110 + 30 * 5 + 12, 'classes and gluings of the collar are recorded', 7.5, 'start', halo=False))
swatch(ly + 160 + 30 * 5, 'white', 'beyond the collar: not part of the situation', stroke='#bbb')
lg.append(f'<rect x="{lx:.0f}" y="{ly + 190 + 30 * 5:.0f}" width="34" height="22" fill="none" stroke="#111" stroke-width="2.4"/>')
lg.append(text(lx + 44, ly + 190 + 30 * 5 + 17, f'centre; every support cell within r = {rho} rings of it', 8, 'start', halo=False))
legend_h = ly + 190 + 30 * 5 + 22

W = wa + 40 + max(560, wb * SB) + 20; H = max(ha + 60, legend_h + 40 + 75 + hb * SB + 30)
if NO_ROT:
    W = wa + 40 + 560 + 20; H = max(ha + 60, legend_h + 60)
svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">',
       '<rect width="100%" height="100%" fill="white"/>']
svg.append('<g transform="translate(12,40)">' + text(0, -12, ('(a)  ' if not NO_ROT else '') + f'situation {st} of the hat A certificate: k = {k}, r = {rho}', 10, 'start', 'bold', halo=False) + ''.join(pa) + '</g>')
svg.append('<g transform="translate(12,40)">' + ''.join(lg) + '</g>')
yb = legend_h + 40 + 75
if not NO_ROT:
    svg.append(f'<g transform="translate({lx + 12:.0f},{yb:.0f})">' + text(0, -12, '(b)  rotated by 120°: the same key', 10, 'start', 'bold', halo=False)
               + f'<g transform="scale({SB})">' + ''.join(pb) + '</g></g>')
svg.append('</svg>')
open(out, 'w').write('\n'.join(svg))
nsup = sum(1 for c in cells if c['sup']); ncol = sum(1 for i, c in enumerate(cells) if not c['sup'] and ring.get(i, 99) <= k)
print(f'{out}: situation {st}, k={k}, rho={rho}; support {nsup}, collar {ncol} tiles; collar flags agree with computed rings: {col_ok}; ring counts {collections.Counter(r for i, r in ring.items() if 1 <= r <= k)}')
