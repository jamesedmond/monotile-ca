// Bootstrap + UI wiring. Owns the Universe lifecycle, the rAF loop and all DOM
// controls; WebGL lives in renderer.ts, hit testing in picking.ts.
import init, { Universe } from './wasm/ui_wasm.js';
import { Renderer } from './renderer';
import { findCellAt } from './picking';
import s21raw from '../../results/hat-tableevolve-r48-s21.jsonl?raw';
import s22raw from '../../results/hat-tableevolve-r48-s22.jsonl?raw';
import s23raw from '../../results/hat-tableevolve-r48-s23.jsonl?raw';
import s33raw from '../../results/spectre-tableevolve-r48-s33.jsonl?raw';
import s32raw from '../../results/spectre-tableevolve-r48-s32.jsonl?raw';
import goucherGlider from '../../results/penrose-goucher-glider.jsonl?raw';
import goucherLooper from '../../results/penrose-goucher-looper.jsonl?raw';
import p3s11 from '../../results/penrosep3-tableevolve-r48-s11.jsonl?raw';
import p3s12 from '../../results/penrosep3-tableevolve-r48-s12.jsonl?raw';
import galleryRaw from '../../results/gallery.jsonl?raw';

interface ClassInfo {
  name: string;
  base: string;
  parent: string;
  subtile: number;
}

function $<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

const canvas = $<HTMLCanvasElement>('canvas');
const overlay = $('overlay');
const tooltip = $('tooltip');
const badge = $('badge');
const familySel = $<HTMLSelectElement>('family');
const radiusInput = $<HTMLInputElement>('radius');
const regenBtn = $<HTMLButtonElement>('regen');
const presetSel = $<HTMLSelectElement>('preset');
const ruleTextEl = $('ruleText');
const playBtn = $<HTMLButtonElement>('play');
const speedInput = $<HTMLInputElement>('speed');
const fillInput = $<HTMLInputElement>('fill');
const seedInput = $<HTMLInputElement>('seed');
const distInput = $<HTMLInputElement>('dist');
const statGen = $('statGen');
const statPop = $('statPop');
const statChanged = $('statChanged');
const nbhdSel = $<HTMLSelectElement>('nbhd');
const curatedSel = $<HTMLSelectElement>('curatedSel');
const tableText = $<HTMLTextAreaElement>('tableText');
const tableErr = $('tableErr');
const controlsEl = $('controls');
const modeSel = $<HTMLSelectElement>('modeSel');
const statesSel = $<HTMLSelectElement>('statesSel');
const gensControls = $('gensControls');
const paintButtons = Array.from(
  document.querySelectorAll<HTMLButtonElement>('#paintSel button'),
);
let paintState = 1;
let ruleStates = 2;
let tableBaseline = ''; // editor text as last loaded (replay or conversion)

const renderer = new Renderer(canvas);

let memory: WebAssembly.Memory | null = null;
let universe: Universe | null = null;
let cellCount = 0;
let classInfo: ClassInfo[] = [];
let cellClasses: Uint16Array = new Uint16Array(0);
let cellDistances: Uint32Array = new Uint32Array(0);
let polyXy: Float32Array = new Float32Array(0);
let polyOffsets: Uint32Array = new Uint32Array(0);
let birthMask = 8; // B3
let survivalMask = 12; // S23
let playing = false;
let stepAcc = 0;
let lastTime = performance.now();

// ---------------------------------------------------------------- rule editor

const PRESETS = [
  { label: 'B3/S23', b: 8, s: 12 },
  { label: 'B2/S12', b: 4, s: 6 },
  { label: 'B23/S234', b: 12, s: 28 },
];
const bChecks: HTMLInputElement[] = [];
const sChecks: HTMLInputElement[] = [];

