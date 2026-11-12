#!/usr/bin/env python3
"""Extreme cycle headings of a certificate, independent of its reference cycle (FINDINGS 10.11.66).

`certify cocycle` and `certify sccs` print a heading window taken about one
reference cycle of the file: the cycle means perpendicular to the reference
heading, divided by the smallest cycle mean along it. That is a valid bound
on every cycle's heading, but it depends on which cycle the file's state
order makes the reference, and it is the smallest window only at the end
where the reference cycle itself sits. Two files with the same situations
and transitions can therefore print windows that differ in the last digit.

This script finds the smallest window: the least and greatest heading of any
cycle of the transition graph. A direction is beyond the upper end exactly
when no cycle has a positive mean perpendicular to it (maximum cycle mean,
Karp), so each end is found by bisection between the reference heading and
the printed bound. Floating point, to about 1e-6 degrees; it is a
measurement, and the printed window of `certify cocycle` (which has an exact
mode) remains the certified bound. The script first recomputes that bound
from the file (columns bound_lo, bound_hi), as a check on its own reading of the format.

    python3 crates/certify/census/rebuild_certificates.py      # writes out/census-certificates/*.json
    python3 crates/certify/census/cycle_headings.py out/census-certificates/*.json

Needs numpy. Each file must hold one strongly connected object and no dead state.
"""
import json, math, sys
import numpy as np


def load(path):
    d = json.load(open(path))
    n = len(d['states'])
    assert d.get('dead') is None, f'{path}: has a dead state'
    edges = d['transitions']
    succ = [[] for _ in range(n)]
    for i, t in enumerate(edges):
        succ[t['from']].append(i)
    # Orientation of each state relative to the first (the rotations of every cycle sum to zero).
    theta = [None] * n
    theta[0] = 0
    queue = [0]
    while queue:
        s = queue.pop()
        for i in succ[s]:
            t = edges[i]
            th = (theta[s] + t['rot']) % 12
            assert theta[t['to']] in (None, th), f'{path}: a cycle rotates'
            if theta[t['to']] is None:
                theta[t['to']] = th
                queue.append(t['to'])
    assert None not in theta, f'{path}: not every state is reached from the first'
    vec = np.zeros((len(edges), 2))
    for i, t in enumerate(edges):
        a, (x, y) = math.radians(30 * theta[t['from']]), t['offset']
        vec[i] = (x * math.cos(a) - y * math.sin(a), x * math.sin(a) + y * math.cos(a))
    src, dst = np.array([t['from'] for t in edges]), np.array([t['to'] for t in edges])
    # The reference cycle of `certify cocycle`: follow each state's first transition from state 0.
    seen, s, walk = {}, 0, []
    while s not in seen:
        seen[s] = len(walk)
        walk.append(succ[s][0])
        s = edges[walk[-1]]['to']
    ref = vec[walk[seen[s]:]].sum(axis=0)
    return n, src, dst, vec, math.degrees(math.atan2(ref[1], ref[0])) % 360


def min_cycle_mean(n, src, dst, w):
    """Karp's minimum cycle mean of edge weights w (the graph is strongly connected)."""
    dist = np.full((n + 1, n), np.inf)
    dist[0, 0] = 0.0
    for k in range(1, n + 1):
        np.minimum.at(dist[k], dst, dist[k - 1][src] + w)
    with np.errstate(invalid='ignore'):
        ratio = (dist[n][None, :] - dist[:n]) / (n - np.arange(n)[:, None])
    ratio[~np.isfinite(dist[:n])] = -np.inf
    worst = ratio.max(axis=0)
    worst[~np.isfinite(dist[n])] = np.inf
    return worst.min()


def means(n, src, dst, vec, heading):
    """Smallest cycle mean along a heading, and the least and greatest cycle mean perpendicular to it."""
    a = math.radians(heading)
    along, perp = vec @ np.array([math.cos(a), math.sin(a)]), vec @ np.array([-math.sin(a), math.cos(a)])
    return min_cycle_mean(n, src, dst, along), min_cycle_mean(n, src, dst, perp), -min_cycle_mean(n, src, dst, -perp)


def extreme(n, src, dst, vec, inside, outside, side):
    """Bisect for the heading beyond which no cycle lies, on the upper (side=1) or lower (side=-1) end."""
    for _ in range(40):
        mid = (inside + outside) / 2
        _, lo, hi = means(n, src, dst, vec, mid)
        if (hi if side > 0 else -lo) > 1e-12:
            inside = mid
        else:
            outside = mid
    return outside


print('file\treference\tbound_lo\tbound_hi\textreme_lo\textreme_hi\twidth')
for path in sys.argv[1:]:
    n, src, dst, vec, ref = load(path)
    along, lo, hi = means(n, src, dst, vec, ref)
    assert along > 0, f'{path}: some cycle does not advance along the reference heading'
    # The bound `certify cocycle` prints (there rounded outward to four decimals).
    bound_lo = ref + math.degrees(math.atan2(min(lo, 0), along))
    bound_hi = ref + math.degrees(math.atan2(max(hi, 0), along))
    ext_lo = extreme(n, src, dst, vec, ref, bound_lo - 1e-6, -1)
    ext_hi = extreme(n, src, dst, vec, ref, bound_hi + 1e-6, 1)
    print(f"{path.split('/')[-1]}\t{ref % 30:.5f}\t{bound_lo % 30:.5f}\t{bound_hi % 30:.5f}\t"
          f"{ext_lo % 30:.6f}\t{ext_hi % 30:.6f}\t{ext_hi - ext_lo:.6f}", flush=True)
