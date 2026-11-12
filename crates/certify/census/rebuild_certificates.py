#!/usr/bin/env python3
"""Rebuild the certificate of every census object from its record (FINDINGS 10.11.66).

The census (FINDINGS 10.11.9) certified 32 travelling objects. Their
certificates are not stored: each is rebuilt from a record in the repository
by one `certify closure` command, in under a minute once the atlas exists.
results/certificates/census-certificates.tsv lists, for every object, that
command and what it produces. This script runs the commands and compares.

    cargo build --release -p certify
    python3 crates/certify/census/rebuild_certificates.py                 # rebuild all 32, compare with the table
    python3 crates/certify/census/rebuild_certificates.py hatA-380        # only these objects
    python3 crates/certify/census/rebuild_certificates.py --check         # also run the independent checker on each
    python3 crates/certify/census/rebuild_certificates.py --write         # rewrite the table from this run

Two atlases are needed, the hat's and the spectre's at radius 11. They are
looked for as <atlas-dir>/atlas-<family>-r11.json (default directory `out`,
where `certify atlas` writes) and generated with
`certify atlas <family> 11 --verify` if missing, a few minutes each. The
rebuilt certificates are written to <out-dir>/<object>.json (default
out/census-certificates).

For each object the script requires that

  * the closure is closed, has no death and no error, every centre ball of
    the flight is in the atlas, and every observed transition of the flight
    is a transition of the closure;
  * `certify sccs` finds exactly one object in the closure, a travelling one
    with its orientation locked, and its signature is the one the census
    recorded (EXPECTED below, taken from the census's own certificates);
  * the row (situation count, class-blind count, population, heading window,
    speed bound) equals the row of the table.

Columns of the table. `collar` is the collar radius k of the closure and
`atlas` the ball radius R. `closure_states` and `closure_transitions`
describe the rebuilt file. `situations` is the size of the object: for a
collar-4 closure it equals `closure_states`; the 257, 256 and 127 of hat A's
and hat B's rules share one closed set of 630 situations at collar 4, so they
are closed at collar 5 and counted after projecting each collar-5 situation
to collar 4 (`certify sccs --project 4`). `class_blind` counts situations
with the tile classes erased. `window_lo`, `window_hi` (degrees mod 30) and
`speed` (units per generation) are the heading window and the lower bound on
the speed that `certify sccs` prints, rounded outward.

Two cautions about comparing with other certificate files of the same
object. The heading window is a bound taken about one reference cycle of the
file (the first-successor walk from its first state), so two files holding
the same situations and transitions in a different order can print windows
that differ in the last digit; each is a valid bound. The signature is a
hash of the sorted situation keys by Rust's `DefaultHasher`, which is fixed
for a given toolchain and not guaranteed across Rust releases; the counts do
not depend on it.
"""
import argparse, csv, json, os, re, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
TABLE = 'results/certificates/census-certificates.tsv'
CERTIFY = 'target/release/certify'
ATLAS_RADIUS = 11
PAPER = {'hatA': 'results/hat-tableevolve-r48-s21.jsonl', 'hatB': 'results/hat-tableevolve-r48-s22.jsonl',
         'hatC': 'results/hat-tableevolve-r48-s23.jsonl', 'spectre': 'results/spectre-tableevolve-r48-s33.jsonl'}
SEEDS, OBJECTS_FILE = 'results/certificates/census-seeds.jsonl', 'results/certificates/census-objects.jsonl'
RULE_NAME = {'hatA': 'hat A', 'hatB': 'hat B', 'hatC': 'hat C', 'spectre': 'spectre'}

# Where each object's record is and how the closure reads it.
#   paper(start, component): the earlier paper's record, from its two-cell seed, on a radius-384 patch;
#   seed(i, start):          record i of census-seeds.jsonl, on a radius-384 patch;
#   settled():               the object's settled record in census-objects.jsonl (found by its key), radius 256.
def paper(start, component=None):
    return ('paper', None, 384, start, component)
def seed(i, start=60):
    return ('seed', i, 384, start, None)
def settled():
    return ('settled', None, 256, 60, None)

