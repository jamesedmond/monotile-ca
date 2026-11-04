#!/usr/bin/env python3
"""Is a loop circuit's region exactly symmetric? Rotate every tile of a track_supertiles --dump by
360/order degrees about the track's centroid (refined to the mean of matched pairs) and look for a
tile whose polygon lands exactly on it (vertex sets equal to 0.03, the dump rounding vertices to 0.01). Reports the matched fraction of
the track and of the whole dumped region, the refined centre and what sits there (a vertex, an edge
midpoint or a tile centroid), and, for matched pairs, whether the hierarchy agrees under the rotation:
per level, the fraction of pairs with the same supertile type, and whether the map between level-k
supertile instances is one-to-one (the decomposition itself symmetric).
Usage: track_symmetry.py <dump.json> [track-index=0] [order=2]"""
import json, math, re, sys, collections

src = sys.argv[1]
tk = int(sys.argv[2]) if len(sys.argv) > 2 else 0
order = int(sys.argv[3]) if len(sys.argv) > 3 else 2
L = 40
TOL = 0.03   # the dump rounds vertices to 0.01

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
cen = [(sum(p[0] for p in c['xy']) / len(c['xy']), sum(p[1] for p in c['xy']) / len(c['xy'])) for c in cells]
track = [k for k, c in enumerate(cells) if c['t'] == tk]
cx = sum(cen[k][0] for k in track) / len(track); cy = sum(cen[k][1] for k in track) / len(track)
ang = 2 * math.pi / order; ca, sa = math.cos(ang), math.sin(ang)
grid = collections.defaultdict(list)
for k, (x, y) in enumerate(cen):
    grid[(round(x), round(y))].append(k)
def vkey(poly):
    return sorted((round(x, 2), round(y, 2)) for x, y in poly)
def rot(p, c):
    x, y = p[0] - c[0], p[1] - c[1]
    return (c[0] + x * ca - y * sa, c[1] + x * sa + y * ca)
def partner(k, c):
    rx, ry = rot(cen[k], c)
    best = None
    for dx in (-1, 0, 1):
        for dy in (-1, 0, 1):
            for j in grid.get((round(rx) + dx, round(ry) + dy), []):
                if math.hypot(cen[j][0] - rx, cen[j][1] - ry) < 0.5:
                    rp = [rot(p, c) for p in cells[k]['xy']]
                    if all(any(math.hypot(a[0] - b[0], a[1] - b[1]) < TOL for b in cells[j]['xy']) for a in rp):
                        best = j
    return best

# pass 1: with the centroid as a trial centre, the true rotation differs by a uniform shift t;
# find t by voting over (tile, nearby tile) pairs, then solve (I - R) x = t for the centre correction.
c = (cx, cy)
r = [rot(cen[k], c) for k in range(len(cells))]
votes = collections.Counter()
for k in range(len(cells)):
    rx, ry = r[k]
    for dx in range(-4, 5):
        for dy in range(-4, 5):
            for j in grid.get((round(rx) + dx, round(ry) + dy), []):
                tx, ty = cen[j][0] - rx, cen[j][1] - ry
                if math.hypot(tx, ty) < 4.5:
                    votes[(round(tx, 1), round(ty, 1))] += 1
(tb, nvotes) = votes.most_common(1)[0]
# the precise shift: mean offset over the voters of the winning bin
offs = []
for k in range(len(cells)):
    rx, ry = r[k]
    for dx in range(-4, 5):
        for dy in range(-4, 5):
            for j in grid.get((round(rx) + dx, round(ry) + dy), []):
                tx, ty = cen[j][0] - rx, cen[j][1] - ry
                if abs(tx - tb[0]) <= 0.06 and abs(ty - tb[1]) <= 0.06:
                    offs.append((tx, ty))
