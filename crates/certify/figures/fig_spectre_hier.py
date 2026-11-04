#!/usr/bin/env python3
"""Paper 2, the spectre's hierarchy (Section 6): drawn from a hier_dump patch and its parameter table.
(a) the spectre, the Mystic pair (the Gamma metatile: two spectres, the odd-rotation one dark), and a
    level-2 supertile with its children labelled slot:type (the substitution on real tiles);
(b) one level-3 supertile containing a pose tile of each lane: level-2 children filled by type and
    labelled slot:type, the lane-108 pose tile (red) with its level-1 metatile and level-2 supertile
    outlined in red, the lane-177 pose tile (blue, inside a Mystic pair) with its chain dashed blue.
Usage: fig_spectre_hier.py <hier.json> <param.tsv> <out.svg> [scale=7] [--plain]
  hier.json from `tiling-core --example hier_dump <spectre record> 0 48 hier.json 10000 0`,
  param.tsv from `certify param spectre '<root>' 48 --out param.tsv` (for the rotation parity).
Greyscale-safe: pale luminance-ordered fills with a label on every child; odd tiles dark."""
import json, re, sys, collections

hier, param, out = sys.argv[1:4]
SC = float(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] != '--plain' else 7.0
PLAIN = '--plain' in sys.argv   # certificate paper: tiles labelled by address, no lane names
L = 40
GREEK = {'Gamma': 'Γ', 'Delta': 'Δ', 'Theta': 'Θ', 'Lambda': 'Λ', 'Xi': 'Ξ', 'Pi': 'Π', 'Sigma': 'Σ', 'Phi': 'Φ', 'Psi': 'Ψ'}
FILL = {'Gamma': '#e9d8ec', 'Delta': '#dfe7f5', 'Theta': '#f8e6c3', 'Lambda': '#e3f0d6', 'Xi': '#f4d9d9',
        'Pi': '#d6ecec', 'Sigma': '#f1e2cf', 'Phi': '#e8e8e8', 'Psi': '#fbf2d6'}
DARK_ODD = '#6b6b6b'; DARK_POSE = '#c0392b'; EDGE = '#333333'
POSE108 = (('0', 'Sigma'), ('4', 'Pi'), '5')            # (slot,type) at levels 1,2 and the level-3 slot
POSE177 = (('0', 'Gamma'), ('0', 'Sigma'), '4')

def expand(addr):
    head, _, cyc = addr.partition(':')
    init = re.findall(r'\(([^)]*)\)', head); rep = re.findall(r'\(([^)]*)\)', cyc)
    o = list(init)
    while len(o) < L: o.append(rep[(len(o) - len(init)) % len(rep)])
    return tuple(o)
def sym(s):
    m = re.match(r'subtile (\d+) of (\w+)', s)
    return (m.group(1), m.group(2)) if m else (None, s.split()[-1])

d = json.load(open(hier)); cells = d['cells']
exp = [expand(c['a']) for c in cells]
maxd = max(c['d'] for c in cells)
interior = [c['d'] <= maxd - 4 for c in cells]
rot = {}
for line in open(param):
    f = line.rstrip('\n').split('\t')
    if f[0] == 'cell': continue
    rot[f[1]] = int(f[2])
odd = [rot.get(c['a'], 0) % 2 == 1 for c in cells]
print(f'{len(cells)} cells, {sum(odd)} odd')

def groups(level):
    g = collections.defaultdict(list)
    for i, e in enumerate(exp): g[e[level + 1:]].append(i)
    return g
def boundary(idx):
    cnt = collections.Counter(); seg = {}
    for i in idx:
        p = cells[i]['xy']
        for a, b in zip(p, p[1:] + p[:1]):
            ka, kb = (round(a[0], 2), round(a[1], 2)), (round(b[0], 2), round(b[1], 2))
            key = tuple(sorted((ka, kb))); cnt[key] += 1; seg[key] = (a, b)
    return [seg[k] for k, n in cnt.items() if n == 1]
def centroid(idx):
    xs = [cells[i]['c'][0] for i in idx]; ys = [cells[i]['c'][1] for i in idx]
    return sum(xs) / len(xs), sum(ys) / len(ys)
def bbox(idx):
    xs = [x for i in idx for x, _ in cells[i]['xy']]; ys = [y for i in idx for _, y in cells[i]['xy']]
    return min(xs), max(xs), min(ys), max(ys)