# key, kind, source, collar, signature and situation count the census recorded.
EXPECTED = [
    ('hatA-381', 'single glider', paper(60), 4, 'cc524fb136bc', 381),
    ('hatA-380', 'single glider', settled(), 4, '1dbb41448014', 380),
    ('hatA-257', 'single glider', settled(), 5, 'e9040de0b644', 257),
    ('hatA-256', 'single glider', settled(), 5, 'f51cff2f64ec', 256),
    ('hatA-127', 'single glider', settled(), 5, '90e54cf418a2', 127),
    ('hatB-381', 'single glider', paper(60), 4, '9ba8346bd1da', 381),
    ('hatB-380', 'single glider', settled(), 4, '25b8be6525b8', 380),
    ('hatB-257', 'single glider', settled(), 5, '371745109611', 257),
    ('hatB-256', 'single glider', settled(), 5, '69d85b70c75b', 256),
    ('hatB-127', 'single glider', settled(), 5, '76e3086b87d3', 127),
    ('hatC-381', 'single glider', paper(200, 1), 4, 'b679e54222be', 381),
    ('hatC-386', 'single glider', settled(), 4, '76337d9ae120', 386),
    ('hatC-129', 'single glider', settled(), 4, 'be7e896d2b60', 129),
    ('hatC-257a', 'single glider', settled(), 4, '5c29f970e0b1', 257),
    ('hatC-257b', 'single glider', settled(), 4, '6248ef60819e', 257),
    ('spectre-2542', 'single glider', paper(60, 0), 4, '6239ceef8610', 2542),
    ('spectre-1454', 'single glider', settled(), 4, 'c5c1b13c1fd6', 1454),
    ('hatA-318', 'formation pair', seed(1, 150), 4, '3755353bd316', 318),
    ('hatA-642', 'formation pair', seed(2, 150), 4, 'a4b48e4b8f84', 642),
    ('hatA-607', 'formation pair', settled(), 4, '010a7aec4b4c', 607),
    ('hatA-638', 'formation pair', settled(), 4, '605b4b68339b', 638),
    ('hatB-607', 'formation pair', settled(), 4, '9e3b0fc1ce00', 607),
    ('hatB-638', 'formation pair', settled(), 4, 'af88fd5abf86', 638),
    ('spectre-3327', 'formation pair', settled(), 4, '25024f4819a6', 3327),
    ('spectre-3785', 'formation pair', settled(), 4, '6b05b5ecf527', 3785),
    ('hatA-441', 'bound pair', seed(10), 4, '46b0f631a6f6', 441),
    ('hatA-561', 'bound pair', settled(), 4, '3b9b97532bd0', 561),
    ('hatB-162', 'glider with passenger', settled(), 4, '3683630b9ea8', 162),
    ('hatB-321a', 'glider with passenger', settled(), 4, '6308b983dec3', 321),
    ('hatB-321b', 'glider with passenger', settled(), 4, '99f1c570727c', 321),
    ('hatB-471', 'glider with passenger', seed(9), 4, 'dded0e762d03', 471),
    ('hatB-473', 'glider with passenger', settled(), 4, '35c1bd44f2fd', 473),
]
COLUMNS = ['object', 'rule', 'kind', 'collar', 'atlas', 'closure_states', 'closure_transitions', 'situations',
           'class_blind', 'population', 'window_lo', 'window_hi', 'speed', 'signature', 'command']

ap = argparse.ArgumentParser(description='Rebuild the census certificates and compare with ' + TABLE)
ap.add_argument('objects', nargs='*', help='object keys (default: all)')
ap.add_argument('--atlas-dir', default='out')
ap.add_argument('--out-dir', default='out/census-certificates')
ap.add_argument('--jobs', type=int, default=4)
ap.add_argument('--check', action='store_true', help='also run crates/certify/check/check_certificate.py on each')
ap.add_argument('--write', action='store_true', help='rewrite the table from this run (all objects only)')
args = ap.parse_args()


def run(*cmd):
    p = subprocess.run(list(map(str, cmd)), cwd=ROOT, capture_output=True, text=True)
    return p.stdout + p.stderr


def atlas_path(family, shown=False):
    """The atlas file; `shown` gives the path as the table prints it (the default directory)."""
    return os.path.join('out' if shown else args.atlas_dir, f'atlas-{family}-r{ATLAS_RADIUS}.json')


settled_index = {}
for i, line in enumerate(open(os.path.join(ROOT, OBJECTS_FILE))):
    m = re.match(r'census object (\S+):', json.loads(line)['note'])
    if m:
        settled_index[m[1]] = i


def command(key, source, collar, atlas, out):
    rule = key.split('-')[0]
    where, index, radius, start, component = source
    record = {'paper': PAPER[rule], 'seed': SEEDS, 'settled': OBJECTS_FILE}[where]
    if where == 'settled':
        index = settled_index[key]
    cmd = ['certify', 'closure', record, atlas, '--radius', radius, '--start', start, '--k', collar]
    if index is not None:
        cmd += ['--index', index]
    if component is not None:
        cmd += ['--component', component]
    return list(map(str, cmd + ['--out', out])), record, index or 0


def need(pattern, text, what, key):
    m = re.search(pattern, text)
    if not m:
        raise AssertionError(f'{key}: {what}\n{text[-1500:]}')
    return m


