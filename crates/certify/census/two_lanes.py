#!/usr/bin/env python3
"""One seed, both spectre lane families (FINDINGS 10.11.64).

Census seeding 2191 of the spectre rule (a row of
results/certificates/census3/spectre.tsv, a six-cell random seed) launches one
glider of each lane family and nothing else: the 2,542-state glider of the
earlier paper and the 1,454-state glider of the second family. This script

  * re-roots the seeding at its seed as a self-contained radius-96 record
    (`reroot_record`, whose control replays both runs side by side by address)
    and writes it to results/certificates/census-two-lanes.jsonl;
  * replays it on a radius-264 patch (`ball_tracks`) and measures both
    gliders against the graph-metric ball of the seed: the generation at which
    they part, each one's farthest ring at every generation, heading and speed;
  * writes the tracks and the outlines of the balls the gliders have reached at
    generations 100, 200, ... 500 to web/src/census/second-lane.json, the data
    of the figure on the census page.

With seeding numbers on the command line it only measures those seedings and
writes nothing (the control: seeding 1984 launches the same two gliders nine
rings apart, and the gap must stay constant if both keep the same ring rate).

    cargo build --release -p tiling-core --example reroot_record --example ball_tracks
    python3 crates/certify/census/two_lanes.py          # build the record and the figure data
    python3 crates/certify/census/two_lanes.py 1984     # measure another seeding
"""
import collections, csv, json, math, os, re, subprocess, sys, tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
BASE = 'results/spectre-tableevolve-r48-s33.jsonl'
SEEDING, SOURCE_RADIUS, RADIUS, BALL_RADIUS, GENS, BIN_DEG = 2191, 384, 96, 264, 500, 0.5
MARK_GENS = [100, 200, 300, 400, 500]
# The two families' headings mod 60 from the 10^6-ring flights (FINDINGS 10.11.6).
FAMILY = {'2542': 48.014, '1454': 57.170}


def run(*cmd):
    return subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True, text=True).stdout


def source_record(seeding):
    rec = json.loads(open(os.path.join(ROOT, BASE)).readline())
    for row in csv.DictReader(open(os.path.join(ROOT, 'results/certificates/census3/spectre.tsv')), delimiter='\t'):
        if int(row['seeding']) == seeding:
            if row['kind'] == 'pair':
                cells, states = [int(row['anchor']), int(row['partner'])], [int(d) for d in row['states']]
            else:
                pairs = [p.split(':') for p in row['states'].split(',')]
                cells, states = [int(a) for a, _ in pairs], [int(b) for _, b in pairs]
            rec.update(initial_cells=cells, initial_states=states, radius=SOURCE_RADIUS)
            return rec, row
    sys.exit(f'spectre seeding {seeding} not found')


def reroot(seeding, radius, tmp):
    src, out = os.path.join(tmp, f'src-{seeding}.jsonl'), os.path.join(tmp, f'root-{seeding}.jsonl')
    rec, row = source_record(seeding)
    open(src, 'w').write(json.dumps(rec) + '\n')
    log = run('target/release/examples/reroot_record', src, out, radius, 2500, SOURCE_RADIUS)
    control = re.search(r'control: identical for (\d+) generations', log)
    assert control, f'seeding {seeding}: reroot_record failed\n{log}'
    return out, row, int(control[1])


def angle(x, y):
    return math.degrees(math.atan2(y, x)) % 360


