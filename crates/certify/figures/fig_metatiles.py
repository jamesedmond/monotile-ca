#!/usr/bin/env python3
"""Paper 2, Figure 1: the hat's metatiles and nested supertiles, drawn from a hier_dump patch.
(a) the hat and the anti-hat, and the four metatiles H, T, P, F as clusters of hats (the anti-hat
    in the H filled dark), each tile labelled with its slot number;
(b) a level-3 F supertile containing a pose tile: level-1 supertiles filled by type, level-2
    outlines medium, level-3 outline heavy, the pose tile filled dark, the nested supertiles
    labelled so the pose tile's address (1,H0)(1,F0)(4,F) can be read off the drawing.
Usage: fig_metatiles.py <hier.json> <out.svg> [scale=7] [--plain]
The hier.json comes from `tiling-core --example hier_dump <record> 0 48 hier.json 10000 0`.
Greyscale-safe: fills are ordered in luminance and every element also carries a label."""
import json, re, sys, collections, math

hier, out = sys.argv[1:3]
SC = float(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3] != '--plain' else 7.0
PLAIN = '--plain' in sys.argv   # certificate paper: no pose-tile vocabulary in the figure's own text
L = 40

def expand(addr):
    head, _, cyc = addr.partition(':')
    init = re.findall(r'\(([^)]*)\)', head); rep = re.findall(r'\(([^)]*)\)', cyc)
    o = list(init)
    while len(o) < L: o.append(rep[(len(o) - len(init)) % len(rep)])
    return tuple(o)

def sym(s):
    """'subtile 4 of F0' -> ('4', 'F0'); 'tile hat' -> (None, 'hat')."""
    m = re.match(r'subtile (\d+) of (\w+)', s)
    if m: return m.group(1), m.group(2)
    return None, s.split()[-1]

d = json.load(open(hier))
cells = d['cells']
exp = [expand(c['a']) for c in cells]
maxd = max(c['d'] for c in cells)
interior = [c['d'] <= maxd - 4 for c in cells]

# fills: pale, luminance-ordered by metatile type; anti-hat and pose tile dark
FILL = {'H': '#dfe7f5', 'T': '#f8e6c3', 'P': '#e3f0d6', 'F': '#f4d9d9'}
DARK_ANTI = '#6b6b6b'
DARK_POSE = '#c0392b'
EDGE = '#333333'

def groups(level):
    """cells grouped by their level-`level` supertile: symbol n of an address is the slot of the
    level-(n-1) object in its level-n parent, so members of one level-n supertile share the address
    from symbol n+1 on."""
    g = collections.defaultdict(list)
    for i, e in enumerate(exp): g[e[level + 1:]].append(i)
    return g

def boundary(idx):
    """boundary edges of the union of polygons of cells idx (edges not shared within the group)."""
    cnt = collections.Counter(); seg = {}
    for i in idx:
        p = cells[i]['xy']
        for a, b in zip(p, p[1:] + p[:1]):
            ka, kb = (round(a[0], 2), round(a[1], 2)), (round(b[0], 2), round(b[1], 2))
            key = tuple(sorted((ka, kb)))
            cnt[key] += 1; seg[key] = (a, b)
    return [seg[k] for k, n in cnt.items() if n == 1]

def centroid(idx):
    xs = [cells[i]['c'][0] for i in idx]; ys = [cells[i]['c'][1] for i in idx]
    return sum(xs) / len(xs), sum(ys) / len(ys)

# ---------- (a) prototiles and metatiles ----------
g1 = groups(1)
metas = {}
for key, idx in g1.items():
    if not all(interior[i] for i in idx): continue
    t = sym(exp[idx[0]][1])[1]                     # level-1 type name: H0, H1, T, P, F0, F1, F2
    base = t[0]
    if base not in metas or len(idx) > len(metas[base][1]):
        metas[base] = (t, idx)
assert set(metas) == {'H', 'T', 'P', 'F'}, metas.keys()
hat = next(i for i in range(len(cells)) if sym(exp[i][0])[1] == 'hat' and interior[i])
anti = next(i for i in range(len(cells)) if sym(exp[i][0])[1] == 'antihat' and interior[i])

# ---------- (b) a level-3 F supertile with a pose tile ----------
g3 = groups(3)
pose_cands = [i for i, e in enumerate(exp)
              if sym(e[1]) == ('1', 'H0') and sym(e[2]) == ('1', 'F0') and sym(e[3])[0] == '4' and sym(e[3])[1].startswith('F')]
