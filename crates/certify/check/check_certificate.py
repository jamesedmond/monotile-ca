#!/usr/bin/env python3
"""Independent checker for a situation-closure certificate.

Verifies Theorem 1 from the artifacts alone: given the atlas (every legal
R-ball around each tile type) and a certificate (situation keys and
transitions), re-derives every transition and checks that the state set
is closed under the step map for every extending ball and contains no
death. Shares nothing with the Rust implementation but the file formats
and the rule table, which is the definition of the automaton. Pure
Python, no dependencies.

    python3 check_certificate.py <certificate.json> <atlas.json> <record.jsonl> [--index 0]

Key format (crates/certify/src/situation.rs): per cell in canonical BFS
order: state u8, class u16 BE, edge count u8, then per intrinsic edge
either (cell index u16 BE, back-edge u8) or FF FF FF for "outside".
"""

import argparse
import json
import math
import sys
from collections import deque

OUTSIDE = b"\xff\xff\xff"
FAN_MARGIN = 2


class CheckError(Exception):
    pass


# ----------------------------------------------------------------- keys


def decode_key(hexkey):
    """-> list of (state, class, edges) with edges = [(idx, back) | None]."""
    b = bytes.fromhex(hexkey)
    cells = []
    p = 0
    while p < len(b):
        state = b[p]
        cls = (b[p + 1] << 8) | b[p + 2]
        ne = b[p + 3]
        p += 4
        edges = []
        for _ in range(ne):
            t = b[p : p + 3]
            p += 3
            edges.append(None if t == OUTSIDE else ((t[0] << 8) | t[1], t[2]))
        cells.append((state, cls, edges))
    return cells


def encode_from(anchor, members, state_of, class_of, edges_of):
    order = [anchor]
    index = {anchor: 0}
    out = bytearray()
    i = 0
    while i < len(order):
        c = order[i]
        i += 1
        out.append(state_of(c))
        out += class_of(c).to_bytes(2, "big")
        es = edges_of(c)
        out.append(len(es))
        for e in es:
            if e is not None and e[0] in members:
                n, back = e
                j = index.get(n)
                if j is None:
                    j = len(order)
                    index[n] = j
                    order.append(n)
                out += j.to_bytes(2, "big")
                out.append(back)
            else:
                out += OUTSIDE
    if len(order) != len(members):
        return None  # collar not edge-connected from this anchor
    return bytes(out), order


def collar(support, k, edges_of):
    depth = {c: 0 for c in support}
    q = deque(support)
    while q:
        c = q.popleft()
        d = depth[c]
        if d >= k:
            continue
        for e in edges_of(c):
            if e is not None and e[0] not in depth:
                depth[e[0]] = d + 1
                q.append(e[0])
    return depth


def encode(support, k, state_of, class_of, edges_of):
    members = collar(support, k, edges_of)
    best = None
    for a in support:
        r = encode_from(a, members, state_of, class_of, edges_of)
        if r is None:
            return None
        if best is None or r[0] < best[0]:
            best = r
    return best


# ----------------------------------------------------------- situations


def bfs_dist(cells, start):
    d = [None] * len(cells)
    d[start] = 0
    q = deque([start])
    while q:
        c = q.popleft()
        for e in cells[c][2]:
            if e is not None and d[e[0]] is None:
                d[e[0]] = d[c] + 1
                q.append(e[0])
    return d


def centre_of(cells):
    support = [i for i, (s, _, _) in enumerate(cells) if s]
    best = None
    for c in support:
        d = bfs_dist(cells, c)
        rho = max(d[s] for s in support)
        if best is None or rho < best[1]:
            best = (c, rho)
    return best