def poly(i, fill, stroke=EDGE, sw=0.35, ox=0.0, oy=0.0):
    pts = ' '.join(f'{(x + ox) * SC:.1f},{-(y + oy) * SC:.1f}' for x, y in cells[i]['xy'])
    return f'<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>'
def outline(segs, sw, ox=0.0, oy=0.0, stroke=EDGE):
    parts = [f'M{(a[0] + ox) * SC:.1f},{-(a[1] + oy) * SC:.1f} L{(b[0] + ox) * SC:.1f},{-(b[1] + oy) * SC:.1f}' for a, b in segs]
    return f'<path d="{" ".join(parts)}" fill="none" stroke="{stroke}" stroke-width="{sw}" stroke-linecap="round"/>'
def text(x, y, s, size=9, anchor='middle', weight='normal', fill='#111', halo=False):
    h = ' stroke="white" stroke-width="3" paint-order="stroke"' if halo else ''
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" font-size="{size}" text-anchor="{anchor}" font-weight="{weight}" fill="{fill}"{h}>{s}</text>'

g1, g2, g3 = groups(1), groups(2), groups(3)
# ---------- (a) spectre, Mystic pair, a level-2 supertile ----------
even_tile = next(i for i in range(len(cells)) if interior[i] and not odd[i] and sym(exp[i][1])[1] != 'Gamma')
gamma = next(idx for key, idx in g1.items() if sym(exp[idx[0]][1])[1] == 'Gamma' and len(idx) == 2 and all(interior[j] for j in idx))
# a full interior level-2 supertile of the type the lane-108 pose tile sits in (Pi): the largest one
lvl2 = max((idx for key, idx in g2.items() if sym(exp[idx[0]][2])[1] == POSE108[1][1] and all(interior[j] for j in idx)), key=len)
row = [('spectre', [even_tile]), ('Mystic pair = Γ', gamma), (f'a level-2 {GREEK[POSE108[1][1]]}', lvl2)]
panel_a = []; cursor = 0.0; gap = 5.0
for label, idx in row:
    x0, x1, y0, y1 = bbox(idx); ox, oy = cursor - x0, -y1
    for i in idx:
        panel_a.append(poly(i, DARK_ODD if odd[i] else FILL.get(sym(exp[i][1])[1], '#fff') if len(idx) > 1 else '#fff', ox=ox, oy=oy))
    if len(idx) == 2:                                    # slots of the pair
        for i in idx:
            cx, cy = cells[i]['c']; panel_a.append(text((cx + ox) * SC, -(cy + oy) * SC + 3, sym(exp[i][1])[0], 8, fill='#fff' if odd[i] else '#111'))
        panel_a.append(outline(boundary(idx), 1.4, ox, oy))
    if len(idx) > 2:                                     # level-1 children labelled slot:type
        for key, kid in g1.items():
            if all(j in set(idx) for j in kid):
                panel_a.append(outline(boundary(kid), 1.2, ox, oy))
                cx, cy = centroid(kid)
                panel_a.append(text((cx + ox) * SC, -(cy + oy) * SC + 3, sym(exp[kid[0]][2])[0] + ':' + GREEK[sym(exp[kid[0]][1])[1]], 8, weight='bold', halo=True))
        panel_a.append(outline(boundary(idx), 2.6, ox, oy))
    panel_a.append(text((cursor + (x1 - x0) / 2) * SC, (-(y0 + oy)) * SC + 12, label, 10, weight='bold'))
    cursor += (x1 - x0) + gap
width_a = cursor * SC; height_a = max(bbox(idx)[3] - bbox(idx)[2] for _, idx in row) * SC + 18

# ---------- (b): one level-3 supertile containing a pose tile of each lane ----------
def chain(spec):
    (s1, t1), (s2, t2), s3 = spec
    return [i for i, e in enumerate(exp) if sym(e[1]) == (s1, t1) and sym(e[2]) == (s2, t2) and sym(e[3])[0] == s3]
c108, c177 = chain(POSE108), chain(POSE177)
best = None
for i in c108:
    idx = g3[exp[i][4:]]
    j = next((j for j in c177 if exp[j][4:] == exp[i][4:]), None)
    if j is not None and all(interior[k] for k in idx) and (best is None or len(idx) > len(best[2])): best = (i, j, idx)
