// Section 13 bootstrap: towards a proof. Two figures cut live from
// hat A mid-flight (the situation and its collar extension), and a
// static saturation timeline drawn from the measured burst intervals.
import { mountNav } from './nav';
import { ensureWasm, wasmMemory } from '../wasm-shared';
import { Universe } from '../wasm/ui_wasm.js';
import s21 from '../../../results/hat-tableevolve-r48-s21.jsonl?raw';

mountNav();

const STATE_FILLS: Record<number, string> = {
  1: '#fab840', // firing
  2: '#94356b', // second state
  3: '#5d4a7a', // third state
};
const COLLAR = '#6f5e2a';
const EXTENSION = '#2e4a66';

/** Replay hat A, cut the support + collar + one further ring out of
 *  the live patch at two generations that the canonical encoder
 *  certifies as the SAME situation at the drawn collar radius (the
 *  situations probe: key 19 first seen at generation 75, met again at
 *  generation 159 — after which the pair branched: the successors at
 *  76 and 160 are different situations), and draw the close-ups. */
const mountSituationFigs = async (container: HTMLElement): Promise<void> => {
  await ensureWasm();
  // r96: generation 159 sits ~80 rings out, beyond the record's own 48.
  const universe = Universe.createFromResultAt(s21.split('\n')[0], 96);
  const n = universe.cellCount();
  const offsets = new Uint32Array(universe.polygonOffsets());
  const xy = new Float32Array(universe.polygonXy());
  const K = 2; // drawn collar radius (legible; the certificate uses 4)

  interface Snapshot {
    state: Uint8Array;
    ring: Int16Array;
  }
  const snapshotAt = (gen: number): Snapshot => {
    universe.step(gen - universe.generation());
    const state = new Uint8Array(
      new Uint8Array(wasmMemory().buffer, universe.statePtr(), n),
    );
    const ring = new Int16Array(n).fill(-1);
    let frontier: number[] = [];
    for (let c = 0; c < n; c++) {
      if (state[c] !== 0) {
        ring[c] = 0;
        frontier.push(c);
      }
    }
    for (let d = 1; d <= K + 1; d++) {
      const next: number[] = [];
      for (const u of frontier) {
        for (const v of universe.neighboursOf(u)) {
          if (ring[v] === -1) {
            ring[v] = d;
            next.push(v);
          }
        }
      }
      frontier = next;
    }
    return { state, ring };
  };
  const snapBareA = snapshotAt(73); // bare-pattern pair: support geometrically
  const snapA = snapshotAt(75);
  const snapBareB = snapshotAt(83); // identical to gen 73's, ring 1 visibly different
  const snapB = snapshotAt(159);

  // The two extensions are geometrically identical (31 tiles, 1-1 by
  // position and shape); they differ in exactly two tiles' CLASSES —
  // find them by matching ring-3 tiles across the support-centroid
  // translation, and mark them in the third panel.
  const classes = new Uint16Array(universe.cellClasses());
  const cxArr = new Float64Array(n);
  const cyArr = new Float64Array(n);
  for (let c = 0; c < n; c++) {
    let sx = 0;
    let sy = 0;
    for (let i = offsets[c]; i < offsets[c + 1]; i++) {
      sx += xy[2 * i];
      sy += xy[2 * i + 1];
    }
    const k = offsets[c + 1] - offsets[c];
    cxArr[c] = sx / k;
    cyArr[c] = sy / k;
  }
  const centroidOf = (snap: Snapshot): [number, number] => {
    let sx = 0;
    let sy = 0;
    let m = 0;
    for (let c = 0; c < n; c++) {
      if (snap.ring[c] === 0) {
        sx += cxArr[c];
        sy += cyArr[c];
        m++;
      }
    }
    return [sx / m, sy / m];
  };
  const [ax, ay] = centroidOf(snapA);
  const [bx, by] = centroidOf(snapB);
  const marks = new Set<number>();
  for (let b = 0; b < n; b++) {
    if (snapB.ring[b] !== K + 1) continue;
    let best = -1;
    let bd = Infinity;
    for (let a = 0; a < n; a++) {
      if (snapA.ring[a] !== K + 1) continue;
      const d =
        (cxArr[b] - cxArr[a] - (bx - ax)) ** 2 + (cyArr[b] - cyArr[a] - (by - ay)) ** 2;
      if (d < bd) {
        bd = d;
        best = a;
      }
    }
    if (best >= 0 && bd < 4 && classes[b] !== classes[best]) marks.add(b);
  }

  const fig = (
    snap: Snapshot,
    withExtension: boolean,
    label: string,
    marked?: Set<number>,
    kDraw = K,
  ): string => {
    const { state, ring } = snap;
    const cells: { c: number; fill: string; dashed?: boolean; mark?: boolean }[] = [];
    for (let c = 0; c < n; c++) {
      if (ring[c] === -1) continue;
      if (ring[c] === 0) cells.push({ c, fill: STATE_FILLS[state[c]] ?? '#fab840' });
      else if (ring[c] <= kDraw) cells.push({ c, fill: COLLAR });
      else if (withExtension && ring[c] === kDraw + 1)
        cells.push({ c, fill: EXTENSION, dashed: true, mark: marked?.has(c) });
    }
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
    const stroke = 0.008 * (maxX - minX);
    const polys = cells
      .map(({ c, fill, dashed, mark }) => {
        const pts: string[] = [];
        for (let i = offsets[c]; i < offsets[c + 1]; i++) {
          pts.push(`${i === offsets[c] ? 'M' : 'L'}${xy[2 * i]} ${-xy[2 * i + 1]}`);
        }
        const dash = mark ? '' : dashed ? ` stroke-dasharray="${stroke * 3} ${stroke * 2}"` : '';
        const strokeCol = mark ? '#e06c75' : '#8b93a2';
        const w = mark ? stroke * 2.5 : stroke;
        return `<path d="${pts.join('')}Z" fill="${fill}" stroke="${strokeCol}" stroke-width="${w}"${dash}/>`;
      })
      .join('');
    const pad = 0.05 * Math.max(maxX - minX, maxY - minY);
    return `<figure><svg viewBox="${minX - pad} ${minY - pad} ${maxX - minX + 2 * pad} ${maxY - minY + 2 * pad}">${polys}</svg><figcaption>${label}</figcaption></figure>`;
  };

  const figHtml = [
    fig(
      snapA,
      false,
      'A <b>situation</b>: hat A mid-flight (generation 75) — the live ' +
        'cells in their states, plus the substrate collar around them ' +
        '(gold; drawn here at radius 2 — the certificate uses the ' +
        'step-sufficient radius 4). Canonically encoded, this is one ' +
        'entry in the flying vocabulary, wherever on the tiling it occurs.',
    ),
    fig(
      snapA,
      true,
      'A <b>collar extension</b>: the next ring of substrate (blue, ' +
        'dashed) — terrain the situation does not determine. This is the ' +
        'extension that happened to occur at generation 75.',
    ),
    fig(
      snapB,
      true,
      '<b>The same situation, met again</b> — 84 generations and 42 ' +
        'rings later (generation 159), certified identical at the drawn ' +
        'collar radius. Its extension <em>looks</em> identical too — ' +
        'all 31 tiles in the same shapes and positions — but the two ' +
        'outlined tiles carry different tile classes: same geometry, ' +
        'different substitution ancestry, different terrain to come. ' +
        'One step later the pair duly branched. A certificate iterates ' +
        'over every legal extension of every situation — including the ' +
        'ones hiding in plain sight.',
      marks,
    ),
    fig(
      snapBareA,
      true,
      '<b>Why the collar is part of the situation</b> — the bare glider ' +
        'pattern with no collar at all, just its first surrounding ring ' +
        '(generation 73). Stripped this far, the pattern recurs again ' +
        'and again along the flight…',
      undefined,
      0,
    ),
    fig(
      snapBareB,
      true,
      '…<b>and its surroundings vary in plain sight</b>: the ' +
        'geometrically identical bare pattern ten generations later, ' +
        'ringed by 14 tiles where generation 73’s ring has 13 — seven ' +
        'of them shaped differently. No collar means wild extension ' +
        'variety; two collar rings force the next ring’s geometry ' +
        'entirely (which is why the previous panel’s differences were ' +
        'invisible); the certificate’s radius 4 is chosen so the step ' +
        'itself is determined.',
      undefined,
      0,
    ),
  ];
  container.innerHTML =
    `<div class="specimens">${figHtml[0]}${figHtml[1]}</div>` +
    `<div class="specimens spec-solo">${figHtml[2]}</div>` +
    `<div class="specimens">${figHtml[3]}${figHtml[4]}</div>`;
};