def embed(cells, centre, ball):
    tiles = ball["tiles"]
    n = len(cells)
    m = [None] * n
    t0 = tiles[0]
    if t0["class"] != cells[centre][1] or len(t0["slots"]) != len(cells[centre][2]):
        return None
    m[centre] = 0
    q = deque([centre])
    while q:
        i = q.popleft()
        b = m[i]
        for e, edge in enumerate(cells[i][2]):
            if edge is None:
                continue
            j, back = edge
            slot = tiles[b]["slots"][e]
            if slot is None or slot[1] != back:
                return None
            b2 = slot[0]
            if m[j] is not None:
                if m[j] != b2:
                    return None
            else:
                t = tiles[b2]
                if t["class"] != cells[j][1] or len(t["slots"]) != len(cells[j][2]):
                    return None
                m[j] = b2
                q.append(j)
    if any(x is None for x in m):
        return None
    image = {}
    for i, b in enumerate(m):
        if b in image:
            return None
        image[b] = i
    for i, (_, _, edges) in enumerate(cells):
        b = m[i]
        for e, edge in enumerate(edges):
            if edge is None:
                slot = tiles[b]["slots"][e]
                if slot is not None and slot[0] in image:
                    return None
    return m


# ------------------------------------------------------------------ rule


def rule_next(rule, own, counts):
    for row in rule["rows"]:
        o = row.get("own")
        if o is not None and o != own:
            continue
        if all(counts[s] >= mn for s, mn in row.get("conds", [])):
            return row["next"]
    return 0


def step(ball, rule, m, cells, r, strata=None):
    """`rule` is one table, or a list of tables for a stratified rule, in which case `strata`
    maps a tile's base type (`ty`) to the index of the table that type uses (default 0)."""
    rules = rule if isinstance(rule, list) else [rule]
    strata = strata or {}
    tiles = ball["tiles"]
    n = len(tiles)
    st = [0] * n
    for i, (s, _, _) in enumerate(cells):
        st[m[i]] = s
    for t in range(n):
        if st[t] and tiles[t]["dist"] + 2 * FAN_MARGIN > r:
            raise CheckError("support cell within 2 fan margins of the ball rim")
    nxt = [0] * n
    vadj = ball["vadj"]
    for t in range(n):
        if tiles[t]["dist"] + FAN_MARGIN > r:
            continue
        counts = [0] * 16
        for v in vadj[t]:
            counts[st[v]] += 1
        nxt[t] = rule_next(rules[strata.get(tiles[t]["ty"], 0)], st[t], counts)
    return nxt


# --------------------------------------------------------------- cocycle


def rotate(v, idx):
    a = math.radians(30.0 * idx)
    s, c = math.sin(a), math.cos(a)
    return (c * v[0] - s * v[1], s * v[0] + c * v[1])


def karp(n, edges, w):
    """Minimum cycle mean (Karp) over directed edges (u, v) with weights w[i]."""
    inf = float("inf")
    dist = [[inf] * n for _ in range(n + 1)]
    dist[0][0] = 0.0
    for k in range(1, n + 1):
        row = dist[k]
        prev = dist[k - 1]
        for i, (u, v) in enumerate(edges):
            cand = prev[u] + w[i]
            if cand < row[v]:
                row[v] = cand
    best = inf
    for v in range(n):
        if dist[n][v] == inf:
            continue
        worst = -inf
        for k in range(n):
            if dist[k][v] < inf:
                worst = max(worst, (dist[n][v] - dist[k][v]) / (n - k))
        best = min(best, worst)
    return best


