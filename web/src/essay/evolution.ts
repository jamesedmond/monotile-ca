// Section 6 bootstrap: genetic search. One panel — the floor-test
// champion, the flat-but-mortal reaction that split the death/growth
// boundary into two regimes.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import floorRec from '../../../results/hat-evolve-r400-s1.jsonl?raw';

mountNav();

mountEssayPanel(document.getElementById('floor')!, {
  record: floorRec.split('\n')[0],
  radius: 128,
  speed: 25,
  autoplay: true,
  follow: true,
  followZoom: 3,
  loopAtGeneration: 540,
  loopPauseMs: 2200,
  downloadName: 'hat-evolve-r400-s1.jsonl',
  reproduce:
    'cargo run --release -p search -- --mode evolve --family hat --states 5 ' +
    '--radius 400 --pop-cap 90 --balance --seed-from results/hat-evolve-r72-s2.jsonl',
  caption:
    '<b>The floor-test champion</b> — flat, and mortal. It travels 100 ' +
    'rings with max population 82, never climbing — the maximum away from ' +
    'the boundary and overall identical, a genuine non-grower — and dies at generation ' +
    '497, verified unchanged at radius 400, 800 and 1,600 (replayed ' +
    'here at 128). Not a coherent spaceship: population swings 9–65 and ' +
    'the centroid lurches and reverses, a chaotic multi-front reaction ' +
    'like the growers — but one that never grows, and therefore dies.',
});