def measure(seeding, tmp, rings=None):
    """Both gliders of a two-glider seeding against the ball of their seed."""
    record, row, _ = reroot(seeding, RADIUS, tmp)
    out = os.path.join(tmp, f'tracks-{seeding}.json')
    run('target/release/examples/ball_tracks', record, 0, BALL_RADIUS, GENS,
        ','.join(map(str, rings or [BALL_RADIUS - 8])), BIN_DEG, out)
    data = json.load(open(out))
    gens = data['generations']
    assert len(gens) == GENS + 1, f'seeding {seeding}: the run stopped at generation {len(gens) - 1}'
    # The gliders have parted for good after the last generation without exactly two components.
    split = 1 + max(g for g, comps in enumerate(gens) if len(comps) != 2)
    # At the last generation the second family's glider is the one counter-clockwise of the other, as
    # seen from the seed; earlier generations are assigned backwards, each glider to its nearer piece.
    a, b = gens[GENS]
    if (angle(b[1], b[2]) - angle(a[1], a[2])) % 360 > 180:
        a, b = b, a
    tracks = {'2542': {GENS: a}, '1454': {GENS: b}}
    for g in range(GENS - 1, split - 1, -1):
        p, q = gens[g]
        if math.hypot(p[1] - a[1], p[2] - a[2]) + math.hypot(q[1] - b[1], q[2] - b[2]) > \
                math.hypot(q[1] - a[1], q[2] - a[2]) + math.hypot(p[1] - b[1], p[2] - b[2]):
            p, q = q, p
        a, b = p, q
        tracks['2542'][g], tracks['1454'][g] = a, b
    stats = {}
    for name, track in tracks.items():
        gs = [g for g in track if g >= 100]
        n, mg = len(gs), sum(gs) / len(gs)
        mx, my = sum(track[g][1] for g in gs) / n, sum(track[g][2] for g in gs) / n
        den = sum((g - mg) ** 2 for g in gs)
        vx = sum((g - mg) * (track[g][1] - mx) for g in gs) / den
        vy = sum((g - mg) * (track[g][2] - my) for g in gs) / den
        stats[name] = dict(heading=angle(vx, vy), speed=math.hypot(vx, vy),
                           ring_rate=(track[GENS][4] - track[100][4]) / (GENS - 100),
                           population=(min(c[0] for c in track.values()), max(c[0] for c in track.values())))
    lead = collections.Counter(tracks['2542'][g][4] - tracks['1454'][g][4] for g in range(max(split, 100), GENS + 1))
    print(f"seeding {seeding} ({row['kind']}): census outcome {row['outcome']}")
    print(f'  the gliders have parted by generation {split}')
    for name, s in stats.items():
        print(f"  {name}: heading {s['heading']:.2f}° ({s['heading'] % 60:.2f}° mod 60), {s['speed']:.4f} units per generation, "
              f"{s['ring_rate']:.3f} rings per generation, population {s['population'][0]}-{s['population'][1]}")
    print(f"  farthest ring of the 2,542 minus farthest ring of the 1,454, generations {max(split, 100)}-{GENS}: "
          f"{min(lead)} to {max(lead)} (counts {dict(sorted(lead.items()))})")
    return record, data, split, tracks, stats, lead


tmp = tempfile.mkdtemp(prefix='two-lanes-')
if sys.argv[1:]:
    for arg in sys.argv[1:]:
        measure(int(arg), tmp)
    sys.exit()

record, data, split, tracks, stats, lead = measure(SEEDING, tmp)
# The checks the page's statements rest on.
for name, s in stats.items():
    off = abs((s['heading'] - FAMILY[name] + 30) % 60 - 30)
    assert off < 0.1, f'{name}: heading {s["heading"]:.3f}° is {off:.3f}° from its family'
    assert abs(s['ring_rate'] - 0.5) < 0.01, f'{name}: {s["ring_rate"]} rings per generation'
assert 0 <= min(lead) and max(lead) <= 3, f'ring gap {sorted(lead)}'

# Second pass: the outline of the ball at the farthest ring reached at each marked generation.
rings = [max(tracks['2542'][g][4], tracks['1454'][g][4]) for g in MARK_GENS]
_, data, split2, tracks2, _, _ = measure(SEEDING, tmp, rings)
assert split2 == split and tracks2 == tracks
gens = data['generations']
trunk = []
for g in range(0, split + 1, 2):
    n = sum(c[0] for c in gens[g])
    trunk.append([round(sum(c[0] * c[1] for c in gens[g]) / n, 1), round(sum(c[0] * c[2] for c in gens[g]) / n, 1)])
figure = {
    'source': f'census3 spectre.tsv row {SEEDING}; crates/certify/census/two_lanes.py',
    'binDeg': BIN_DEG,
    'split': split,
    'trunk': trunk,
    'tracks': {name: [[round(track[g][1], 1), round(track[g][2], 1)] for g in range(split, GENS + 1, 2)]
               for name, track in tracks.items()},
    'marks': [{'gen': g, 'ring': ring, 'reach': [round(r, 1) for r in o['reach']],
               'at': {name: [round(track[g][1], 1), round(track[g][2], 1), track[g][4]] for name, track in tracks.items()}}
              for g, ring, o in zip(MARK_GENS, rings, data['outlines'])],
    'stats': {name: {'heading': round(s['heading'], 2), 'speed': round(s['speed'], 4)} for name, s in stats.items()},
}
for mark, o in zip(figure['marks'], data['outlines']):
    assert o['ring'] == mark['ring']
path = os.path.join(ROOT, 'web/src/census/second-lane.json')
open(path, 'w').write(json.dumps(figure, separators=(',', ':')) + '\n')
print(f'wrote {os.path.relpath(path, ROOT)}: balls at rings {rings} (generations {MARK_GENS})')

rec = json.loads(open(record).readline())
_, row, control = reroot(SEEDING, RADIUS, tmp)
rec['note'] = (f"census two lanes: the spectre rule, census3 spectre.tsv row {SEEDING} ({row['kind']}), a six-cell seed that "
               f"launches one glider of each lane family (the 2,542 and the 1,454) and nothing else, re-rooted at its seed; "
               f"FINDINGS 10.11.64")
path = os.path.join(ROOT, 'results/certificates/census-two-lanes.jsonl')
open(path, 'w').write(json.dumps(rec, separators=(',', ':')) + '\n')
print(f"wrote {os.path.relpath(path, ROOT)}: radius {rec['radius']}, {rec['outcome']}, max population {rec['max_population']}, "
      f"re-rooting control identical for {control} generations")
