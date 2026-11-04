import math, cmath, collections, numpy as np
from spectrelife_port import build, step, load_pattern
reflected,tiles,nbr,pos=build('spectrelife/SpectreLife/tiling.txt'); N=len(tiles)
interior=[len(nbr[i])>=6 for i in range(N)]
def components(cur):
    seen=[False]*N; comps=[]
    for i in range(N):
        if cur[i]==0 or seen[i]: continue
        st=[i]; seen[i]=True; cells=[]
        while st:
            c=st.pop(); cells.append(c)
            for j in nbr[c]:
                if cur[j]!=0 and not seen[j]: seen[j]=True; st.append(j)
        comps.append(cells)
    return comps
def heading(v): return math.degrees(cmath.phase(v))%360
# ---------- the twelve gliders
cur=[0]*N
for i,s in load_pattern('spectrelife/SpectreLife/egliders.slp').items(): cur[i]=s
tracks=collections.defaultdict(list)   # glider id -> list of (gen, centroid)
ids=None
for g in range(0,260):
    comps=components(cur)
    cents=[sum(pos[c] for c in cells)/len(cells) for cells in comps]
    if ids is None: ids=list(range(len(comps))); prev=cents; order=list(range(len(comps)))
    # match each component to the nearest previous centroid
    assign={}
    for ci,c in enumerate(cents):
        k=min(range(len(prev)),key=lambda k: abs(prev[k]-c)); assign[ci]=k
    for ci,cells in enumerate(comps):
        if all(interior[c] for c in cells): tracks[assign[ci]].append((g,cents[ci],len(cells)))
    prev=[None]*len(prev)
    for ci,k in assign.items(): prev[k]=cents[ci]
    prev=[p if p is not None else complex(1e9,1e9) for p in prev]
    cur=step(nbr,cur)
print("egliders: components at start:",len(ids))
rows=[]
for k,tr in sorted(tracks.items()):
    tr=[t for t in tr if t[0]>=20]
    if len(tr)<40: continue
    gens=np.array([t[0] for t in tr]); xs=np.array([t[1].real for t in tr]); ys=np.array([t[1].imag for t in tr])
    A=np.vstack([gens,np.ones_like(gens)]).T; (vx,_),(vy,_)=np.linalg.lstsq(A,xs,rcond=None)[0],np.linalg.lstsq(A,ys,rcond=None)[0]
    v=complex(vx,vy); h=heading(v); pops=sorted(set(t[2] for t in tr))
    rows.append((h,abs(v),len(tr),pops))
rows.sort()
print(" heading      mod60   speed u/gen  gens  populations")
for h,sp,n,pops in rows: print(f" {h:8.3f}  {h%60:7.3f}   {sp:6.3f}      {n:4d}  {pops}")
hs=np.array([r[0] for r in rows]); hs.sort(); gaps=np.diff(np.concatenate([hs,[hs[0]+360]])); print("gaps:",np.round(gaps,2))
# ---------- the gun with eater
cur=[0]*N
for i,s in load_pattern('spectrelife/SpectreLife/eglider-gun-with-eater.slp').items(): cur[i]=s
hist={}; visited=set(); pops=[]
period=None
for g in range(0,600):
    key=tuple(i for i in range(N) if cur[i]) , tuple(cur[i] for i in range(N) if cur[i])
    if key in hist and period is None: period=g-hist[key]; start=hist[key]
    hist.setdefault(key,g)
    for i in range(N):
        if cur[i] in (1,3): visited.add(i)
    pops.append(sum(1 for s in cur if s))
    cur=step(nbr,cur)
print(f"\ngun+eater: period {period} (first repeat of generation {start if period else '-'}), population range {min(pops)}-{max(pops)}, state-2 cells {sum(1 for s in cur if s==2)}")
vp=np.array([[pos[i].real,pos[i].imag] for i in visited]); mu=vp.mean(axis=0); U,S,Vt=np.linalg.svd(vp-mu,full_matrices=False); d=Vt[0]
h=math.degrees(math.atan2(d[1],d[0]))%180
print(f"gun-to-eater corridor direction (mod 180): {h:.3f}°, mod 60: {h%60:.3f}°; corridor length {2*S[0]/math.sqrt(len(vp)):.0f} units, {len(visited)} cells ever live")
