// Patch explorer: the boards themselves, no automaton running. Pick a
// family and radius, switch lenses (chirality two-coloring, per-class
// rainbow, hierarchy ancestors, plain), and click any tile to read its
// address, class, and neighbour count. Everything shown is the actual
// data structure the engine runs on — a patch regenerated from
// (family, root, radius), deterministically, every time. Cells the
// library flags as seam artifacts (degenerate cone roots) are always
// painted red, whatever the lens.
import { Universe } from './wasm/ui_wasm.js';
import { Renderer } from './renderer';
import { ensureWasm, wasmMemory } from './wasm-shared';
import { deadColors, isChiralMinority, type ClassInfo } from './essay-panel';

export type Lens = 'chirality' | 'classes' | 'hierarchy' | 'plain';

export interface ExplorerOptions {
  /** 'explore' (default): lenses + click-for-address. 'recurrence':
   *  N-ball matching only, hat/spectre dropdown, whisper colors. */
  mode?: 'explore' | 'recurrence';
  family?: string;
  radius?: number;
  root?: string; // explicit root (the cone demo); hides the controls
  lens?: Lens;
  /** Hide family/radius/lens controls (fixed demos). */
  fixed?: boolean;
  /** Default crop-in factor over the whole-patch fit (default 2.6). */
  zoom?: number;
  caption?: string;
}

function hsl(h: number, s: number, l: number): [number, number, number] {
  const f = (n: number): number => {
    const k = (n + h / 30) % 12;
    const a = s * Math.min(l, 1 - l);
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  };
  return [f(0), f(8), f(4)];
}

const segments = (address: string): string[] =>
  address.replace(/:/g, '').match(/\([^)]*\)/g) ?? [];

// Recurrence highlight: the clicked cell in a hot near-white, matched
// centres in the renderer's alive amber, their surrounding rings in a
// dimmer gold so dozens of overlapping balls don't flood the board.
const BALL_SOURCE: [number, number, number] = [255, 243, 205];
const BALL_CENTRE: [number, number, number] = [250, 184, 64];
const BALL_RING = hsl(45, 0.45, 0.3);

