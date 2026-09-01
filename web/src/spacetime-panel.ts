// SpaceTimePanel: the NKS-style space-time rendering of a replayed
// record — non-quiescent cells drawn as tile-shaped prisms stacked
// along z = time, older layers fading (opaquely) toward the background
// until they vanish, with the substrate's tile outlines drawn at the
// current time slice. Because the objects of this project are sparse
// (populations 12–32 in an empty universe), the worldtube of a glider
// is a thin luminous column climbing through empty space: speed is
// slope, collisions are vertices, loops are helices. The camera's xy
// follows the same constant-velocity fit as the 2D panel while its z
// climbs with time; drag orbits, ctrl/cmd-scroll (or +/−) zooms.
//
// Deliberately flat-shaded: top faces at full state colour, side walls
// darkened by a fixed per-edge factor. No transparency (fade mixes
// toward the background colour, then the fragment is discarded), so no
// depth-sorting is needed.
import { Universe } from './wasm/ui_wasm.js';
import { ensureWasm, wasmMemory } from './wasm-shared';
import type { LaunchMsg, WorkerOut } from './flight-worker';
import { TrackFitter } from './track-fit';

export interface SpaceTimeOptions {
  record: string;
  radius?: number;
  /** Fly the record under the sliding-window instrument instead of a
   *  fixed patch: the tube climbs forever, the substrate re-roots at
   *  each hop, and layer prisms are baked in global (frame-composed)
   *  coordinates. */
  flight?: {
    windowRadius?: number;
    launchRadius?: number;
    launchGens?: number;
    selectHeading?: number;
  };
  speed?: number;
  autoplay?: boolean;
  loopAtGeneration?: number;
  loopPauseMs?: number;
  /** World units of height per generation (tube slope; default 1.6). */
  dzPerGeneration?: number;
  /** Generations over which a layer fades to nothing (default 48). */
  fadeGenerations?: number;
  /** Initial camera distance in world units (default 90). */
  cameraDistance?: number;
  /** 'dark' (essay, default) or 'light' (paper palette on white). */
  theme?: 'dark' | 'light';
  /** Auto-orient the camera relative to the object's fitted heading
   *  (default true; any manual drag takes over until reset). */
  autoOrient?: boolean;
  /** Camera azimuth offset from directly-behind, degrees (default 80). */
  headingOffsetDeg?: number;
  /** Camera presets cycled by the view button: (offsetDeg, elevationRad). */
  views?: { offsetDeg: number; elevation: number; label: string }[];
  /** Follow one glider (the spatial cluster nearest the camera) rather
   *  than the global centroid — for records that launch several. */
  followComponent?: boolean;
  caption?: string;
  downloadName?: string;
  reproduce?: string;
}

const MAX_CELLS_PER_LAYER = 48;
const FLOATS_PER_VERT = 6; // x y z shade state birth
/** Tile outlines have ≤ 14 vertices ⇒ ≤ 12 top + 28 wall triangles. */
const VERTS_PER_CELL = 40 * 3;
const BG_DARK: [number, number, number] = [0.043, 0.051, 0.063];
const BG_LIGHT: [number, number, number] = [1, 1, 1];

const VS = `#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in float a_shade;
layout(location = 2) in float a_state;
layout(location = 3) in float a_birth;
uniform mat4 u_mvp;
uniform float u_now;
uniform float u_fade;
out float v_shade;
out float v_state;
out float v_age;
void main() {
  v_shade = a_shade;
  v_state = a_state;
  v_age = (u_now - a_birth) / u_fade;
  gl_Position = u_mvp * vec4(a_pos, 1.0);
}`;

const FS = `#version 300 es
precision highp float;
in float v_shade;
in float v_state;
in float v_age;
uniform vec3 u_bg;
uniform vec3 u_c1;
uniform vec3 u_c2;
uniform vec3 u_c3;
out vec4 o_color;
void main() {
  if (v_age >= 1.0 || v_age < 0.0) discard;
  vec3 base = v_state < 1.5 ? u_c1 : (v_state < 2.5 ? u_c2 : u_c3);
  float keep = pow(1.0 - v_age, 1.3);
  o_color = vec4(mix(u_bg, base * v_shade, keep), 1.0);
}`;

