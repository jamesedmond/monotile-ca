// Specimen tiles: one large annotated example of each prototile, its
// polygon extracted from the project's real exact-arithmetic geometry
// (a tiny patch is generated and a representative cell plucked out) —
// even the pictures are provenance-true. Edges are colored by length
// class; side count and distinct lengths are computed from the data
// and injected into the caption.
import { Universe } from './wasm/ui_wasm.js';
import { ensureWasm } from './wasm-shared';
import type { ClassInfo } from './essay-panel';

export interface Specimen {
  family: string;
  base: string;
  label: string;
  fill: string;
  /** Extra caption text after the computed geometry line. */
  note?: string;
}

const EDGE_COLORS = ['#f0b24a', '#6aa5e8', '#6fce87'];
const BASE_FILLS: Record<string, string> = {
  kite: '#233046',
  dart: '#3a2338',
  hat: '#233046',
  antihat: '#3a2338',
  spectre: '#1f3a2e',
  thick: '#233046',
  thin: '#3a2338',
};

// The recurrence viewer's ring gold — the figures and the viewer speak
// the same colour.
const RING_GOLD = '#6f5e2a';

/** A tile and its neighbourhood out to `rings` (default 1), drawn to
 *  scale as one SVG — how the specimens actually interlock, gap-free.
 *  With outerGold, the outermost ring is highlighted; `{outer}` and
 *  `{total}` in the label are replaced by computed tile counts. */
