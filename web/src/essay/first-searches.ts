// Section 5 bootstrap: the mortal era. Every panel replays a committed
// record — the seeds-sweep flag that failed verification, and the
// gallery's travellers, champions and growers.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import gallery from '../../../results/gallery.jsonl?raw';
import seeds from '../../../results/hat-seeds-r20-h1024.jsonl?raw';

mountNav();

const grec = (i: number): string => gallery.split('\n')[i];
const greplay = (i: number, radius: number, horizon: number): string =>
  `cargo run --release -p tiling-core --example verify_candidate results/gallery.jsonl ${i} ${radius} ${horizon}`;
const expander = seeds.split('\n')[29]; // B256/S245, ball seed — the false flag

mountEssayPanel(document.getElementById('referee-small')!, {
  record: expander,
  speed: 15,
  autoplay: true,
  loopAtGeneration: 230,
  loopPauseMs: 1500,
  downloadName: 'hat-seeds-r20-B256S245.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/hat-seeds-r20-h1024.jsonl 29 48 8192',
  caption:
    '<b>Radius 20</b> — B256/S245 from a tile-and-ring seed exits the ' +
    'patch at generation 208, population still under the 64-cell bar, ' +
    'all activity outbound. Flagged as a glider candidate.',
});

mountEssayPanel(document.getElementById('referee-large')!, {
  record: expander,
  radius: 48,
  speed: 30,
  autoplay: true,
  loopAtGeneration: 620,
  loopPauseMs: 1500,
  downloadName: 'hat-seeds-r20-B256S245.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/hat-seeds-r20-h1024.jsonl 29 48 8192',
  caption:
    '<b>Radius 48</b> — the identical record with room to grow. It ' +
    'nearly dies (six cells at generation 100), then swells: past 600 ' +
    'cells by generation 500, a thousand by 600. A slow expander, not a ' +
    'glider — the radius-20 flag was an artifact of early boundary exit.',
});

mountEssayPanel(document.getElementById('traveller')!, {
  record: grec(0),
  speed: 8,
  autoplay: true,
  follow: true,
  followZoom: 3.5,
  loopAtGeneration: 170,
  loopPauseMs: 1800,
  downloadName: 'gallery-hat-walker.jsonl',
  reproduce: greplay(0, 48, 8192),
  caption:
    '<b>B25/S25 again, at walking pace</b> — the advance is pulsed: a ' +
    'surge of ~9–12 length-units over eight generations, a near-stall, ' +
    'another surge; ~0.65 units per generation net, population swinging ' +
    '6–33. At generation ~96 a period-2 blinker pair calves off one ' +
    'corner; the reaction recoils, stalls, and evaporates by ~120. The ' +
    'trap is not a wall but an anchor — a stable residue the traveller ' +
    'cannot leave behind.',
});

mountEssayPanel(document.getElementById('perclass')!, {
  record: grec(2),
  radius: 96,
  speed: 25,
  autoplay: true,
  follow: true,
  followZoom: 3,
  loopAtGeneration: 560,
  loopPauseMs: 1800,
  downloadName: 'gallery-hat-perclass.jsonl',
  reproduce: greplay(2, 96, 8192),
  caption:
    '<b>The per-class champion</b> — 17 rule tables, one per hat tile ' +
    'class, tuned by genetic search: mostly B25/S25 with a handful of ' +
    'per-class tweaks. It travels <b>65 rings</b> — the 2-state record — ' +
    'then dies by generation ~420 into the same period-2 residue as ' +
    'ever, verified unchanged at radius 112 and 160. Full per-class ' +
    'freedom bought distance, not escape.',
});

mountEssayPanel(document.getElementById('kstate')!, {
  record: grec(3),
  speed: 20,
  autoplay: true,
  follow: true,
  followZoom: 3,
  loopAtGeneration: 320,
  loopPauseMs: 1800,
  downloadName: 'gallery-spectre-k4.jsonl',
  reproduce: greplay(3, 160, 4096),
  caption:
    '<b>Spectre, k = 4</b> — a Generations traveller: amber cells are ' +
    'firing; the wake behind them ages through two ember shades, one ' +
    'step darker each generation, before dying. It travels 64 rings ' +
    'and then locks into a ' +
    'period-4 oscillator — exact state recurrence, identical at radius ' +
    '80, 160 and 320. The period-2 monopoly is broken; the mortality is ' +
    'not.',
});

mountEssayPanel(document.getElementById('grower-small')!, {
  record: grec(4),
  speed: 20,
  autoplay: true,
  follow: true,
  followZoom: 6,
  loopAtGeneration: 210,
  loopPauseMs: 1500,
  downloadName: 'gallery-hat-k5-grower.jsonl',
  reproduce: greplay(4, 144, 8192),
  caption:
    '<b>Radius 72</b> — the hat k = 5 grower reaches the boundary at ' +
    'generation 184, peak population 70. At this scale it passes for a ' +
    'glider with a long tail.',
});

mountEssayPanel(document.getElementById('grower-large')!, {
  record: grec(4),
  radius: 144,
  speed: 45,
  autoplay: true,
  follow: true,
  followZoom: 3.5,
  loopAtGeneration: 600,
  loopPauseMs: 2000,
  downloadName: 'gallery-hat-k5-grower.jsonl',
  reproduce: greplay(4, 144, 8192),
  caption:
    '<b>Radius 144</b> — the same record, doubled: the object is now a ' +
    'family of separated fronts (the camera follows their centroid), ' +
    'boundary at generation 558, peak population 140. Twice the radius, ' +
    'twice the ' +
    'peak — but between forks the width holds, which is exactly why ' +
    'closely-spaced verification radii saw a flat population and passed ' +
    'it.',
});
