// Section 7 bootstrap: validation on Penrose. The as-printed panel
// replays the committed glider record with row 2's next-state set back
// to the paper's printed value — a one-field patch that kills it.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import gliderRec from '../../../results/penrose-goucher-glider.jsonl?raw';
import looperRec from '../../../results/penrose-goucher-looper.jsonl?raw';
import s11 from '../../../results/penrosep3-tableevolve-r48-s11.jsonl?raw';
import s12 from '../../../results/penrosep3-tableevolve-r48-s12.jsonl?raw';

mountNav();

const glider = gliderRec.split('\n')[0];
const looper = looperRec.split('\n')[0];
const looperSeed = (JSON.parse(looper).initial_cells as number[])[0];
const asPrinted = ((): string => {
  const r = JSON.parse(glider);
  r.table_rule.rows[1].next = 3; // the paper's printed Table 1, row 2
  r.note = 'Goucher Table 1 as printed (row 2 next-state 3) — dies';
  return JSON.stringify(r);
})();

mountEssayPanel(document.getElementById('printed')!, {
  record: asPrinted,
  radius: 48,
  speed: 2,
  autoplay: true,
  follow: true,
  followZoom: 6,
  loopAtGeneration: 8,
  loopPauseMs: 2500,
  caption:
    '<b>As printed</b> — head-plus-tail seed under the literal Table 1. ' +
    'The tail becomes wing, the wing fades, and with no rule row ever ' +
    'producing a head, the pattern is dead at generation 4.',
});

mountEssayPanel(document.getElementById('corrected')!, {
  record: glider,
  radius: 48,
  speed: 12,
  autoplay: true,
  follow: true,
  followZoom: 6,
  loopAtGeneration: 96,
  loopPauseMs: 2000,
  downloadName: 'penrose-goucher-glider.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example goucher_smoke 96 4000 p3',
  caption:
    '<b>Corrected (row 2 → 1)</b> — the same seed glides: one head cell ' +
    'reborn a step further along its ribbon every other generation, ' +
    'population exactly 10 and flat at radius 24, 48 and 96, two ' +
    'generations per ring. The first glider on an aperiodic tiling in this essay.',
});

mountEssayPanel(document.getElementById('looper')!, {
  record: looper,
  speed: 15,
  autoplay: true,
  focus: { cell: looperSeed, zoom: 3.5 },
  downloadName: 'penrose-goucher-looper.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example goucher_smoke 64 20000 p2',
  caption:
    '<b>P2, the cartwheel orbit</b> — the identical corrected rule on ' +
    'kites and darts closes into an exact period-40 loop, endlessly. ' +
    'The committed records also pin periods 20 and 200; all exact ' +
    'matches to the published sequence.',
});

mountEssayPanel(document.getElementById('phoenix')!, {
  record: s11.split('\n')[0],
  radius: 28,
  speed: 12,
  autoplay: true,
  loopAtGeneration: 30,
  loopPauseMs: 2000,
  downloadName: 'penrosep3-tableevolve-r48-s11.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/penrosep3-tableevolve-r48-s11.jsonl 0 384 50000',
  caption:
    '<b>The phoenix (run s11)</b> — a blind-GA rail-glider twice as fast ' +
    'as Goucher’s: ~1 ring per generation, ballistic, max population 32, ' +
    'flat to radius 384. One two-cell seed launches <em>two</em> gliders ' +
    'on five-fold headings ~144° apart, each aimed at a corner of the ' +
    'patch — rail-riding, confirmed by eye and by exponent.',
});

mountEssayPanel(document.getElementById('wanderer')!, {
  record: s12.split('\n')[0],
  speed: 15,
  autoplay: true,
  follow: true,
  followZoom: 3,
  loopAtGeneration: 185,
  loopPauseMs: 2000,
  downloadName: 'penrosep3-tableevolve-r48-s12.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/penrosep3-tableevolve-r48-s12.jsonl 0 384 50000',
  caption:
    '<b>A dressed wanderer (run s12)</b> — the other discovered class: a ' +
    'compact, ash-free clump that meanders more than it glides, dressed ' +
    'in a live halo, yet verified flat to radius 384 — ≥ 381 rings where ' +
    'every hat and spectre traveller died by ~100. The flat-population ' +
    'immortal corner, unoccupied on the monotiles, is occupied on P3 — ' +
    'and by a wanderer, not only by rail-riders.',
});

mountEssayPanel(document.getElementById('phoenix-close')!, {
  record: s11.split('\n')[0],
  radius: 96,
  speed: 5,
  autoplay: true,
  follow: true,
  followZoom: 9,
  followCluster: 0.08,
  loopAtGeneration: 100,
  loopPauseMs: 2000,
  downloadName: 'penrosep3-tableevolve-r48-s11.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/penrosep3-tableevolve-r48-s11.jsonl 0 384 50000',
  caption:
    '<b>The phoenix, close up</b> — no cell in this pattern survives a ' +
    'single generation without changing state (states 1 and 2 have no ' +
    'rows at all and drop to ground; state 3’s only row turns it into a ' +
    'head): what travels is pure rebirth, one ring ahead ' +
    'every step, shedding a pulsing exhaust cluster that flares and ' +
    'dies behind it. The camera tracks one of the two gliders across a ' +
    'radius-96 board.',
});
