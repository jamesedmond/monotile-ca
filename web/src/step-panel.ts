// Anatomy of one synchronous step: a small Life grid rendered as DOM
// cells. The engine is the same wasm Universe as every other panel —
// only the pedagogy layer is HTML. Phase 1 overlays every cell's
// live-neighbour count and tints its fate, all judged on the frozen
// present; phase 2 commits the step. Click cells to rewrite the
// present, hover to see which rule line fires.
import { Universe } from './wasm/ui_wasm.js';
import { ensureWasm, wasmMemory } from './wasm-shared';

export interface StepPanelOptions {
  /** Grid side length (default 9). */
  n?: number;
  /** 'vertex' = Moore (default), 'edge' = von Neumann. */
  neighbourhood?: 'edge' | 'vertex';
  /** Birth/survival counts and a display name (default Life B3/S23). */
  rule?: { birth: number[]; survive: number[]; name: string };
  /** Show a play button (steps continuously; overlays pause it). */
  play?: boolean;
  /** Live cells at generation zero, as (x, y). */
  seed: [number, number][];
  caption?: string;
}

const mask = (counts: number[]): number => counts.reduce((m, k) => m | (1 << k), 0);

export function mountStepPanel(container: HTMLElement, opts: StepPanelOptions): void {
  const n = opts.n ?? 9;
  const nb = opts.neighbourhood ?? 'vertex';
  const rule = opts.rule ?? { birth: [3], survive: [2, 3], name: 'B3/S23' };
  container.classList.add('step-panel');
  container.innerHTML = `
    <div class="sp-grid${n > 12 ? ' large' : ''}" style="grid-template-columns: repeat(${n}, 1fr)"></div>
    <div class="sp-rule">&nbsp;</div>
    <div class="ep-bar">
      ${opts.play ? '<button class="sp-play" title="play/pause">▶</button>' : ''}
      <button class="sp-count">count neighbours</button>
      <button class="ep-reset" title="restore the seed">⟲</button>
      <span class="ep-stats">gen <b class="ep-gen">0</b> · pop <b class="ep-pop">0</b></span>
      <span class="ep-spacer"></span>
      <span>click a cell to toggle it</span>
    </div>
    <div class="ep-caption"></div>
  `;
  const q = <T extends HTMLElement>(sel: string): T => container.querySelector(sel) as T;
  const gridEl = q('.sp-grid');
  const ruleEl = q('.sp-rule');
  const countBtn = q<HTMLButtonElement>('.sp-count');
  q('.ep-caption').innerHTML = opts.caption ?? '';

  let universe: Universe | null = null;
  let counted = false;
  const cellEls: HTMLDivElement[] = [];

  const state = (): Uint8Array =>
    new Uint8Array(wasmMemory().buffer, universe!.statePtr(), n * n);

  /** In-bounds neighbours of cell i under the panel's adjacency — the
   *  same sets the engine's graph encodes (dead boundary beyond). */
  const neighbours = (i: number): number[] => {
    const x = i % n;
    const y = Math.floor(i / n);
    const out: number[] = [];
    for (let dy = -1; dy <= 1; dy++) {
      for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dy === 0) continue;
        if (nb === 'edge' && dx !== 0 && dy !== 0) continue;
        const nx = x + dx;
        const ny = y + dy;
        if (nx >= 0 && nx < n && ny >= 0 && ny < n) out.push(ny * n + nx);
      }
    }
    return out;
  };

  const liveCount = (s: Uint8Array, i: number): number =>
    neighbours(i).reduce((sum, j) => sum + (s[j] === 1 ? 1 : 0), 0);

  const render = (): void => {
    if (!universe) return;
    const s = state();
    for (let i = 0; i < n * n; i++) {
      const el = cellEls[i];
      el.classList.toggle('alive', s[i] === 1);
      el.classList.remove('born', 'dies');
      if (counted) {
        const c = liveCount(s, i);
        el.textContent = String(c);
        if (s[i] === 1 && !rule.survive.includes(c)) el.classList.add('dies');
        if (s[i] === 0 && rule.birth.includes(c)) el.classList.add('born');
      } else {
        el.textContent = '';
      }
    }
    gridEl.classList.toggle('counted', counted);
    q('.ep-gen').textContent = String(universe.generation());
    q('.ep-pop').textContent = String(universe.population());
  };

  const sTag = `S${rule.survive.join('')}`;
  const bTag = `B${rule.birth.join('')}`;
  const verdict = (alive: boolean, c: number): string => {
    if (alive) {
      return rule.survive.includes(c)
        ? `alive, <b>${c}</b> live neighbours → survives (${sTag})`
        : `alive, <b>${c}</b> live neighbours → dies`;
    }
    return rule.birth.includes(c)
      ? `dead, <b>${c}</b> live neighbours → born (${bTag})`
      : `dead, <b>${c}</b> live neighbours → stays dead`;
  };

  countBtn.addEventListener('click', () => {
    if (!universe) return;
    if (counted) {
      universe.step(1);
      counted = false;
      countBtn.textContent = 'count neighbours';
    } else {
      counted = true;
      countBtn.textContent = 'apply the step';
    }
    render();
  });
  q('.ep-reset').addEventListener('click', () => {
    if (!universe) return;
    universe.restartFromInitial();
    counted = false;
    countBtn.textContent = 'count neighbours';
    render();
  });

  for (let i = 0; i < n * n; i++) {
    const el = document.createElement('div');
    el.className = 'sp-cell';
    el.addEventListener('click', () => {
      universe?.toggleCell(i);
      render();
    });
    el.addEventListener('mouseenter', () => {
      if (!universe) return;
      el.classList.add('hover');
      const nbrs = neighbours(i);
      for (const j of nbrs) cellEls[j].classList.add('nbr');
      const s = state();
      ruleEl.innerHTML = verdict(s[i] === 1, liveCount(s, i));
    });
    el.addEventListener('mouseleave', () => {
      el.classList.remove('hover');
      for (const j of neighbours(i)) cellEls[j].classList.remove('nbr');
      ruleEl.innerHTML = '&nbsp;';
    });
    cellEls.push(el);
    gridEl.append(el);
  }

  let timer: ReturnType<typeof setInterval> | null = null;
  const setPlaying = (p: boolean): void => {
    const btn = container.querySelector<HTMLButtonElement>('.sp-play');
    if (timer) clearInterval(timer);
    timer = null;
    if (p) {
      counted = false;
      countBtn.textContent = 'count neighbours';
      timer = setInterval(() => {
        universe?.step(1);
        render();
      }, 220);
    }
    if (btn) btn.textContent = p ? '⏸' : '▶';
  };
  container.querySelector<HTMLButtonElement>('.sp-play')?.addEventListener('click', () => {
    setPlaying(!timer);
  });
  countBtn.addEventListener('click', () => setPlaying(false));
  q('.ep-reset').addEventListener('click', () => setPlaying(false));

  void ensureWasm().then(() => {
    universe = Universe.createGrid(n, nb);
    universe.setRule(mask(rule.birth), mask(rule.survive));
    for (const [x, y] of opts.seed) universe.toggleCell(y * n + x);
    render();
  });
}
