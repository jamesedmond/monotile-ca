// Essay page bootstrap: mounts the live panels. Records are imported
// verbatim from results/ at build time — single source of truth with
// the repo's committed evidence.
import { mountEssayPanel } from './essay-panel';
import { mountSpaceTimePanel } from './spacetime-panel';
import s21 from '../../results/hat-tableevolve-r48-s21.jsonl?raw';
import s22 from '../../results/hat-tableevolve-r48-s22.jsonl?raw';
import s23 from '../../results/hat-tableevolve-r48-s23.jsonl?raw';
import s33 from '../../results/spectre-tableevolve-r48-s33.jsonl?raw';
import s32 from '../../results/spectre-tableevolve-r48-s32.jsonl?raw';

mountEssayPanel(document.getElementById('hero')!, {
  record: s21.split('\n')[0],
  radius: 96,
  speed: 14,
  autoplay: true,
  loopAtGeneration: 190,
  loopPauseMs: 1500,
  follow: true,
  followZoom: 5,
  caption:
    '<b>Hat glider A</b> — record <code>hat-tableevolve-r48-s21</code>. ' +
    'Two gliders launch from a two-cell seed; one dies on the other’s wake ' +
    'at generation 42, and the survivor settles onto its lane: 225.523°, ' +
    'one ring every two generations, population never above 16. Flown one ' +
    'million rings without drift. Ctrl-scroll or +/− to zoom, change speed, ' +
    'or step it by hand.',
  downloadName: 'hat-tableevolve-r48-s21.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate ' +
    'results/hat-tableevolve-r48-s21.jsonl 0 384 60000',
});

// Capture-pipeline URL params (paper/figures/fig-worldtube, fig-atlas):
// ?tubetheme=light — paper palette; ?tubefade=N — history length;
// ?tuberecord=s21|s22|s23|s33 — which record the worldtube replays;
// ?tubefollow=one — follow a single glider of a multi-launch record.
const tubeParams = new URLSearchParams(location.search);
const tubeTheme = tubeParams.get('tubetheme') === 'light' ? 'light' : 'dark';
const tubeFade = Number(tubeParams.get('tubefade') ?? '') || 64;
const tubeRecords: Record<string, string> = { s21, s22, s23, s33, s32 };
const tubeRecord = tubeRecords[tubeParams.get('tuberecord') ?? 's21'] ?? s21;
const tubeFollowOne = tubeParams.get('tubefollow') === 'one';

mountSpaceTimePanel(document.getElementById('worldtube')!, {
  record: tubeRecord.split('\n')[0],
  radius: 96,
  theme: tubeTheme,
  followComponent: tubeFollowOne,
  speed: 10,
  autoplay: true,
  loopAtGeneration: 185,
  loopPauseMs: 2000,
  dzPerGeneration: 1.6,
  fadeGenerations: tubeFade,
  cameraDistance: 110,
  caption:
    '<b>Space-time view</b> of the same record: each generation adds a ' +
    'layer of tile-shaped prisms; older layers fade over 64 generations. ' +
    'The two launch gliders braid until one dies on the other’s tail at ' +
    'generation 42 (a worldline vertex); the survivor’s tube climbs at ' +
    'constant slope — two generations per ring — along its compass ' +
    'heading. Drag to orbit, ctrl/cmd-scroll or +/− to zoom.',
  downloadName: 'hat-tableevolve-r48-s21.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate ' +
    'results/hat-tableevolve-r48-s21.jsonl 0 384 60000',
});
