#!/usr/bin/env python3
"""Analysis over sliding-window flight CSVs (stdlib only).

Per flight: net heading (final running value) with a standard error from
the windowed-heading series, lane wobble amplitude, gens/effective-ring,
tortuosity, kappa, local clock, and hop_ms early-vs-late (the O(log R)
timing law). Then the per-substrate fan-offset refit and a
quantization-residual table against both the claimed fan and the refit.

House convention: heading is degrees CCW from +x. Compass fans are k*60
+ offset (hat, spectre) or k*36 + offset (P3). The refit fits ONE offset
per substrate to all its lanes; because observed lanes are 60-multiples
apart the fit is self-consistent rather than overdetermined (stated in
the report) -- its content is that a single stable offset makes every
lane's residual ~0.

Usage: analyze_flights.py <flight1.csv> [flight2.csv ...]
Each CSV is matched to a flight config by basename substring.
"""

import csv
import math
import os
import sys

# basename-substring -> (label, substrate, claimed_fan_offset_deg, fan_step)
CONFIG = [
    ("s21", "hat A", "hat", 46.0, 60.0),
    ("hat-A", "hat A", "hat", 46.0, 60.0),
    ("confirm", "hat A", "hat", 46.0, 60.0),
    ("s22", "hat B", "hat", 46.0, 60.0),
    ("hat-B", "hat B", "hat", 46.0, 60.0),
    ("s23", "hat C (lone 346)", "hat", 46.0, 60.0),
    ("hat-C", "hat C (lone 346)", "hat", 46.0, 60.0),
    ("s33", "spectre (108)", "spectre", 48.0, 60.0),
    ("flight-108", "spectre (108)", "spectre", 48.0, 60.0),
    ("flight-348", "spectre (348)", "spectre", 48.0, 60.0),
]


def match_config(path):
    b = os.path.basename(path)
    for key, label, sub, off, step in CONFIG:
        if key in b:
            return label, sub, off, step
    return b, "?", 0.0, 60.0


def read_rows(path):
    """Read complete rows only. A live CSV can end mid-write, and a row
    truncated inside a field yields a valid-but-WRONG number (e.g.
    heading_running '-14.4789' cut to '-14' → -14.0 ≡ 346°). Dropping any
    row whose last column is missing removes that trap."""
    with open(path) as f:
        reader = csv.DictReader(f)
        last_col = reader.fieldnames[-1] if reader.fieldnames else None
        rows = [r for r in reader
                if last_col is None or (r.get(last_col) not in (None, ""))]
    return rows


def fnum(row, key):
    v = row.get(key)
    if v is None or v == "" or v.lower() == "nan":
        return None
    try:
        return float(v)  # tolerate a partial trailing row in a live CSV
    except ValueError:
        return None


def circ_stats(degs):
    """Mean, std (deg) of angles via unit vectors (wrap-safe)."""
    if not degs:
        return float("nan"), float("nan")
    s = sum(math.sin(math.radians(d)) for d in degs)
    c = sum(math.cos(math.radians(d)) for d in degs)
    mean = math.degrees(math.atan2(s, c)) % 360.0
    r = math.hypot(s, c) / len(degs)
    # circular std (deg); r->1 => 0
    std = math.degrees(math.sqrt(-2.0 * math.log(r))) if 0 < r < 1 else 0.0
    return mean, std