export function mountBallFigure(
  container: HTMLElement,
  opts: { family: string; label: string; rings?: number; outerGold?: boolean },
): void {
  container.classList.add('specimens');
  void ensureWasm().then(() => {
    let universe: Universe;
    try {
      universe = Universe.create(opts.family, 4);
    } catch {
      return;
    }
    const classInfo = JSON.parse(universe.classInfoJson()) as ClassInfo[];
    const classes = new Uint16Array(universe.cellClasses());
    const offsets = new Uint32Array(universe.polygonOffsets());
    const xy = new Float32Array(universe.polygonXy());
    // Rings out from cell 0 by BFS over the real adjacency.
    const nRings = opts.rings ?? 1;
    const rings: number[][] = [[0]];
    const seen = new Set([0]);
    for (let d = 0; d < nRings; d++) {
      const next: number[] = [];
      for (const u of rings[d]) {
        for (const v of universe.neighboursOf(u)) {
          if (seen.has(v)) continue;
          seen.add(v);
          next.push(v);
        }
      }
      rings.push(next);
    }
    const cells = rings.flatMap((ring, d) => ring.map((c) => ({ c, d })));
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    const cellPts = cells.map(({ c }) => {
      const pts: [number, number][] = [];
      for (let i = offsets[c]; i < offsets[c + 1]; i++) {
        const x = xy[2 * i];
        const y = -xy[2 * i + 1];
        pts.push([x, y]);
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
      return pts;
    });
    const stroke = 0.012 * Math.max(maxX - minX, 1);
    const polys = cells.map(({ c, d }, i) => {
      const base = classInfo[classes[c]]?.base ?? '';
      const fill =
        opts.outerGold && d === nRings ? RING_GOLD : (BASE_FILLS[base] ?? '#233046');
      return `<path d="${cellPts[i].map(([x, y], j) => `${j ? 'L' : 'M'}${x} ${y}`).join('')}Z" fill="${fill}" stroke="#8b93a2" stroke-width="${stroke}"/>`;
    });
    const pad = 0.06 * Math.max(maxX - minX, maxY - minY);
    const label = opts.label
      .replace('{outer}', String(rings[nRings].length))
      .replace('{total}', String(cells.length));
    const fig = document.createElement('figure');
    fig.innerHTML = `
      <svg viewBox="${minX - pad} ${minY - pad} ${maxX - minX + 2 * pad} ${maxY - minY + 2 * pad}">${polys.join('')}</svg>
      <figcaption>${label}</figcaption>`;
    container.append(fig);
  });
}

/** The hop mechanism in miniature: a 2-ball window around root A with
 *  a rim tile B marked; the 2-ball regenerated around B; and the two
 *  superimposed — shared tiles highlighted, discarded world dimmed.
 *  Three SVGs from one exact-geometry patch. */
export function mountHopFigure(container: HTMLElement, opts: { family: string }): void {
  container.classList.add('specimens');
  void ensureWasm().then(() => {
    let universe: Universe;
    try {
      universe = Universe.create(opts.family, 8);
    } catch {
      return;
    }
    const classInfo = JSON.parse(universe.classInfoJson()) as ClassInfo[];
    const classes = new Uint16Array(universe.cellClasses());
    const offsets = new Uint32Array(universe.polygonOffsets());
    const xy = new Float32Array(universe.polygonXy());
    const n = universe.cellCount();
    const cx = new Float64Array(n);
    const cy = new Float64Array(n);
    for (let c = 0; c < n; c++) {
      let sx = 0;
      let sy = 0;
      for (let i = offsets[c]; i < offsets[c + 1]; i++) {
        sx += xy[2 * i];
        sy += -xy[2 * i + 1];
      }
      cx[c] = sx / (offsets[c + 1] - offsets[c]);
      cy[c] = sy / (offsets[c + 1] - offsets[c]);
    }
    const ball = (root: number, rings: number): Set<number> => {
      const seen = new Set([root]);
      let frontier = [root];
      for (let d = 0; d < rings; d++) {
        const next: number[] = [];
        for (const u of frontier) {
          for (const v of universe.neighboursOf(u)) {
            if (!seen.has(v)) {
              seen.add(v);
              next.push(v);
            }
          }
        }
        frontier = next;
      }
      return seen;
    };
    const ballA = ball(0, 2);
    // B: the rim tile of A's ball furthest along +x — the hop direction.
    let B = -1;
    for (const c of ballA) if (B < 0 || cx[c] > cx[B]) B = c;
    const ballB = ball(B, 2);

    const AMBER = '#fab840';
    const SHARED = '#6f5e2a';
    const RETIRED = '#20242b';
    const tile = (c: number, fill: string, dim = false): string => {
      const pts: string[] = [];
      for (let i = offsets[c]; i < offsets[c + 1]; i++) {
        pts.push(`${i === offsets[c] ? 'M' : 'L'}${xy[2 * i]} ${-xy[2 * i + 1]}`);
      }
      const stroke = dim ? '#3a3f49' : '#8b93a2';
      return `<path d="${pts.join('')}Z" fill="${fill}" stroke="${stroke}" stroke-width="STROKE"/>`;
    };
    const baseFill = (c: number): string =>
      BASE_FILLS[classInfo[classes[c]]?.base ?? ''] ?? '#233046';

    const fig = (
      cells: { c: number; fill: string; dim?: boolean }[],
      label: string,
    ): string => {
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (const { c } of cells) {
        for (let i = offsets[c]; i < offsets[c + 1]; i++) {
          minX = Math.min(minX, xy[2 * i]);
          maxX = Math.max(maxX, xy[2 * i]);
          minY = Math.min(minY, -xy[2 * i + 1]);
          maxY = Math.max(maxY, -xy[2 * i + 1]);
        }
      }
      const stroke = 0.012 * (maxX - minX);
      const pad = 0.05 * Math.max(maxX - minX, maxY - minY);
      const polys = cells
        .map(({ c, fill, dim }) => tile(c, fill, dim).replace('STROKE', String(stroke)))
        .join('');
      return `<figure><svg viewBox="${minX - pad} ${minY - pad} ${maxX - minX + 2 * pad} ${maxY - minY + 2 * pad}">${polys}</svg><figcaption>${label}</figcaption></figure>`;
    };

    const figA = fig(
      [...ballA].map((c) => ({ c, fill: c === B ? AMBER : baseFill(c) })),
      'A window: the 2-ball around root A. We wish to extend the tiling around tile B (amber) on the rim.',
    );
    const figB = fig(
      [...ballB].map((c) => ({ c, fill: c === B ? AMBER : baseFill(c) })),
      'The hop: a fresh 2-ball, regenerated from B’s own address. ' +
        'Tile generation is fully deterministic, so the overlapping ' +
        'tiles are guaranteed to match.',
    );
    const union = [...new Set([...ballA, ...ballB])].map((c) => {
      if (c === B) return { c, fill: AMBER };
      if (ballA.has(c) && ballB.has(c)) return { c, fill: SHARED };
      if (ballA.has(c)) return { c, fill: RETIRED, dim: true };
      return { c, fill: baseFill(c) };
    });
    const figU = fig(
      union,
      'Superimposed: the shared tiles (gold) coincide exactly; grey is the discarded old window. The tiling has been extended around B.',
    );
    // One more step: a rim tile C of B's ball, continuing the same way.
    const ballAB = new Set([...ballA, ...ballB]);
    let C = -1;
    for (const c of ballB) if (C < 0 || cx[c] > cx[C]) C = c;
    const ballC = ball(C, 2);
    const union2 = [...new Set([...ballAB, ...ballC])].map((c) => {
      if (c === C) return { c, fill: AMBER };
      if (ballAB.has(c) && ballC.has(c)) return { c, fill: SHARED };
      if (ballAB.has(c)) return { c, fill: RETIRED, dim: true };
      return { c, fill: baseFill(c) };
    });
    const figC = fig(
      union2,
      'And again, from rim tile C: repeated, this builds a corridor — aimed differently, it extends the tiling in any direction.',
    );
    container.innerHTML = figA + figB + figU + figC;
  });
}

export function mountSpecimens(container: HTMLElement, specs: Specimen[]): void {
  container.classList.add('specimens');
  void ensureWasm().then(() => {
    for (const spec of specs) {
      const fig = document.createElement('figure');
      container.append(fig);
      let universe: Universe;
      try {
        universe = Universe.create(spec.family, 3);
      } catch {
        continue;
      }
      const classInfo = JSON.parse(universe.classInfoJson()) as ClassInfo[];
      const classes = new Uint16Array(universe.cellClasses());
      const offsets = new Uint32Array(universe.polygonOffsets());
      const xy = new Float32Array(universe.polygonXy());
      let cell = -1;
      for (let c = 0; c < universe.cellCount(); c++) {
        if (classInfo[classes[c]]?.base === spec.base) {
          cell = c;
          break;
        }
      }
      if (cell < 0) continue;
      const s = offsets[cell];
      const e = offsets[cell + 1];
      const pts: [number, number][] = [];
      for (let i = s; i < e; i++) pts.push([xy[2 * i], -xy[2 * i + 1]]);
      const xs = pts.map((p) => p[0]);
      const ys = pts.map((p) => p[1]);
      const minX = Math.min(...xs);
      const minY = Math.min(...ys);
      const w = Math.max(...xs) - minX;
      const h = Math.max(...ys) - minY;
      const pad = 0.12 * Math.max(w, h);

      // Edge lengths, classed by rounding (exact geometry: classes are crisp).
      const lengths: number[] = [];
      for (let i = 0; i < pts.length; i++) {
        const [ax, ay] = pts[i];
        const [bx, by] = pts[(i + 1) % pts.length];
        lengths.push(Math.hypot(bx - ax, by - ay));
      }
      const distinct: number[] = [];
      const cls = lengths.map((l) => {
        const k = distinct.findIndex((d) => Math.abs(d - l) < 1e-3);
        if (k >= 0) return k;
        distinct.push(l);
        return distinct.length - 1;
      });
      // Canonical colour order: rank classes by length (shortest first), so
      // the same physical edge length gets the same colour on every
      // specimen — traversal order must not pick the palette.
      const rank: number[] = [];
      distinct
        .map((d, i) => i)
        .sort((a, b) => distinct[a] - distinct[b])
        .forEach((orig, r) => {
          rank[orig] = r;
        });

      // Corners: vertices whose interior angle is not straight (the hat's
      // kite construction leaves one collinear vertex — 14 edges, 13 sides).
      let corners = 0;
      for (let i = 0; i < pts.length; i++) {
        const [ax, ay] = pts[(i + pts.length - 1) % pts.length];
        const [bx, by] = pts[i];
        const [cx2, cy2] = pts[(i + 1) % pts.length];
        const cross = (bx - ax) * (cy2 - by) - (by - ay) * (cx2 - bx);
        if (Math.abs(cross) > 1e-4) corners++;
      }

      const path = pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x} ${y}`).join('') + 'Z';
      const edges = pts
        .map(([ax, ay], i) => {
          const [bx, by] = pts[(i + 1) % pts.length];
          return `<line x1="${ax}" y1="${ay}" x2="${bx}" y2="${by}" stroke="${EDGE_COLORS[rank[cls[i]] % 3]}" stroke-width="${0.035 * Math.max(w, h)}" stroke-linecap="round"/>`;
        })
        .join('');
      fig.innerHTML = `
        <svg viewBox="${minX - pad} ${minY - pad} ${w + 2 * pad} ${h + 2 * pad}">
          <path d="${path}" fill="${spec.fill}"/>${edges}
        </svg>
        <figcaption></figcaption>`;
      const unit = Math.min(...distinct);
      const ratios = distinct
        .map((d) => d / unit)
        .sort((a, b) => a - b)
        .map((r) => (Math.abs(r - 1) < 1e-3 ? '1' : r.toFixed(3)));
      fig.querySelector('figcaption')!.innerHTML =
        `<b>${spec.label}</b> — ${corners} sides` +
        (corners !== pts.length ? ` (${pts.length} edges, ${pts.length - corners} pair collinear)` : '') +
        ', ' +
        (distinct.length === 1
          ? 'every edge the same length'
          : `edge lengths in ratio ${ratios.join(' : ')} (colored by length)`) +
        (spec.note ? `. ${spec.note}` : '.');
    }
  });
}