export function mountExplorerPanel(container: HTMLElement, opts: ExplorerOptions): void {
  container.classList.add('essay-panel', 'explorer-panel');
  container.innerHTML = `
    <div class="ep-stage">
      <canvas class="ep-canvas"></canvas>
      <div class="ep-loading">loading the tiling…</div>
    </div>
    <div class="ep-bar">
      <select class="xp-family" title="tiling family">
        <option value="hat">hat</option>
        <option value="spectre">spectre</option>
        <option value="penrosep2">Penrose P2 (kites &amp; darts)</option>
        <option value="penrosep3">Penrose P3 (rhombs)</option>
      </select>
      <label class="ep-speedlabel">r <input class="xp-radius" type="range" min="6" max="32" step="1" /><span class="xp-rval"></span></label>
      <select class="xp-lens" title="lens">
        <option value="plain">plain</option>
        <option value="chirality">two-coloring</option>
        <option value="classes">classes</option>
        <option value="hierarchy">hierarchy</option>
      </select>
      <label class="ep-speedlabel xp-levelbox">level <input class="xp-level" type="range" min="1" max="6" step="1" /><span class="xp-lval"></span></label>
      <button class="xp-zoomout" title="zoom out">−</button>
      <button class="xp-zoomin" title="zoom in (or ctrl/cmd-scroll)">+</button>
      <span class="ep-spacer"></span>
      <span class="xp-cells"></span>
    </div>
    <div class="xp-info">click a tile to read its address</div>
    <div class="ep-caption"></div>
  `;
  const q = <T extends HTMLElement>(sel: string): T => container.querySelector(sel) as T;
  const canvas = q<HTMLCanvasElement>('.ep-canvas');
  const loading = q('.ep-loading');
  const familySel = q<HTMLSelectElement>('.xp-family');
  const radiusInput = q<HTMLInputElement>('.xp-radius');
  const lensSel = q<HTMLSelectElement>('.xp-lens');
  const levelInput = q<HTMLInputElement>('.xp-level');
  const levelBox = q('.xp-levelbox');
  const info = q('.xp-info');
  q('.ep-caption').innerHTML = opts.caption ?? '';

  const mode = opts.mode ?? 'explore';
  const idlePrompt =
    mode === 'recurrence'
      ? 'click a tile to light up every matching N-ball'
      : 'click a tile to read its address';
  familySel.value = opts.family ?? 'hat';
  radiusInput.value = String(opts.radius ?? 14);
  lensSel.value = opts.lens ?? 'chirality';
  levelInput.value = '2';
  if (opts.fixed) q('.ep-bar').style.display = 'none';
  if (mode === 'recurrence') {
    // Only the two monotile families, an N slider, nothing else.
    for (const o of Array.from(familySel.options)) {
      if (o.value !== 'hat' && o.value !== 'spectre') o.remove();
    }
    radiusInput.parentElement!.style.display = 'none';
    lensSel.style.display = 'none';
    lensSel.value = 'chirality';
    const nBox = document.createElement('label');
    nBox.className = 'ep-speedlabel';
    nBox.innerHTML = 'N <input class="xp-nball" type="range" min="1" max="5" step="1" value="1" /><span class="xp-nval">1</span>';
    lensSel.after(nBox);
    info.textContent = idlePrompt;
  }

  let universe: Universe | null = null;
  let renderer: Renderer | null = null;
  let centroids = new Float32Array(0);
  let classInfo: ClassInfo[] = [];
  let cellClasses = new Uint16Array(0);
  let degrees = new Uint32Array(0);
  let artifacts: Set<number> = new Set();
  let tilePitch = 1;
  let selected = -1;
  let distances = new Uint32Array(0);
  let keyLevels: string[][] = []; // [level][cell] N-ball keys, built lazily

  const lensColors = (): Uint8Array => {
    const n = universe!.cellCount();
    const lens = lensSel.value as Lens;
    let out: Uint8Array;
    if (lens === 'chirality') {
      if (mode === 'recurrence') {
        // Recurrence ground stays the whisper two-coloring — the amber/gold
        // ball highlights carry the information there.
        out = deadColors(classInfo, cellClasses, n);
      } else {
        // The playground's dead-tile palette: equal-lightness hue split,
        // navy majority / plum minority (matching the interlock figures'
        // plum anti-hat), so the two-coloring reads against the plain lens.
        out = new Uint8Array(n * 4);
        const maj = hsl(210, 0.3, 0.14);
        const min = hsl(290, 0.3, 0.26);
        for (let c = 0; c < n; c++) {
          out.set(isChiralMinority(classInfo[cellClasses[c]]) ? min : maj, c * 4);
          out[c * 4 + 3] = 255;
        }
      }
    } else {
      out = new Uint8Array(n * 4);
      if (lens === 'classes') {
        for (let c = 0; c < n; c++) {
          const [r, g, b] = hsl((cellClasses[c] * 137.508) % 360, 0.3, 0.3);
          out.set([r, g, b, 255], c * 4);
        }
      } else if (lens === 'hierarchy') {
        const k = Number(levelInput.value);
        for (let c = 0; c < n; c++) {
          const key = segments(universe!.addressOf(c)).slice(k).join('');
          let h = 2166136261;
          for (let i = 0; i < key.length; i++) h = ((h ^ key.charCodeAt(i)) * 16777619) >>> 0;
          const [r, g, b] = hsl(h % 360, 0.28, 0.26);
          out.set([r, g, b, 255], c * 4);
        }
      } else {
        // Plain = the two-coloring's majority everywhere (same border too),
        // so toggling plain <-> two-coloring moves only the minority tiles.
        const [r, g, b] = hsl(210, 0.3, 0.14);
        for (let c = 0; c < n; c++) out.set([r, g, b, 255], c * 4);
      }
    }
    for (const a of artifacts) out.set([176, 56, 56, 255], a * 4);
    return out;
  };

  const applyLens = (): void => {
    if (!universe || !renderer) return;
    levelBox.style.display = lensSel.value === 'hierarchy' ? '' : 'none';
    q('.xp-lval').textContent = levelInput.value;
    // Plain and two-coloring share fills and the brighter outline grey, so
    // toggling between them moves nothing but the minority tiles.
    renderer.setLineColor(
      (lensSel.value === 'chirality' || lensSel.value === 'plain') && mode !== 'recurrence'
        ? [0.415, 0.44, 0.48]
        : null,
    );
    renderer.updateColors(lensColors());
    renderer.draw();
  };

  const loadPatch = (): void => {
    if (!renderer) return;
    loading.style.display = '';
    selected = -1;
    // Let the shimmer paint before the (blocking) transducer build.
    setTimeout(() => {
      const family = familySel.value;
      const radius = Number(radiusInput.value);
      q('.xp-rval').textContent = String(radius);
      try {
        universe = opts.root
          ? Universe.createWithRoot(family, opts.root, radius)
          : Universe.create(family, radius);
      } catch (err) {
        loading.textContent = `failed: ${err instanceof Error ? err.message : String(err)}`;
        return;
      }
      const n = universe.cellCount();
      const polyXy = new Float32Array(universe.polygonXy());
      const polyOffsets = new Uint32Array(universe.polygonOffsets());
      classInfo = JSON.parse(universe.classInfoJson()) as ClassInfo[];
      cellClasses = new Uint16Array(universe.cellClasses());
      degrees = new Uint32Array(universe.cellDegrees());
      distances = new Uint32Array(universe.cellDistances());
      artifacts = new Set(Array.from(universe.seedArtifactCells()));
      keyLevels = [];
      renderer!.setPatch(
        new Float32Array(universe.triVertices()),
        new Uint32Array(universe.triCells()),
        polyXy,
        polyOffsets,
        lensColors(),
        n,
      );
      centroids = new Float32Array(n * 2);
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (let c = 0; c < n; c++) {
        const s = polyOffsets[c];
        const e = polyOffsets[c + 1];
        let sx = 0;
        let sy = 0;
        for (let i = s; i < e; i++) {
          sx += polyXy[2 * i];
          sy += polyXy[2 * i + 1];
          if (polyXy[2 * i] < minX) minX = polyXy[2 * i];
          if (polyXy[2 * i + 1] < minY) minY = polyXy[2 * i + 1];
          if (polyXy[2 * i] > maxX) maxX = polyXy[2 * i];
          if (polyXy[2 * i + 1] > maxY) maxY = polyXy[2 * i + 1];
        }
        centroids[2 * c] = sx / (e - s);
        centroids[2 * c + 1] = sy / (e - s);
      }
      tilePitch = Math.max(maxX - minX, maxY - minY) / (2 * radius + 1);
      renderer!.resize();
      renderer!.fitBounds(minX, minY, maxX, maxY);
      // Free explorers open cropped-in so the tiling fills the frame;
      // fixed demos (the seam) show the whole patch.
      if (!opts.fixed) renderer!.zoom *= opts.zoom ?? 2.6;
      const arts = artifacts.size
        ? ` · <b style="color:#e06c75">${artifacts.size} seam artifacts</b>`
        : '';
      q('.xp-cells').innerHTML = `${n} tiles${arts}`;
      info.textContent = idlePrompt;
      loading.style.display = 'none';
      applyLens();
    }, 30);
  };

  /** N-ball keys by levelled refinement with angular order: level 0 is
   *  the tile class; level L combines a cell's level-(L-1) key with the
   *  lexicographically minimal rotation of its neighbours' level-(L-1)
   *  keys taken in angular order. Equal level-N keys = congruent
   *  N-balls (classes pin the geometry). Built lazily, memoized. */
  const buildKeys = (upTo: number): void => {
    if (!universe) return;
    if (keyLevels.length === 0) keyLevels.push(Array.from(cellClasses, String));
    const n = universe.cellCount();
    while (keyLevels.length <= upTo) {
      const prev = keyLevels[keyLevels.length - 1];
      const cur: string[] = new Array(n);
      for (let c = 0; c < n; c++) {
        const cx = centroids[2 * c];
        const cy = centroids[2 * c + 1];
        const ring = Array.from(universe.neighboursOf(c))
          .map((j) => ({ a: Math.atan2(centroids[2 * j + 1] - cy, centroids[2 * j] - cx), k: prev[j] }))
          .sort((p, q) => p.a - q.a)
          .map((p) => p.k);
        let best = '';
        for (let r = 0; r < ring.length; r++) {
          const rot = ring.slice(r).concat(ring.slice(0, r)).join(';');
          if (!best || rot < best) best = rot;
        }
        cur[c] = `${prev[c]}|${best}`;
      }
      // Intern to short ids: equality is all later levels (and the
      // matcher) need, and it stops key length growing exponentially
      // with N.
      const ids = new Map<string, number>();
      for (let c = 0; c < n; c++) {
        let id = ids.get(cur[c]);
        if (id === undefined) {
          id = ids.size;
          ids.set(cur[c], id);
        }
        cur[c] = String(id);
      }
      keyLevels.push(cur);
    }
  };

  /** Recurrence mode: paint the selected N-ball and every congruent
   *  site — matched centres in amber, the cells of each ball's rings in
   *  a paler gold — over the lens ground. Pure color painting; engine
   *  state is never touched in this mode. */
  const showRecurrence = (): void => {
    if (!universe || !renderer || selected < 0) return;
    const N = Number(q<HTMLInputElement>('.xp-nball').value);
    const radius = Number(radiusInput.value);
    const colors = lensColors();
    if (distances[selected] + N > radius) {
      info.textContent = 'too close to the boundary for a complete ball — try an inner tile';
      colors.set([...BALL_SOURCE, 255], selected * 4);
    } else {
      buildKeys(N);
      const keys = keyLevels[N];
      const n = universe.cellCount();
      const mark = new Uint8Array(n); // 1 = ring cell, 2 = matched centre
      let matches = 0;
      for (let c = 0; c < n; c++) {
        if (distances[c] + N > radius || keys[c] !== keys[selected]) continue;
        matches++;
        let frontier = [c];
        const seen = new Set([c]);
        for (let d = 0; d < N; d++) {
          const next: number[] = [];
          for (const u of frontier) {
            for (const v of universe.neighboursOf(u)) {
              if (seen.has(v)) continue;
              seen.add(v);
              next.push(v);
              if (mark[v] === 0) mark[v] = 1;
            }
          }
          frontier = next;
        }
        mark[c] = 2;
      }
      for (let c = 0; c < n; c++) {
        if (mark[c] === 2) colors.set([...BALL_CENTRE, 255], c * 4);
        else if (mark[c] === 1) colors.set([...BALL_RING, 255], c * 4);
      }
      colors.set([...BALL_SOURCE, 255], selected * 4);
      info.innerHTML = `this tile and its <b>${N}</b>-ring neighbourhood recur at <b>${matches}</b> sites in this patch`;
    }
    renderer.updateColors(colors);
    renderer.draw();
  };

  const zoomBy = (factor: number, px?: number, py?: number): void => {
    if (!renderer) return;
    if (px === undefined || py === undefined) renderer.zoom *= factor;
    else renderer.zoomAt(px, py, factor);
    renderer.draw();
  };
  q('.xp-zoomin').addEventListener('click', () => zoomBy(1.35));
  q('.xp-zoomout').addEventListener('click', () => zoomBy(1 / 1.35));
  canvas.addEventListener(
    'wheel',
    (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      zoomBy(Math.exp(-e.deltaY * 0.0015), e.clientX - rect.left, e.clientY - rect.top);
    },
    { passive: false },
  );

  // Drag pans; a press that never moves more than a few pixels is a click.
  let dragFrom: [number, number] | null = null;
  let dragged = false;
  canvas.addEventListener('pointerdown', (e) => {
    dragFrom = [e.clientX, e.clientY];
    dragged = false;
    canvas.setPointerCapture(e.pointerId);
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!dragFrom || !renderer) return;
    const dx = e.clientX - dragFrom[0];
    const dy = e.clientY - dragFrom[1];
    if (!dragged && Math.hypot(dx, dy) < 4) return;
    dragged = true;
    renderer.panBy(-dx, -dy);
    renderer.draw();
    dragFrom = [e.clientX, e.clientY];
  });
  canvas.addEventListener('pointerup', (e) => {
    dragFrom = null;
    if (dragged || !universe || !renderer) return;
    const rect = canvas.getBoundingClientRect();
    const [wx, wy] = renderer.screenToWorld(e.clientX - rect.left, e.clientY - rect.top);
    let best = -1;
    let bestD = tilePitch * tilePitch;
    for (let c = 0; c < universe.cellCount(); c++) {
      const dx = centroids[2 * c] - wx;
      const dy = centroids[2 * c + 1] - wy;
      const d = dx * dx + dy * dy;
      if (d < bestD) {
        bestD = d;
        best = c;
      }
    }
    if (best < 0) return;
    if (mode === 'recurrence') {
      selected = best === selected ? -1 : best;
      if (selected < 0) {
        info.textContent = idlePrompt;
        applyLens();
      } else {
        showRecurrence();
      }
      return;
    }
    universe.clear();
    if (best === selected) {
      selected = -1;
      info.textContent = idlePrompt;
    } else {
      selected = best;
      universe.toggleCell(best);
      const cls = classInfo[cellClasses[best]];
      info.innerHTML =
        `<code>${universe.addressOf(best)}</code> · class <b>${cls?.name ?? '?'}</b>` +
        ` · <b>${degrees[best]}</b> neighbours` +
        (artifacts.has(best) ? ' · <b style="color:#e06c75">seam artifact</b>' : '');
    }
    renderer.updateState(
      new Uint8Array(wasmMemory().buffer, universe.statePtr(), universe.cellCount()),
    );
    renderer.draw();
  });

  familySel.addEventListener('change', loadPatch);
  radiusInput.addEventListener('change', loadPatch);
  lensSel.addEventListener('change', applyLens);
  levelInput.addEventListener('input', applyLens);
  if (mode === 'recurrence') {
    // Sliding N re-runs the match live, so the thinning-out is watchable.
    const nInput = q<HTMLInputElement>('.xp-nball');
    nInput.addEventListener('input', () => {
      q('.xp-nval').textContent = nInput.value;
      showRecurrence();
    });
  }

  const observer = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting || renderer) continue;
      void ensureWasm().then(() => {
        renderer = new Renderer(canvas);
        new ResizeObserver(() => {
          renderer?.resize();
          renderer?.draw();
        }).observe(canvas);
        loadPatch();
      });
    }
  });
  observer.observe(container);
}