function buildRuleGrid(): void {
  const grid = $('ruleGrid');
  grid.append(document.createElement('span'));
  for (let k = 0; k < 8; k++) {
    const d = document.createElement('span');
    d.textContent = String(k);
    grid.append(d);
  }
  for (const [tag, arr] of [['B', bChecks], ['S', sChecks]] as const) {
    const t = document.createElement('span');
    t.className = 'rowtag';
    t.textContent = tag;
    grid.append(t);
    for (let k = 0; k < 8; k++) {
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.title = `${tag}${k}`;
      cb.addEventListener('change', onRuleEdited);
      grid.append(cb);
      arr.push(cb);
    }
  }
  presetSel.append(new Option('custom', '-1'));
  PRESETS.forEach((p, i) => presetSel.append(new Option(p.label, String(i))));
  presetSel.addEventListener('change', () => {
    const p = PRESETS[Number(presetSel.value)];
    if (p) {
      birthMask = p.b;
      survivalMask = p.s;
      applyRule();
    }
  });
}

type Mode = 'gens' | 'table';
function setMode(mode: Mode): void {
  controlsEl.dataset.mode = mode;
  modeSel.value = mode;
}

/** Grey out the Generations editors (per-class stratified records are
 * displayed, not editable — regenerating or switching mode re-arms). */
function setGensLocked(locked: boolean): void {
  gensControls.classList.toggle('locked', locked);
}

function syncPaintButtons(): void {
  const paintable = Math.max(ruleStates, 2) - 1; // paintable states: 1..k-1
  if (paintState > paintable) paintState = 1;
  // k = 2 leaves only a lone '1' button — noise; hide the control and
  // paint state 1 implicitly (clicks still toggle live/dead).
  $('paintSel').hidden = paintable < 2;
  paintButtons.forEach((b) => {
    const s = Number(b.dataset.state);
    b.hidden = s > paintable;
    b.classList.toggle('active', s === paintState);
  });
}
paintButtons.forEach((b) =>
  b.addEventListener('click', () => {
    paintState = Number(b.dataset.state);
    syncPaintButtons();
  }),
);

function onRuleEdited(): void {
  const mask = (arr: HTMLInputElement[]): number => arr.reduce((m, cb, k) => m | (cb.checked ? 1 << k : 0), 0);
  birthMask = mask(bChecks);
  survivalMask = mask(sChecks);
  applyRule();
}

/**
 * Sync the rule editor (checkboxes / preset / text) to birthMask/survivalMask.
 * `pushToEngine` also writes the rule into the engine — true for a fresh patch
 * or a manual edit, but FALSE when replaying a result record, whose engine
 * already holds the (possibly per-class stratified) recorded rule that a
 * single uniform setRule would clobber. The editor then just displays the
 * record's class-0 table; toggling a box deliberately collapses to uniform.
 */
function applyRule(pushToEngine = true): void {
  bChecks.forEach((cb, k) => (cb.checked = ((birthMask >> k) & 1) === 1));
  sChecks.forEach((cb, k) => (cb.checked = ((survivalMask >> k) & 1) === 1));
  const digits = (m: number): string =>
    Array.from({ length: 8 }, (_, k) => k)
      .filter((k) => (m >> k) & 1)
      .join(',');
  ruleTextEl.textContent =
    ruleStates > 2
      ? `B${digits(birthMask)}/S${digits(survivalMask)} · k=${ruleStates}`
      : `B${digits(birthMask)}/S${digits(survivalMask)}`;
  presetSel.value = String(PRESETS.findIndex((p) => p.b === birthMask && p.s === survivalMask));
  statesSel.value = String(ruleStates);
  if (pushToEngine && universe) {
    universe.setGenerationsRule(birthMask, survivalMask, ruleStates);
    renderer.setStates(universe.states());
  }
  syncPaintButtons();
}

// ------------------------------------------------------------ patch lifecycle

function showOverlay(msg: string, isError = false): void {
  overlay.hidden = false;
  overlay.textContent = msg;
  overlay.classList.toggle('error', isError);
}