void mountSituationFigs(document.getElementById('situation-figs')!);

// ---- Saturation timeline: measured novelty bursts, then nothing. ----
const timeline = (): string => {
  const W = 900;
  const H = 150;
  const X0 = 40;
  const X1 = W - 20;
  const GENS = 1520;
  const x = (g: number): number => X0 + ((X1 - X0) * g) / GENS;
  const bursts: [number, number][] = [
    [60, 169],
    [223, 234],
    [288, 325],
    [403, 438],
  ];
  let s = `<line x1="${X0}" y1="90" x2="${X1}" y2="90" stroke="#3a4152" stroke-width="1.5"/>`;
  for (const g of [0, 500, 1000, 1500]) {
    s += `<line x1="${x(g)}" y1="86" x2="${x(g)}" y2="94" stroke="#3a4152"/>`;
    s += `<text x="${x(g)}" y="112" fill="#8b93a2" font-size="12" text-anchor="middle">${g}</text>`;
  }
  for (const [a, b] of bursts) {
    s += `<rect x="${x(a)}" y="70" width="${Math.max(x(b) - x(a), 3)}" height="20" fill="#fab840" opacity="0.85"/>`;
  }
  s += `<line x1="${x(434)}" y1="40" x2="${x(434)}" y2="68" stroke="#fab840" stroke-width="1.5"/>`;
  s += `<text x="${x(434)}" y="32" fill="#d6dae2" font-size="12" text-anchor="middle">last new situation: gen 434</text>`;
  s += `<rect x="${x(446)}" y="70" width="${x(1520) - x(446)}" height="20" fill="#2a3040"/>`;
  s += `<text x="${x(980)}" y="60" fill="#8b93a2" font-size="12" text-anchor="middle">1,074 generations of entirely fresh terrain — nothing new (doubled-window control)</text>`;
  s += `<text x="${x(240)}" y="135" fill="#8b93a2" font-size="12" text-anchor="middle">novelty bursts (amber)</text>`;
  return `<figure class="sat-timeline"><svg viewBox="0 0 ${W} ${H}">${s}</svg><figcaption>Distinct-situation novelty along hat A's flight: bursts as the terrain's hierarchy unfolds, saturation at 167, and silence across the doubled window.</figcaption></figure>`;
};
const sat = document.getElementById('saturation')!;
sat.innerHTML = timeline();
