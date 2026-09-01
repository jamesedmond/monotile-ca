// Section 11 bootstrap: the survey. The page is numbers, two compass
// roses drawn from the measured constants — and the fan-over-the-ball
// figures, which draw the outer ring of a real graph-metric ball from
// live geometry and overlay the lane spokes on it.
import { mountNav } from './nav';
import { ensureWasm } from '../wasm-shared';
import { Universe } from '../wasm/ui_wasm.js';

mountNav();

interface Lane {
  deg: number;
  label: string;
  /** A 10^5-ring fill-in lane rather than a 10^6 discovery lane. */
  minor?: boolean;
}

const rose = (caption: string, offset: number, lanes: Lane[]): string => {
  const R = 100;
  const pt = (deg: number, r: number): [number, number] => [
    r * Math.cos((deg * Math.PI) / 180),
    -r * Math.sin((deg * Math.PI) / 180),
  ];
  let s = `<circle cx="0" cy="0" r="${R}" fill="none" stroke="#2a3040" stroke-width="1.5"/>`;
  for (let k = 0; k < 6; k++) {
    const [x, y] = pt(offset + k * 60, R);
    s += `<line x1="0" y1="0" x2="${x}" y2="${y}" stroke="#3a4152" stroke-dasharray="4 4" stroke-width="1.5"/>`;
  }
  for (const l of lanes) {
    const [x, y] = pt(l.deg, R * 0.92);
    const w = l.minor ? 2 : 3;
    const col = l.minor ? '#c49a4a' : '#fab840';
    s += `<line x1="0" y1="0" x2="${x}" y2="${y}" stroke="${col}" stroke-width="${w}" stroke-linecap="round"/>`;
    const [tx, ty] = pt(l.deg, R * 1.24);
    s += `<text x="${tx}" y="${ty}" fill="#aeb7c6" font-size="10" text-anchor="middle" dominant-baseline="middle">${l.label}</text>`;
  }
  return `<figure><svg viewBox="-172 -140 344 280">${s}</svg><figcaption>${caption}</figcaption></figure>`;
};

document.getElementById('roses')!.innerHTML =
  rose(
    'The hat fan — six spokes at 45.523° + k·60° (dashed); measured lanes in amber. A and B share a spoke to three decimals.',
    45.523,
    [
      { deg: 225.523, label: 'A · B 225.523°' },
      { deg: 345.523, label: 'C 345.523°' },
    ],
  ) +
  rose(
    'The spectre fan — six spokes at 48.014° + k·60°; the twin lanes sit exactly 240.000° apart.',
    48.014,
    [
      { deg: 108.014, label: '108.014°' },
      { deg: 348.014, label: '348.014°' },
    ],
  );
document.getElementById('roses')!.classList.add('specimens');

document.getElementById('roses-full')!.innerHTML =
  rose(
    'The hat fan, filled: all six spokes carry measured lanes — the discovery lanes at 10⁶ path-rings, the other four at 10⁵.',
    45.523,
    [
      { deg: 45.523, label: '45.53°', minor: true },
      { deg: 105.523, label: '105.52°', minor: true },
      { deg: 165.523, label: '165.52°', minor: true },
      { deg: 225.523, label: 'A · B 225.523°' },
      { deg: 285.523, label: '285.53°', minor: true },
      { deg: 345.523, label: 'C 345.523°' },
    ],
  ) +
  rose(
    'The spectre fan, filled: six spokes, six measured lanes.',
    48.014,
    [
      { deg: 48.014, label: '48.02°', minor: true },
      { deg: 108.014, label: '108.014°' },
      { deg: 168.014, label: '168.01°', minor: true },
      { deg: 228.014, label: '228.02°', minor: true },
      { deg: 288.014, label: '288.02°', minor: true },
      { deg: 348.014, label: '348.014°' },
    ],
  );
document.getElementById('roses-full')!.classList.add('specimens');

// --- the fan over the metric ball ----------------------------------
// The outer ring of a radius-N ball, from live geometry, with the six
// lane spokes drawn from the root: the spokes leave through the
// outline's corners — the cusp alignment of section 10.8, visible.
const BALL_N = 27;

interface FanSpec {
  family: string;
  offset: number;
  caption: (ringTiles: number) => string;
}