function regenerate(): void {
  setGensLocked(false);
  const family = familySel.value;
  const radius = Math.min(256, Math.max(2, Math.round(Number(radiusInput.value) || 12)));
  radiusInput.value = String(radius);
  showOverlay(`Generating ${family} patch (radius ${radius})…`);
  regenBtn.disabled = true;
  hideTooltip();
  // Universe.create() is synchronous WASM work that can take seconds; calling
  // it immediately would block the main thread before the overlay we just made
  // visible ever paints. Defer past the next paint: double-rAF lands after
  // layout/paint is scheduled, the setTimeout lets the compositor flush it.
  requestAnimationFrame(() =>
    requestAnimationFrame(() =>
      setTimeout(() => {
        try {
          const next = Universe.createWithNeighbourhood(family, radius, nbhdSel.value);
          universe?.free(); // release the old patch only once the new one exists
          universe = next;
          loadPatch(radius);
          overlay.hidden = true;
        } catch (err) {
          showOverlay(`Failed to generate patch: ${err instanceof Error ? err.message : String(err)}`, true);
        } finally {
          regenBtn.disabled = false;
        }
      }, 0),
    ),
  );
}

/** Pull all static patch data out of the new Universe and rebuild GL state.
 * `pushRule` forwards to applyRule: true for a fresh patch (push the editor's
 * rule into it), false when replaying a record (keep its recorded rule). */
