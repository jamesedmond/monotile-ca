// EssayPanel: a self-contained live CA panel for the computational
// essay. Each panel replays one committed ResultRecord through the
// same wasm engine that produced it (the "click the picture, get the
// evidence" principle): canvas + minimal transport controls + caption
// + a download button exposing the record and its reproduction
// command. Panels lazy-initialise when scrolled into view and pause
// when off-screen.
import { Universe } from './wasm/ui_wasm.js';
import { Renderer } from './renderer';
import { ensureWasm, wasmMemory } from './wasm-shared';
import { TrackFitter } from './track-fit';

export interface PanelOptions {
  /** One JSONL ResultRecord line (imported verbatim from results/).
   *  Omit when `makeUniverse` supplies the universe directly. */
  record?: string;
  /** Construct the universe directly (e.g. Universe.createGrid for the
   *  square-grid demos) instead of replaying a record. */
  makeUniverse?: () => Universe;
  /** Patch radius to replay at (record's own radius if omitted). */
  radius?: number;
  /** Generations per second while playing. */
  speed?: number;
  autoplay?: boolean;
  /** Restart from the initial state after this generation (looping). */
  loopAtGeneration?: number;
  /** Pause before looping, in milliseconds. */
  loopPauseMs?: number;
  /** Follow the live-cell centroid instead of showing the whole patch. */
  follow?: boolean;
  /** Zoom multiple relative to the whole-patch fit (with follow). */
  followZoom?: number;
  /** With follow: track only the cluster of live cells within this
   *  fraction of the patch span of the previous camera target — keeps
   *  the camera on ONE object when a seed spawns several (otherwise
   *  the centroid sits in the empty middle). Falls back to the global
   *  centroid if the tracked cluster dies. */
  followCluster?: number;
  /** With followCluster: when the tracked cluster is lost, lock to the
   *  live cell FARTHEST from the seed instead of the nearest — follows
   *  the traveller of a pair rather than the homebody. */
  followFarthest?: boolean;
  /** Static close-up: centre on this cell at zoom x the whole-patch
   *  fit, camera locked (no follow). */
  focus?: { cell: number; zoom: number };
  /** Causality-filter overlay: tint the allowed light cone — every
   *  cell within (seed extent + generation x this fan margin) rings —
   *  a step lighter, so the reader can watch activity stay inside it
   *  (or, for a gamed rule, jump outside it). */
  lightCone?: number;
  /** Cross-radius overlay: highlight a two-cell band at each of these
   *  graph distances — the verification radii, drawn on the board. */
  rings?: number[];
  /** Caption HTML rendered under the canvas. */
  caption?: string;
  /** Filename for the record download. */
  downloadName?: string;
  /** CLI command shown in the reproduce popover. */
  reproduce?: string;
}

export interface ClassInfo {
  name?: string;
  base: string;
  parent?: string;
}

function hsl(h: number, s: number, l: number): [number, number, number] {
  const f = (n: number): number => {
    const k = (n + h / 30) % 12;
    const a = s * Math.min(l, 1 - l);
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  };
  return [f(0), f(8), f(4)];
}

/** The minority tiles of the natural two-coloring (tiling-core's
 *  Chirality strata): antihat, Gamma/Mystic pair, dart, thin. */
export function isChiralMinority(info: ClassInfo | undefined): boolean {
  return (
    !!info &&
    (info.base === 'antihat' ||
      info.parent === 'Gamma' ||
      info.base === 'dart' ||
      info.base === 'thin')
  );
}

/** Per-cell dead-tile tint. The two-coloring is carried by LUMINANCE,
 *  not hue: at these light levels human hue discrimination is nearly
 *  gone (an earlier equal-lightness palette hid the antihat rows for
 *  months; the light theme's tint exposed them in one look). Majority
 *  tiles sit barely above the background, desaturated; the chirality
 *  minority is a clear step lighter with a touch of colour. */
export function deadColors(classInfo: ClassInfo[], cellClasses: Uint16Array, cellCount: number): Uint8Array {
  const out = new Uint8Array(cellCount * 4);
  const majority = hsl(215, 0.08, 0.075);
  const minority = hsl(220, 0.06, 0.085);
  for (let c = 0; c < cellCount; c++) {
    const tint = isChiralMinority(classInfo[cellClasses[c]]) ? minority : majority;
    out.set(tint, c * 4);
    out[c * 4 + 3] = 255;
  }
  return out;
}

