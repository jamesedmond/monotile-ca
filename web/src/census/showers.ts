// Census page: showers and ash (FINDINGS 10.11.9, 10.11.65). A shower
// is not one object, so these are ordinary fixed-patch panels: each
// replays a census seed on the ball around it, with the tiles every
// piece has passed over left tinted. The records and the figures in
// the captions are made by crates/certify/census/showers.py.
import '../essay/essay.css';
import './census.css';
import { mountEssayPanel } from '../essay-panel';
import raw from '../../../results/certificates/census-showers.jsonl?raw';

const records = raw.split('\n').filter((l) => l.trim() !== '');
const record = (key: string): string => {
  const line = records.find((l) => l.includes(`"note":"census shower ${key}:`));
  if (!line) throw new Error(`no shower record for ${key}`);
  return line;
};
const reproduce = (index: number, radius: number, gens: number): string =>
  `cargo run --release -p tiling-core --example verify_candidate results/certificates/census-showers.jsonl ${index} ${radius} ${gens}`;

interface Shower {
  id: string;
  key: string;
  radius: number;
  /** Stop before anything reaches the edge of the patch. */
  loopAt: number;
  speed?: number;
  trail?: boolean;
  view?: [number, number, number, number];
  caption: string;
}

const SHOWERS: Shower[] = [
  {
    id: 'shower-spectre-ten',
    key: 'spectre-164',
    radius: 96,
    loopAt: 180,
    speed: 30,
    trail: true,
    caption:
      '<b>A burst.</b> A two-tile seed under the spectre rule. Ten gliders leave, four on lanes of the first ' +
      'family and six on lanes of the second, and after generation 113 nothing is left near the seed. There are ' +
      'still ten pieces at generation 600. The run restarts at generation 180, before the first glider reaches ' +
      'the edge of the patch.',
  },
  {
    id: 'shower-spectre-cascade',
    key: 'spectre-2537',
    radius: 128,
    loopAt: 245,
    speed: 30,
    trail: true,
    caption:
      '<b>Pieces that make more pieces.</b> A six-cell seed under the spectre rule. Four pieces leave the seed. ' +
      'The one travelling left forks at about generation 130: gliders leave it on several lanes and it carries ' +
      'on. There are 7 pieces at generation 150, 11 at 300 and 19 at 450. The run restarts at generation 245.',
  },
  {
    id: 'shower-hatC',
    key: 'hatC-813',
    radius: 128,
    loopAt: 900,
    speed: 30,
    trail: true,
    caption:
      '<b>A shower that does not stop.</b> A two-tile seed under hat C’s rule: 175 live cells in 18 pieces at ' +
      'generation 150, and on a patch large enough to hold them all, 944 cells in 114 pieces at generation 600. ' +
      'This patch ends 128 steps from the seed, and gliders that reach its edge vanish. Within 124 steps of the ' +
      'seed the run is the same, piece for piece, as a run on a patch of radius 780, for all 900 generations shown.',
  },
  {
    id: 'shower-hatC-site',
    key: 'hatC-813',
    radius: 128,
    loopAt: 900,
    speed: 8,
    view: [-85, -85, 85, 85],
    caption:
      '<b>The site, close up.</b> The same run without the tint, showing the tiles within about 20 steps of the ' +
      'seed. From about generation 190 a six-cell oscillator sits just left of the centre, switching between two ' +
      'shapes, while gliders are launched around it and later run into it.',
  },
];

for (const s of SHOWERS) {
  const index = records.indexOf(record(s.key));
  mountEssayPanel(document.getElementById(s.id)!, {
    record: record(s.key),
    radius: s.radius,
    speed: s.speed ?? 12,
    autoplay: true,
    loopAtGeneration: s.loopAt,
    loopPauseMs: 4000,
    loopHoldEnd: true,
    trail: s.trail,
    view: s.view,
    downloadName: `census-shower-${s.key}.jsonl`,
    reproduce: reproduce(index, s.radius, 2 * s.radius),
    caption: s.caption,
  });
}