const LINE_VS = `#version 300 es
layout(location = 0) in vec2 a_pos;
uniform mat4 u_mvp;
uniform float u_z;
void main() { gl_Position = u_mvp * vec4(a_pos, u_z, 1.0); }`;

const LINE_FS = `#version 300 es
precision highp float;
uniform vec3 u_color;
out vec4 o_color;
void main() { o_color = vec4(u_color, 1.0); }`;

// ---------------------------------------------------------- mat4 helpers

function perspective(fovY: number, aspect: number, near: number, far: number): Float32Array {
  const f = 1 / Math.tan(fovY / 2);
  const out = new Float32Array(16);
  out[0] = f / aspect;
  out[5] = f;
  out[10] = (far + near) / (near - far);
  out[11] = -1;
  out[14] = (2 * far * near) / (near - far);
  return out;
}

function lookAt(eye: number[], center: number[], up: number[]): Float32Array {
  const sub = (a: number[], b: number[]): number[] => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
  const norm = (v: number[]): number[] => {
    const l = Math.hypot(v[0], v[1], v[2]) || 1;
    return [v[0] / l, v[1] / l, v[2] / l];
  };
  const cross = (a: number[], b: number[]): number[] => [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
  const dot = (a: number[], b: number[]): number => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const z = norm(sub(eye, center));
  const x = norm(cross(up, z));
  const y = cross(z, x);
  // Column-major view matrix.
  return new Float32Array([
    x[0], y[0], z[0], 0,
    x[1], y[1], z[1], 0,
    x[2], y[2], z[2], 0,
    -dot(x, eye), -dot(y, eye), -dot(z, eye), 1,
  ]);
}

function mul(a: Float32Array, b: Float32Array): Float32Array {
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      out[c * 4 + r] = s;
    }
  }
  return out;
}

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(sh) ?? 'shader compile failed');
  }
  return sh;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const p = gl.createProgram()!;
  gl.attachShader(p, compile(gl, gl.VERTEX_SHADER, vs));
  gl.attachShader(p, compile(gl, gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(p) ?? 'program link failed');
  }
  return p;
}

/** Figure-1 palette (render_frames): vermilion, green, pale blue. */
const LIGHT_STATE_COLORS: [number[], number[], number[]] = [
  [0.835, 0.369, 0.0],
  [0.373, 0.722, 0.471],
  [0.718, 0.804, 0.91],
];

/** State colours matching the 2D renderer's alive/dying palette. */
function stateColors(k: number): [number[], number[], number[]] {
  const alive = [0.98, 0.72, 0.25];
  const ember = alive.map((v, i) => v + ([0.55, 0.16, 0.42][i] - v) * 0.85);
  const dead = [0.16, 0.18, 0.22];
  const phase = (s: number): number[] => {
    if (k <= 2 || s === 1) return alive;
    const age = (s - 1) / Math.max(k - 1, 1);
    return ember.map((v, i) => v + (dead[i] - v) * age);
  };
  return [alive, phase(2), phase(3)];
}

