// Section 9 bootstrap: tiling on the fly. One corridor panel — the
// sliding-window instrument with its full trail kept, zoomed out so
// the fog-of-war structure reads: visited world, and nothing else.
import { mountNav } from './nav';
import { mountFlightPanel } from '../flight-panel';
import { mountHopFigure } from '../specimens';
import s21 from '../../../results/hat-tableevolve-r48-s21.jsonl?raw';

mountNav();

mountHopFigure(document.getElementById('hop-figure')!, { family: 'hat' });

mountFlightPanel(document.getElementById('corridor')!, {
  record: s21.split('\n')[0],
  speed: 20,
  autoplay: true,
  zoomLevel: 0.12,
  trail: 60,
  caption:
    '<b>The corridor</b> — hat glider A under the campaign instrument ' +
    '(window radius 24, launch 48). Each hexagonal block is one window; ' +
    'the glider rides the newest, and everything behind it is kept as ' +
    'visited terrain. Beyond the corridor no tiling exists — it was ' +
    'never computed. Watch the front edge: a new window snaps into place ' +
    'each time the glider closes to within the safety margin. The ' +
    'odometer’s heading column is the exactness control running live.',
  downloadName: 'hat-tableevolve-r48-s21.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example slide ' +
    'results/hat-tableevolve-r48-s21.jsonl 0 24 --launch-radius 48 ' +
    '--launch-gens 60 --validate 96',
});
