// Census page: the second spectre lane family (FINDINGS 10.11.6,
// 10.11.7, 10.11.64). One census seed launches a glider of each family;
// a live panel shows them part and leave their paths on the tiling, and
// a drawn figure shows both paths against the graph-metric ball of the
// seed. The record and the figure's data are made and checked by
// crates/certify/census/two_lanes.py.
import '../essay/essay.css';
import './census.css';
import { mountEssayPanel } from '../essay-panel';
import record from '../../../results/certificates/census-two-lanes.jsonl?raw';
import figureRaw from './second-lane.json?raw';

interface Mark {
  gen: number;
  ring: number;
  /** Farthest reach of the ring's tiles per direction bin, in units. */
  reach: number[];
  /** Each glider's centre (x, y) and farthest ring at this generation. */
  at: Record<Family, [number, number, number]>;
}
interface Figure {
  binDeg: number;
  split: number;
  trunk: [number, number][];
  tracks: Record<Family, [number, number][]>;
  marks: Mark[];
  stats: Record<Family, { heading: number; speed: number }>;
}
type Family = '2542' | '1454';
const fig = JSON.parse(figureRaw) as Figure;

// --- the live panel --------------------------------------------------
// The record is replayed on the radius-128 ball of its seed, large
// enough to fill the view. The camera frames the wedge the two gliders
// travel in (they leave on the 288° and 297° lanes of this record's
// frame, down the screen) for the first 180 generations.
const RINGS = [30, 60, 90];
mountEssayPanel(document.getElementById('two-lanes')!, {
  record: record.split('\n')[0],
  speed: 10,
  autoplay: true,
  radius: 128,
  loopAtGeneration: 180,
  loopPauseMs: 4000,
  loopHoldEnd: true,
  trail: true,
  rings: RINGS,
  view: [-130, -300, 180, 10],
  downloadName: 'census-two-lanes.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/certificates/census-two-lanes.jsonl 0 128 400',
  caption:
    `<b>One seed, two gliders.</b> The six-cell seed at the top is one pattern for ${fig.split} generations and ` +
    `then parts into the 2,542 glider (left) and the 1,454 glider (right). The tiles each has passed over stay ` +
    `tinted. The blue bands are the tiles ${RINGS.join(', ')} steps from the seed; the two gliders reach each band ` +
    `within a few generations of each other. The run restarts after 180 generations.`,
});

// --- the paths against the ball of the seed ---------------------------
const COLOR: Record<Family, string> = { '2542': '#f0b24a', '1454': '#7cc4e8' };
const INK = { ring: '#39414f', outer: '#7a8496', ground: '#10141a', text: '#aeb7c6', dim: '#778091' };
const LANE: Record<Family, number> = { '2542': 48.014, '1454': 57.17 };
const NAME: Record<Family, string> = { '2542': '2,542', '1454': '1,454' };
const FAMILIES: Family[] = ['2542', '1454'];

const rad = (deg: number): number => (deg * Math.PI) / 180;
/** Frame point -> SVG point (y flipped for display, as everywhere). */
const pt = (x: number, y: number): string => `${x.toFixed(1)} ${(-y).toFixed(1)}`;
const polar = (deg: number, r: number): [number, number] => [r * Math.cos(rad(deg)), r * Math.sin(rad(deg))];
/** A ball's reach in a direction, interpolated between bins. */
const reachAt = (m: Mark, deg: number): number => {
  const n = m.reach.length;
  const f = (((deg % 360) + 360) % 360) / fig.binDeg - 0.5;
  const i = Math.floor(f);
  const t = f - i;
  return m.reach[((i % n) + n) % n] * (1 - t) + m.reach[(((i + 1) % n) + n) % n] * t;
};

