// FlightPanel: an unbounded sliding-window flight — the instrument
// behind the paper's million-ring measurements, running live. The
// simulation runs in a Web Worker that races ahead of the display by a
// generation buffer, so the expensive window hops (patch generation)
// never stall the on-screen glider; the panel consumes buffered
// generations at display speed. Zoomed in the hops are invisible and
// the glider simply flies forever; zoomed out the substrate is visibly
// generated just ahead of it, and the last few retired windows remain
// as faded, already-visited terrain. The exact window→launch-frame
// transform keeps everything in one continuous global frame, so
// nothing ever jumps.
import { Renderer, type Theme } from './renderer';
import { TrackFitter } from './track-fit';
import { deadColors, isChiralMinority, type ClassInfo } from './essay-panel';
import type { GenMsg, WindowMsg, WorkerOut } from './flight-worker';

export interface FlightOptions {
  /** One JSONL ResultRecord line (imported verbatim from results/). */
  record: string;
  /** Flight window radius (default 24, the flight campaign's). */
  windowRadius?: number;
  /** Launch window radius (default 48). */
  launchRadius?: number;
  /** Generations on the launch window before the first hop (default 60). */
  launchGens?: number;
  /** Lane pick (degrees) for a launch that emits several objects. */
  selectHeading?: number;
  /** Generations per second while playing. */
  speed?: number;
  autoplay?: boolean;
  /** Kiosk chrome: no control bar, odometer overlay only. */
  kiosk?: boolean;
  /** 'dark' (default, the app palette) or 'light' — the paper's print
   *  palette: Figure 1 state colors on plain white, no chirality tint. */
  theme?: Theme;
  /** Initial zoom position in [0, 1]: 0 = whole window, 1 = close-up. */
  zoomLevel?: number;
  /** Retired windows kept as visited terrain (0 disables the trail). */
  trail?: number;
  /** Trail fade toward the background: 0 (default) keeps original
   *  colors — a seamless continuous world; ~0.55 for a dimmed trail. */
  trailFade?: number;
  /** Worker lookahead in generations (buffer absorbing hop pauses). */
  lookahead?: number;
  /** Caption HTML rendered under the canvas (ignored in kiosk mode). */
  caption?: string;
  downloadName?: string;
  /** CLI command shown in the reproduce popover. */
  reproduce?: string;
}

