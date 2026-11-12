#!/usr/bin/env python3
"""Settled-object records for the census objects (FINDINGS 10.11.62, 10.11.63).

For each object in OBJECTS: take the census seeding that produced it (a row
of results/certificates/census3/<rule>.tsv; for the discovery paper's four
gliders the paper's own record, and for hat A's 318 and 642 their seed in
census-seeds.jsonl), replay it on the census's
radius-384 patch to SNAPSHOT_GEN, keep only the object (the component nearest
a heading, plus every component within a few rings for a two-glider object),
and re-root it as a self-contained record (`reroot_record`, whose control
replays both runs side by side by address). Each record is then checked twice:

  * it flies under the web panel's settings (`slide`, window 24, launch
    radius 48, 60 launch generations) with its population inside the range
    of its certificate component;
  * every generation of a 3,000-generation flight is a state of the
    certificate that the census closed for it, joined by one of its
    transitions (`certify trace`: 0 off-certificate, 0 unknown transitions).

Writes results/certificates/census-objects.jsonl in OBJECTS order. Needs the
bucket certificates in out/census3/ and out/census5/ (regenerate with
`certify census`, see FINDINGS 10.11.5 and 10.11.9), the two rebuilt by
rebuild_certificates.py in out/census-certificates/, and release builds of
certify and the two examples.

    cargo build --release -p certify -p tiling-core --example reroot_record --example slide
    python3 crates/certify/census/census_objects.py            # every object
    python3 crates/certify/census/census_objects.py hatA-380   # rebuild one, keep the rest
"""
import csv, json, os, re, subprocess, sys, tempfile
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
BASE = {'hatA': 'results/hat-tableevolve-r48-s21.jsonl', 'hatB': 'results/hat-tableevolve-r48-s22.jsonl',
        'hatC': 'results/hat-tableevolve-r48-s23.jsonl', 'spectre': 'results/spectre-tableevolve-r48-s33.jsonl'}
RULE_NAME = {'hatA': "hat A's rule", 'hatB': "hat B's rule", 'hatC': "hat C's rule", 'spectre': 'the spectre rule'}
SNAPSHOT_GEN, SOURCE_RADIUS, RADIUS, HORIZON = 300, 384, 96, 2500