function ballFigure(): string {
  const outer = fig.marks[fig.marks.length - 1];
  const R = Math.max(...outer.reach);
  const V = 1.13 * R;
  const px = (2 * V) / 760; // one CSS pixel at the figure's full width, in units
  // Outermost first, so the inner outlines sit on its ground.
  const outline = (m: Mark): string =>
    m.reach.map((r, i) => `${i === 0 ? 'M' : 'L'}${pt(...polar((i + 0.5) * fig.binDeg, r))}`).join('') + 'Z';
  let s = fig.marks
    .slice()
    .reverse()
    .map((m) =>
      m === outer
        ? `<path d="${outline(m)}" fill="${INK.ground}" stroke="${INK.outer}" stroke-width="${(1.6 * px).toFixed(2)}" stroke-linejoin="round"/>`
        : `<path d="${outline(m)}" fill="none" stroke="${INK.ring}" stroke-width="${(1.1 * px).toFixed(2)}" stroke-linejoin="round"/>`,
    )
    .join('');
  // Generation labels, on the side away from the paths.
  for (const m of fig.marks) {
    const [x, y] = polar(138, reachAt(m, 138) - 13 * px);
    s += `<text x="${x.toFixed(1)}" y="${(-y).toFixed(1)}" fill="${INK.dim}" font-size="${(11 * px).toFixed(1)}" text-anchor="middle" dominant-baseline="middle" transform="rotate(-48 ${pt(x, y)})">${m === outer ? `generation ${m.gen}, ring ${m.ring}` : m.gen}</text>`;
  }
  // The twelve lane directions, ticked outside the outermost outline.
  for (const f of FAMILIES) {
    for (let k = 0; k < 6; k++) {
      const deg = LANE[f] + 60 * k;
      const r = reachAt(outer, deg);
      const [x1, y1] = polar(deg, r + 7 * px);
      const [x2, y2] = polar(deg, r + 19 * px);
      s += `<line x1="${x1.toFixed(1)}" y1="${(-y1).toFixed(1)}" x2="${x2.toFixed(1)}" y2="${(-y2).toFixed(1)}" stroke="${COLOR[f]}" stroke-width="${(2 * px).toFixed(2)}" stroke-linecap="round" opacity="0.75"/>`;
    }
  }
  // The shared start, then each glider's path with its position at every marked generation.
  const trunk = fig.trunk.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${pt(x, y)}`).join('');
  s += `<path d="${trunk}" fill="none" stroke="${INK.text}" stroke-width="${(2 * px).toFixed(2)}" stroke-linecap="round"/>`;
  for (const f of FAMILIES) {
    const d = fig.tracks[f].map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${pt(x, y)}`).join('');
    s += `<path d="${d}" fill="none" stroke="${COLOR[f]}" stroke-width="${(2 * px).toFixed(2)}" stroke-linejoin="round" stroke-linecap="round"/>`;
    for (const m of fig.marks) {
      const [x, y, ring] = m.at[f];
      s += `<circle cx="${x.toFixed(1)}" cy="${(-y).toFixed(1)}" r="${(3.6 * px).toFixed(1)}" fill="${COLOR[f]}"><title>the ${NAME[f]} glider at generation ${m.gen}: farthest tile on ring ${ring}</title></circle>`;
    }
  }
  s += `<circle cx="0" cy="0" r="${(3 * px).toFixed(1)}" fill="${INK.text}"><title>the seed</title></circle>`;
  // Labels at the two corners the gliders reach.
  const corner = (f: Family): [number, number] => {
    const deg = fig.stats[f].heading;
    return polar(deg, reachAt(outer, deg));
  };
  const label = (f: Family, lines: string[], anchor: string, dx: number, dy: number): string => {
    const [x, y] = corner(f);
    return lines
      .map(
        (line, i) =>
          `<text x="${(x + dx * px).toFixed(1)}" y="${(-y + (dy + 15 * i) * px).toFixed(1)}" fill="${i === 0 ? COLOR[f] : INK.text}" font-size="${(12 * px).toFixed(1)}" text-anchor="${anchor}">${line}</text>`,
      )
      .join('');
  };
  s += label('2542', ['2,542 glider', 'major corner'], 'end', -14, 30);
  s += label('1454', ['1,454 glider', 'minor corner'], 'start', 16, 22);
  return (
    `<svg viewBox="${(-V).toFixed(0)} ${(-V).toFixed(0)} ${(2 * V).toFixed(0)} ${(2 * V).toFixed(0)}" role="img" ` +
    `aria-label="The ball around the seed at five generations, with the paths of the two gliders ending at a major and a minor corner of its outline.">${s}</svg>`
  );
}

const first = fig.marks[0];
const outer = fig.marks[fig.marks.length - 1];
document.getElementById('ball-figure')!.innerHTML =
  ballFigure() +
  `<figcaption><b>The two paths against the ball of the seed.</b> Each outline is the ball the gliders have reached ` +
  `at generations ${fig.marks.map((m) => m.gen).join(', ').replace(/, (\d+)$/, ' and $1')}: the tiles within ` +
  `${first.ring} steps of the seed at generation ${first.gen}, within ${outer.ring} at generation ${outer.gen}. ` +
  `The dots are the gliders at those generations. The ticks outside the last outline are the twelve lane ` +
  `directions, <span style="color:${COLOR['2542']}">six of the first family</span> and ` +
  `<span style="color:${COLOR['1454']}">six of the second</span>.</figcaption>`;

for (const f of FAMILIES) {
  for (const el of document.querySelectorAll<HTMLElement>(`[data-speed="${f}"]`)) {
    el.textContent = fig.stats[f].speed.toFixed(3);
  }
}