t = (sum(o[0] for o in offs) / len(offs), sum(o[1] for o in offs) / len(offs))
det = (1 - ca) ** 2 + sa ** 2
def solve(t):   # (I - R) x = t
    return ((1 - ca) * t[0] - sa * t[1]) / det, (sa * t[0] + (1 - ca) * t[1]) / det
x = solve(t)
c = (c[0] + x[0], c[1] + x[1])
# pass 2: exact matching about the corrected centre, then refine the centre from the exact pairs
# (each pair gives c = (I - R)^{-1} (p_j - R p_k)) and match once more
for it in range(3):
    pairs = [(k, partner(k, c)) for k in range(len(cells))]
    matched = [(k, j) for k, j in pairs if j is not None]
    if not matched:
        break
    ests = []
    for k, j in matched:
        rk = (cen[k][0] * ca - cen[k][1] * sa, cen[k][0] * sa + cen[k][1] * ca)
        ests.append(solve((cen[j][0] - rk[0], cen[j][1] - rk[1])))
    c = (sum(e[0] for e in ests) / len(ests), sum(e[1] for e in ests) / len(ests))
tr_matched = sum(1 for k, j in matched if cells[k]['t'] == tk)
tr_on_track = sum(1 for k, j in matched if cells[k]['t'] == tk and cells[j]['t'] == tk)
print(f'order-{order} rotation about ({c[0]:.4f}, {c[1]:.4f}) (shift vote {nvotes} of {len(cells)}): region {len(matched)}/{len(cells)} tiles land exactly on a tile; '
      f'track {tr_matched}/{len(track)} land on a tile, {tr_on_track}/{len(track)} on a track tile')
if not matched:
    sys.exit(0)
unm = [k for k in track if partner(k, c) is None]
if unm:
    dd = sorted(math.hypot(cen[k][0] - c[0], cen[k][1] - c[1]) for k in unm)
    print(f'  unmatched track tiles: {len(unm)}, at distances {dd[0]:.1f}..{dd[-1]:.1f} from the centre (track extends to {max(math.hypot(cen[k][0] - c[0], cen[k][1] - c[1]) for k in track):.1f})')
# what is at the centre?
near = min(range(len(cells)), key=lambda k: math.hypot(cen[k][0] - c[0], cen[k][1] - c[1]))
verts = [(x, y) for dx in (-2, -1, 0, 1, 2) for dy in (-2, -1, 0, 1, 2) for k in grid.get((round(c[0]) + dx, round(c[1]) + dy), []) for x, y in cells[k]['xy']]
dv = min((math.hypot(x - c[0], y - c[1]) for x, y in verts), default=9)
dc = math.hypot(cen[near][0] - c[0], cen[near][1] - c[1])
poly = cells[near]['xy']; mids = [((poly[i][0] + poly[(i + 1) % len(poly)][0]) / 2, (poly[i][1] + poly[(i + 1) % len(poly)][1]) / 2) for i in range(len(poly))]
dm = min(math.hypot(x - c[0], y - c[1]) for x, y in mids)
print(f'centre: nearest vertex {dv:.4f}, nearest edge midpoint {dm:.4f}, nearest tile centroid {dc:.4f} (cell {cells[near]["i"]}, ring {cells[near]["d"]})')
# hierarchy under the rotation, over matched pairs within the track's own extent
for lev in range(1, 8):
    same = sum(1 for k, j in matched if typ(exp[k][lev]) == typ(exp[j][lev]))
    inst = collections.defaultdict(set)
    for k, j in matched:
        inst[exp[k][lev + 1:]].add(exp[j][lev + 1:])
    one_to_one = all(len(v) == 1 for v in inst.values())
    n_inst = len(inst)
    print(f'  level {lev}: same supertile type in {same}/{len(matched)} pairs ({100 * same / len(matched):.1f}%); '
          f'{n_inst} level-{lev} instances in the region, instance map one-to-one: {one_to_one}'
          + ('' if one_to_one else f' ({sum(1 for v in inst.values() if len(v) > 1)} instances split)'))
