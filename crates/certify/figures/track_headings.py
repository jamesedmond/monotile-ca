#!/usr/bin/env python3
"""Do the straight stretches of a loop circuit run along the glider lanes? Input: a track_supertiles
--dump with `paths` (the wanderer's live-cell centroid at every generation of one period, per track).
Each closed path is simplified by Douglas-Peucker with tolerance EPS units; every resulting segment
of length >= LMIN is reported with its heading, reduced mod 60, and its angular distance to the two
spectre lane families (48.0138 and 57.1701 mod 60). A length-weighted histogram of headings mod 60 in
3-degree bins follows, with the share of straight length within +-3 degrees of each family.
Usage: track_headings.py <dump.json> [eps=2.0] [lmin=12] [family1=48.0138] [family2=57.1701]"""
import json, math, sys, collections

src = sys.argv[1]
EPS = float(sys.argv[2]) if len(sys.argv) > 2 else 2.0
LMIN = float(sys.argv[3]) if len(sys.argv) > 3 else 12.0
F1 = float(sys.argv[4]) if len(sys.argv) > 4 else 48.0138     # lane family 1 (spectre glider; hat: 45.5225)
F2 = float(sys.argv[5]) if len(sys.argv) > 5 else 57.1701     # lane family 2 (spectre second family; hat: none)

def dp(pts, eps):
    """Douglas-Peucker on an open polyline; returns kept indices."""
    keep = [0, len(pts) - 1]
    stack = [(0, len(pts) - 1)]
    while stack:
        a, b = stack.pop()
        if b - a < 2: continue
        ax, ay = pts[a]; bx, by = pts[b]
        L = math.hypot(bx - ax, by - ay)
        best, bi = 0.0, None
        for i in range(a + 1, b):
            px, py = pts[i]
            d = abs((bx - ax) * (ay - py) - (ax - px) * (by - ay)) / L if L > 1e-9 else math.hypot(px - ax, py - ay)
            if d > best: best, bi = d, i
        if best > eps:
            keep.append(bi); stack.append((a, bi)); stack.append((bi, b))
    return sorted(set(keep))

def angdist(a, b, m=60.0):
    d = (a - b) % m
    return min(d, m - d)

d = json.load(open(src))
for k, (path, tr) in enumerate(zip(d['paths'], d['tracks'])):
    pts = [(p[0], p[1]) for p in path[:tr['period']]]
    # the period's first generation is also its last: close the loop by appending the start, then
    # start the polyline at the point farthest from the centroid so a corner is not split
    cx = sum(x for x, _ in pts) / len(pts); cy = sum(y for _, y in pts) / len(pts)
    s0 = max(range(len(pts)), key=lambda i: math.hypot(pts[i][0] - cx, pts[i][1] - cy))
    pts = pts[s0:] + pts[:s0] + [pts[s0]]
    keep = dp(pts, EPS)
    segs = []
    for a, b in zip(keep, keep[1:]):
        (ax, ay), (bx, by) = pts[a], pts[b]
        L = math.hypot(bx - ax, by - ay)
        if L >= LMIN:
            h = math.degrees(math.atan2(by - ay, bx - ax)) % 360
            segs.append((L, h, b - a))
    total = sum(L for L, _, _ in segs)
    print(f'track {k}: period {tr["period"]}, {tr["size"]} cells; path length {sum(math.hypot(pts[i+1][0]-pts[i][0], pts[i+1][1]-pts[i][1]) for i in range(len(pts)-1)):.0f} u; '
          f'{len(keep) - 1} DP segments at eps {EPS}, {len(segs)} of length >= {LMIN} u ({total:.0f} u)')
    print(f'   {"len":>6} {"gens":>5} {"heading":>8} {"mod60":>6} {"to F1":>6} {"to F2":>6}')
    for L, h, g in sorted(segs, reverse=True):
        print(f'   {L:6.1f} {g:5d} {h:8.2f} {h % 60:6.2f} {angdist(h, F1):6.2f} {angdist(h, F2):6.2f}')
    hist = collections.Counter()
    for L, h, _ in segs:
        hist[int((h % 60) // 3) * 3] += L
    near1 = sum(L for L, h, _ in segs if angdist(h, F1) <= 3); near2 = sum(L for L, h, _ in segs if angdist(h, F2) <= 3)
    print('   heading mod 60, length-weighted (3-degree bins): ' + ' '.join(f'{b}:{v:.0f}' for b, v in sorted(hist.items())))
    if total:
        print(f'   within 3 degrees of family 1 ({F1}): {near1:.0f} u ({100 * near1 / total:.0f}%); of family 2 ({F2}): {near2:.0f} u ({100 * near2 / total:.0f}%); elsewhere {100 * (total - near1 - near2) / total:.0f}%')