def cocycle(n, dead, transitions):
    edges = [(t["from"], t["to"]) for t in transitions if t["to"] != dead]
    motions = [(t["rot"], tuple(t["offset"])) for t in transitions if t["to"] != dead]
    succ = [[] for _ in range(n)]
    for i, (u, _) in enumerate(edges):
        succ[u].append(i)
    theta = [None] * n
    theta[0] = 0
    q = deque([0])
    violations = 0
    while q:
        s = q.popleft()
        for i in succ[s]:
            v = edges[i][1]
            th = (theta[s] + motions[i][0]) % 12
            if theta[v] is None:
                theta[v] = th
                q.append(v)
            elif theta[v] != th:
                violations += 1
    unreached = sum(1 for s in range(n) if s != dead and theta[s] is None)
    vecs = [rotate(motions[i][1], theta[edges[i][0]] or 0) for i in range(len(edges))]
    # reference cycle by first-successor walk
    seen = {}
    walk = []
    s = 0
    while s not in seen:
        seen[s] = len(walk)
        i = succ[s][0]
        walk.append(i)
        s = edges[i][1]
    cyc = walk[seen[s] :]
    sx = sum(vecs[i][0] for i in cyc)
    sy = sum(vecs[i][1] for i in cyc)
    norm = math.hypot(sx, sy)
    d = (sx / norm, sy / norm)
    dp = (-d[1], d[0])
    par = [v[0] * d[0] + v[1] * d[1] for v in vecs]
    perp = [v[0] * dp[0] + v[1] * dp[1] for v in vecs]
    mu_par = karp(n, edges, par)
    mu_par_max = -karp(n, edges, [-x for x in par])
    mu_perp_min = karp(n, edges, perp)
    mu_perp_max = -karp(n, edges, [-x for x in perp])
    heading = math.degrees(math.atan2(d[1], d[0])) % 360.0
    lo = math.degrees(math.atan2(min(mu_perp_min, 0.0), mu_par))
    hi = math.degrees(math.atan2(max(mu_perp_max, 0.0), mu_par))
    return {
        "rotation_violations": violations,
        "unreached": unreached,
        "cycle_len": len(cyc),
        "cycle_disp": norm,
        "heading": heading,
        "mu_par": (mu_par, mu_par_max),
        "mu_perp": (mu_perp_min, mu_perp_max),
        "window": (heading + lo, heading + hi),
    }


