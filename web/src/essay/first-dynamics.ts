// Section 4 bootstrap: the game on the board. Live soups run the real
// engine on real hat patches; the bookmark panels replay committed
// records from results/gallery.jsonl.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import { Universe } from '../wasm/ui_wasm.js';
import gallery from '../../../results/gallery.jsonl?raw';

mountNav();

const rec = (i: number): string => gallery.split('\n')[i];
const replay = (i: number, radius: number, horizon: number): string =>
  `cargo run --release -p tiling-core --example verify_candidate results/gallery.jsonl ${i} ${radius} ${horizon}`;

const soup = (birth: number[], survival: number[], seed: number): Universe => {
  const u = Universe.create('hat', 16);
  const mask = (counts: number[]): number => counts.reduce((m, k) => m | (1 << k), 0);
  u.setRule(mask(birth), mask(survival));
  u.randomize(330, seed, 8);
  return u;
};

mountEssayPanel(document.getElementById('life-hat')!, {
  makeUniverse: () => soup([3], [2, 3], 7),
  speed: 6,
  autoplay: true,
  loopAtGeneration: 30,
  loopPauseMs: 1500,
  reproduce: 'cargo run --release -p search   # full census, both families',
  caption:
    '<b>B3/S23 on the hat</b> — Life’s rule on the census patch (radius ' +
    '16, 870 tiles), one-third soup. It starves: this soup is dead by ' +
    'generation 14, and others leave only a four-cell remnant. Whatever ' +
    'made B3/S23 special on the square grid did not survive the move. ' +
    'The engine is the same one the last two sections ran; only the ' +
    'neighbour list changed.',
});

mountEssayPanel(document.getElementById('explosive')!, {
  makeUniverse: () => soup([2], [2, 3], 7),
  speed: 12,
  autoplay: true,
  loopAtGeneration: 120,
  loopPauseMs: 1500,
  reproduce: 'cargo run --release -p search   # full census, both families',
  caption:
    '<b>B2/S23</b> — lower Life’s birth threshold by one and the same ' +
    'soup eats the board: the chaotic front reaches the patch boundary ' +
    'at generation 15 and is still churning at generation 400. Rules ' +
    'like this are bucketed by that boundary contact — the flag doubles ' +
    'as the search’s explosion filter, since nothing that outgrows the ' +
    'patch can be classified on it.',
});

mountEssayPanel(document.getElementById('front')!, {
  makeUniverse: () => {
    // The isotropy control experiment's recipe (tiling-core
    // examples/isotropy.rs): B2/S12346 from a 40% soup within six
    // rings of the centre, on a large patch.
    const u = Universe.create('hat', 110);
    u.setRule(1 << 2, (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 6));
    u.randomize(400, 21, 6);
    return u;
  },
  speed: 12,
  autoplay: true,
  loopAtGeneration: 220,
  loopPauseMs: 2000,
  reproduce: 'cargo run --release -p tiling-core --example isotropy',
  caption:
    '<b>The shape of growth</b> — B2/S12346, a chaotic Eden-type rule, ' +
    'from a 40% soup within six rings of the centre: the isotropy ' +
    'experiment’s recipe on a radius-110 patch (41,612 tiles). The ' +
    'front leaves frozen ash behind and advances as a near-circle: ' +
    'six-fold modulation 1.6% at radius 110, on a board whose own ' +
    'metric ball is close to hexagonal. The control that matters: the same ' +
    'rule on a periodic hexagonal lattice rounds identically (1.3%). ' +
    'The circle is generic front-averaging, not aperiodic magic — at ' +
    'the level of growth, the monotiles are a six-fold medium that ' +
    'ties a hex lattice.',
});

mountEssayPanel(document.getElementById('osc-hat')!, {
  record: rec(6),
  speed: 30,
  autoplay: true,
  downloadName: 'gallery-hat-p99.jsonl',
  reproduce: replay(6, 12, 1024),
  caption:
    '<b>Hat, period 99</b> — B2/S1345 from a two-cell seed: twenty cells ' +
    'at most, returning to their exact configuration every 99 ' +
    'generations. The period is exact — cycle detection over full state ' +
    'snapshots, never a hash guess.',
});

mountEssayPanel(document.getElementById('osc-spectre')!, {
  record: rec(7),
  speed: 30,
  autoplay: true,
  downloadName: 'gallery-spectre-p150.jsonl',
  reproduce: replay(7, 12, 1024),
  caption:
    '<b>Spectre, period 150</b> — B23456/S03457 from a five-cell seed ' +
    '(a tile and its ring), 121 cells at its peak. Long-period ' +
    'oscillation is easy to find on both tilings; travelling is the ' +
    'hard part.',
});

mountEssayPanel(document.getElementById('filament')!, {
  record: rec(1),
  radius: 48,
  speed: 45,
  autoplay: true,
  loopAtGeneration: 1080,
  loopPauseMs: 2000,
  downloadName: 'gallery-spectre-filament.jsonl',
  reproduce: replay(1, 96, 4096),
  caption:
    '<b>The spectre filament</b> — B2/S256, replayed at radius 48 ' +
    'until it strikes the boundary (generation 1,015). ' +
    'Population grows linearly (~0.25 cells per generation) and the ' +
    'whole filament stays dynamically active — a body that churns ' +
    'without dying, freezing, or exploding. On an instrumented ' +
    'radius-128 run, its heading held to ±10° for all 3,600 wall-clean ' +
    'generations.',
});

mountEssayPanel(document.getElementById('walker')!, {
  record: rec(0),
  speed: 10,
  autoplay: true,
  follow: true,
  followZoom: 3.5,
  loopAtGeneration: 200,
  loopPauseMs: 1800,
  downloadName: 'gallery-hat-walker.jsonl',
  reproduce: replay(0, 48, 8192),
  caption:
    '<b>The traveller</b> — B25/S25 from a tile-plus-ring seed. It ' +
    'travels ~23 rings in pulses — surging, near-stalling, surging ' +
    'again, population between 6 and 33 — then at generation ~96 it ' +
    'sheds a pair of period-2 blinkers from one corner, recoils, and ' +
    'evaporates by ~120. The blinkers remain: exact period 2, confirmed ' +
    'at generation 129. The first travelling object ever seen on this ' +
    'tiling, and it is mortal.',
});
