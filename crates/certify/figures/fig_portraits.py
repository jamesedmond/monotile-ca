#!/usr/bin/env python3
"""Certificate paper: portraits of the certified objects, one settled frame each, all at one scale.
Each panel is a frame written by `tiling-core --example render_frames` (a record replayed to a
generation and cropped to the object with a few units of padding); this script nests the frames in a
grid at a common units-per-point scale and labels them.
Usage: fig_portraits.py <out.svg> <cols> <label>=<frame.svg> [<label>=<frame.svg> ...]
Labels may contain '|' for a second line."""
import re, sys

out, cols = sys.argv[1], int(sys.argv[2])
items = [a.split('=', 1) for a in sys.argv[3:]]
PANEL = 26.0        # world units shown per panel (square crop, centred)
SC = 6.6            # SVG units per world unit
GAP = 10
LABEL_H = 30
frames = []
for label, path in items:
    s = open(path).read()
    vb = [float(v) for v in re.search(r'viewBox="([^"]+)"', s).group(1).split()]
    body = s[s.index('>', s.index('<svg')) + 1:s.rindex('</svg>')]
    frames.append((label, vb, body))
n = len(frames); rows = (n + cols - 1) // cols
W = cols * (PANEL * SC + GAP) + GAP; H = rows * (PANEL * SC + LABEL_H + GAP) + GAP
el = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W:.0f}" height="{H:.0f}" viewBox="0 0 {W:.0f} {H:.0f}">',
      f'<rect width="{W:.0f}" height="{H:.0f}" fill="white"/>']
for i, (label, vb, body) in enumerate(frames):
    r, c = divmod(i, cols)
    x = GAP + c * (PANEL * SC + GAP); y = GAP + r * (PANEL * SC + LABEL_H + GAP)
    # centre the frame's viewBox in a PANEL x PANEL window (clip to it)
    cx, cy = vb[0] + vb[2] / 2, vb[1] + vb[3] / 2
    vx, vy = cx - PANEL / 2, cy - PANEL / 2
    el.append(f'<clipPath id="clip{i}"><rect x="{x:.1f}" y="{y:.1f}" width="{PANEL * SC:.1f}" height="{PANEL * SC:.1f}"/></clipPath>')
    el.append(f'<g clip-path="url(#clip{i})"><svg x="{x:.1f}" y="{y:.1f}" width="{PANEL * SC:.1f}" height="{PANEL * SC:.1f}" '
              f'viewBox="{vx:.2f} {vy:.2f} {PANEL:.2f} {PANEL:.2f}" preserveAspectRatio="xMidYMid slice">{body}</svg></g>')
    el.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{PANEL * SC:.1f}" height="{PANEL * SC:.1f}" fill="none" stroke="#999" stroke-width="0.6"/>')
    for j, line in enumerate(label.split('|')):
        el.append(f'<text x="{x + PANEL * SC / 2:.1f}" y="{y + PANEL * SC + 13 + 11 * j:.1f}" font-size="{9 if j == 0 else 7.5}" '
                  f'font-weight="{"bold" if j == 0 else "normal"}" text-anchor="middle" font-family="Helvetica, Arial, sans-serif" fill="#111">{line}</text>')
el.append('</svg>')
open(out, 'w').write('\n'.join(el))
print(f'{out}: {n} panels, {cols} columns, {PANEL:.0f} units per panel')