function buildBallFan(fig: HTMLElement, spec: FanSpec): void {
  let u: Universe;
  try {
    u = Universe.create(spec.family, BALL_N);
  } catch {
    fig.remove();
    return;
  }
  const offs = new Uint32Array(u.polygonOffsets());
  const xy = new Float32Array(u.polygonXy());
  // BFS from the root: the outermost ring is the frontier at depth N.
  let frontier = [0];
  const seen = new Set([0]);
  for (let d = 0; d < BALL_N; d++) {
    const next: number[] = [];
    for (const c of frontier) {
      for (const v of u.neighboursOf(c)) {
        if (!seen.has(v)) {
          seen.add(v);
          next.push(v);
        }
      }
    }
    frontier = next;
  }
  // Root centroid = origin; y flipped for display, as everywhere.
  let cx = 0;
  let cy = 0;
  for (let i = offs[0]; i < offs[1]; i++) {
    cx += xy[2 * i] / (offs[1] - offs[0]);
    cy += xy[2 * i + 1] / (offs[1] - offs[0]);
  }
  let R = 0;
  const paths = frontier
    .map((c) => {
      let d = '';
      for (let i = offs[c]; i < offs[c + 1]; i++) {
        const x = xy[2 * i] - cx;
        const y = -(xy[2 * i + 1] - cy);
        R = Math.max(R, Math.hypot(x, y));
        d += `${i === offs[c] ? 'M' : 'L'}${x.toFixed(3)} ${y.toFixed(3)}`;
      }
      return `<path d="${d}Z" fill="#1c2634" stroke="#3a4152" stroke-width="${(0.014 * R).toFixed(3)}"/>`;
    })
    .join('');
  let spokes = '';
  for (let k = 0; k < 6; k++) {
    const a = ((spec.offset + k * 60) * Math.PI) / 180;
    spokes += `<line x1="0" y1="0" x2="${(1.04 * R * Math.cos(a)).toFixed(3)}" y2="${(-1.04 * R * Math.sin(a)).toFixed(3)}" stroke="#fab840" stroke-width="${(0.02 * R).toFixed(3)}" stroke-linecap="round"/>`;
  }
  const v = (1.1 * R).toFixed(1);
  fig.innerHTML =
    `<svg viewBox="-${v} -${v} ${(2.2 * R).toFixed(1)} ${(2.2 * R).toFixed(1)}">${paths}${spokes}<circle cx="0" cy="0" r="${(0.02 * R).toFixed(3)}" fill="#fab840"/></svg>` +
    `<figcaption>${spec.caption(frontier.length)}</figcaption>`;
}

function mountBallFans(container: HTMLElement, specs: FanSpec[]): void {
  container.classList.add('specimens');
  const figs = specs.map((spec) => {
    const fig = document.createElement('figure');
    fig.innerHTML = `<figcaption>building the radius-${BALL_N} ball…</figcaption>`;
    container.append(fig);
    return { fig, spec };
  });
  const io = new IntersectionObserver((entries) => {
    if (!entries.some((e) => e.isIntersecting)) return;
    io.disconnect();
    void ensureWasm().then(async () => {
      for (const { fig, spec } of figs) {
        await new Promise((r) => setTimeout(r, 30)); // paint between builds
        buildBallFan(fig, spec);
      }
    });
  });
  io.observe(container);
}

mountBallFans(document.getElementById('ball-fan')!, [
  {
    family: 'hat',
    offset: 45.523,
    caption: (n) =>
      `<b>The hat fan over the real metric</b> — ring 27 of a live ` +
      `radius-27 ball (${n} tiles), spokes at 45.523° + k·60°. Every ` +
      `spoke exits through a corner of the outline: the lanes are the ` +
      `ball's cusp directions.`,
  },
  {
    family: 'spectre',
    offset: 48.014,
    caption: (n) =>
      `<b>The spectre, same construction</b> — ring 27 (${n} tiles), ` +
      `spokes at 48.014° + k·60°. Same alignment, the spectre's offset. ` +
      `The corners are blunt at this radius; the radius-384 flank-line ` +
      `fit above is what puts all twelve cusps on the lanes to ~0.1°.`,
  },
]);