# ------------------------------------------------------------------ main


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("certificate")
    ap.add_argument("atlas")
    ap.add_argument("record")
    ap.add_argument("--index", type=int, default=0)
    ap.add_argument("--no-cocycle", action="store_true")
    ap.add_argument("--strata", default="",
                    help="for a record with table_rules (a stratified rule): TY:INDEX,... giving the "
                         "table index used by tiles of atlas base type TY (unlisted types use table 0)")
    args = ap.parse_args()
    strata = {int(a): int(b) for a, b in (p.split(":") for p in args.strata.split(",") if p)}

    with open(args.certificate) as f:
        cert = json.load(f)
    with open(args.atlas) as f:
        atlas = json.load(f)
    with open(args.record) as f:
        lines = [l for l in f if l.strip()]
    record = json.loads(lines[args.index])
    rule = record["table_rule"] if record.get("table_rule") is not None else record["table_rules"]
    k = cert["k"]
    r = cert["r"]
    if atlas["radius"] != r:
        raise SystemExit(f"atlas radius {atlas['radius']} != certificate R {r}")
    print(f"certificate {args.certificate}: k={k} R={r}, rule from {args.record} ({record.get('note', '')})")

    # Atlas preprocessing: tuple slots, class -> type map, degree range.
    balls = atlas["balls"]
    class_ty = {}
    degrees = set()
    for b in balls:
        for t in b["tiles"]:
            t["slots"] = [None if s is None else (s[0], s[1]) for s in t["slots"]]
            prev = class_ty.setdefault(t["class"], t["ty"])
            if prev != t["ty"]:
                raise SystemExit("atlas class maps to two tile types")
            if t["dist"] + FAN_MARGIN <= r:
                degrees.add(len(b["vadj"][t_index(b, t)]))
    by_type = {}
    for i, b in enumerate(balls):
        by_type.setdefault(b["root_ty"], []).append(i)
    print(f"atlas: {len(balls)} balls, types {sorted(by_type)}, vertex degrees {sorted(degrees)}")

    # The rule must not ignite the vacuum at any occurring degree (tiles
    # far from the support are assumed to stay quiescent).
    for deg in sorted(degrees):
        counts = [0] * 16
        counts[0] = deg
        if any(rule_next(rl, 0, counts) != 0 for rl in (rule if isinstance(rule, list) else [rule])):
            raise SystemExit(f"rule ignites the vacuum at degree {deg}: closure unsound")

    states = cert["states"]
    dead = cert.get("dead")
    n = len(states)
    index = {}
    for i, s in enumerate(states):
        key = s["sit"]["key"]
        if key in index:
            raise SystemExit(f"duplicate state key at {i} and {index[key]}")
        index[key] = i
    listed = {}
    for t in cert["transitions"]:
        listed.setdefault((t["from"], t["ball"]), []).append(t)
    for key, ts in listed.items():
        if len(ts) != 1:
            raise SystemExit(f"transition listed twice for {key}")

    failures = []
    found = 0
    deaths = 0
    motion_mismatch = 0
    unlisted = 0
    wrong_successor = 0
    for sid, s in enumerate(states):
        if sid == dead:
            continue
        cells = decode_key(s["sit"]["key"])
        c0, rho = centre_of(cells)
        ty = class_ty.get(cells[c0][1])
        if ty is None:
            failures.append(f"state {sid}: centre class {cells[c0][1]} unknown to atlas")
            continue
        n_balls = 0
        for bi in by_type[ty]:
            ball = balls[bi]
            m = embed(cells, c0, ball)
            if m is None:
                continue
            n_balls += 1
            found += 1
            tiles = ball["tiles"]
            try:
                nxt = step(ball, rule, m, cells, r, strata)
            except CheckError as e:
                failures.append(f"state {sid} ball {bi}: {e}")
                continue
            support = [t for t in range(len(tiles)) if nxt[t]]
            t_listed = listed.get((sid, bi))
            if not support:
                deaths += 1
                failures.append(f"state {sid} ball {bi}: DEATH")
                continue
            enc = encode(
                support,
                k,
                lambda c: nxt[c],
                lambda c: tiles[c]["class"],
                lambda c: tiles[c]["slots"],
            )
            if enc is None:
                failures.append(f"state {sid} ball {bi}: successor collar not edge-connected")
                continue
            key, order = enc
            if any(tiles[c]["dist"] + 1 > r for c in order):
                failures.append(f"state {sid} ball {bi}: successor collar reaches the ball rim")
                continue
            to = index.get(key.hex())
            if to is None:
                failures.append(f"state {sid} ball {bi}: successor situation is not in the certificate")
                continue
            if t_listed is None:
                unlisted += 1
                failures.append(f"state {sid} ball {bi}: transition not listed (to {to})")
                continue
            t = t_listed[0]
            if t["to"] != to:
                wrong_successor += 1
                failures.append(f"state {sid} ball {bi}: listed successor {t['to']} but found {to}")
                continue
            succ_cells = decode_key(key.hex())
            c2, _ = centre_of(succ_cells)
            t2 = tiles[order[c2]]
            if t2["rot"] != t["rot"] or any(abs(a - b) > 1e-9 for a, b in zip(t2["offset"], t["offset"])):
                motion_mismatch += 1
                failures.append(f"state {sid} ball {bi}: motion mismatch")
        if n_balls == 0:
            failures.append(f"state {sid}: no atlas ball extends it")

    n_listed = len(cert["transitions"])
    missing = n_listed - (found - unlisted)
    print(
        f"states {n} (dead: {dead}); transitions listed {n_listed}, re-derived {found}, "
        f"unlisted {unlisted}, wrong successor {wrong_successor}, motion mismatches {motion_mismatch}, deaths {deaths}"
    )
    if missing:
        failures.append(f"{missing} listed transitions were not re-derived")
    for f in failures[:15]:
        print("  FAIL:", f)
    if not args.no_cocycle and dead is None:
        c = cocycle(n, dead, cert["transitions"])
        print(
            "cocycle: rotation violations {rotation_violations}, unreached {unreached}; reference cycle length {cycle_len}, "
            "displacement {cycle_disp:.6f}, heading {heading:.6f}; along [{mu_par[0]:.6f}, {mu_par[1]:.6f}]; "
            "perpendicular [{mu_perp[0]:.9f}, {mu_perp[1]:.9f}]; window [{wlo:.4f}, {whi:.4f}] (rounded outward)".format(
                wlo=math.floor(c["window"][0] * 1e4) / 1e4, whi=math.ceil(c["window"][1] * 1e4) / 1e4, **c
            )
        )
    ok = not failures
    print("VERDICT:", "PASS — closed, death-free, transitions exactly as listed" if ok else f"FAIL ({len(failures)} problems)")
    sys.exit(0 if ok else 1)


def t_index(ball, tile):
    # tiles are in list order; recover the index of this tile dict
    return ball["tiles"].index(tile)


if __name__ == "__main__":
    main()
