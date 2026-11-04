#!/usr/bin/env python3
"""A loop circuit on the supertile hierarchy (FINDINGS 10.11.56). Input: the JSON written by
`tiling-core --example track_supertiles ... --dump`. Tiles are filled by the type of their level-FL
supertile (the spectre metatile palette of the paper figures), level-TL boundaries drawn medium and
level-FL boundaries heavy; each circuit's track is filled in its own colour; the wanderer of the
shortest-period track is drawn in state colours at six phases of its period (darkest = phase 0).
Usage: fig_track.py <dump.json> <out.svg> [fill_level=4] [thin_level=3] [scale=4.5]"""
import json, re, sys, collections

src, out = sys.argv[1:3]
FL = int(sys.argv[3]) if len(sys.argv) > 3 else 4
TL = int(sys.argv[4]) if len(sys.argv) > 4 else 3
SC = float(sys.argv[5]) if len(sys.argv) > 5 else 4.5
L = 40
FILL = {'Gamma': '#e9d8ec', 'Delta': '#dfe7f5', 'Theta': '#f8e6c3', 'Lambda': '#e3f0d6', 'Xi': '#f4d9d9',
        'Pi': '#d6ecec', 'Sigma': '#f1e2cf', 'Phi': '#e8e8e8', 'Psi': '#fbf2d6',
        'H': '#dfe7f5', 'T': '#f8e6c3', 'P': '#e3f0d6', 'F': '#f4d9d9'}   # the hat's metatiles, as in the paper figures
GREEK = {'Gamma': 'Γ', 'Delta': 'Δ', 'Theta': 'Θ', 'Lambda': 'Λ', 'Xi': 'Ξ', 'Pi': 'Π', 'Sigma': 'Σ', 'Phi': 'Φ', 'Psi': 'Ψ'}
TRACK = ['#d55e00', '#0072b2', '#009e73', '#cc79a7']
STATE = {1: '#b3261e', 2: '#2e7d32', 3: '#1f4e9c'}
FONT = 'font-family="Helvetica, Arial, sans-serif"'

def expand(addr):
    head, _, cyc = addr.partition(':')
    init = re.findall(r'\(([^)]*)\)', head); rep = re.findall(r'\(([^)]*)\)', cyc)
    o = list(init)
    while len(o) < L: o.append(rep[(len(o) - len(init)) % len(rep)])
    return tuple(o)
def typ(s):
    m = re.match(r'subtile (\d+) of (\w+)', s)
    return m.group(2) if m else s.split()[-1]

d = json.load(open(src)); cells = d['cells']
exp = [expand(c['a']) for c in cells]
idx = {c['i']: k for k, c in enumerate(cells)}
xs = [x for c in cells for x, _ in c['xy']]; ys = [y for c in cells for _, y in c['xy']]
x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
W, H = (x1 - x0) * SC + 20, (y1 - y0) * SC + 60
def P(x, y): return (x - x0) * SC + 10, (y1 - y) * SC + 10
def Ps(x, y): return '{:.1f},{:.1f}'.format(*P(x, y))

svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">',
       f'<rect width="{W:.0f}" height="{H:.0f}" fill="white"/>']
# tiles by level-FL type
svg.append('<g stroke="#bbb" stroke-width="0.25">')
for k, c in enumerate(cells):
    t = typ(exp[k][FL]) if FL < len(exp[k]) else 'Phi'
    svg.append(f'<polygon points="{" ".join(Ps(*p) for p in c["xy"])}" fill="{FILL.get(t, "#eee")}"/>')
svg.append('</g>')
# tracks
for tk in range(len(d['tracks'])):
    svg.append(f'<g fill="{TRACK[tk % len(TRACK)]}" fill-opacity="0.55" stroke="{TRACK[tk % len(TRACK)]}" stroke-width="0.4">')
    for k, c in enumerate(cells):
        if c['t'] == tk:
            svg.append(f'<polygon points="{" ".join(Ps(*p) for p in c["xy"])}"/>')
    svg.append('</g>')
# supertile boundaries from shared polygon edges
edges = {}
for k, c in enumerate(cells):
    xy = c['xy']
    for j in range(len(xy)):
        a = (round(xy[j][0], 1), round(xy[j][1], 1)); b = (round(xy[(j + 1) % len(xy)][0], 1), round(xy[(j + 1) % len(xy)][1], 1))
        edges.setdefault((min(a, b), max(a, b)), []).append(k)
def boundary(level, width, col):
    segs = [f'M{Ps(*a)}L{Ps(*b)}' for (a, b), owners in edges.items() if len(owners) == 2 and exp[owners[0]][level + 1:] != exp[owners[1]][level + 1:]]
    svg.append(f'<path d="{"".join(segs)}" stroke="{col}" stroke-width="{width}" fill="none" stroke-linecap="round" stroke-linejoin="round"/>')
boundary(TL, 0.8, '#555'); boundary(FL, 2.0, '#000')
# the wanderer at six phases (darkest at phase 0)
for fi, fr in enumerate(d['frames']):
    op = 1.0 - 0.12 * fi
    svg.append(f'<g fill-opacity="{op:.2f}" stroke="#111" stroke-width="0.5">')
    for c, s in fr['live']:
        if c in idx:
            svg.append(f'<polygon points="{" ".join(Ps(*p) for p in cells[idx[c]]["xy"])}" fill="{STATE.get(s, "#000")}"/>')
    svg.append('</g>')
# level-FL supertile labels at the centroid of their visible cells
groups = collections.defaultdict(list)
for k, c in enumerate(cells):
    groups[exp[k][FL + 1:]].append(k)
for key, ks in groups.items():
    if len(ks) < 40: continue
    cx = sum(p[0] for k in ks for p in cells[k]['xy']) / sum(len(cells[k]['xy']) for k in ks)
    cy = sum(p[1] for k in ks for p in cells[k]['xy']) / sum(len(cells[k]['xy']) for k in ks)
    t = typ(exp[ks[0]][FL]); px, py = P(cx, cy)
    svg.append(f'<text x="{px:.1f}" y="{py + 5:.1f}" font-size="15" text-anchor="middle" {FONT} font-weight="bold" fill="#222" stroke="white" stroke-width="3" paint-order="stroke">{GREEK.get(t, t)}</text>')
# legend
ly = H - 38
items = [(TRACK[k % len(TRACK)], f'track of the period-{t["period"]} circuit ({t["size"]} cells)') for k, t in enumerate(d['tracks'])]
lx = 12
for col, lab in items:
    svg.append(f'<rect x="{lx}" y="{ly}" width="14" height="10" fill="{col}" fill-opacity="0.55" stroke="{col}" stroke-width="0.6"/>')
    svg.append(f'<text x="{lx + 18}" y="{ly + 9}" font-size="9" {FONT} fill="#111">{lab}</text>'); lx += 18 + 6.0 * len(lab)
svg.append(f'<text x="12" y="{ly + 24}" font-size="9" {FONT} fill="#111">fill: type of the level-{FL} supertile; thin lines level-{TL} boundaries, heavy lines level-{FL}; the wanderer in state colours at six phases of the shortest period (darkest first)</text>')
svg.append('</svg>')
open(out, 'w').write('\n'.join(svg))
print(f'{out}: {len(cells)} tiles, {len(d["tracks"])} tracks, {len(groups)} level-{FL} supertiles in view')
