// Section 2 bootstrap: the game, played on the reference board. Every
// panel here is the project's own wasm engine running on a square grid
// built as a plain graph — the same machinery that later plays on the
// monotilings.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import { mountStepPanel } from '../step-panel';
import { Universe } from '../wasm/ui_wasm.js';

mountNav();

const mask = (counts: number[]): number => counts.reduce((m, k) => m | (1 << k), 0);

/** Standard Life glider with its corner at (x, y). */
const glider = (x: number, y: number): [number, number][] => [
  [x + 1, y],
  [x + 2, y + 1],
  [x, y + 2],
  [x + 1, y + 2],
  [x + 2, y + 2],
];

/** R-pentomino with its corner at (x, y). */
const rPentomino = (x: number, y: number): [number, number][] => [
  [x + 1, y],
  [x + 2, y],
  [x, y + 1],
  [x + 1, y + 1],
  [x + 1, y + 2],
];

const seedGrid = (n: number, nb: 'edge' | 'vertex', cells: [number, number][]): Universe => {
  const u = Universe.createGrid(n, nb);
  for (const [x, y] of cells) u.toggleCell(y * n + x);
  return u;
};

mountEssayPanel(document.getElementById('life-glider')!, {
  makeUniverse: () => seedGrid(36, 'vertex', glider(3, 3)),
  speed: 12,
  autoplay: true,
  loopAtGeneration: 116,
  loopPauseMs: 1200,
  caption:
    '<b>Life</b> (B3/S23) on a 36×36 grid, and its famous glider: five ' +
    'cells that copy themselves one square along the diagonal every four ' +
    'generations. The pattern works at every position because every ' +
    'position is the same. At the boundary of this finite board it dies — ' +
    'cells beyond the edge are permanently dead, here and in every ' +
    'simulation in this essay.',
});

mountStepPanel(document.getElementById('one-step')!, {
  n: 9,
  seed: [
    [2, 3], [2, 4], [2, 5], // blinker
    [6, 2], [7, 2], [6, 3], [7, 3], // block
    [6, 6], // a lone cell
  ],
  caption:
    '<b>One step, in full.</b> Press <em>count neighbours</em>: every ' +
    'cell is annotated with its live-neighbour count and its fate — green ' +
    'digits will be born, red will die — all judged on the frozen present ' +
    'before anything changes. That simultaneity is the whole trick of a ' +
    'cellular automaton. Press <em>apply the step</em> to commit, toggle ' +
    'cells to test your own predictions, hover to see the rule fire.',
});

const compare = (nb: 'edge' | 'vertex'): Universe => seedGrid(25, nb, rPentomino(11, 11));
mountEssayPanel(document.getElementById('nbr-vertex')!, {
  makeUniverse: () => compare('vertex'),
  speed: 8,
  autoplay: true,
  loopAtGeneration: 90,
  loopPauseMs: 1500,
  caption:
    '<b>Moore neighbourhood</b> (vertex adjacency, 8 neighbours): the ' +
    'R-pentomino under B3/S23 boils over for a thousand generations on an ' +
    'open board. This is Life as Conway defined it.',
});
mountEssayPanel(document.getElementById('nbr-edge')!, {
  makeUniverse: () => compare('edge'),
  speed: 8,
  autoplay: true,
  loopAtGeneration: 90,
  loopPauseMs: 1500,
  caption:
    '<b>Von Neumann neighbourhood</b> (edge adjacency, 4 neighbours): the ' +
    'same seed, the same B3/S23, a different universe. With at most four ' +
    'neighbours, three-neighbour births are rare and the pattern starves.',
});

mountStepPanel(document.getElementById('replicator')!, {
  n: 23,
  neighbourhood: 'edge',
  rule: { birth: [1, 3], survive: [1, 3], name: 'parity' },
  play: true,
  seed: [
    [11, 10], [11, 11], [11, 12], [12, 12], // an L, so orientation is visible
  ],
  caption:
    '<b>The parity rule</b> (B13/S13) under von Neumann adjacency: a cell ' +
    'is alive next exactly when an odd number of its four neighbours is ' +
    'alive now. Every pattern — try your own — is a replicator: four ' +
    'copies, then sixteen, orientation preserved. Press ▶, or count ' +
    'neighbours to watch the parity arithmetic cell by cell.',
});

mountEssayPanel(document.getElementById('brain')!, {
  makeUniverse: () => {
    const u = Universe.createGrid(56, 'vertex');
    u.setGenerationsRule(mask([2]), 0, 3);
    u.randomize(180, 11, 99);
    return u;
  },
  speed: 12,
  autoplay: true,
  loopAtGeneration: 250,
  loopPauseMs: 1500,
  caption:
    '<b>Brian’s Brain</b> — the k = 3 Generations rule B2, with no ' +
    'survival at all: every firing cell (amber) spends one generation ' +
    'dying (ember) and then rests; only firing cells count as neighbours. ' +
    'A random soup dissolves almost entirely into gliders. On a periodic ' +
    'grid, this family is glider-prolific — hold that thought.',
});
