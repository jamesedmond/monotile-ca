#!/usr/bin/env python3
"""Paper 2, the certificate graph (Section 3.4): hat A's closed set unrolled along the lane.
Every situation is placed where its centre lies relative to the reference situation S0, following a
breadth-first spanning tree of the transition graph with the cocycle's exact motions (rotation index
and translation per transition, translations rotated by the source situation's orientation). The lane
is turned horizontal and the perpendicular offset is exaggerated so the corridor's width shows.
Transitions the seeding flight used are heavy; the others thin. A transition whose motion does not
land on its target's tree position is a recurrent edge: its landing point is marked hollow and the
segment is styled by the displacement class between landing point and target, A, B or A + B, the
three vectors every cycle displacement is built from (Section 3.3). Branching situations (two or three
successors) are ringed; the reference situation and the launch's entry situation are labelled.
Usage: fig_certificate.py <certificate.json> <out.svg> [S0=14] [perp_scale=6] [--tight]"""
import json, sys, math, collections

src, out = sys.argv[1:3]
S0 = int(sys.argv[3]) if len(sys.argv) > 3 else 14
PS = float(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] != '--tight' else 3.0
TIGHT = '--tight' in sys.argv   # certificate paper: no in-figure legend sentence, labels beside the nodes, slim margins
c = json.load(open(src))
T = c['transitions']; S = c['states']; obs = set(c['observed'])
entry = c['entry']['entry_state'] if c.get('entry') else None
adj = collections.defaultdict(list)
for i, t in enumerate(T): adj[t['from']].append(i)

def rot(v, th):
    a = math.radians(30 * th); return (v[0] * math.cos(a) - v[1] * math.sin(a), v[0] * math.sin(a) + v[1] * math.cos(a))
# breadth-first tree from S0: orientation potential and positions
theta = {S0: 0}; pos = {S0: (0.0, 0.0)}; q = [S0]; tree = set()
while q:
    s = q.pop(0)
    for i in adj[s]:
        to = T[i]['to']
        if to in theta: continue
        g = rot(T[i]['offset'], theta[s])
        theta[to] = (theta[s] + T[i]['rot']) % 12; pos[to] = (pos[s][0] + g[0], pos[s][1] + g[1]); tree.add(i); q.append(to)
assert len(pos) == len(S), (len(pos), len(S))
# lane direction: the flight's displacement in this frame
sx = sy = 0.0
for i in c['observed']:
    g = rot(T[i]['offset'], theta[T[i]['from']]); sx += g[0]; sy += g[1]
lane = math.atan2(sy, sx)
def frame(p):        # along the lane, perpendicular (exaggerated)
    x, y = p; ca, sa = math.cos(-lane), math.sin(-lane)
    return x * ca - y * sa, (x * sa + y * ca) * PS
P = {s: frame(p) for s, p in pos.items()}
# recurrent edges and their displacement classes
# the cocycle generators A, B (certify cocycle --exact, in the certificate's reference frame where the lane is at
# 45.5225 deg); turned into this layout's frame by the multiple of 30 deg that carries that lane to ours
A0, B0 = (39.0, 23 * math.sqrt(3)), (24.0, 14 * math.sqrt(3))
k_turn = round((math.degrees(lane) - 45.5224878) / 30.0) % 12
A, B = rot(A0, k_turn), rot(B0, k_turn)
LA, LB, LAB = math.hypot(*A), math.hypot(*B), math.hypot(A[0] + B[0], A[1] + B[1])
def cls(d):
    """the displacement as a small integer combination m A + n B (None when zero)."""
    if math.hypot(*d) < 0.5: return None
    for m in range(-3, 4):
        for n in range(-3, 4):
            if math.hypot(d[0] - m * A[0] - n * B[0], d[1] - m * A[1] - n * B[1]) < 0.5:
                return {(1, 0): 'A', (0, 1): 'B', (1, 1): 'A+B'}.get((m, n), f'{m}A+{n}B'.replace('1A', 'A').replace('1B', 'B').replace('+-', '−'))
    return 'other'
edges = []   # (i, from_pt, land_pt, target_pt, class)
counts = collections.Counter()
for i, t in enumerate(T):
    g = rot(t['offset'], theta[t['from']])
    land = (pos[t['from']][0] + g[0], pos[t['from']][1] + g[1])
    d = (land[0] - pos[t['to']][0], land[1] - pos[t['to']][1])
    k = cls(d); counts[k] += 1
    edges.append((i, P[t['from']], frame(land), P[t['to']], k))
succ = collections.defaultdict(set)
for t in T: succ[t['from']].add(t['to'])
branching = {s for s, v in succ.items() if len(v) >= 2}
observed_states = {s for s, st in enumerate(S) if str(st.get('origin', '')).startswith('observed')}

