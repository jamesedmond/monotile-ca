"""Port of SpectreLife.lua (Trevorrow, Oct 2026) — exact spectre geometry in Z[zeta12] (a,b,c,d,k), the shared-vertex neighbour relation, and the EdmondGlider rule with state 2 stable. Needs his package: glu1.sourceforge.io/scripts/SpectreLife.zip (append ?nocache=1 if a cached 404 comes back); run from a directory containing spectrelife/SpectreLife/. FINDINGS 10.11.47."""
import cmath, math, sys, collections
UNIT=[(1,0,0,0),(0,1,0,0),(0,0,1,0),(0,0,0,1),(-1,0,1,0),(0,-1,0,1)]
UNIT=UNIT+[tuple(-x for x in u) for u in UNIT]
EDGE_ANGLES=[0,300,30,90,0,60,150,210,120,180,180,240,330,270]
Z=cmath.exp(1j*math.pi/6)
def load(path):
    lines=open(path).read().split('\n'); parity=lines[0].strip(); reflected=(parity=='odd')
    tiles=[]
    for ln in lines[1:]:
        if not ln.strip(): continue
        a,b,c,d,k=map(int,ln.split()); tiles.append((a,b,c,d,k))
    return reflected,tiles
def vertex_offsets(reflected):
    offs={}
    for k in range(12):
        def unit(angle):
            i=angle//30
            if reflected: i=-i
            return UNIT[(i+k)%12]
        e0,e90=unit(0),unit(90); cur=[-e0[j]-e90[j] for j in range(4)]; o=[]
        for i in range(14):
            o.append(tuple(cur)); u=unit(EDGE_ANGLES[i]); cur=[cur[j]+u[j] for j in range(4)]
        assert tuple(cur)==o[0], "outline does not close"
        offs[k]=o
    return offs
def build(path):
    reflected,tiles=load(path); offs=vertex_offsets(reflected)
    vt=collections.defaultdict(list)
    for idx,(a,b,c,d,k) in enumerate(tiles):
        for o in offs[k]: vt[(a+o[0],b+o[1],c+o[2],d+o[3])].append(idx)
    nbr=[]
    for idx,(a,b,c,d,k) in enumerate(tiles):
        s=set()
        for o in offs[k]: s.update(vt[(a+o[0],b+o[1],c+o[2],d+o[3])])
        s.discard(idx); nbr.append(sorted(s))
    pos=[a+b*Z+c*Z*Z+d*Z**3 for (a,b,c,d,k) in tiles]
    return reflected,tiles,nbr,pos
def step(nbr,cur):
    nxt=[0]*len(cur)
    for i,s in enumerate(cur):
        if s==0:
            n0=n1=0
            for j in nbr[i]:
                if cur[j]==0: n0+=1
                elif cur[j]==1: n1+=1
            if n0>=3 and n1>=1: nxt[i]=3
        elif s==3:
            n1=n3=0
            for j in nbr[i]:
                if cur[j]==1: n1+=1
                elif cur[j]==3: n3+=1
            if n1>=1 and n3<3: nxt[i]=1
        elif s==2: nxt[i]=2
    return nxt
def load_pattern(path):
    st={}
    for ln in open(path):
        ln=ln.strip()
        if not ln or ln.startswith('#') or '=' not in ln or ln.startswith('SpectreLife') or ln.startswith('view') or ln.startswith('rule'): continue
        for tok in ln.split():
            i,s=tok.split('='); st[int(i)-1]=int(s)
    return st
if __name__=='__main__':
    reflected,tiles,nbr,pos=build('spectrelife/SpectreLife/tiling.txt')
    deg=collections.Counter((t[4]%2,len(n)) for t,n in zip(tiles,nbr))
    print("reflected:",reflected,"tiles:",len(tiles)); print("degrees (k parity, degree):",sorted(deg.items()))