function loadPatch(radius: number, pushRule = true): void {
  const u = universe;
  if (!u) return;
  cellCount = u.cellCount();
  classInfo = JSON.parse(u.classInfoJson()) as ClassInfo[];
  cellClasses = u.cellClasses();
  cellDistances = u.cellDistances();
  polyXy = u.polygonXy();
  polyOffsets = u.polygonOffsets();
  renderer.setPatch(u.triVertices(), u.triCells(), polyXy, polyOffsets, buildDeadColors(), cellCount);
  renderer.setStates(u.states()); // k-state Generations: fade dying phases

  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (let i = 0; i < polyXy.length; i += 2) {
    const x = polyXy[i], y = polyXy[i + 1];
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  renderer.fitBounds(minX, minY, maxX, maxY);

  distInput.value = String(Math.round(radius / 2)); // "within distance" defaults to radius/2
  const artifacts = u.seedArtifactCount(); // expected 0; warn if the seed left junk
  badge.hidden = artifacts === 0;
  badge.textContent = `seed artifacts: ${artifacts}`;
  hoverCell = -1;
  applyRule(pushRule); // fresh patch: push editor rule; replay: keep recorded rule
}

/** hsl -> [r, g, b] bytes (h in degrees, s/l in 0..1). */
function hsl(h: number, s: number, l: number): [number, number, number] {
  const f = (n: number): number => {
    const k = (n + h / 30) % 12;
    const a = s * Math.min(l, 1 - l);
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  };
  return [f(0), f(8), f(4)];
}

/**
 * Static per-cell dead-tile tint (RGBA). Hue keyed on the class "base" string
 * so hat vs antihat tiles are distinguishable; the spectre family has a single
 * base, so fall back to class-index parity to keep the structure visible.
 */
function buildDeadColors(): Uint8Array {
  const bases = [...new Set(classInfo.map((c) => c.base))].sort();
  const out = new Uint8Array(cellCount * 4);
  for (let c = 0; c < cellCount; c++) {
    const cls = cellClasses[c];
    const base = classInfo[cls]?.base ?? '';
    const hue = bases.length > 1 ? 210 + bases.indexOf(base) * 80 : cls % 2 === 0 ? 170 : 280;
    const [r, g, b] = hsl(hue, 0.3, 0.13);
    out[c * 4] = r;
    out[c * 4 + 1] = g;
    out[c * 4 + 2] = b;
    out[c * 4 + 3] = 255;
  }
  return out;
}

// -------------------------------------------------------------- frame loop

function frame(now: number): void {
  requestAnimationFrame(frame);
  const dt = Math.min((now - lastTime) / 1000, 0.25);
  lastTime = now;
  if (universe && memory) {
    if (playing) {
      stepAcc += dt * Number(speedInput.value);
      const n = Math.floor(stepAcc);
      stepAcc -= n;
      if (n > 0) universe.step(n);
    }
    // Re-create the state view every frame: wasm memory may grow, detaching old buffers.
    renderer.updateState(new Uint8Array(memory.buffer, universe.statePtr(), cellCount));
    statGen.textContent = String(universe.generation());
    statPop.textContent = String(universe.population());
    statChanged.textContent = String(universe.lastChanged());
  }
  renderer.draw();
}

// -------------------------------------------------------------- interaction

function eventWorld(e: MouseEvent): [number, number] {
  const r = canvas.getBoundingClientRect();
  return renderer.screenToWorld(e.clientX - r.left, e.clientY - r.top);
}

let drag: { x: number; y: number; moved: boolean } | null = null;
let hoverCell = -1;
let hoverAddr = '';

function hideTooltip(): void {
  tooltip.hidden = true;
  hoverCell = -1;
}

canvas.addEventListener('mousedown', (e) => {
  if (e.button === 0) drag = { x: e.clientX, y: e.clientY, moved: false };
});

window.addEventListener('mousemove', (e) => {
  if (!drag) return;
  const dx = e.clientX - drag.x;
  const dy = e.clientY - drag.y;
  if (!drag.moved && Math.hypot(dx, dy) < 4) return; // dead zone: still a click
  drag.moved = true;
  renderer.panBy(dx, dy);
  drag.x = e.clientX;
  drag.y = e.clientY;
});

window.addEventListener('mouseup', (e) => {
  if (drag && !drag.moved && universe) {
    const [wx, wy] = eventWorld(e);
    const cell = findCellAt(wx, wy, polyXy, polyOffsets);
    if (cell >= 0 && memory) {
      // Paint the brush state; painting the same state again erases.
      const cur = new Uint8Array(memory.buffer, universe.statePtr(), cellCount)[cell];
      universe.setCellState(cell, cur === paintState ? 0 : paintState);
    }
  }
  drag = null;
});

canvas.addEventListener('mousemove', (e) => {
  if (!universe || drag) {
    hideTooltip();
    return;
  }
  const [wx, wy] = eventWorld(e);
  const cell = findCellAt(wx, wy, polyXy, polyOffsets);
  if (cell < 0) {
    hideTooltip();
    return;
  }
  if (cell !== hoverCell) {
    hoverCell = cell;
    hoverAddr = universe.addressOf(cell); // throttled: only re-queried on cell change
  }
  const info = classInfo[cellClasses[cell]];
  tooltip.textContent = `cell ${cell} · ${info?.name ?? '?'} · dist ${cellDistances[cell]}\n${hoverAddr}`;
  tooltip.hidden = false;
  const r = canvas.getBoundingClientRect();
  tooltip.style.left = `${e.clientX - r.left + 14}px`;
  tooltip.style.top = `${e.clientY - r.top + 14}px`;
});

canvas.addEventListener('mouseleave', hideTooltip);

canvas.addEventListener(
  'wheel',
  (e) => {
    e.preventDefault();
    const r = canvas.getBoundingClientRect();
    renderer.zoomAt(e.clientX - r.left, e.clientY - r.top, Math.exp(-e.deltaY * 0.0015));
  },
  { passive: false },
);

// -------------------------------------------------------------- controls

regenBtn.addEventListener('click', () => {
  playing = false;
  playBtn.textContent = 'Play';
  regenerate();
});
$<HTMLButtonElement>('step').addEventListener('click', () => universe?.step(1));
$<HTMLButtonElement>('stepBack').addEventListener('click', () => {
  playing = false;
  playBtn.textContent = 'Play';
  universe?.stepBack();
});
playBtn.addEventListener('click', () => {
  playing = !playing;
  stepAcc = 0;
  playBtn.textContent = playing ? 'Pause' : 'Play';
});
$<HTMLButtonElement>('clear').addEventListener('click', () => universe?.clear());
$<HTMLButtonElement>('reset').addEventListener('click', () => {
  playing = false;
  playBtn.textContent = 'Play';
  universe?.resetToPinned();
});
$<HTMLButtonElement>('randomize').addEventListener('click', () => {
  // slider is percent (5-50); the engine takes permille. Scatters the
  // selected paint state over the current board (layering; Clear for
  // a fresh soup).
  universe?.scatterState(
    Number(fillInput.value) * 10,
    Number(seedInput.value) | 0,
    Number(distInput.value) | 0,
    paintState,
  );
});
speedInput.addEventListener('input', () => ($('speedVal').textContent = speedInput.value));
modeSel.addEventListener('change', () => {
  setMode(modeSel.value as Mode);
  setGensLocked(false);
  if (modeSel.value === 'gens') {
    applyRule(); // re-arm the B/S rule on the engine
  }
  // table mode: the editor is now visible; Apply installs the rule.
});
statesSel.addEventListener('change', () => {
  ruleStates = Number(statesSel.value);
  if (controlsEl.dataset.mode === 'table') {
    // Rewrite (or insert) the DSL's `states K` line and re-apply.
    const rest = tableText.value
      .split('\n')
      .filter((l) => !/^\s*states\s+\d+\s*$/i.test(l));
    tableText.value = [`states ${ruleStates}`, ...rest].join('\n');
    applyTableFromText();
  } else {
    applyRule();
  }
});
nbhdSel.addEventListener('change', () => {
  playing = false;
  playBtn.textContent = 'Play';
  regenerate(); // adjacency is a property of the generated board
});
fillInput.addEventListener('input', () => ($('fillVal').textContent = fillInput.value));

// -------------------------------------------------------------- result replay

const loadBtn = $<HTMLButtonElement>('loadBtn');
const loadInput = $<HTMLInputElement>('loadResult');
const recordSel = $<HTMLSelectElement>('recordSel');
let records: string[] = []; // raw JSON lines from the loaded file

function recordLabel(line: string, i: number): string {
  try {
    const r = JSON.parse(line) as {
      note?: string;
      rule?: { birth: number; survival: number };
      outcome?: Record<string, unknown>;
    };
    const digits = (m: number): string =>
      Array.from({ length: 8 }, (_, k) => k).filter((k) => (m >> k) & 1).join('');
    const rule = r.rule ? `B${digits(r.rule.birth)}/S${digits(r.rule.survival)}` : '?';
    const outcome = r.outcome ? Object.entries(r.outcome)[0] : null;
    return `${i}: ${rule} ${r.note ?? ''} ${outcome ? JSON.stringify(outcome[1]) : ''}`;
  } catch {
    return `${i}: (unparseable)`;
  }
}

function loadRecord(i: number, radiusOverride?: number): void {
  const line = records[i];
  if (!line) return;
  playing = false;
  playBtn.textContent = 'Play';
  showOverlay(
    radiusOverride === undefined
      ? 'Replaying result record…'
      : `Replaying record at radius ${radiusOverride}…`,
  );
  hideTooltip();
  // Same paint-before-blocking dance as regenerate().
  requestAnimationFrame(() =>
    requestAnimationFrame(() =>
      setTimeout(() => {
        try {
          const next =
            radiusOverride === undefined
              ? Universe.createFromResult(line)
              : Universe.createFromResultAt(line, radiusOverride);
          universe?.free();
          universe = next;
          familySel.value = next.family();
          radiusInput.value = String(next.radius());
          try {
            nbhdSel.value =
              (JSON.parse(line) as { neighbourhood?: string }).neighbourhood ?? 'edge';
          } catch {
            /* label-only */
          }
          birthMask = next.ruleBirth();
          survivalMask = next.ruleSurvival();
          ruleStates = Math.max(2, next.states());
          // false: keep the record's (possibly per-class) rule; only sync
          // the editor display to its class-0 table.
          loadPatch(next.radius(), false);
          // Flip the UI into the record's mode and populate its editors.
          try {
            const r = JSON.parse(line) as {
              table_rule?: { states: number; rows: TableRow[] };
              stratification?: string;
            };
            if (r.table_rule) {
              setMode('table');
              setGensLocked(false);
              tableText.value = formatTableDsl(r.table_rule);
              tableBaseline = tableText.value;
              statesSel.value = String(Math.min(5, Math.max(2, r.table_rule.states)));
              tableErr.textContent = '';
              ruleTextEl.textContent = `table · ${r.table_rule.states} states · ${r.table_rule.rows.length} rows`;
            } else {
              setMode('gens');
              const stratified =
                r.stratification !== undefined && r.stratification !== 'Uniform';
              setGensLocked(stratified);
              if (stratified) {
                ruleTextEl.textContent = `${ruleTextEl.textContent} · per-class (read-only)`;
              }
            }
          } catch {
            /* label only */
          }
          syncPaintButtons();
          reloadBtn.hidden = false;
          overlay.hidden = true;
        } catch (err) {
          showOverlay(
            `Failed to replay result: ${err instanceof Error ? err.message : String(err)}`,
            true,
          );
        }
      }, 0),
    ),
  );
}

function currentRadius(): number {
  return Math.min(256, Math.max(2, Math.round(Number(radiusInput.value) || 12)));
}

/** Honour the radius box across record selections (never below the
 * record's own radius, so its initial cells always fit). */
function replayRadiusFor(line: string): number {
  let recorded = 2;
  try {
    recorded = (JSON.parse(line) as { radius?: number }).radius ?? recorded;
  } catch {
    /* malformed line fails later with a proper message */
  }
  return Math.max(currentRadius(), recorded);
}

loadBtn.addEventListener('click', () => loadInput.click());
loadInput.addEventListener('change', async () => {
  const file = loadInput.files?.[0];
  loadInput.value = '';
  if (!file) return;
  const text = await file.text();
  records = text.split('\n').map((l) => l.trim()).filter(Boolean);
  if (records.length === 0) return;
  recordSel.innerHTML = '';
  records.forEach((line, i) => recordSel.append(new Option(recordLabel(line, i), String(i))));
  recordSel.hidden = records.length <= 1;
  recordSel.value = '0';
  loadRecord(0, replayRadiusFor(records[0]));
});
recordSel.addEventListener('change', () => {
  const i = Number(recordSel.value);
  loadRecord(i, replayRadiusFor(records[i] ?? ''));
});

const reloadBtn = $<HTMLButtonElement>('reloadBtn');
reloadBtn.addEventListener('click', () => {
  radiusInput.value = String(currentRadius());
  loadRecord(Number(recordSel.value), currentRadius());
});

// -------------------------------------------------------------- curated presets

const lines = (raw: string): string[] => raw.split('\n').map((l) => l.trim()).filter(Boolean);
const CURATED: { key: string; label: string; lines: string[] }[] = [
  { key: 'hat-a', label: 'hat glider A (s21)', lines: lines(s21raw) },
  { key: 'hat-b', label: 'hat glider B (s22)', lines: lines(s22raw) },
  { key: 'hat-c', label: 'hat glider C — triple launch (s23)', lines: lines(s23raw) },
  { key: 'spectre-a', label: 'spectre glider A — twin launch (s33)', lines: lines(s33raw) },
  { key: 'looper', label: 'spectre looper (s32) — traps at radius 192', lines: lines(s32raw) },
  { key: 'goucher', label: 'Goucher 2012 P3 glider (corrected table)', lines: lines(goucherGlider) },
  { key: 'goucher-loopers', label: 'Goucher rule on P2 — loopers 40/20/200', lines: lines(goucherLooper) },
  { key: 'phoenix', label: 'P3 phoenix rail-glider (s11)', lines: lines(p3s11) },
  { key: 'p3-wanderer', label: 'P3 dressed wanderer (s12)', lines: lines(p3s12) },
  { key: 'gallery', label: 'early gallery — walker, filament, oscillators…', lines: lines(galleryRaw) },
];

function loadCurated(key: string): void {
  const entry = CURATED.find((c) => c.key === key);
  if (!entry) return;
  records = entry.lines;
  recordSel.innerHTML = '';
  records.forEach((line, i) => recordSel.append(new Option(recordLabel(line, i), String(i))));
  recordSel.hidden = records.length <= 1;
  recordSel.value = '0';
  loadRecord(0, replayRadiusFor(records[0]));
}

curatedSel.append(new Option('choose…', ''));
CURATED.forEach((c) => curatedSel.append(new Option(c.label, c.key)));
curatedSel.addEventListener('change', () => {
  if (curatedSel.value) loadCurated(curatedSel.value);
});

// -------------------------------------------------------------- table rules

interface TableRow {
  own?: number;
  conds?: [number, number][];
  next: number;
}

/** Parse the playground DSL: optional `states K` line, then one row per
 * line — `own | conds -> next`, own a state or `*`, conds `nS>=M` terms
 * joined by `&` (or `otherwise`). Throws with a line-numbered message. */
function parseTableDsl(text: string): { states: number; rows: TableRow[] } {
  let states = 4;
  const rows: TableRow[] = [];
  const srcLines = text.split('\n');
  for (let li = 0; li < srcLines.length; li++) {
    const raw = srcLines[li].trim();
    if (raw === '' || raw.startsWith('#')) continue;
    const sm = raw.match(/^states\s+(\d+)$/i);
    if (sm) {
      states = Number(sm[1]);
      continue;
    }
    const m = raw.match(/^(\*|\d+)\s*\|\s*(.*?)\s*(?:->|→)\s*(\d+)$/);
    if (!m) throw new Error(`line ${li + 1}: expected "own | conds -> next", got "${raw}"`);
    const row: TableRow = { next: Number(m[3]) };
    if (m[1] !== '*') row.own = Number(m[1]);
    const condsText = m[2].trim();
    if (condsText !== '' && condsText !== '-' && !/^otherwise$/i.test(condsText)) {
      row.conds = condsText.split(/&|∧|\band\b/).map((t) => {
        const cm = t.trim().match(/^n\s*(\d+)\s*(?:>=|≥)\s*(\d+)$/);
        if (!cm) throw new Error(`line ${li + 1}: bad condition "${t.trim()}" (want nS>=M)`);
        return [Number(cm[1]), Number(cm[2])] as [number, number];
      });
    }
    if (row.next >= states || (row.own !== undefined && row.own >= states)) {
      throw new Error(`line ${li + 1}: state out of range for states ${states}`);
    }
    rows.push(row);
  }
  if (rows.length === 0) throw new Error('no rules — write at least one row');
  return { states, rows };
}

function formatTableDsl(tr: { states: number; rows: TableRow[] }): string {
  const row = (r: TableRow): string =>
    `${r.own ?? '*'} | ${(r.conds ?? []).map(([s, m]) => `n${s}>=${m}`).join(' & ') || 'otherwise'} -> ${r.next}`;
  return [`states ${tr.states}`, ...tr.rows.map(row)].join('\n');
}

function applyTableFromText(): void {
  if (!universe) return;
  try {
    const parsed = parseTableDsl(tableText.value);
    universe.setTableRuleJson(JSON.stringify(parsed));
    renderer.setStates(universe.states());
    tableErr.textContent = '';
    ruleTextEl.textContent = `table · ${parsed.states} states · ${parsed.rows.length} rows`;
    presetSel.value = '-1';
    ruleStates = Math.max(2, parsed.states);
    statesSel.value = String(Math.min(5, ruleStates));
    syncPaintButtons();
  } catch (err) {
    tableErr.textContent = err instanceof Error ? err.message : String(err);
  }
}
$<HTMLButtonElement>('tableApply').addEventListener('click', applyTableFromText);

/** Expand the current Generations B/S(+k) rule into its exact
 * equivalent priority table — the descending-thresholds encoding of
 * exact neighbour counts — so per-state editing continues in Table
 * mode from a faithful starting point. */
function gensToTableDsl(birth: number, survival: number, k: number): string {
  const lines: string[] = [`states ${k}`];
  // Exact-count mask -> rows: group counts into maximal runs of equal
  // output (counts >= 8 are outside the mask domain, hence 'no'), and
  // emit each run at its LOWER bound, high runs first — priority
  // matching then reads exact-count semantics off >= thresholds. The
  // implicit no-match -> 0 absorbs a trailing zero run.
  const maskRows = (own: number, mask: number, yes: number, no: number): void => {
    const out = (c: number): number => (c >= 8 ? no : (mask >> c) & 1 ? yes : no);
    let c = 8;
    while (c >= 0) {
      const o = out(c);
      let low = c;
      while (low - 1 >= 0 && out(low - 1) === o) low--;
      if (low === 0) {
        if (o !== 0) lines.push(`${own} | otherwise -> ${o}`);
      } else {
        lines.push(`${own} | n1>=${low} -> ${o}`);
      }
      c = low - 1;
    }
  };
  maskRows(0, birth, 1, 0);
  maskRows(1, survival, 1, k > 2 ? 2 : 0);
  for (let s = 2; s < k - 1; s++) lines.push(`${s} | otherwise -> ${s + 1}`);
  return lines.join('\n');
}
$<HTMLButtonElement>('toTableBtn').addEventListener('click', () => {
  tableText.value = gensToTableDsl(birthMask, survivalMask, ruleStates);
  tableBaseline = tableText.value;
  setMode('table');
  setGensLocked(false);
  applyTableFromText();
});

$<HTMLButtonElement>('tableReset').addEventListener('click', () => {
  if (!tableBaseline) {
    tableErr.textContent = 'nothing loaded yet';
    return;
  }
  tableText.value = tableBaseline;
  applyTableFromText();
});

// Prefill with hat A's table — the reader's first mutation target.
try {
  const hatA = JSON.parse(lines(s21raw)[0]) as { table_rule?: { states: number; rows: TableRow[] } };
  if (hatA.table_rule) {
    tableText.value = formatTableDsl(hatA.table_rule);
    tableBaseline = tableText.value;
  }
} catch {
  /* leave empty */
}

// -------------------------------------------------------------- boot

async function boot(): Promise<void> {
  buildRuleGrid();
  applyRule();
  new ResizeObserver(() => renderer.resize()).observe($('stage'));
  showOverlay('Loading WASM module…');
  try {
    const wasm = await init(new URL('./wasm/ui_wasm_bg.wasm', import.meta.url));
    memory = wasm.memory;
  } catch (err) {
    showOverlay(
      'WASM module not built — run wasm-pack first:\n\n' +
        'wasm-pack build crates/ui-wasm --target web --release -d ../../web/src/wasm\n\n' +
        `(${err instanceof Error ? err.message : String(err)})`,
      true,
    );
    return;
  }
  const curated = new URLSearchParams(location.search).get('curated');
  if (curated && CURATED.some((c) => c.key === curated)) {
    curatedSel.value = curated;
    loadCurated(curated);
  } else {
    regenerate();
  }
  requestAnimationFrame(frame);
}

void boot();