# key, rule, census3 seeding, heading (deg), distance from the seed (units) or None, gather rings,
# certificate, certificate signature, population range, description
C3, C5, RC, CC = 'out/census3/', 'out/census5/', 'results/certificates/', 'out/census-certificates/'
OBJECTS = [
    # Convoys (FINDINGS 10.11.9, 10.11.10).
    ('hatA-638', 'hatA', 271, 225.2, None, 8, C3 + 'hatA.tsv.bucket3.json', '605b4b68339b', (12, 15),
     '638-state formation pair (a 257-family glider and the 381 glider)'),
    ('hatA-607', 'hatA', 3318, 165.0, None, 10, C3 + 'hatA.tsv.bucket11.json', '010a7aec4b4c', (12, 15),
     '607-state formation pair (a 257-family glider and the 380 glider)'),
    ('hatA-561', 'hatA', 3283, 165.04, 634.7, 8, C3 + 'hatA.tsv.bucket10.json', '3b9b97532bd0', (11, 15),
     '561-state bound pair (a 257-family glider and the 381 glider, fused for about half the cycle)'),
    ('hatB-638', 'hatB', 276, 225.2, None, 8, C3 + 'hatB.tsv.bucket3.json', 'af88fd5abf86', (10, 13),
     '638-state formation pair (the 381 glider and a 630-family glider)'),
    ('hatB-607', 'hatB', 2912, 105.6, None, 10, C3 + 'hatB.tsv.bucket11.json', '9e3b0fc1ce00', (10, 13),
     '607-state formation pair (the 381 glider and a 630-family glider)'),
    ('hatB-162', 'hatB', 2547, 165.86, 637.5, 0, C3 + 'hatB.tsv.bucket6.json', '3683630b9ea8', (9, 12),
     '162-state glider with passenger (a 630-family glider carrying a fused 4-5-cell attachment)'),
    ('hatB-321a', 'hatB', 3437, 165.45, None, 0, C3 + 'hatB.tsv.bucket13.json', '6308b983dec3', (9, 12),
     '321-state glider with passenger (231 class-blind)'),
    ('hatB-321b', 'hatB', 2630, 285.87, None, 0, C3 + 'hatB.tsv.bucket9.json', '99f1c570727c', (9, 12),
     '321-state glider with passenger (233 class-blind)'),
    ('hatB-473', 'hatB', 2982, 225.45, None, 0, C3 + 'hatB.tsv.bucket8.json', '35c1bd44f2fd', (9, 11),
     '473-state glider with passenger (the 381 glider carrying a fused 4-5-cell attachment)'),
    ('spectre-3327', 'spectre', 1545, 227.7, None, 10, C3 + 'spectre.tsv.bucket2.json', '25024f4819a6', (8, 12),
     '3,327-state formation pair (two 2,542 gliders, fused in about half their states)'),
    ('spectre-3785', 'spectre', 236, 225.58, 430.5, 5, C3 + 'spectre-6327.tsv.bucket0.json', '6b05b5ecf527', (9, 12),
     '3,785-state formation pair (two 2,542 gliders)'),
    # Single gliders (FINDINGS 10.11.5, 10.11.63). The 257, 256 and 127 of hat A's and hat B's rules are
    # one closed set at collar 4 and separate at collar 5, so they are checked on their collar-5 certificates.
    ('hatA-380', 'hatA', 3, 225.9, None, 0, C3 + 'hatA.tsv.bucket2.json', '1dbb41448014', (6, 7),
     '380-state single glider'),
    ('hatA-257', 'hatA', 21, 345.2, None, 0, C5 + 'hatA.tsv.bucket4.json', 'e9040de0b644', (6, 8),
     "257-state single glider (the earlier paper's captured object; one of the three gliders of the collar-4 630 set)"),
    ('hatA-256', 'hatA', 1, 225.5, None, 0, C5 + 'hatA.tsv.bucket0.json', 'f51cff2f64ec', (6, 8),
     '256-state single glider (one of the three gliders of the collar-4 630 set)'),
    ('hatA-127', 'hatA', 19, 165.5, None, 0, C5 + 'hatA.tsv.bucket3.json', '90e54cf418a2', (6, 8),
     '127-state single glider (one of the three gliders of the collar-4 630 set)'),
    ('hatB-380', 'hatB', 2, 224.8, None, 0, C3 + 'hatB.tsv.bucket0.json', '25b8be6525b8', (5, 6),
     '380-state single glider'),
    ('hatB-257', 'hatB', 20, 345.2, None, 0, C5 + 'hatB.tsv.bucket4.json', '371745109611', (5, 7),
     '257-state single glider (one of the three gliders of the collar-4 630 set)'),
    ('hatB-256', 'hatB', 6, 226.1, None, 0, C5 + 'hatB.tsv.bucket0.json', '69d85b70c75b', (5, 7),
     '256-state single glider (one of the three gliders of the collar-4 630 set)'),
    ('hatB-127', 'hatB', 24, 164.5, None, 0, C5 + 'hatB.tsv.bucket3.json', '76e3086b87d3', (5, 7),
     '127-state single glider (one of the three gliders of the collar-4 630 set)'),
    ('hatC-386', 'hatC', 12, 105.9, None, 0, C3 + 'hatC.tsv.bucket1.json', '76337d9ae120', (6, 10),
     '386-state single glider'),
    ('hatC-129', 'hatC', 30, 165.1, None, 0, C3 + 'hatC.tsv.bucket2.json', 'be7e896d2b60', (6, 10),
     '129-state single glider'),
    ('hatC-257a', 'hatC', 237, 104.5, None, 0, RC + 'hat-C-s23-lane105-k4-R11.json', '5c29f970e0b1', (6, 10),
     "257-state single glider (the earlier paper's 105-degree glider)"),
    ('hatC-257b', 'hatC', 469, 46.5, None, 0, C3 + 'hatC.tsv.bucket3.json', '6248ef60819e', (6, 10),
     'second 257-state single glider'),
    ('spectre-1454', 'spectre', 2, 176.4, None, 0, C3 + 'spectre.tsv.bucket0.json', 'c5c1b13c1fd6', (4, 8),
     '1,454-state glider of the second lane family'),
    # The discovery paper's four gliders (FINDINGS 10.11.67), cut from the paper's own records (seeding None)
    # so that a flight shows the glider alone: hat A's seed also launches a glider that dies at generation
    # 42, hat C's breaks into three gliders after about 150 generations, the spectre's launches two.
    ('hatA-381', 'hatA', None, 226.5, None, 0, RC + 'hat-A-s21-k4-R9.json', 'cc524fb136bc', (6, 8),
     "381-state single glider (the discovery paper's hat A)"),
    ('hatB-381', 'hatB', None, 226.5, None, 0, RC + 'hat-B-s22-k4-R10.json', '9ba8346bd1da', (5, 7),
     "381-state single glider (the discovery paper's hat B)"),
    ('hatC-381', 'hatC', None, 341.4, None, 0, RC + 'hat-C-s23-lane345a-k4-R11.json', 'b679e54222be', (6, 9),
     "381-state single glider (the discovery paper's hat C on its 345-degree heading)"),
    ('spectre-2542', 'spectre', None, 107.8, None, 0, RC + 'spectre-s33-lane108-k4-R9.json', '6239ceef8610', (4, 6),
     "2,542-state single glider (the discovery paper's spectre glider on its 108-degree heading)"),
    # Hat A's 318 and 642 formation pairs, cut from their two-cell seeds in census-seeds.jsonl (seeding
    # ('seeds', i)) so that a flight starts with the settled pair; checked on the certificates that
    # rebuild_certificates.py rebuilds from those seeds.
    ('hatA-318', 'hatA', ('seeds', 1), 105.2, None, 10, CC + 'hatA-318.json', '3755353bd316', (12, 15),
     '318-state formation pair (two gliders of the 257 family)'),
    ('hatA-642', 'hatA', ('seeds', 2), 225.3, None, 10, CC + 'hatA-642.json', 'a4b48e4b8f84', (12, 15),
     '642-state formation pair (a 257-family glider and the 380 glider)'),
]