export function mountSpaceTimePanel(container: HTMLElement, opts: SpaceTimeOptions): () => void {
  container.classList.add('essay-panel');
  container.innerHTML = `
    <div class="ep-stage">
      <canvas class="st-canvas" title="drag to orbit; ctrl/cmd-scroll to zoom"></canvas>
      <div class="ep-loading">loading the tiling…</div>
    </div>
    <div class="ep-bar">
      <button class="ep-play" title="play/pause">▶</button>
      <button class="ep-step" title="one generation">+1</button>
      <button class="ep-reset" title="restart from the initial state">⟲</button>
      <button class="ep-zoomout" title="zoom out">−</button>
      <button class="ep-zoomin" title="zoom in (or ctrl/cmd-scroll)">+</button>
      <button class="ep-view" title="cycle camera view"></button>
      <label class="ep-speedlabel"><input class="ep-speed" type="range" min="1" max="30" step="1" /><span class="ep-speedval"></span>/s</label>
      <span class="ep-stats">gen <b class="ep-gen">0</b> · pop <b class="ep-pop">0</b></span>
      <span class="ep-spacer"></span>
      <button class="ep-download" title="download the ResultRecord this panel is replaying">⤓ record</button>
    </div>
    <details class="ep-repro"><summary>reproduce this run</summary><pre></pre></details>
    <div class="ep-caption"></div>
  `;
  const q = <T extends HTMLElement>(sel: string): T => container.querySelector(sel) as T;
  const canvas = q<HTMLCanvasElement>('.st-canvas');
  const loading = q('.ep-loading');
  const playBtn = q<HTMLButtonElement>('.ep-play');
  const speedInput = q<HTMLInputElement>('.ep-speed');
  const speedVal = q('.ep-speedval');
  const genEl = q('.ep-gen');
  const popEl = q('.ep-pop');

  speedInput.value = String(opts.speed ?? 10);
  speedVal.textContent = speedInput.value;
  q('.ep-caption').innerHTML = opts.caption ?? '';
  const repro = q<HTMLDetailsElement>('.ep-repro');
  if (opts.reproduce) {
    repro.querySelector('pre')!.textContent = opts.reproduce;
  } else {
    repro.hidden = true;
  }

  const dz = opts.dzPerGeneration ?? 1.6;
  const fadeGens = opts.fadeGenerations ?? 48;
  const light = opts.theme === 'light';
  const bg = light ? BG_LIGHT : BG_DARK;
  const lineColor = light ? [0.72, 0.745, 0.79] : [0.16, 0.17, 0.2];
  // Heading-relative camera presets (angle-test winners I and M).
  const views = opts.views ?? [
    { offsetDeg: opts.headingOffsetDeg ?? 80, elevation: 0.262, label: 'above' },
    { offsetDeg: 135, elevation: -0.175, label: 'below' },
  ];
  let viewIndex = 0;
  const maxLayers = fadeGens + 2;
  const slotVerts = MAX_CELLS_PER_LAYER * VERTS_PER_CELL;
  const slotFloats = slotVerts * FLOATS_PER_VERT;

  let universe: Universe | null = null;
  let gl: WebGL2RenderingContext | null = null;
  let prismProg: WebGLProgram | null = null;
  let lineProg: WebGLProgram | null = null;
  let prismVao: WebGLVertexArrayObject | null = null;
  let lineVao: WebGLVertexArrayObject | null = null;
  let prismBuf: WebGLBuffer | null = null;
  let lineBuf: WebGLBuffer | null = null;
  let lineVertCount = 0;
  let failed = false;
  let disposed = false;
  const isFlight = !!opts.flight;
  let worker: Worker | null = null;
  let queue: WorkerOut[] = [];
  let workerState: Uint8Array = new Uint8Array(0);
  let curGen = 0;
  let curPop = 0;
  let ready = false;
  let cellCount = 0;
  let centroids: Float32Array = new Float32Array(0);
  let cellTris: Float32Array[] = []; // per cell: flat xy triangle list
  let cellOutline: Float32Array[] = []; // per cell: flat xy ring
  let playing = opts.autoplay ?? false;
  let visible = false;
  let started = false;
  let stepAcc = 0;
  let lastTime = performance.now();
  let loopPauseUntil = 0;
  const fitter = new TrackFitter(48);
  const layerScratch = new Float32Array(slotFloats);

  // Camera: azimuth behind+offset relative to the object's fitted
  // heading, elevation from the active preset; a manual drag takes
  // over until reset or the next view change.
  let azimuth = -1.05; // radians; swings to heading-relative once moving
  let elevation = views[0].elevation;
  let autoOrient = opts.autoOrient ?? true;
  let distance = opts.cameraDistance ?? 90;
  let camX = 0;
  let camY = 0;
  let camZ = 0;

  playBtn.textContent = playing ? '⏸' : '▶';
  const setPlaying = (p: boolean): void => {
    playing = p;
    playBtn.textContent = p ? '⏸' : '▶';
  };

  const viewBtn = q<HTMLButtonElement>('.ep-view');
  viewBtn.textContent = `⟲ view: ${views[0].label}`;
  viewBtn.addEventListener('click', () => {
    viewIndex = (viewIndex + 1) % views.length;
    viewBtn.textContent = `⟲ view: ${views[viewIndex].label}`;
    autoOrient = true; // re-engage heading tracking for the new preset
  });

  const state = (): Uint8Array =>
    isFlight
      ? workerState
      : new Uint8Array(wasmMemory().buffer, universe!.statePtr(), cellCount);

  /** (Re)read the current patch/window geometry into the per-cell
   *  arrays and the substrate line buffer. Flight windows are local
   *  coordinates; compose with the flight frame so everything lives in
   *  one global world and the tube keeps climbing through it. */
  interface GeomBundle {
    cellCount: number;
    states: number;
    polyXy: Float32Array;
    polyOffsets: Uint32Array;
    triVerts: Float32Array;
    triCells: Uint32Array;
    frame: number[];
  }
  const applyGeometry = (g: GeomBundle): void => {
    if (!gl) return;
    cellCount = g.cellCount;
    const { polyXy, polyOffsets, triVerts, triCells } = g;
    const f = g.frame;
    const gx = (x: number, y: number): number => f[0] * x + f[1] * y + f[4];
    const gy = (x: number, y: number): number => f[2] * x + f[3] * y + f[5];
    const triLists: number[][] = Array.from({ length: cellCount }, () => []);
    for (let v = 0; v < triCells.length; v++) {
      triLists[triCells[v]].push(
        gx(triVerts[2 * v], triVerts[2 * v + 1]),
        gy(triVerts[2 * v], triVerts[2 * v + 1]),
      );
    }
    cellTris = triLists.map((l) => Float32Array.from(l));
    cellOutline = [];
    centroids = new Float32Array(cellCount * 2);
    for (let c = 0; c < cellCount; c++) {
      const s = polyOffsets[c];
      const e = polyOffsets[c + 1];
      const ring = new Float32Array((e - s) * 2);
      let sx = 0;
      let sy = 0;
      for (let i = s; i < e; i++) {
        const x = gx(polyXy[2 * i], polyXy[2 * i + 1]);
        const y = gy(polyXy[2 * i], polyXy[2 * i + 1]);
        ring[2 * (i - s)] = x;
        ring[2 * (i - s) + 1] = y;
        sx += x;
        sy += y;
      }
      cellOutline.push(ring);
      centroids[2 * c] = sx / (e - s);
      centroids[2 * c + 1] = sy / (e - s);
    }
    const lines: number[] = [];
    for (let c = 0; c < cellCount; c++) {
      const ring = cellOutline[c];
      const n = ring.length / 2;
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        lines.push(ring[2 * i], ring[2 * i + 1], ring[2 * j], ring[2 * j + 1]);
      }
    }
    lineVertCount = lines.length / 2;
    gl.bindVertexArray(lineVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, lineBuf);
    gl.bufferData(gl.ARRAY_BUFFER, Float32Array.from(lines), gl.DYNAMIC_DRAW);
    gl.bindVertexArray(null);
    if (prismProg) {
      const [c1, c2, c3] = light ? LIGHT_STATE_COLORS : stateColors(g.states);
      gl.useProgram(prismProg);
      gl.uniform3fv(gl.getUniformLocation(prismProg, 'u_c1'), c1);
      gl.uniform3fv(gl.getUniformLocation(prismProg, 'u_c2'), c2);
      gl.uniform3fv(gl.getUniformLocation(prismProg, 'u_c3'), c3);
    }
  };

  const readGeometry = (): void => {
    if (!universe) return;
    applyGeometry({
      cellCount: universe.cellCount(),
      states: universe.states(),
      polyXy: new Float32Array(universe.polygonXy()),
      polyOffsets: new Uint32Array(universe.polygonOffsets()),
      triVerts: new Float32Array(universe.triVertices()),
      triCells: new Uint32Array(universe.triCells()),
      frame: [1, 0, 0, 1, 0, 0],
    });
  };

  const makeUniverse = (): Universe =>
    opts.radius === undefined
      ? Universe.createFromResult(opts.record)
      : Universe.createFromResultAt(opts.record, opts.radius);

  const showBanner = (text: string): void => {
    const note = document.createElement('div');
    note.className = 'ep-loading';
    note.textContent = text;
    canvas.parentElement?.append(note);
  };

  /** One replay engine step. Returns false when stepping stopped. */
  const stepOne = (): boolean => {
    if (!universe || failed) return false;
    try {
      universe.step(1);
    } catch (err) {
      failed = true;
      setPlaying(false);
      showBanner(String(err instanceof Error ? err.message : err));
      return false;
    }
    writeLayer(Number(universe.generation()));
    return true;
  };

  /** Flight mode: launch (or relaunch) the background worker — the
   *  expensive window hops happen there, so the tube never stalls. */
  const startFlightWorker = (): void => {
    worker?.terminate();
    queue = [];
    ready = false;
    failed = false;
    curGen = 0;
    curPop = 0;
    const fl = opts.flight!;
    worker = new Worker(new URL('./flight-worker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = (e: MessageEvent<WorkerOut>) => {
      queue.push(e.data);
    };
    const launch: LaunchMsg = {
      type: 'launch',
      record: opts.record,
      windowRadius: fl.windowRadius ?? 24,
      launchRadius: fl.launchRadius ?? 48,
      launchGens: fl.launchGens ?? 60,
      selectHeading: fl.selectHeading,
      lookahead: 96,
    };
    worker.postMessage(launch);
  };

  /** Consume buffered worker messages until one generation lands.
   *  Geometry (window) messages apply inline; returns false when the
   *  buffer holds no generation yet or the flight failed. */
  const consumeOne = (): boolean => {
    if (failed) return false;
    while (queue.length > 0) {
      const msg = queue.shift()!;
      if (msg.type === 'window') {
        applyGeometry({
          cellCount: msg.cellCount,
          states: msg.states,
          polyXy: msg.polyXy,
          polyOffsets: msg.polyOffsets,
          triVerts: msg.triVertices,
          triCells: msg.triCells,
          frame: Array.from(msg.frame),
        });
        ready = true;
        continue;
      }
      if (msg.type === 'fail') {
        failed = true;
        setPlaying(false);
        showBanner(`flight ended: ${msg.message}`);
        return false;
      }
      workerState = msg.state;
      curGen = msg.gen;
      curPop = msg.population;
      writeLayer(msg.gen);
      return true;
    }
    return false;
  };

  /** Append one generation's live cells as prisms into its ring slot. */
  const writeLayer = (gen: number): void => {
    if (!gl || cellCount === 0) return;
    layerScratch.fill(0);
    const st = state();
    if (st.length < cellCount) return;
    const zb = gen * dz;
    const zt = zb + dz;
    let o = 0;
    let written = 0;
    for (let c = 0; c < cellCount && written < MAX_CELLS_PER_LAYER; c++) {
      const s = st[c];
      if (s === 0) continue;
      written++;
      const put = (x: number, y: number, z: number, shade: number): void => {
        layerScratch[o++] = x;
        layerScratch[o++] = y;
        layerScratch[o++] = z;
        layerScratch[o++] = shade;
        layerScratch[o++] = s;
        layerScratch[o++] = gen;
      };
      const tris = cellTris[c];
      for (let i = 0; i < tris.length; i += 2) put(tris[i], tris[i + 1], zt, 1.0);
      const ring = cellOutline[c];
      const n = ring.length / 2;
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        const x0 = ring[2 * i];
        const y0 = ring[2 * i + 1];
        const x1 = ring[2 * j];
        const y1 = ring[2 * j + 1];
        // Fixed fake lighting from the edge direction.
        const len = Math.hypot(x1 - x0, y1 - y0) || 1;
        const shade = 0.5 + 0.28 * Math.abs((y1 - y0) / len);
        put(x0, y0, zb, shade);
        put(x1, y1, zb, shade);
        put(x1, y1, zt, shade);
        put(x0, y0, zb, shade);
        put(x1, y1, zt, shade);
        put(x0, y0, zt, shade);
      }
    }
    const slot = Number(gen) % maxLayers;
    gl.bindBuffer(gl.ARRAY_BUFFER, prismBuf);
    gl.bufferSubData(gl.ARRAY_BUFFER, slot * slotFloats * 4, layerScratch);
  };

  const clearLayers = (): void => {
    if (!gl) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, prismBuf);
    gl.bufferData(gl.ARRAY_BUFFER, maxLayers * slotFloats * 4, gl.DYNAMIC_DRAW);
  };

  const resetRun = (): void => {
    if (!isFlight && !universe) return;
    if (isFlight) {
      // No rewind on a flight — relaunch from the record.
      canvas.parentElement?.querySelectorAll('.ep-loading').forEach((el) => el.remove());
      startFlightWorker();
    } else {
      universe!.restartFromInitial();
    }
    fitter.reset();
    autoOrient = opts.autoOrient ?? true;
    clearLayers();
    if (isFlight) {
      camX = 0;
      camY = 0;
      camZ = dz;
    } else {
      writeLayer(0);
      const c = activeCentroid();
      if (c) {
        camX = c[0];
        camY = c[1];
        camZ = dz;
      }
    }
  };

  playBtn.addEventListener('click', () => setPlaying(!playing));
  q('.ep-step').addEventListener('click', () => {
    setPlaying(false);
    if (isFlight) {
      if (consumeOne()) worker?.postMessage({ type: 'ack', gen: curGen });
    } else {
      stepOne();
    }
  });
  q('.ep-reset').addEventListener('click', resetRun);
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
  const zoomBy = (f: number): void => {
    distance = Math.min(Math.max(distance / f, 20), 600);
  };
  q('.ep-zoomin').addEventListener('click', () => zoomBy(1.3));
  q('.ep-zoomout').addEventListener('click', () => zoomBy(1 / 1.3));
  canvas.addEventListener(
    'wheel',
    (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      zoomBy(Math.exp(-e.deltaY * 0.0015));
    },
    { passive: false },
  );
  let dragging = false;
  let dragX = 0;
  let dragY = 0;
  canvas.addEventListener('pointerdown', (e) => {
    autoOrient = false; // manual orbit takes over until reset
    dragging = true;
    dragX = e.clientX;
    dragY = e.clientY;
    canvas.setPointerCapture(e.pointerId);
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!dragging) return;
    azimuth -= (e.clientX - dragX) * 0.008;
    elevation = Math.min(Math.max(elevation + (e.clientY - dragY) * 0.006, -1.45), 1.45);
    dragX = e.clientX;
    dragY = e.clientY;
  });
  canvas.addEventListener('pointerup', () => {
    dragging = false;
  });

  const activeCentroid = (): [number, number] | null => {
    if (cellCount === 0 || (!isFlight && !universe)) return null;
    const st = state();
    const live: number[] = [];
    for (let c = 0; c < cellCount; c++) {
      if (st[c] !== 0) live.push(c);
    }
    if (live.length === 0) return null;
    if (!opts.followComponent) {
      let sx = 0;
      let sy = 0;
      for (const c of live) {
        sx += centroids[2 * c];
        sy += centroids[2 * c + 1];
      }
      return [sx / live.length, sy / live.length];
    }
    // Single-linkage clustering by centroid distance (supports are tiny),
    // then follow the cluster nearest the current camera target.
    const cluster = new Array(live.length).fill(-1);
    let nClusters = 0;
    for (let i = 0; i < live.length; i++) {
      if (cluster[i] !== -1) continue;
      cluster[i] = nClusters;
      const stack = [i];
      while (stack.length > 0) {
        const a = stack.pop()!;
        const ax = centroids[2 * live[a]];
        const ay = centroids[2 * live[a] + 1];
        for (let b = 0; b < live.length; b++) {
          if (cluster[b] !== -1) continue;
          const dx = centroids[2 * live[b]] - ax;
          const dy = centroids[2 * live[b] + 1] - ay;
          // Link scale merges co-travelling gliders (hat C's parallel
          // pair, ~20-40 units apart) into one followed subject while
          // widely-separated launch partners stay distinct.
          if (dx * dx + dy * dy < 40 * 40) {
            cluster[b] = nClusters;
            stack.push(b);
          }
        }
      }
      nClusters++;
    }
    let best: [number, number] | null = null;
    let bestD = Infinity;
    for (let k = 0; k < nClusters; k++) {
      let sx = 0;
      let sy = 0;
      let n = 0;
      for (let i = 0; i < live.length; i++) {
        if (cluster[i] === k) {
          sx += centroids[2 * live[i]];
          sy += centroids[2 * live[i] + 1];
          n++;
        }
      }
      const cx = sx / n;
      const cy = sy / n;
      const d = (cx - camX) * (cx - camX) + (cy - camY) * (cy - camY);
      if (d < bestD) {
        bestD = d;
        best = [cx, cy];
      }
    }
    return best;
  };

  const frame = (now: number): void => {
    if (disposed) return;
    requestAnimationFrame(frame);
    if (!visible || !gl || (!isFlight && !universe)) return;
    const dt = Math.min((now - lastTime) / 1000, 0.25);
    lastTime = now;
    if (playing && now >= loopPauseUntil) {
      stepAcc += dt * Number(speedInput.value);
      let n = Math.floor(stepAcc);
      stepAcc -= n;
      // Every generation gets its layer; bound the per-frame burst.
      n = Math.min(n, 8);
      let advanced = false;
      for (let i = 0; i < n; i++) {
        if (!(isFlight ? consumeOne() : stepOne())) break;
        advanced = true;
      }
      if (isFlight && advanced) worker?.postMessage({ type: 'ack', gen: curGen });
      if (!isFlight && opts.loopAtGeneration && universe!.generation() >= opts.loopAtGeneration) {
        loopPauseUntil = now + (opts.loopPauseMs ?? 1500);
        resetRun();
      }
    }
    const gen = isFlight ? curGen : Number(universe!.generation());
    genEl.textContent = String(gen);
    popEl.textContent = String(isFlight ? curPop : universe!.population());

    // Camera: xy from the fitted track, z riding the top layer.
    const c = activeCentroid();
    if (c) fitter.sample(gen, c[0], c[1]);
    const target = fitter.target(gen);
    const k = 1 - Math.exp(-3 * dt);
    if (target) {
      camX += (target[0] - camX) * k;
      camY += (target[1] - camY) * k;
    }
    camZ += ((gen + 1) * dz - camZ) * k;
    if (autoOrient) {
      const view = views[viewIndex];
      const v = fitter.velocity();
      if (v && Math.hypot(v[0], v[1]) > 0.02) {
        const want =
          Math.atan2(v[1], v[0]) + Math.PI + (view.offsetDeg * Math.PI) / 180;
        const wrap = ((want - azimuth + Math.PI) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI) - Math.PI;
        const ke = 1 - Math.exp(-1.2 * dt);
        azimuth += wrap * ke;
        elevation += (view.elevation - elevation) * ke;
      }
    }

    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, canvas.clientWidth);
    const h = Math.max(1, canvas.clientHeight);
    if (canvas.width !== Math.round(w * dpr)) canvas.width = Math.round(w * dpr);
    if (canvas.height !== Math.round(h * dpr)) canvas.height = Math.round(h * dpr);
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(bg[0], bg[1], bg[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    const eye = [
      camX + distance * Math.cos(elevation) * Math.cos(azimuth),
      camY + distance * Math.cos(elevation) * Math.sin(azimuth),
      camZ + distance * Math.sin(elevation),
    ];
    // Look slightly below the top layer so the tube fills the frame.
    const mvp = mul(
      perspective(0.9, w / h, 1, 4000),
      lookAt(eye, [camX, camY, camZ - 8], [0, 0, 1]),
    );

    // Substrate outline at the "present" plane. Tied to the smoothed
    // camera height rather than the discrete top-layer z, so the plane
    // glides with the camera instead of jumping each generation; the
    // newest prisms poke up through it as each layer of frozen time
    // lands (the one honestly step-wise event).
    gl.useProgram(lineProg);
    gl.uniformMatrix4fv(gl.getUniformLocation(lineProg!, 'u_mvp'), false, mvp);
    gl.uniform1f(gl.getUniformLocation(lineProg!, 'u_z'), camZ);
    gl.bindVertexArray(lineVao);
    gl.drawArrays(gl.LINES, 0, lineVertCount);

    // Prism layers.
    gl.useProgram(prismProg);
    gl.uniformMatrix4fv(gl.getUniformLocation(prismProg!, 'u_mvp'), false, mvp);
    gl.uniform1f(gl.getUniformLocation(prismProg!, 'u_now'), gen + 1);
    gl.bindVertexArray(prismVao);
    gl.drawArrays(gl.TRIANGLES, 0, maxLayers * slotVerts);
    gl.bindVertexArray(null);
  };

  const start = async (): Promise<void> => {
    if (started) return;
    started = true;
    await ensureWasm();
    await new Promise((r) => setTimeout(r, 30));
    if (!isFlight) {
      try {
        universe = makeUniverse();
      } catch (err) {
        loading.textContent = `failed to load record: ${err instanceof Error ? err.message : String(err)}`;
        return;
      }
    }

    gl = canvas.getContext('webgl2');
    if (!gl) {
      loading.textContent = 'WebGL2 not supported';
      return;
    }
    prismProg = link(gl, VS, FS);
    lineProg = link(gl, LINE_VS, LINE_FS);
    gl.enable(gl.DEPTH_TEST);

    // Prism ring buffer.
    prismBuf = gl.createBuffer();
    prismVao = gl.createVertexArray();
    gl.bindVertexArray(prismVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, prismBuf);
    gl.bufferData(gl.ARRAY_BUFFER, maxLayers * slotFloats * 4, gl.DYNAMIC_DRAW);
    const stride = FLOATS_PER_VERT * 4;
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, stride, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 1, gl.FLOAT, false, stride, 16);
    gl.enableVertexAttribArray(3);
    gl.vertexAttribPointer(3, 1, gl.FLOAT, false, stride, 20);

    // Substrate line list (one segment per polygon edge); filled — and
    // refilled after each flight hop — by readGeometry.
    lineBuf = gl.createBuffer();
    lineVao = gl.createVertexArray();
    gl.bindVertexArray(lineVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, lineBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    if (isFlight) startFlightWorker();
    else readGeometry();

    // Static uniforms (state colours are set per-geometry).
    gl.useProgram(lineProg);
    gl.uniform3fv(gl.getUniformLocation(lineProg, 'u_color'), lineColor);
    gl.useProgram(prismProg);
    gl.uniform1f(gl.getUniformLocation(prismProg, 'u_fade'), fadeGens);
    gl.uniform3fv(gl.getUniformLocation(prismProg, 'u_bg'), bg);

    if (isFlight) {
      camX = 0;
      camY = 0;
      camZ = dz;
    } else {
      resetRun();
    }
    loading.remove();
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
  return () => {
    disposed = true;
    observer.disconnect();
    worker?.terminate();
    worker = null;
    (universe as { free?: () => void } | null)?.free?.();
    universe = null;
  };
}
