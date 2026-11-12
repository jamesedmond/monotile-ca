#!/usr/bin/env python3
"""Showers: what the census's "grower" seedings do after generation 150 (FINDINGS 10.11.65).

The census (FINDINGS 10.11.9) labels a seeding "grower" when, at generation
150, it has more live cells or more separate pieces than the census follows one
at a time: 562 seedings of hat C's rule and 70 of the spectre rule. This script
follows such seedings further. Each is re-rooted at its seed (`reroot_record`,
whose control replays both runs side by side by address) and replayed for 600
generations on a radius-320 patch, which nothing reaches before the end
(`ball_tracks`: every vertex-connected component of every generation with its
population, centroid and nearest and farthest ring from the seed).

    cargo build --release -p tiling-core --example reroot_record --example ball_tracks
    python3 crates/certify/census/showers.py survey     # every spectre shower, every ninth hat C shower
    python3 crates/certify/census/showers.py long       # hat C seeding 813 to generation 1,500, and the edge check
    python3 crates/certify/census/showers.py            # the records the showers page replays

`survey` writes results/certificates/census-showers-survey.tsv, one row per
seeding: live cells and pieces at generations 150, 300, 450 and 600, the live
cells within 40 rings of the seed at 300, 450 and 600, and the last generation
with a live cell within 40 rings. The default mode writes
results/certificates/census-showers.jsonl, the page's records, each a census
seeding re-rooted at its seed, and prints the same figures for each.

`long` follows hat C seeding 813 for 1,500 generations on a radius-780 patch,
and checks the page's radius-128 patch against it: the page lets gliders run
into the edge of its patch, so every piece wholly within 124 rings of the seed
must be the same piece (cells, centroid, rings) on both patches at every
generation shown.
"""
import csv, json, os, re, subprocess, sys, tempfile
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
BASE = {'hatC': 'results/hat-tableevolve-r48-s23.jsonl', 'spectre': 'results/spectre-tableevolve-r48-s33.jsonl'}
RULE_NAME = {'hatC': "hat C's rule", 'spectre': 'the spectre rule'}
SOURCE_RADIUS, BALL_RADIUS, GENS, SITE = 384, 320, 600, 40
MARKS = [150, 300, 450, 600]
# The page's records: key, rule, census3 seeding, record radius, what it shows.
PAGE = [
    ('spectre-164', 'spectre', 164, 96, 'a two-tile seed that bursts into ten gliders and leaves nothing behind'),
    ('spectre-2537', 'spectre', 2537, 128, 'a six-cell seed whose pieces go on making new pieces far from the seed'),
    ('hatC-813', 'hatC', 813, 128, 'a two-tile seed that is still launching gliders at generation 1,500'),
]
LONG = ('hatC', 813, 780, 1500, 128, 124, 900)  # rule, seeding, radius, generations; page radius, rings compared, generations compared
TMP = tempfile.mkdtemp(prefix='showers-')


def run(*cmd):
    return subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True, text=True).stdout


def census_rows(rule):
    return list(csv.DictReader(open(os.path.join(ROOT, f'results/certificates/census3/{rule}.tsv')), delimiter='\t'))


def reroot(rule, row, radius):
    rec = json.loads(open(os.path.join(ROOT, BASE[rule])).readline())
    if row['kind'] == 'pair':
        cells, states = [int(row['anchor']), int(row['partner'])], [int(d) for d in row['states']]
    else:
        pairs = [p.split(':') for p in row['states'].split(',')]
        cells, states = [int(a) for a, _ in pairs], [int(b) for _, b in pairs]
    rec.update(initial_cells=cells, initial_states=states, radius=SOURCE_RADIUS)
    src = os.path.join(TMP, f"src-{rule}-{row['seeding']}.jsonl")
    out = os.path.join(TMP, f"root-{rule}-{row['seeding']}-{radius}.jsonl")
    open(src, 'w').write(json.dumps(rec) + '\n')
    log = run('target/release/examples/reroot_record', src, out, radius, 2500, SOURCE_RADIUS)
    control = re.search(r'control: identical for (\d+) generations', log)
    assert control, f"{rule} seeding {row['seeding']}: reroot_record failed\n{log}"
    return out, int(control[1])


def follow(rule, row):
    """Replay a seeding for GENS generations and summarise it at the marked generations."""
    record, _ = reroot(rule, row, 96)
    out = os.path.join(TMP, f"tracks-{rule}-{row['seeding']}.json")
    run('target/release/examples/ball_tracks', record, 0, BALL_RADIUS, GENS, BALL_RADIUS - 8, 2, out)
    gens = json.load(open(out))['generations']
    os.remove(out)
    assert len(gens) == GENS + 1, f"{rule} seeding {row['seeding']}: stopped at generation {len(gens) - 1}"
    s = {'rule': rule, 'seeding': row['seeding'], 'kind': 'two-tile' if row['kind'] == 'pair' else 'six-cell'}
    for g in MARKS:
        s[f'cells{g}'] = sum(c[0] for c in gens[g])
        s[f'pieces{g}'] = len(gens[g])
        if g > 150:
            s[f'site{g}'] = sum(c[0] for c in gens[g] if c[3] <= SITE)
    s['site_last'] = max((g for g, comps in enumerate(gens) if any(c[3] <= SITE for c in comps)), default=0)
    s['largest600'] = max((c[0] for c in gens[GENS]), default=0)
    return s