best = None
for i in pose_cands:
    idx = g3[exp[i][4:]]
    if all(interior[j] for j in idx) and (best is None or len(idx) > len(best[1])):
        best = (i, idx)
assert best, "no fully interior level-3 F supertile with a pose tile in this patch"
pose, sup3 = best
sup2 = groups(2)[exp[pose][3:]]
sup1 = g1[exp[pose][2:]]

# ---------- (c) a level-2 F supertile: the substitution on real tiles ----------
g2 = groups(2)
best2 = None
for key, idx in g2.items():
    if sym(exp[idx[0]][2])[1] == 'F0' and all(interior[j] for j in idx) and (best2 is None or len(idx) > len(best2)):
        best2 = idx
assert best2, "no interior level-2 F0"
sup2c = best2

# ---------- SVG helpers ----------
def poly(i, fill, stroke=EDGE, sw=0.35, ox=0.0, oy=0.0):
    pts = ' '.join(f'{(x + ox) * SC:.1f},{-(y + oy) * SC:.1f}' for x, y in cells[i]['xy'])
    return f'<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>'

def outline(segs, sw, ox=0.0, oy=0.0, stroke=EDGE):
    parts = [f'M{(a[0] + ox) * SC:.1f},{-(a[1] + oy) * SC:.1f} L{(b[0] + ox) * SC:.1f},{-(b[1] + oy) * SC:.1f}' for a, b in segs]
    return f'<path d="{" ".join(parts)}" fill="none" stroke="{stroke}" stroke-width="{sw}" stroke-linecap="round"/>'

def text(x, y, s, size=9, anchor='middle', weight='normal', fill='#111', halo=False):
    h = ' stroke="white" stroke-width="3" paint-order="stroke"' if halo else ''
    size = size * 1.35                     # font scale: the drawing is 660 units wide, prints at 482 pt
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" font-size="{size:.1f}" text-anchor="{anchor}" font-weight="{weight}" fill="{fill}"{h}>{s}</text>'

def bbox(idx):
    xs = [x for i in idx for x, _ in cells[i]['xy']]; ys = [y for i in idx for _, y in cells[i]['xy']]
    return min(xs), max(xs), min(ys), max(ys)

svg = []
# panel (a): lay out hat, anti-hat, then H, T, P, F left to right on one row
row = [('hat', [hat]), ('anti-hat', [anti])] + [(metas[b][0].rstrip('012') if b != 'T' else 'T', metas[b][1]) for b in 'HTPF']
cursor = 0.0; y_a = 0.0; gap = 6.0
panel_a = []
for label, idx in row:
    x0, x1, y0, y1 = bbox(idx)
    ox = cursor - x0; oy = -y1                     # top-align
    for i in idx:
        b = sym(exp[i][0])[1]
        fill = DARK_ANTI if b == 'antihat' else (FILL[label[0]] if label in ('H', 'T', 'P', 'F') else '#ffffff')
        panel_a.append(poly(i, fill, ox=ox, oy=oy))
    if label in ('H', 'T', 'P', 'F'):
        panel_a.append(outline(boundary(idx), 1.4, ox, oy))
        for i in idx:                              # slot numbers
            s = sym(exp[i][1])[0]
            cx, cy = cells[i]['c']
            panel_a.append(text((cx + ox) * SC, -(cy + oy) * SC + 3, s, 8, fill='#fff' if sym(exp[i][0])[1] == 'antihat' else '#111'))
    cx = cursor + (x1 - x0) / 2
    panel_a.append(text(cx * SC, (-(y0 + oy)) * SC + 12, label, 10, weight='bold'))
    cursor += (x1 - x0) + gap
width_a = cursor * SC
height_a = max(bbox(idx)[3] - bbox(idx)[2] for _, idx in row) * SC + 18

# panel (b): the level-3 F supertile
x0, x1, y0, y1 = bbox(sup3)
ox = -x0; oy = -y1
panel_b = []
for i in sup3:
    t = sym(exp[i][1])[1][0]
    fill = DARK_POSE if i == pose else (DARK_ANTI if sym(exp[i][0])[1] == 'antihat' else FILL[t])
    panel_b.append(poly(i, fill, ox=ox, oy=oy))
for key, idx in g1.items():                        # level-1 outlines inside sup3
    if all(j in set(sup3) for j in idx):
        panel_b.append(outline(boundary(idx), 0.9, ox, oy))
for key, idx in groups(2).items():
    if all(j in set(sup3) for j in idx):
        panel_b.append(outline(boundary(idx), 1.8, ox, oy))