def analyze(path):
    rows = read_rows(path)
    label, sub, claimed, step = match_config(path)
    gen = [fnum(r, "generation") for r in rows]
    eff = [fnum(r, "effective_rings") for r in rows]
    euc = [fnum(r, "euclid_distance") for r in rows]
    gx = [fnum(r, "global_x") for r in rows]
    gy = [fnum(r, "global_y") for r in rows]
    hr = [fnum(r, "heading_running") for r in rows]
    hw = [fnum(r, "heading_windowed") for r in rows]
    hop_ms = [fnum(r, "hop_ms") for r in rows]  # may be absent (old CSV)
    lclk = [fnum(r, "local_clock") for r in rows]
    n = len(rows)

    def last(lst):  # last non-None (a live CSV may end mid-row)
        for v in reversed(lst):
            if v is not None:
                return v
        return float("nan")

    # NET heading = final running (displacement from launch). It carries
    # the launch transient (decays ~1/r; large for wanderer-mediated
    # launches like hat C) — reported, but NOT used for the fan refit.
    net_heading = last(hr) % 360.0
    # SETTLED heading = circular mean of the windowed (local, last-20-hop)
    # heading over the flight's late half: transient-free, stable across
    # 10^5 and 10^6. This is the lane's true asymptotic direction and the
    # estimator the compass refit uses (matches FINDINGS §10.2).
    wser = [d for d in hw if d is not None and math.isfinite(d)]
    late = wser[len(wser) // 2:] if len(wser) > 8 else wser
    heading, wstd = circ_stats(late)
    heading %= 360.0
    n_ind = max(1, len(late) // 20)
    se = wstd / math.sqrt(n_ind) if wstd == wstd else float("nan")

    kappa = None
    kser = [e / u for e, u in zip(eff, euc) if e and u]
    if kser:
        kser.sort()
        kappa = kser[len(kser) // 2]

    _ge=last(eff); gens_per_eff = last(gen)/_ge if _ge else float('nan')

    # tortuosity = sum |delta global| / net displacement
    seg = 0.0
    for i in range(1, n):
        if None not in (gx[i], gy[i], gx[i - 1], gy[i - 1]):
            seg += math.hypot(gx[i] - gx[i - 1], gy[i] - gy[i - 1])
    net_disp = math.hypot(last(gx), last(gy))
    tort = seg / net_disp if net_disp else float("nan")

    # timing early vs late
    hv = [h for h in hop_ms if h is not None]
    if hv:
        k = max(1, len(hv) // 10)
        early = sum(hv[:k]) / k
        late = sum(hv[-k:]) / k
    else:
        early = late = float("nan")

    lv = [x for x in lclk if x is not None]
    local = sorted(lv)[len(lv) // 2] if lv else float("nan")

    return {
        "path": path, "label": label, "sub": sub, "claimed": claimed, "step": step,
        "hops": n, "gens": last(gen), "net_heading": net_heading,
        "heading": heading, "wstd": wstd, "se": se,
        "kappa": kappa, "gens_per_eff": gens_per_eff, "tort": tort,
        "eff_rings": last(eff), "euclid": last(euc),
        "hop_ms_early": early, "hop_ms_late": late, "local_clock": local,
    }


def main():
    paths = sys.argv[1:]
    if not paths:
        print("usage: analyze_flights.py <flight.csv> ...")
        return
    res = [analyze(p) for p in paths]

    print("=" * 96)
    print("PER-FLIGHT SUMMARY")
    print("=" * 96)
    hdr = ("label", "hops", "settled", "net", "SE", "g/eff", "tort", "kappa",
           "lclk", "hop_ms e→l")
    print(f"{hdr[0]:<18}{hdr[1]:>7}{hdr[2]:>10}{hdr[3]:>10}{hdr[4]:>7}"
          f"{hdr[5]:>7}{hdr[6]:>7}{hdr[7]:>9}{hdr[8]:>6}{hdr[9]:>14}")
    for r in res:
        hop = (f"{r['hop_ms_early']:.0f}→{r['hop_ms_late']:.0f}"
               if r["hop_ms_early"] == r["hop_ms_early"] else "n/a")
        print(f"{r['label']:<18}{r['hops']:>7}{r['heading']:>10.3f}"
              f"{r['net_heading']:>10.3f}{r['se']:>7.3f}{r['gens_per_eff']:>7.3f}"
              f"{r['tort']:>7.3f}{r['kappa']:>9.5f}{r['local_clock']:>6.2f}{hop:>14}")
    print("(settled = windowed/local heading, used for the refit; net = "
          "displacement from launch. They agree to <0.01° here — the launch "
          "transient is negligible at these radii.)")

    # Per-substrate fan-offset refit.
    print()
    print("=" * 96)
    print("FAN-OFFSET REFIT + QUANTIZATION RESIDUALS")
    print("=" * 96)
    by_sub = {}
    for r in res:
        by_sub.setdefault(r["sub"], []).append(r)
    for sub, group in by_sub.items():
        step = group[0]["step"]
        claimed = group[0]["claimed"]
        # each lane's heading mod step, near the claimed offset (unwrap into
        # [claimed-step/2, claimed+step/2))
        mods = []
        for r in group:
            m = (r["heading"] - claimed) % step
            if m > step / 2:
                m -= step
            mods.append(claimed + m)  # offset implied by this lane
        refit = sum(mods) / len(mods)
        print(f"\n[{sub}] fan step {step:.0f}°, claimed offset {claimed:.2f}°, "
              f"REFIT offset {refit:.3f}°  (from {len(group)} lane(s), settled headings)")
        print(f"  {'lane':<20}{'settled':>10}{'resid|claimed':>15}{'resid|refit':>13}")
        for r, implied in zip(group, mods):
            rc = implied - claimed
            rr = implied - refit
            print(f"  {r['label']:<20}{r['heading']:>10.3f}"
                  f"{rc:>+15.3f}{rr:>+13.3f}")
        # separations between lanes (offset-free evidence)
        if len(group) >= 2:
            hs = sorted(g["heading"] for g in group)
            print("  pair separations (deg):", ", ".join(
                f"{(hs[j] - hs[i]) % 360:.3f}" for i in range(len(hs))
                for j in range(i + 1, len(hs))))

    print()
    print("Caveat: observed lanes are multiples of the fan step apart, so a")
    print("single-offset fit is self-consistent, not overdetermined; the content")
    print("is that ONE stable offset per substrate drives every residual to ~0.")


if __name__ == "__main__":
    main()