export function mountEssayPanel(container: HTMLElement, opts: PanelOptions): void {
  container.classList.add('essay-panel');
  container.innerHTML = `
    <div class="ep-stage">
      <canvas class="ep-canvas"></canvas>
      <div class="ep-loading">loading the tiling…</div>
    </div>
    <div class="ep-bar">
      <button class="ep-play" title="play/pause">▶</button>
      <button class="ep-step" title="one generation">+1</button>
      <button class="ep-reset" title="restart from the initial state">⟲</button>
      <button class="ep-zoomout" title="zoom out">−</button>
      <button class="ep-zoomin" title="zoom in (or ctrl/cmd-scroll the view)">+</button>
      <label class="ep-speedlabel"><input class="ep-speed" type="range" min="1" max="60" step="1" /><span class="ep-speedval"></span>/s</label>
      <span class="ep-stats">gen <b class="ep-gen">0</b> · pop <b class="ep-pop">0</b></span>
      <span class="ep-spacer"></span>
      <button class="ep-download" title="download the ResultRecord this panel is replaying">⤓ record</button>
    </div>
    <details class="ep-repro"><summary>reproduce this run</summary><pre></pre></details>
    <div class="ep-caption"></div>
  `;
  const q = <T extends HTMLElement>(sel: string): T => container.querySelector(sel) as T;
  const canvas = q<HTMLCanvasElement>('.ep-canvas');
  const loading = q('.ep-loading');
  const playBtn = q<HTMLButtonElement>('.ep-play');
  const speedInput = q<HTMLInputElement>('.ep-speed');
  const speedVal = q('.ep-speedval');
  const genEl = q('.ep-gen');
  const popEl = q('.ep-pop');

  speedInput.value = String(opts.speed ?? 12);
  speedVal.textContent = speedInput.value;
  q('.ep-caption').innerHTML = opts.caption ?? '';
  const repro = q<HTMLDetailsElement>('.ep-repro');
  if (opts.reproduce) {
    repro.querySelector('pre')!.textContent = opts.reproduce;
  } else {
    repro.hidden = true;
  }

  let universe: Universe | null = null;
  let renderer: Renderer | null = null;
  let centroids: Float32Array = new Float32Array(0);
  let cellCount = 0;
  let playing = opts.autoplay ?? false;
  let visible = false;
  let started = false;
  let stepAcc = 0;
  let lastTime = performance.now();
  let loopPauseUntil = 0;
  let fitZoom = 1;
  let coneDistances: Uint32Array = new Uint32Array(0);
  let coneBase: Uint8Array = new Uint8Array(0);
  let coneSeedMax = 0;
  let lastConeGen = -1;

  playBtn.textContent = playing ? '⏸' : '▶';

  const setPlaying = (p: boolean): void => {
    playing = p;
    playBtn.textContent = p ? '⏸' : '▶';
  };

  playBtn.addEventListener('click', () => setPlaying(!playing));
  q('.ep-step').addEventListener('click', () => {
    setPlaying(false);
    universe?.step(1);
  });
  q('.ep-reset').addEventListener('click', () => universe?.restartFromInitial());
  speedInput.addEventListener('input', () => {
    speedVal.textContent = speedInput.value;
  });
  if (!opts.record) q<HTMLButtonElement>('.ep-download').style.display = 'none';
  q('.ep-download').addEventListener('click', () => {
    if (!opts.record) return;
    const blob = new Blob([opts.record], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = opts.downloadName ?? 'record.jsonl';
    a.click();
    URL.revokeObjectURL(a.href);
  });
  const zoomBy = (factor: number, px?: number, py?: number): void => {
    if (!renderer) return;
    if (opts.follow || px === undefined || py === undefined) {
      renderer.zoom *= factor; // follow-cam owns the centre
    } else {
      renderer.zoomAt(px, py, factor);
    }
    renderer.draw();
  };
  q('.ep-zoomin').addEventListener('click', () => zoomBy(1.35));
  q('.ep-zoomout').addEventListener('click', () => zoomBy(1 / 1.35));
  canvas.addEventListener(
    'wheel',
    (e) => {
      if (!e.ctrlKey && !e.metaKey) return; // don't hijack page scroll
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      zoomBy(Math.exp(-e.deltaY * 0.0015), e.clientX - rect.left, e.clientY - rect.top);
    },
    { passive: false },
  );

  /** Mean position of live cells (world coords) for the follow-cam.
   *  With followCluster, only cells near the previous target count, so
   *  the camera stays on one object among several. */
  let clusterAt: [number, number] | null = null;
  let patchSpan = 1;
  const activeCentroid = (): [number, number] | null => {
    if (!universe) return null;
    const state = new Uint8Array(wasmMemory().buffer, universe.statePtr(), cellCount);
    const sum = (filter: boolean): [number, number, number] => {
      const r = filter && opts.followCluster ? opts.followCluster * patchSpan : Infinity;
      const r2 = r * r;
      let sx = 0;
      let sy = 0;
      let n = 0;
      for (let c = 0; c < cellCount; c++) {
        if (state[c] === 0) continue;
        const x = centroids[2 * c];
        const y = centroids[2 * c + 1];
        if (clusterAt) {
          const dx = x - clusterAt[0];
          const dy = y - clusterAt[1];
          if (dx * dx + dy * dy > r2) continue;
        }
        sx += x;
        sy += y;
        n++;
      }
      return [sx, sy, n];
    };
    if (opts.followFarthest && opts.followCluster) {
      // Anchor on the farthest live cell from the seed every frame —
      // the traveller of a pair always wins, before and after capture.
      let far = -1;
      let farD = -Infinity;
      for (let c = 0; c < cellCount; c++) {
        if (state[c] === 0) continue;
        const dx = centroids[2 * c] - centroids[0];
        const dy = centroids[2 * c + 1] - centroids[1];
        const d = dx * dx + dy * dy;
        if (d > farD) {
          farD = d;
          far = c;
        }
      }
      if (far < 0) return null;
      clusterAt = [centroids[2 * far], centroids[2 * far + 1]];
      const [fx, fy, fn] = sum(true);
      if (fn === 0) return null;
      clusterAt = [fx / fn, fy / fn];
      return clusterAt;
    }
    let [sx, sy, n] = sum(true);
    if (n === 0 && opts.followCluster && clusterAt) {
      // Tracked cluster gone (or the group split past the radius):
      // lock onto one live cell and re-filter around it — never the
      // global centroid, which sits in the empty middle. Nearest to the
      // previous target by default; farthest from the seed (cell 0)
      // with followFarthest.
      const ox = opts.followFarthest ? centroids[0] : clusterAt[0];
      const oy = opts.followFarthest ? centroids[1] : clusterAt[1];
      let best = -1;
      let bestD = opts.followFarthest ? -Infinity : Infinity;
      for (let c = 0; c < cellCount; c++) {
        if (state[c] === 0) continue;
        const dx = centroids[2 * c] - ox;
        const dy = centroids[2 * c + 1] - oy;
        const d = dx * dx + dy * dy;
        if (opts.followFarthest ? d > bestD : d < bestD) {
          bestD = d;
          best = c;
        }
      }
      if (best >= 0) {
        clusterAt = [centroids[2 * best], centroids[2 * best + 1]];
        [sx, sy, n] = sum(true);
      }
    }
    if (n === 0) return null;
    clusterAt = [sx / n, sy / n];
    return clusterAt;
  };

  // Follow-cam smoothing: see TrackFitter (shared with the space-time
  // panel's camera).
  const fitter = new TrackFitter(48);

  const frame = (now: number): void => {
    requestAnimationFrame(frame);
    if (!visible || !universe || !renderer) return;
    const dt = Math.min((now - lastTime) / 1000, 0.25);
    lastTime = now;
    if (playing && now >= loopPauseUntil) {
      stepAcc += dt * Number(speedInput.value);
      const n = Math.floor(stepAcc);
      stepAcc -= n;
      if (n > 0) universe.step(n);
      if (opts.loopAtGeneration && universe.generation() >= opts.loopAtGeneration) {
        loopPauseUntil = now + (opts.loopPauseMs ?? 1200);
        universe.restartFromInitial();
        // Show the initial state during the pause; snap the camera home.
        fitter.reset();
        clusterAt = null;
        const home = activeCentroid();
        if (opts.follow && home) renderer.centerOn(home[0], home[1]);
      }
    }
    renderer.updateState(new Uint8Array(wasmMemory().buffer, universe.statePtr(), cellCount));
    const gen = universe.generation();
    if (opts.lightCone !== undefined && gen !== lastConeGen) {
      lastConeGen = gen;
      const allowed = coneSeedMax + gen * opts.lightCone;
      const tinted = coneBase.slice();
      for (let c = 0; c < cellCount; c++) {
        if (coneDistances[c] <= allowed) {
          tinted[c * 4] = Math.min(255, tinted[c * 4] + 16);
          tinted[c * 4 + 1] = Math.min(255, tinted[c * 4 + 1] + 16);
          tinted[c * 4 + 2] = Math.min(255, tinted[c * 4 + 2] + 18);
        }
      }
      renderer.updateColors(tinted);
    }
    genEl.textContent = String(gen);
    popEl.textContent = String(universe.population());
    if (opts.follow) {
      const c = activeCentroid();
      if (c) fitter.sample(gen, c[0], c[1]);
      const target = fitter.target(gen);
      if (target) {
        // Framerate-independent easing toward the fitted path.
        const k = 1 - Math.exp(-3 * dt);
        const [cx, cy] = renderer.screenToWorld(canvas.clientWidth / 2, canvas.clientHeight / 2);
        renderer.centerOn(cx + (target[0] - cx) * k, cy + (target[1] - cy) * k);
      }
    }
    renderer.draw();
  };

  const start = async (): Promise<void> => {
    if (started) return;
    started = true;
    await ensureWasm();
    // Let the loading shimmer paint before the (blocking) patch build.
    await new Promise((r) => setTimeout(r, 30));
    try {
      if (opts.makeUniverse) {
        universe = opts.makeUniverse();
      } else if (opts.record) {
        universe =
          opts.radius === undefined
            ? Universe.createFromResult(opts.record)
            : Universe.createFromResultAt(opts.record, opts.radius);
      } else {
        throw new Error('panel needs a record or makeUniverse');
      }
    } catch (err) {
      loading.textContent = `failed to load record: ${err instanceof Error ? err.message : String(err)}`;
      return;
    }
    cellCount = universe.cellCount();
    const polyXy = universe.polygonXy();
    const polyOffsets = universe.polygonOffsets();
    const classInfo = JSON.parse(universe.classInfoJson()) as ClassInfo[];
    const cellClasses = new Uint16Array(universe.cellClasses());
    renderer = new Renderer(canvas);
    const baseColors = deadColors(classInfo, cellClasses, cellCount);
    if (opts.rings) {
      const dist = new Uint32Array(universe.cellDistances());
      for (let c = 0; c < cellCount; c++) {
        for (const r of opts.rings) {
          if (dist[c] === r || dist[c] === r - 1) {
            baseColors[c * 4] = Math.min(255, baseColors[c * 4] + 18);
            baseColors[c * 4 + 1] = Math.min(255, baseColors[c * 4 + 1] + 26);
            baseColors[c * 4 + 2] = Math.min(255, baseColors[c * 4 + 2] + 46);
            break;
          }
        }
      }
    }
    renderer.setPatch(
      new Float32Array(universe.triVertices()),
      new Uint32Array(universe.triCells()),
      new Float32Array(polyXy),
      new Uint32Array(polyOffsets),
      baseColors,
      cellCount,
    );
    renderer.setStates(universe.states());
    if (opts.lightCone !== undefined) {
      coneDistances = new Uint32Array(universe.cellDistances());
      coneBase = baseColors;
      const st = new Uint8Array(wasmMemory().buffer, universe.statePtr(), cellCount);
      for (let c = 0; c < cellCount; c++) {
        if (st[c] !== 0 && coneDistances[c] > coneSeedMax) coneSeedMax = coneDistances[c];
      }
    }

    // Cell centroids (for the follow-cam) + whole-patch bounds.
    centroids = new Float32Array(cellCount * 2);
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (let c = 0; c < cellCount; c++) {
      const s = polyOffsets[c];
      const e = polyOffsets[c + 1];
      let sx = 0;
      let sy = 0;
      for (let i = s; i < e; i++) {
        const x = polyXy[2 * i];
        const y = polyXy[2 * i + 1];
        sx += x;
        sy += y;
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
      centroids[2 * c] = sx / (e - s);
      centroids[2 * c + 1] = sy / (e - s);
    }
    renderer.resize();
    renderer.fitBounds(minX, minY, maxX, maxY);
    fitZoom = renderer.zoom;
    patchSpan = Math.max(maxX - minX, maxY - minY);
    if (opts.follow) {
      renderer.zoom = fitZoom * (opts.followZoom ?? 5);
      const c = activeCentroid();
      if (c) renderer.centerOn(c[0], c[1]);
    } else if (opts.focus) {
      renderer.zoom = fitZoom * opts.focus.zoom;
      renderer.centerOn(centroids[2 * opts.focus.cell], centroids[2 * opts.focus.cell + 1]);
    }
    loading.remove();
    new ResizeObserver(() => renderer?.resize()).observe(canvas);
    lastTime = performance.now();
    requestAnimationFrame(frame);
  };

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        visible = entry.isIntersecting;
        if (visible) {
          lastTime = performance.now();
          void start();
        }
      }
    },
    { rootMargin: '200px' },
  );
  observer.observe(container);
}