panel_b.append(outline(boundary(sup3), 3.0, ox, oy))
panel_b.append(outline(boundary(sup1), 2.2, ox, oy, stroke=DARK_POSE))
panel_b.append(outline(boundary(sup2), 2.6, ox, oy, stroke=DARK_POSE))
# labels: short names with white halos on the drawing; the nesting is spelled out in the caption line
cx, cy = centroid(sup1)
panel_b.append(text((cx + ox) * SC, -(cy + oy) * SC - 6, sym(exp[pose][1])[1], 9, weight='bold', fill=DARK_POSE, halo=True))
cx, cy = centroid(sup2)
panel_b.append(text((cx + ox) * SC + 22, -(cy + oy) * SC + 14, sym(exp[pose][2])[1] + ', level 2', 9, weight='bold', fill=DARK_POSE, halo=True))
cx, cy = centroid(sup3)
panel_b.append(text((cx + ox) * SC, -(cy + oy) * SC, sym(exp[pose][3])[1] + ', level 3', 11, weight='bold', halo=True))
pc = cells[pose]['c']
panel_b.append(text((pc[0] + ox) * SC, -(pc[1] + oy) * SC + 3, '1', 8, fill='#fff'))
width_b = (x1 - x0) * SC; height_b = (y1 - y0) * SC
addr = '(1, H0)(1, F0)(' + sym(exp[pose][3])[0] + ', ' + sym(exp[pose][3])[1] + ')'
caption_b = [f'pose tile (dark red): hat 1 of the H0 (thin red outline), which is child 1 of the level-2 F0 (thick red outline),',
             f'which is child {sym(exp[pose][3])[0]} of the level-3 {sym(exp[pose][3])[1]}: address {addr}']
if PLAIN:
    caption_b = [f'the dark red hat: hat 1 of the H0 (thin red outline), which is child 1 of the level-2 F0 (thick red outline),',
                 f'which is child {sym(exp[pose][3])[0]} of the level-3 {sym(exp[pose][3])[1]}: address {addr}']

# panel (c)
x0c, x1c, y0c, y1c = bbox(sup2c)
oxc, oyc = -x0c, -y1c
panel_c = []
for i in sup2c:
    t = sym(exp[i][1])[1][0]
    fill = DARK_ANTI if sym(exp[i][0])[1] == 'antihat' else FILL[t]
    panel_c.append(poly(i, fill, ox=oxc, oy=oyc))
sup2c_set = set(sup2c)
for key, idx in g1.items():
    if all(j in sup2c_set for j in idx):
        panel_c.append(outline(boundary(idx), 1.6, oxc, oyc))
        cx, cy = centroid(idx)
        panel_c.append(text((cx + oxc) * SC, -(cy + oyc) * SC + 3, sym(exp[idx[0]][2])[0] + ':' + sym(exp[idx[0]][1])[1], 8, weight='bold', halo=True))
panel_c.append(outline(boundary(sup2c), 3.0, oxc, oyc))
width_c = (x1c - x0c) * SC; height_c = (y1c - y0c) * SC
caption_c = ['a level-2 F0: the substitution F \u2192 F H P H F F', 'on the hats; each child is labelled slot:type']

W = max(width_a, width_b + 40 + max(width_c, 400), 640) + 20; H = height_a + max(height_b, height_c) + 72
svg.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">')
svg.append('<rect width="100%" height="100%" fill="white"/>')
svg.append(f'<g transform="translate(10,16)">' + text(0, -6, '(a)', 11, 'start', 'bold') + ''.join(panel_a) + '</g>')
svg.append(f'<g transform="translate(10,{height_a + 40:.0f})">' + text(0, -6, '(b)', 11, 'start', 'bold') + ''.join(panel_b)
           + text(0, height_b + 14, caption_b[0], 9, 'start') + text(0, height_b + 26, caption_b[1], 9, 'start') + '</g>')
svg.append(f'<g transform="translate({width_b + 40:.0f},{height_a + 40:.0f})">' + text(0, -6, '(c)', 11, 'start', 'bold') + ''.join(panel_c)
           + text(0, height_c + 14, caption_c[0], 9, 'start') + text(0, height_c + 26, caption_c[1], 9, 'start') + '</g>')
svg.append('</svg>')
open(out, 'w').write('\n'.join(svg))
print(f'{out}: metatiles {[(b, metas[b][0], len(metas[b][1])) for b in "HTPF"]}; level-3 F of {len(sup3)} tiles, pose cell {pose} {cells[pose]["a"]}')