export function mountFlightPanel(container: HTMLElement, opts: FlightOptions): void {
  container.classList.add('essay-panel', 'flight-panel');
  container.innerHTML = `
    <div class="ep-stage">
      <canvas class="ep-canvas"></canvas>
      <div class="ep-loading">loading the tiling…</div>
      <div class="fp-odometer"></div>
    </div>
    <div class="ep-bar">
      <button class="ep-play" title="play/pause">▶</button>
      <button class="ep-step" title="one generation">+1</button>
      <button class="ep-reset" title="relaunch from the seed">⟲</button>
      <label class="fp-zoomlabel">zoom <input class="fp-zoom" type="range" min="0" max="1" step="0.001" /></label>
      <label class="ep-speedlabel"><input class="ep-speed" type="range" min="1" max="60" step="1" /><span class="ep-speedval"></span>/s</label>
      <span class="ep-spacer"></span>
      <button class="ep-download" title="download the ResultRecord this flight is flying">⤓ record</button>
    </div>
    <details class="ep-repro"><summary>reproduce this flight</summary><pre></pre></details>
    <div class="ep-caption"></div>
  `;
  const q = <T extends HTMLElement>(sel: string): T => container.querySelector(sel) as T;
  const canvas = q<HTMLCanvasElement>('.ep-canvas');
  const loading = q('.ep-loading');
  const odometer = q('.fp-odometer');
  const playBtn = q<HTMLButtonElement>('.ep-play');
  const speedInput = q<HTMLInputElement>('.ep-speed');
  const speedVal = q('.ep-speedval');
  const zoomInput = q<HTMLInputElement>('.fp-zoom');

  speedInput.value = String(opts.speed ?? 10);
  speedVal.textContent = speedInput.value;
  zoomInput.value = String(opts.zoomLevel ?? 0.8);
  q('.ep-caption').innerHTML = opts.caption ?? '';
  const repro = q<HTMLDetailsElement>('.ep-repro');
  if (opts.reproduce && !opts.kiosk) {
    repro.querySelector('pre')!.textContent = opts.reproduce;
  } else {
    repro.hidden = true;
  }
  if (opts.kiosk) {
    q('.ep-bar').style.display = 'none';
    q('.ep-caption').style.display = 'none';
  }

  const trailCap = opts.trail ?? 10;
  const lookahead = opts.lookahead ?? 64;

  let worker: Worker | null = null;
  let renderer: Renderer | null = null;
  // Buffered stream from the worker (gens in order; windows apply from
  // their gen onward).
  let genQueue: GenMsg[] = [];
  let windowQueue: WindowMsg[] = [];
  let shown: GenMsg | null = null; // the generation currently on screen
  let frame: number[] = [1, 0, 0, 1, 0, 0];
  let ppuOut = 1;
  let ppuIn = 100;
  let calibrated = false;
  let playing = opts.autoplay ?? false;
  let visible = false;
  let started = false;
  let stepAcc = 0;
  let lastTime = performance.now();
  let failed = false;
  let lastHops = -1;
  let hopHeadings: number[] = [];
  const aloftSince = new Date();
  const fitter = new TrackFitter(48);

  /** Decimal places for the heading, from the heading's spread over the
   *  last 32 hops (~430 rings). Successive-hop deltas are useless here —
   *  adjacent estimator windows share most of their points, so the value
   *  is smooth long before it is accurate; the spread tracks genuine
   *  convergence. Ratcheted (precision only grows), gated to 2 dp until
   *  the spread window fills, capped at 5 dp for the long-haul kiosk. */
  let shownDecimals = 1;
  const headingDecimals = (): number => {
    if (hopHeadings.length < 8) return shownDecimals;
    const ref = hopHeadings[0];
    let lo = 0;
    let hi = 0;
    for (const h of hopHeadings) {
      const d = ((h - ref + 540) % 360) - 180; // unwrap to (-180, 180]
      if (d < lo) lo = d;
      if (d > hi) hi = d;
    }
    const spread = hi - lo;
    let dp = spread > 0 ? Math.ceil(-Math.log10(spread)) : 5;
    dp = Math.min(dp, hopHeadings.length >= 32 ? 5 : 2);
    shownDecimals = Math.min(5, Math.max(shownDecimals, Math.max(1, dp)));
    return shownDecimals;
  };

  playBtn.textContent = playing ? '⏸' : '▶';
  const setPlaying = (p: boolean): void => {
    playing = p;
    playBtn.textContent = p ? '⏸' : '▶';
  };

  /** Swap the live window's geometry in (a hop, or the launch window). */
  const applyWindow = (w: WindowMsg): void => {
    if (!renderer) return;
    const t0 = performance.now();
    renderer.retireCurrent(trailCap);
    // Light theme: white ground with a subtle chirality tint — the
    // minority tiles of the natural two-coloring (antihat, Gamma pair,
    // dart, thin: tiling-core's Chirality strata) in pale blue-gray.
    // Dark keeps the class-tinted substrate.
    let ground: Uint8Array;
    const classInfo = JSON.parse(w.classInfoJson) as ClassInfo[];
    if ((opts.theme ?? 'dark') === 'light') {
      ground = new Uint8Array(w.cellCount * 4).fill(255);
      for (let c = 0; c < w.cellCount; c++) {
        if (isChiralMinority(classInfo[w.cellClasses[c]])) {
          ground.set([241, 244, 250, 255], c * 4);
        }
      }
    } else {
      ground = deadColors(classInfo, w.cellClasses, w.cellCount);
    }
    renderer.setPatch(
      w.triVertices,
      w.triCells,
      w.polyXy,
      w.polyOffsets,
      ground,
      w.cellCount,
    );
    renderer.setStates(w.states);
    frame = Array.from(w.frame);
    renderer.setFrame(frame);
    // Zoom endpoints are calibrated once, from tile pitch (a constant of
    // the tiling), and never change — so nothing rescales when the large
    // launch window hops to the smaller flight window, or if the window
    // ever auto-grows. The wide endpoint frames the flight window's span
    // even while the launch window (larger) is on screen.
    if (!calibrated) {
      calibrated = true;
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (let i = 0; i < w.polyXy.length; i += 2) {
        const x = w.polyXy[i];
        const y = w.polyXy[i + 1];
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
      const span = Math.max(maxX - minX, maxY - minY, 1e-6);
      const view = Math.min(canvas.clientWidth, canvas.clientHeight);
      const tile = span / (2 * w.windowRadius + 1); // mean tile pitch
      const flightSpan = tile * (2 * (opts.windowRadius ?? 24) + 1);
      // Wide endpoint: ~5 window spans, enough to frame the whole
      // default trail corridor, not just the live window.
      ppuOut = (0.9 * view) / (5 * flightSpan);
      ppuIn = view / (14 * tile); // close-up: ~14 tiles across
      applyZoom();
    }
    performance.measure('applyWindow', { start: t0 });
  };

  /** Consume up to `n` buffered generations; returns how many shown. */
  const consume = (n: number): number => {
    if (!renderer) return 0;
    let shownCount = 0;
    while (shownCount < n && genQueue.length > 0) {
      const g = genQueue.shift()!;
      while (windowQueue.length > 0 && windowQueue[0].gen <= g.gen) {
        applyWindow(windowQueue.shift()!);
      }
      shown = g;
      shownCount++;
    }
    if (shownCount > 0 && shown && worker) {
      renderer.updateState(shown.state);
      worker.postMessage({ type: 'ack', gen: shown.gen });
    }
    return shownCount;
  };

  const applyZoom = (): void => {
    if (!renderer) return;
    const z = Number(zoomInput.value);
    renderer.zoom = Math.exp(Math.log(ppuOut) * (1 - z) + Math.log(ppuIn) * z);
  };
  zoomInput.addEventListener('input', applyZoom);
  canvas.addEventListener(
    'wheel',
    (e) => {
      if (!e.ctrlKey && !e.metaKey && !opts.kiosk) return;
      e.preventDefault();
      const dz = -e.deltaY * 0.0007;
      zoomInput.value = String(Math.min(1, Math.max(0, Number(zoomInput.value) + dz)));
      applyZoom();
    },
    { passive: false },
  );

  const fail = (message: string): void => {
    failed = true;
    setPlaying(false);
    loading.style.display = '';
    loading.textContent = `flight ended: ${message}`;
  };

  const spawn = (): void => {
    worker?.terminate();
    genQueue = [];
    windowQueue = [];
    shown = null;
    calibrated = false;
    failed = false;
    lastHops = -1;
    hopHeadings = [];
    shownDecimals = 1;
    fitter.reset();
    renderer?.clearRetired();
    worker = new Worker(new URL('./flight-worker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = (e: MessageEvent<WorkerOut>) => {
      const msg = e.data;
      if (msg.type === 'gen') genQueue.push(msg);
      else if (msg.type === 'window') windowQueue.push(msg);
      else if (msg.type === 'fail') fail(msg.message);
    };
    worker.postMessage({
      type: 'launch',
      record: opts.record,
      windowRadius: opts.windowRadius ?? 24,
      launchRadius: opts.launchRadius ?? 48,
      launchGens: opts.launchGens ?? 60,
      selectHeading: opts.selectHeading,
      lookahead,
    });
  };

  playBtn.addEventListener('click', () => setPlaying(!playing));
  q('.ep-step').addEventListener('click', () => {
    setPlaying(false);
    consume(1);
  });
  q('.ep-reset').addEventListener('click', () => {
    if (!started) return;
    loading.style.display = '';
    loading.textContent = 'relaunching…';
    spawn();
  });
  speedInput.addEventListener('input', () => {
    speedVal.textContent = speedInput.value;
  });
  q('.ep-download').addEventListener('click', () => {
    const blob = new Blob([opts.record], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = opts.downloadName ?? 'record.jsonl';
    a.click();
    URL.revokeObjectURL(a.href);
  });

  const fmt = (n: number): string => n.toLocaleString('en-GB');
  const updateOdometer = (): void => {
    if (!shown) return;
    const heading = ((shown.heading % 360) + 360) % 360; // paper convention: [0, 360)
    if (shown.hops !== lastHops && Number.isFinite(heading)) {
      lastHops = shown.hops;
      hopHeadings.push(heading);
      if (hopHeadings.length > 32) hopHeadings.shift();
    }
    const parts = [
      `gen <b>${fmt(shown.gen)}</b>`,
      `path <b>${fmt(shown.pathRings)}</b> tiles`,
      `hops <b>${fmt(shown.hops)}</b>`,
      Number.isFinite(heading) ? `heading <b>${heading.toFixed(headingDecimals())}°</b>` : '',
      `pop <b>${shown.population}</b>`,
    ];
    if (opts.kiosk) {
      parts.push(
        `aloft since <b>${aloftSince.toLocaleString('en-GB', {
          day: 'numeric',
          month: 'short',
          year: 'numeric',
          hour: '2-digit',
          minute: '2-digit',
        })}</b> · terrain that never cycles`,
      );
    }
    odometer.innerHTML = parts.filter(Boolean).join(' · ');
  };

  const tick = (now: number): void => {
    requestAnimationFrame(tick);
    if (!visible || !renderer || failed) return;
    const dt = Math.min((now - lastTime) / 1000, 0.25);
    lastTime = now;

    // First frames: wait for the launch window + generation 0.
    if (!shown) {
      if (consume(1) === 0) return;
      loading.style.display = 'none';
      const g0 = shown as GenMsg | null;
      if (g0) renderer.centerOn(g0.global[0], g0.global[1]);
      applyZoom();
    } else if (playing) {
      stepAcc += dt * Number(speedInput.value);
      const n = Math.floor(stepAcc);
      if (n > 0) {
        const consumed = consume(n);
        stepAcc -= n;
        if (consumed < n) stepAcc = 0; // underrun: hold, don't bank debt
      }
    }

    if (shown) {
      fitter.sample(shown.gen, shown.global[0], shown.global[1]);
      const target = fitter.target(shown.gen) ?? shown.global;
      const k = 1 - Math.exp(-3 * dt);
      const [cx, cy] = renderer.screenToWorld(canvas.clientWidth / 2, canvas.clientHeight / 2);
      renderer.centerOn(cx + (target[0] - cx) * k, cy + (target[1] - cy) * k);
      updateOdometer();
    }
    renderer.draw();
  };

  const start = (): void => {
    if (started) return;
    started = true;
    renderer = new Renderer(canvas, opts.theme ?? 'dark');
    renderer.setRetiredFade(opts.trailFade ?? 0);
    new ResizeObserver(() => renderer?.resize()).observe(canvas);
    spawn();
    requestAnimationFrame(tick);
  };

  const observer = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      visible = entry.isIntersecting;
      if (visible) start();
    }
  });
  observer.observe(container);
}
