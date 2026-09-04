// Section 3 bootstrap: the boards. Two explorer panels — the free one,
// and the cone-seed demonstration pinned to a degenerate root.
import { mountNav } from './nav';
import { mountExplorerPanel } from '../explorer-panel';
import { mountBallFigure, mountSpecimens } from '../specimens';

mountNav();

mountSpecimens(document.getElementById('penrose-tiles')!, [
  { family: 'penrosep2', base: 'kite', label: 'The kite', fill: '#233046' },
  { family: 'penrosep2', base: 'dart', label: 'The dart', fill: '#3a2338' },
  { family: 'penrosep3', base: 'thick', label: 'The thick rhomb', fill: '#233046' },
  { family: 'penrosep3', base: 'thin', label: 'The thin rhomb', fill: '#3a2338' },
]);

mountSpecimens(document.getElementById('monotiles')!, [
  {
    family: 'hat',
    base: 'hat',
    label: 'The hat',
    fill: '#233046',
    note: 'Needs mirrored copies (anti-hats) to tile',
  },
  {
    family: 'spectre',
    base: 'spectre',
    label: 'The spectre',
    fill: '#1f3a2e',
    note: 'Tiles without reflections — the chiral einstein (handed: it comes in one mirror image only)',
  },
]);

mountBallFigure(document.getElementById('penrose-ball')!, {
  family: 'penrosep2',
  label: 'They fit together like this — a dart and its full ring of neighbours, no gaps.',
});
mountBallFigure(document.getElementById('hat-ball')!, {
  family: 'hat',
  label: 'An anti-hat (plum) ringed by hats — mirrored, and four neighbours always.',
});
mountBallFigure(document.getElementById('spectre-ball')!, {
  family: 'spectre',
  label: 'A spectre among spectres — one shape, never mirrored.',
});
mountBallFigure(document.getElementById('hat-ball2')!, {
  family: 'hat',
  rings: 2,
  outerGold: true,
  label:
    'The same anti-hat, two rings out: {outer} tiles in the second ring (gold), {total} in all.',
});
mountBallFigure(document.getElementById('spectre-ball2')!, {
  family: 'spectre',
  rings: 2,
  outerGold: true,
  label: 'The spectre’s 2-ball: {outer} tiles in the second ring, {total} in all.',
});

mountExplorerPanel(document.getElementById('recurrence')!, {
  mode: 'recurrence',
  family: 'hat',
  radius: 27,
  zoom: 2.43, // 1.8 plus one click of the + button (x1.35)
  caption:
    '<b>Recurrence.</b> Click a tile: every site whose N-ring ' +
    'neighbourhood matches lights up. Slide N up and the matches thin ' +
    'out — larger patches recur too, just more sparsely, and never on a ' +
    'schedule.',
});

mountExplorerPanel(document.getElementById('explorer')!, {
  family: 'hat',
  radius: 18,
  lens: 'plain',
  caption:
    '<b>The boards.</b> A patch is identified by (family, root address, ' +
    'radius) and regenerated from scratch, identically, every time this ' +
    'page loads — the entire experimental record system of the project ' +
    'rests on that determinism. Click any tile to read its address, class ' +
    'and neighbour count; switch lenses to see the two-coloring (hat against ' +
    'anti-hat; on the spectre, the Mystic pairs against the rest), the tile ' +
    'classes, or the substitution hierarchy; slide the level to watch ' +
    'supertiles emerge.',
});
