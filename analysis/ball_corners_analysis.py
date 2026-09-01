#!/usr/bin/env python3
"""Flank-intersection cusp fit for ball_corners.rs output: locate the
graph-metric ball's corner (cusp) directions and compare with the
glider lane headings. The corners are asymmetric V-cusps; smoothing-
based estimators (parabola fits) are biased by up to degrees (the
spectre's one-sided plateau shoulder produced a +2.6 deg artifact) —
fit straight lines to the two flanks (0.75..3.5 deg each side, 0.25
deg bins, per-bin regression slope of dist on rho) and intersect.

Usage: ball_corners_analysis.py <ball.csv> <lane0> (lanes at lane0+60k)
"""
import csv, sys

def main(path, lane0):
    lanes = [(lane0 + 60*k) % 360 for k in range(6)]
    N = 1440
    acc = [[0.0]*5 for _ in range(N)]
    with open(path) as f:
        for row in csv.DictReader(f):
            d = float(row["dist"]); th = float(row["theta_deg"]); rho = float(row["rho"])
            a = acc[int(th/0.25) % N]
            a[0]+=1; a[1]+=rho; a[2]+=d; a[3]+=rho*rho; a[4]+=rho*d
    prof = [(n*sxy-sx*sy)/(n*sxx-sx*sx) if n > 5 else float("nan")
            for n,sx,sy,sxx,sxy in acc]
    def linfit(pairs):
        n=len(pairs); sx=sum(x for x,_ in pairs); sy=sum(y for _,y in pairs)
        sxx=sum(x*x for x,_ in pairs); sxy=sum(x*y for x,y in pairs)
        b=(n*sxy-sx*sy)/(n*sxx-sx*sx); a=(sy-b*sx)/n
        s2=sum((y-(a+b*x))**2 for x,y in pairs)/(n-2)
        return a, b, (s2*n/(n*sxx-sx*sx))**0.5
    devs = []
    for L in lanes:
        left  = [(-k*0.25, prof[int((L-k*0.25)/0.25)%N]) for k in range(3,15)]
        right = [( k*0.25, prof[int((L+k*0.25)/0.25)%N]) for k in range(3,15)]
        left  = [(x,y) for x,y in left  if y==y]
        right = [(x,y) for x,y in right if y==y]
        aL,bL,seL = linfit(left); aR,bR,seR = linfit(right)
        x = (aR-aL)/(bL-bR)
        se = ((seL**2+seR**2)**0.5)/abs(bL-bR)
        devs.append(x)
        print(f"lane {L:7.3f}: cusp at lane{x:+.3f} deg (+-{se:.3f}), "
              f"kappa_cusp {aL+bL*x:.5f}, flanks {bL:+.5f}/{bR:+.5f}")
    m = sum(devs)/len(devs)
    sd = (sum((d-m)**2 for d in devs)/(len(devs)-1))**0.5
    print(f"mean cusp-minus-lane {m:+.4f} +- {sd/len(devs)**0.5:.4f} (SE, 6 cusps)")

if __name__ == "__main__":
    main(sys.argv[1], float(sys.argv[2]))