xs = [p[0] for p in P.values()]; ys = [p[1] for p in P.values()]
X0, X1, Y0, Y1 = min(xs), max(xs), min(ys), max(ys)
SC = 5.2; M = 16 if TIGHT else 70
stacks = collections.defaultdict(list)
for s_, p_ in P.items(): stacks[(round(p_[0], 2), round(p_[1], 2))].append(s_)
maxstack = max(len(v) for v in stacks.values())
TOP = 5.0 * maxstack + 30
W = (X1 - X0) * SC + 2 * M + (16 if TIGHT else 40); H = (Y1 - Y0) * SC + TOP + (175 if TIGHT else 200)   # tight: room for a landing marker past the last node
def pt(p): return (p[0] - X0) * SC + M, (Y1 - p[1]) * SC + TOP
STYLE = collections.defaultdict(lambda: ('#c0392b', 0.9, None), {None: ('#aaa', 0.5, None), 'A': ('#4a6fa5', 0.9, None), 'B': ('#c8892b', 0.9, '3,2'), 'A+B': ('#4f8a4b', 0.9, '1.2,1.8'), '2A+B': ('#7b4f9e', 0.9, '5,2')})
svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">', '<rect width="100%" height="100%" fill="white"/>']
FS = 2.1                                   # font scale: the drawing is ~1000 units wide and prints at 482 pt
def text(x, y, s, size=8, anchor='middle', weight='normal', fill='#222', halo=False):
    h = ' stroke="white" stroke-width="2.5" paint-order="stroke"' if halo else ''
    size = size * FS
    return f'<text x="{x:.1f}" y="{y:.1f}" font-family="Helvetica, Arial, sans-serif" font-size="{size:.1f}" text-anchor="{anchor}" font-weight="{weight}" fill="{fill}"{h}>{s}</text>'
# edges: thin first, flight edges on top
for heavy in (False, True):
    for i, a, land, b, k in edges:
        if (i in obs) != heavy: continue
        col, sw, dash = STYLE[k]
        if heavy and k is None: col, sw = '#333', 1.3
        elif heavy: sw = 1.8
        elif k is not None: sw = 1.4
        x1, y1 = pt(a); x2, y2 = pt(land)
        da = f' stroke-dasharray="{dash}"' if (dash and not heavy) else ''
        svg.append(f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" stroke="{col}" stroke-width="{sw}"{da} stroke-linecap="round"/>')
        if k is not None:
            svg.append(f'<circle cx="{x2:.1f}" cy="{y2:.1f}" r="2.4" fill="white" stroke="{col}" stroke-width="1.2"/>')
# nodes: situations at the same position are stacked as beads above the base point
for key, ss in stacks.items():
    x, y = pt(P[ss[0]])
    for j, s in enumerate(sorted(ss, key=lambda s: (s not in observed_states, s))):
        yy = y - 5.0 * j
        fill = '#222' if s in observed_states else 'white'
        svg.append(f'<circle cx="{x:.1f}" cy="{yy:.1f}" r="2.2" fill="{fill}" stroke="#222" stroke-width="0.6"/>')
        if s in branching:
            svg.append(f'<circle cx="{x:.1f}" cy="{yy:.1f}" r="4.4" fill="none" stroke="#c0392b" stroke-width="1.1"/>')
for s, lab in ((S0, f'S0 = situation {S0}'), (entry, f'entry, situation {entry}')):
    if s is None: continue
    x, y = pt(P[s]); svg.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="6" fill="none" stroke="#222" stroke-width="1.2"/>')
    if TIGHT: svg.append(text(x + 9, y + 30, lab, 8, 'start', 'bold', halo=True))
    else: svg.append(text(x, y - 10, lab, 8, 'middle', 'bold', halo=True))
# scale bar and legend
bx, by = M, H - (118 if TIGHT else 150)
svg.append(f'<line x1="{bx}" y1="{by}" x2="{bx + 50 * SC:.1f}" y2="{by}" stroke="#222" stroke-width="1.2"/>')
svg.append(text(bx + 25 * SC, by - 6, '50 units along the lane', 8))
if not TIGHT:
    svg.append(text(bx, by + 20, f'perpendicular offset exaggerated ×{PS:.0f}; filled beads observed in flight, open beads found by the closure only; branching situations ringed', 7.5, 'start'))
lx = M; ly = H - (96 if TIGHT else 112)
for k, lab in ((None, 'transition landing on its target'), ('A', f'recurrent, displacement A ({LA:.1f})'), ('B', f'recurrent, displacement B ({LB:.1f})'), ('A+B', f'recurrent, displacement A + B ({LAB:.1f})'), ('2A+B', f'recurrent, displacement 2A + B ({math.hypot(2*A[0]+B[0], 2*A[1]+B[1]):.1f})')):
    col, sw, dash = STYLE[k]; da = f' stroke-dasharray="{dash}"' if dash else ''
    svg.append(f'<line x1="{lx}" y1="{ly}" x2="{lx + 22}" y2="{ly}" stroke="{col}" stroke-width="{sw + 0.6}"{da}/>' + text(lx + 28, ly + 3, f'{lab}: {counts[k]}', 7.5, 'start'))
    ly += 19 if TIGHT else 22
svg.append('</svg>')
open(out, 'w').write('\n'.join(svg))
print(f'{out}: {len(S)} states, {len(T)} transitions, classes {dict(counts)}, lane {math.degrees(lane):.3f} deg in the S0 frame, extent along {X1 - X0:.1f} units, perp {(Y1 - Y0) / PS:.2f} units')