def rebuild(obj):
    key, kind, source, collar, signature, situations = obj
    rule = key.split('-')[0]
    family = 'spectre' if rule == 'spectre' else 'hat'
    out = os.path.join(args.out_dir, f'{key}.json')
    cmd, record, index = command(key, source, collar, atlas_path(family), out)
    t0 = time.time()
    log = run(CERTIFY, *cmd[1:])
    need(r'closure verdict: CLOSED, no death', log, 'the closure did not close', key)
    c = need(r'closure: (\d+) states, (\d+) transitions, dead state: None, errors: 0 ', log, 'dead state or errors', key)
    need(r'death transitions: 0\n', log, 'death transitions', key)
    t = need(r'flight terrain: (\d+)/(\d+) centre balls found in atlas', log, 'no terrain line', key)
    assert t[1] == t[2], f'{key}: flight terrain {t[0]}'
    o = need(r'control: (\d+)/(\d+) observed transitions are closure transitions \((\d+) observed', log, 'no control line', key)
    assert o[1] == o[2] and o[3] == '0', f'{key}: {o[0]}'
    listing = run(CERTIFY, 'sccs', *(['--project', '4'] if collar != 4 else []), out)
    need(r'\n?1 distinct objects across 1 files', listing, 'the closure holds more than one object', key)
    m = need(r'  ([0-9a-f]{12}) \| (\d+) states \((\d+) class-blind\), pop (\d+)-(\d+), settled \| window \[[^\]]*\] '
             r'\(mod 30 \[([\d.]+)°, ([\d.]+)°\], width [\d.]+°; reference cycle [\d.]+°\), speed ≥ ([\d.]+), lock yes',
             listing, 'not a settled travelling object with locked orientation', key)
    assert m[1] == signature, f'{key}: signature {m[1]}, the census recorded {signature}'
    assert int(m[2]) == situations, f'{key}: {m[2]} situations, the census recorded {situations}'
    if collar == 4:
        assert c[1] == m[2], f'{key}: the closure has {c[1]} states and its object {m[2]}'
    shown, _, _ = command(key, source, collar, atlas_path(family, shown=True), f'out/census-certificates/{key}.json')
    row = dict(object=key, rule=RULE_NAME[rule], kind=kind, collar=str(collar), atlas=str(ATLAS_RADIUS),
               closure_states=c[1], closure_transitions=c[2], situations=m[2], class_blind=m[3],
               population=f'{m[4]}-{m[5]}', window_lo=m[6], window_hi=m[7], speed=m[8], signature=m[1],
               command=' '.join(shown))
    checked = ''
    if args.check:
        verdict = run('python3', 'crates/certify/check/check_certificate.py', out, atlas_path(family), record, '--index', index)
        need(r'VERDICT: PASS', verdict, 'the independent checker did not pass', key)
        checked = '; independent checker: PASS'
    print(f"{key}: {m[2]} situations ({m[3]} class-blind), signature {m[1]}, window {m[6]}-{m[7]} mod 30, "
          f"speed >= {m[8]}{checked} ({time.time() - t0:.0f}s)", flush=True)
    return row


todo = [o for o in EXPECTED if not args.objects or o[0] in args.objects]
unknown = set(args.objects) - {o[0] for o in EXPECTED}
if unknown:
    sys.exit(f'unknown objects: {sorted(unknown)}')
if args.write and args.objects:
    sys.exit('--write rewrites the whole table; run it without object keys')
os.makedirs(os.path.join(ROOT, args.out_dir), exist_ok=True)
for family in sorted({'spectre' if o[0].startswith('spectre') else 'hat' for o in todo}):
    if not os.path.exists(os.path.join(ROOT, atlas_path(family))):
        print(f'generating {atlas_path(family)} (a few minutes)', flush=True)
        run(CERTIFY, 'atlas', family, ATLAS_RADIUS, '--verify', '--out', atlas_path(family))
        assert os.path.exists(os.path.join(ROOT, atlas_path(family))), f'atlas generation failed for {family}'
with ThreadPoolExecutor(max_workers=args.jobs) as pool:
    rows = list(pool.map(rebuild, todo))

path = os.path.join(ROOT, TABLE)
if args.write:
    with open(path, 'w') as f:
        f.write('\t'.join(COLUMNS) + '\n')
        for row in rows:
            f.write('\t'.join(row[c] for c in COLUMNS) + '\n')
    print(f'wrote {TABLE}: {len(rows)} objects')
elif not os.path.exists(path):
    sys.exit(f'{TABLE} does not exist; run with --write')
else:
    table = {r['object']: r for r in csv.DictReader(open(path), delimiter='\t')}
    differ = [(r['object'], c, table.get(r['object'], {}).get(c), r[c]) for r in rows for c in COLUMNS
              if table.get(r['object'], {}).get(c) != r[c]]
    for key, column, was, now in differ:
        print(f'DIFFERS {key} {column}: table {was!r}, rebuilt {now!r}')
    if differ:
        sys.exit(f'{len(differ)} differences from {TABLE}')
    print(f'{len(rows)} objects rebuilt; every row equals {TABLE}')