assert best, 'no interior level-3 supertile holding pose tiles of both lanes'
pose108, pose177, sup3 = best; sup3s = set(sup3)
x0, x1, y0, y1 = bbox(sup3); ox, oy = -x0, -y1
LANE = {108: (pose108, DARK_POSE, None), 177: (pose177, '#2c4f8a', '4,2.5')}
pb = []
for i in sup3:
    fill = DARK_POSE if i == pose108 else ('#2c4f8a' if i == pose177 else (DARK_ODD if odd[i] else FILL[sym(exp[i][2])[1]]))
    pb.append(poly(i, fill, ox=ox, oy=oy))
for key, idx in g1.items():
    if all(j in sup3s for j in idx): pb.append(outline(boundary(idx), 0.8, ox, oy))
pose_l2 = {tuple(exp[pose108][3:]), tuple(exp[pose177][3:])}
for key, idx in g2.items():
    if all(j in sup3s for j in idx):
        pb.append(outline(boundary(idx), 1.7, ox, oy))
        cx, cy = centroid(idx)
        dy = -13 if key in pose_l2 else 0
        pb.append(text((cx + ox) * SC, -(cy + oy) * SC + 3 + dy, sym(exp[idx[0]][3])[0] + ':' + GREEK[sym(exp[idx[0]][2])[1]], 8, weight='bold', halo=True))
pb.append(outline(boundary(sup3), 3.0, ox, oy))
for lane, (pose, col, dash) in LANE.items():
    o1 = outline(boundary(g1[exp[pose][2:]]), 2.0, ox, oy, stroke=col)
    o2 = outline(boundary(g2[exp[pose][3:]]), 2.6, ox, oy, stroke=col)
    if dash: o1 = o1.replace('stroke-linecap', f'stroke-dasharray="{dash}" stroke-linecap'); o2 = o2.replace('stroke-linecap', f'stroke-dasharray="{dash}" stroke-linecap')
    pb += [o1, o2]
    pc = cells[pose]['c']
    pb.append(text((pc[0] + ox) * SC + (14 if lane == 108 else -14), -(pc[1] + oy) * SC + (-10 if lane == 108 else 14), (f'lane {lane}' if not PLAIN else ('red' if lane == 108 else 'blue')), 8, 'start' if lane == 108 else 'end', 'bold', fill=col, halo=True))
pb.append(text(4, 10, f'a level-3 {GREEK[sym(exp[pose108][3])[1]]}', 10, 'start', 'bold', halo=True))
wb, hb = (x1 - x0) * SC, (y1 - y0) * SC
cap = [f'lane 108 pose tile: ({POSE108[0][0]}, {GREEK[POSE108[0][1]]})({POSE108[1][0]}, {GREEK[POSE108[1][1]]})({POSE108[2]}, {GREEK[sym(exp[pose108][3])[1]]})   '
       f'lane 177 pose tile: ({POSE177[0][0]}, {GREEK[POSE177[0][1]]})({POSE177[1][0]}, {GREEK[POSE177[1][1]]})({POSE177[2]}, {GREEK[sym(exp[pose177][3])[1]]})']
if PLAIN:
    cap = [f'red: ({POSE108[0][0]}, {GREEK[POSE108[0][1]]})({POSE108[1][0]}, {GREEK[POSE108[1][1]]})({POSE108[2]}, {GREEK[sym(exp[pose108][3])[1]]})   '
           f'blue: ({POSE177[0][0]}, {GREEK[POSE177[0][1]]})({POSE177[1][0]}, {GREEK[POSE177[1][1]]})({POSE177[2]}, {GREEK[sym(exp[pose177][3])[1]]})']

W = width_a + wb + 50; H = max(height_a, hb) + 30
svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">', '<rect width="100%" height="100%" fill="white"/>']
svg.append('<g transform="translate(10,16)">' + text(0, -6, '(a)', 11, 'start', 'bold') + ''.join(panel_a) + '</g>')
svg.append(f'<g transform="translate({width_a + 34:.0f},16)">' + text(0, -6, '(b)', 11, 'start', 'bold') + ''.join(pb) + '</g>')
svg.append('</svg>')
open(out, 'w').write('\n'.join(svg))
print(f'{out}: level-2 {POSE108[1][1]} of {len(lvl2)} tiles; level-3 {sym(exp[pose108][3])[1]} of {len(sup3)} tiles; poses {cells[pose108]["a"]} / {cells[pose177]["a"]}')