def run(*cmd):
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True).stdout


def source_record(rule, seeding):
    rec = json.loads(open(os.path.join(ROOT, BASE[rule])).readline())
    if seeding is None:  # the discovery paper's own seed
        rec.update(radius=SOURCE_RADIUS)
        return rec, 'paper'
    if isinstance(seeding, tuple):  # a record of census-seeds.jsonl, replayed on the larger patch
        rec = json.loads(open(os.path.join(ROOT, 'results/certificates/census-seeds.jsonl')).read().splitlines()[seeding[1]])
        rec.update(radius=SOURCE_RADIUS)
        return rec, 'seeds'
    for row in csv.DictReader(open(os.path.join(ROOT, f'results/certificates/census3/{rule}.tsv')), delimiter='\t'):
        if int(row['seeding']) == seeding:
            if row['kind'] == 'pair':
                cells, states = [int(row['anchor']), int(row['partner'])], [int(d) for d in row['states']]
            else:
                pairs = [p.split(':') for p in row['states'].split(',')]
                cells, states = [int(a) for a, _ in pairs], [int(b) for _, b in pairs]
            rec.update(initial_cells=cells, initial_states=states, radius=SOURCE_RADIUS)
            return rec, row['kind']
    sys.exit(f'{rule} seeding {seeding} not found')


def build(obj):
    key, rule, seeding, heading, distance, gather, certificate, sig, pop, what = obj
    tmp = tempfile.mkdtemp(prefix=f'census-{key}-')
    src, out = os.path.join(tmp, 'src.jsonl'), os.path.join(tmp, 'out.jsonl')
    rec, kind = source_record(rule, seeding)
    open(src, 'w').write(json.dumps(rec) + '\n')
    args = [src, out, RADIUS, HORIZON, SOURCE_RADIUS, SNAPSHOT_GEN, heading, gather] + ([distance] if distance else [])
    log = run('target/release/examples/reroot_record', *map(str, args))
    control = re.search(r'control: identical for (\d+) generations', log)
    assert control, f'{key}: reroot_record failed\n{log}'
    fly = run('target/release/examples/slide', out, '0', '24', '--launch-radius', '48', '--launch-gens', '60', '--rings', '3000')
    band = re.search(r'population band: min (\d+) max (\d+)', fly)
    flies = 'reached target: true' in fly and band and pop[0] <= int(band[1]) and int(band[2]) <= pop[1]
    trace = run('target/release/certify', 'trace', out, certificate, '--gens', '3000')
    on = re.search(r'off-certificate generations: (\d+); transitions not in the certificate: (\d+)', trace)
    certified = bool(on) and on[1] == '0' and on[2] == '0'
    record = json.loads(open(out).readline())
    seed = f'{kind} seeding' if kind == 'pair' else f'soup seeding ({kind})'
    source = (f"the discovery paper's record {BASE[rule]}" if kind == 'paper' else
              f'census-seeds.jsonl #{seeding[1]} (a two-cell seed)' if kind == 'seeds' else
              f'census3 {rule}.tsv row {seeding} ({seed})')
    record['note'] = (f"census object {key}: {RULE_NAME[rule]}, {what}; certificate signature {sig}; "
                      f"the settled object itself: {source} replayed to generation "
                      f"{SNAPSHOT_GEN} and everything else removed; FINDINGS {'10.11.67' if kind in ('paper', 'seeds') else '10.11.62'}")
    print(f"{key}: {len(record['initial_cells'])} cells, max population {record['max_population']}, "
          f"re-rooting control identical for {control[1]} generations; "
          f"flies with population {band[1]}-{band[2]}: {'yes' if flies else 'NO'}; "
          f"3,000 generations on its certificate: {'yes' if certified else 'NO'}", flush=True)
    return record, flies and certified


path = os.path.join(ROOT, 'results/certificates/census-objects.jsonl')
# With keys on the command line only those objects are rebuilt; the rest are kept from the existing file.
only = set(sys.argv[1:])
kept = {}
if only and os.path.exists(path):
    for line in open(path):
        kept[re.match(r'census object (\S+):', json.loads(line)['note'])[1]] = json.loads(line)
todo = [o for o in OBJECTS if not only or o[0] in only or o[0] not in kept]
with ThreadPoolExecutor(max_workers=6) as pool:
    results = dict(zip((o[0] for o in todo), pool.map(build, todo)))
if not all(ok for _, ok in results.values()):
    sys.exit('some records failed a check; nothing written')
records = [results[o[0]][0] if o[0] in results else kept[o[0]] for o in OBJECTS]
open(path, 'w').write(''.join(json.dumps(r, separators=(',', ':')) + '\n' for r in records))
print(f'wrote {len(records)} records to {path} ({len(results)} rebuilt)')