def growers(rule):
    return [r for r in census_rows(rule) if r['outcome'].startswith('grower')]


COLUMNS = ['rule', 'seeding', 'kind'] + [f'cells{g}' for g in MARKS] + [f'pieces{g}' for g in MARKS] + \
    [f'site{g}' for g in MARKS[1:]] + ['site_last', 'largest600']

if sys.argv[1:] == ['survey']:
    jobs = [('spectre', r) for r in growers('spectre')] + [('hatC', r) for r in growers('hatC')[::9]]
    with ThreadPoolExecutor(max_workers=8) as pool:
        rows = list(pool.map(lambda j: follow(*j), jobs))
    path = os.path.join(ROOT, 'results/certificates/census-showers-survey.tsv')
    with open(path, 'w') as f:
        f.write('\t'.join(COLUMNS) + '\n')
        for s in rows:
            f.write('\t'.join(str(s[c]) for c in COLUMNS) + '\n')
    for rule in ('spectre', 'hatC'):
        rs = [s for s in rows if s['rule'] == rule]
        empty = [s for s in rs if s['site_last'] < 300]
        steady = [s for s in rs if s['cells600'] <= 1.25 * s['cells300']]
        both = [s for s in empty if s in steady]
        print(f"{RULE_NAME[rule]}: {len(rs)} showers followed to generation {GENS}")
        print(f"  no live cell within {SITE} rings of the seed after generation 300: {len(empty)}")
        print(f"  live cells at 600 no more than 1.25 times those at 300: {len(steady)}; both: {len(both)}")
        print(f"  a live cell within {SITE} rings at generation 600: {sum(1 for s in rs if s['site600'] > 0)}")
        cells = sorted(s['cells600'] for s in rs)
        print(f"  live cells at 600: median {cells[len(cells) // 2]}, range {cells[0]}-{cells[-1]}")
    print(f'wrote {os.path.relpath(path, ROOT)}')
    sys.exit()

if sys.argv[1:] == ['long']:
    rule, seeding, radius, gens_long, page_radius, inside, gens_page = LONG
    row = next(r for r in census_rows(rule) if int(r['seeding']) == seeding)
    record, _ = reroot(rule, row, 96)
    big, small = os.path.join(TMP, 'long.json'), os.path.join(TMP, 'page.json')
    run('target/release/examples/ball_tracks', record, 0, radius, gens_long, radius - 8, 2, big)
    subprocess.run(['target/release/examples/ball_tracks', record, '0', str(page_radius), str(gens_page), str(page_radius - 8), '2', small],
                   cwd=ROOT, capture_output=True, env={**os.environ, 'BALL_TRACKS_THROUGH': '1'})
    long_run, page_run = json.load(open(big))['generations'], json.load(open(small))['generations']
    assert len(long_run) == gens_long + 1 and len(page_run) == gens_page + 1
    print(f'{RULE_NAME[rule]}, seeding {seeding}, on a radius-{radius} patch:')
    for g in range(300, gens_long + 1, 300):
        comps = long_run[g]
        print(f"  generation {g}: {sum(c[0] for c in comps)} cells in {len(comps)} pieces; "
              f"{sum(c[0] for c in comps if c[3] <= SITE)} cells within {SITE} rings of the seed")
    within = lambda comps: sorted(tuple(c) for c in comps if c[4] <= inside)
    differ = [g for g in range(gens_page + 1) if within(long_run[g]) != within(page_run[g])]
    print(f'  pieces wholly within {inside} rings, radius {page_radius} against radius {radius}, generations 0-{gens_page}: '
          + (f'first difference at generation {differ[0]}' if differ else 'identical at every generation'))
    assert not differ
    sys.exit()

records = []
for key, rule, seeding, radius, what in PAGE:
    row = next(r for r in census_rows(rule) if int(r['seeding']) == seeding)
    s = follow(rule, row)
    record, control = reroot(rule, row, radius)
    rec = json.loads(open(record).readline())
    seed = 'two-tile seed' if row['kind'] == 'pair' else f"six-cell seed, {row['kind']}"
    rec['note'] = (f"census shower {key}: {RULE_NAME[rule]}, census3 {rule}.tsv row {seeding} ({seed}), {what}; "
                   f"re-rooted at its seed; FINDINGS 10.11.65")
    records.append(rec)
    print(f"{key}: re-rooting control identical for {control} generations at radius {radius}; " +
          '; '.join(f"generation {g}: {s[f'cells{g}']} cells in {s[f'pieces{g}']} pieces" for g in MARKS) +
          f"; last live cell within {SITE} rings of the seed: generation {s['site_last']}; "
          f"cells there at 300/450/600: {s['site300']}/{s['site450']}/{s['site600']}", flush=True)
path = os.path.join(ROOT, 'results/certificates/census-showers.jsonl')
with open(path, 'w') as f:
    for rec in records:
        f.write(json.dumps(rec, separators=(',', ':')) + '\n')
print(f'wrote {os.path.relpath(path, ROOT)}: {len(records)} records')
