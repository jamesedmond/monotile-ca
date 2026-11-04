"""Heading of a tracked object: TRACK lines (gen, cell, ring, address) mapped through the
parametric table of the radius-512 default-root hat patch (cell, address, rot, U, V with U, V in
Z[zeta12] coordinates (1, zeta, zeta^2, zeta^3)); position = U + sqrt3 * V in the root frame.
Total-least-squares direction over generations >= settle, sign from net displacement."""
import sys, cmath, math, numpy as np
param, track, settle = sys.argv[1], sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 100
z = cmath.exp(1j*math.pi/6)
def val(s):
    a,b,c,d = (int(x) for x in s.split(','))
    return a + b*z + c*z**2 + d*z**3
pos = {}
with open(param) as f:
    next(f)
    for line in f:
        c,a,r,U,V = line.rstrip('\n').split('\t')
        pos[a] = val(U) + math.sqrt(3)*val(V)
pts = []
for line in open(track):
    _, g, cell, ring, addr = line.rstrip('\n').split('\t')
    if int(g) >= settle and addr in pos:
        p = pos[addr]; pts.append((int(g), p.real, p.imag))
pts = np.array(pts)
g, x, y = pts[:,0], pts[:,1], pts[:,2]
X = np.column_stack([x - x.mean(), y - y.mean()])
w, v = np.linalg.eigh(X.T @ X)
d = v[:, 1]
if np.dot(d, [x[-1]-x[0], y[-1]-y[0]]) < 0: d = -d
heading = math.degrees(math.atan2(d[1], d[0])) % 360
perp = X @ v[:, 0]
span = math.hypot(x[-1]-x[0], y[-1]-y[0])
speed = span / (g[-1]-g[0])
near = min((45.5225 + 60*k for k in range(6)), key=lambda s: abs((heading - s + 180) % 360 - 180))
print(f"{len(pts)} points, gens {g[0]}-{g[-1]}, span {span:.1f} u, speed {speed:.4f} u/gen")
print(f"heading {heading:.4f}  (mod 60: {heading%60:.4f}); nearest hat spoke {near:.4f}, diff {((heading-near+180)%360-180):+.4f}; perp rms {perp.std():.2f} u")
