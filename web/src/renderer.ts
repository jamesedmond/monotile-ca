// WebGL2 renderer. Geometry lives in per-window GPU bundles: one live
// window (per-frame cell state in an R8UI texture indexed by cell id)
// plus an optional ring of retired windows — a flight's already-visited
// terrain, drawn faded underneath the live window. Static-patch users
// (playground, essay panels) have one live window and never retire.
//
// Vertex path: model frame (rotation u_frame + offset u_off) then view
// scale. u_off is (frame translation − camera center), composed in f64
// on the CPU, so vertex coordinates stay window-local and bounded while
// the camera roams an unbounded global frame (infinite flights). The
// static-patch path uses the identity frame with u_off = −center, which
// reproduces the classic (a_pos − u_center) · u_scale exactly.

const TEX_W = 1024; // state/color texture width; cell c -> texel (c & 1023, c >> 10)

export type Theme = 'dark' | 'light';

// Dark: the app's atmospheric palette (amber alive, ember dying, tinted
// ground). Light: the paper's print palette — Figure 1's state colors
// verbatim (state 1 vermilion #d55e00, state 2 green #5fb878, state 3+
// pale blue #b7cde8) on a plain white ground with gray outlines.
const THEMES = {
  dark: { bg: [0.043, 0.051, 0.063], line: [0.16, 0.17, 0.2] },
  light: { bg: [1.0, 1.0, 1.0], line: [0.72, 0.73, 0.76] },
} as const;

const FILL_VS = `#version 300 es
layout(location = 0) in vec2 a_pos;
layout(location = 1) in float a_cell;
uniform vec4 u_frame;
uniform vec2 u_off;
uniform vec2 u_scale;
flat out int v_cell;
void main() {
  v_cell = int(a_cell + 0.5);
  vec2 p = vec2(dot(u_frame.xy, a_pos), dot(u_frame.zw, a_pos)) + u_off;
  gl_Position = vec4(p * u_scale, 0.0, 1.0);
}`;

const fillFs = (theme: Theme): string => {
  const [br, bg2, bb] = THEMES[theme].bg;
  const stateColor =
    theme === 'dark'
      ? `vec3 alive = vec3(0.98, 0.72, 0.25);
  vec3 c;
  if (s == 0u) {
    c = dead;
  } else if (s == 1u || u_states <= 2.0) {
    c = alive;            // alive: warm amber
  } else {
    // Dying phases 2..k-1 fade amber -> a cool ember toward the dead tint.
    float age = (float(s) - 1.0) / max(u_states - 1.0, 1.0); // 0..1
    vec3 ember = mix(alive, vec3(0.55, 0.16, 0.42), 0.85);
    c = mix(ember, dead, age);
  }`
      : `vec3 c;
  if (s == 0u) {
    c = dead;
  } else if (s == 1u) {
    c = vec3(0.835, 0.369, 0.000);   // vermilion #d55e00
  } else if (s == 2u) {
    c = vec3(0.373, 0.722, 0.471);   // green #5fb878
  } else {
    c = vec3(0.718, 0.804, 0.910);   // pale blue #b7cde8
  }`;
  return `#version 300 es
precision highp float;
precision highp int;
precision highp usampler2D;
precision highp sampler2D;
flat in int v_cell;
uniform usampler2D u_state;
uniform sampler2D u_colors;
uniform float u_states;   // k; >2 enables dying-phase fade
uniform float u_fade;     // 0 live; retired windows fade toward the background
out vec4 o_color;
void main() {
  ivec2 tc = ivec2(v_cell & ${TEX_W - 1}, v_cell >> ${Math.log2(TEX_W)});
  uint s = texelFetch(u_state, tc, 0).r;
  vec3 dead = texelFetch(u_colors, tc, 0).rgb;
  ${stateColor}
  o_color = vec4(mix(c, vec3(${br}, ${bg2}, ${bb}), u_fade), 1.0);
}`;
};

const LINE_VS = `#version 300 es
layout(location = 0) in vec2 a_pos;
uniform vec4 u_frame;
uniform vec2 u_off;
uniform vec2 u_scale;
void main() {
  vec2 p = vec2(dot(u_frame.xy, a_pos), dot(u_frame.zw, a_pos)) + u_off;
  gl_Position = vec4(p * u_scale, 0.0, 1.0);
}`;

const lineFs = (theme: Theme): string => {
  const [br, bg2, bb] = THEMES[theme].bg;
  return `#version 300 es
precision highp float;
uniform float u_fade;
uniform vec3 u_line;
out vec4 o_color;
void main() {
  o_color = vec4(mix(u_line, vec3(${br}, ${bg2}, ${bb}), u_fade), 1.0);
}`;
};

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type);
  if (!sh) throw new Error('createShader failed');
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    throw new Error(`shader compile failed: ${gl.getShaderInfoLog(sh) ?? ''}`);
  }
  return sh;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const prog = gl.createProgram();
  if (!prog) throw new Error('createProgram failed');
  gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, vs));
  gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    throw new Error(`program link failed: ${gl.getProgramInfoLog(prog) ?? ''}`);
  }
  return prog;
}

/** All GPU state for one window's geometry. */
interface WindowGpu {
  posBuf: WebGLBuffer;
  cellBuf: WebGLBuffer;
  lineBuf: WebGLBuffer;
  fillVao: WebGLVertexArrayObject;
  lineVao: WebGLVertexArrayObject;
  stateTex: WebGLTexture;
  colorTex: WebGLTexture;
  texRows: number;
  staging: Uint8Array;
  triVertCount: number;
  lineVertCount: number;
  frameM: [number, number, number, number];
  frameT: [number, number];
}

export class Renderer {
  private gl: WebGL2RenderingContext;
  private fillProg: WebGLProgram;
  private lineProg: WebGLProgram;
  private fillFrame: WebGLUniformLocation | null;
  private fillOff: WebGLUniformLocation | null;
  private fillScale: WebGLUniformLocation | null;
  private fillStates: WebGLUniformLocation | null;
  private fillFade: WebGLUniformLocation | null;
  private lineFrame: WebGLUniformLocation | null;
  private lineOff: WebGLUniformLocation | null;
  private lineScale: WebGLUniformLocation | null;
  private lineFade: WebGLUniformLocation | null;
  private lineU: WebGLUniformLocation | null;
  private lineColor: [number, number, number];
  private current: WindowGpu | null = null;
  private retired: WindowGpu[] = [];
  // How far retired windows mix toward the background (0 = original
  // colors, a seamless continuous world; ~0.55 = clearly faded trail).
  private retiredFillFade = 0;
  private retiredLineFade = 0;
  // View: world-space center + pixels-per-world-unit (in CSS pixels, y up).
  private cssW = 1;
  private cssH = 1;
  private cx = 0;
  private cy = 0;
  private ppu = 100;

  constructor(
    private canvas: HTMLCanvasElement,
    private theme: Theme = 'dark',
  ) {
    const gl = canvas.getContext('webgl2');
    if (!gl) throw new Error('WebGL2 not supported');
    this.gl = gl;
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);

    this.fillProg = link(gl, FILL_VS, fillFs(theme));
    this.lineProg = link(gl, LINE_VS, lineFs(theme));
    this.fillFrame = gl.getUniformLocation(this.fillProg, 'u_frame');
    this.fillOff = gl.getUniformLocation(this.fillProg, 'u_off');
    this.fillScale = gl.getUniformLocation(this.fillProg, 'u_scale');
    this.fillStates = gl.getUniformLocation(this.fillProg, 'u_states');
    this.fillFade = gl.getUniformLocation(this.fillProg, 'u_fade');
    this.lineFrame = gl.getUniformLocation(this.lineProg, 'u_frame');
    this.lineOff = gl.getUniformLocation(this.lineProg, 'u_off');
    this.lineScale = gl.getUniformLocation(this.lineProg, 'u_scale');
    this.lineFade = gl.getUniformLocation(this.lineProg, 'u_fade');
    this.lineU = gl.getUniformLocation(this.lineProg, 'u_line');
    this.lineColor = [...THEMES[theme].line] as [number, number, number];
    gl.useProgram(this.fillProg);
    gl.uniform1i(gl.getUniformLocation(this.fillProg, 'u_state'), 0);
    gl.uniform1i(gl.getUniformLocation(this.fillProg, 'u_colors'), 1);
    gl.uniform1f(this.fillStates, 2);

    this.resize();
  }

  private makeTexture(): WebGLTexture {
    const gl = this.gl;
    const tex = gl.createTexture();
    if (!tex) throw new Error('createTexture failed');
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return tex;
  }

  private dispose(w: WindowGpu): void {
    const gl = this.gl;
    gl.deleteBuffer(w.posBuf);
    gl.deleteBuffer(w.cellBuf);
    gl.deleteBuffer(w.lineBuf);
    gl.deleteVertexArray(w.fillVao);
    gl.deleteVertexArray(w.lineVao);
    gl.deleteTexture(w.stateTex);
    gl.deleteTexture(w.colorTex);
  }

  /**
   * Retire the live window into the faded-terrain ring (its state is
   * cleared, so only substrate tints remain), keeping at most `cap`
   * retired windows. Call before setPatch on a flight hop.
   */
  retireCurrent(cap: number): void {
    if (!this.current) return;
    const gl = this.gl;
    const w = this.current;
    w.staging.fill(0);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, w.stateTex);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, w.texRows, gl.RED_INTEGER, gl.UNSIGNED_BYTE, w.staging);
    this.retired.push(w);
    this.current = null;
    while (this.retired.length > cap) {
      this.dispose(this.retired.shift()!);
    }
  }

  /** Fade applied to retired windows (0 = original colors). The line
   *  fade rides a bit above the fill fade so outlines recede first. */
  setRetiredFade(fade: number): void {
    this.retiredFillFade = Math.min(Math.max(fade, 0), 1);
    this.retiredLineFade = Math.min(this.retiredFillFade + (this.retiredFillFade > 0 ? 0.2 : 0), 1);
  }

  /** Drop all retired windows (relaunch). */
  clearRetired(): void {
    for (const w of this.retired) this.dispose(w);
    this.retired.length = 0;
  }

  /** Upload patch geometry + per-cell dead-tint color table (RGBA, 4 bytes/cell)
   *  as the live window (replacing — not retiring — any existing one). */
  setPatch(
    triVerts: Float32Array,
    triCells: Uint32Array,
    polyXy: Float32Array,
    polyOffsets: Uint32Array,
    cellColors: Uint8Array,
    cellCount: number,
  ): void {
    const gl = this.gl;
    if (this.current) this.dispose(this.current);

    const buf = (): WebGLBuffer => {
      const b = gl.createBuffer();
      if (!b) throw new Error('createBuffer failed');
      return b;
    };
    const vao = (): WebGLVertexArrayObject => {
      const v = gl.createVertexArray();
      if (!v) throw new Error('createVertexArray failed');
      return v;
    };
    const posBuf = buf();
    const cellBuf = buf();
    const lineBuf = buf();

    gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
    gl.bufferData(gl.ARRAY_BUFFER, triVerts, gl.STATIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, cellBuf);
    // Cell ids as float attribute (exact for ids < 2^24); shader rounds back to int.
    gl.bufferData(gl.ARRAY_BUFFER, Float32Array.from(triCells), gl.STATIC_DRAW);

    // One line segment (2 verts) per polygon edge; edge count == vertex count per ring.
    const cells = polyOffsets.length - 1;
    const totalVerts = polyOffsets[cells];
    const lines = new Float32Array(totalVerts * 4);
    let j = 0;
    for (let c = 0; c < cells; c++) {
      const s = polyOffsets[c];
      const e = polyOffsets[c + 1];
      for (let i = s; i < e; i++) {
        const k = i + 1 < e ? i + 1 : s;
        lines[j++] = polyXy[2 * i];
        lines[j++] = polyXy[2 * i + 1];
        lines[j++] = polyXy[2 * k];
        lines[j++] = polyXy[2 * k + 1];
      }
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, lineBuf);
    gl.bufferData(gl.ARRAY_BUFFER, lines, gl.STATIC_DRAW);

    const fillVao = vao();
    gl.bindVertexArray(fillVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, cellBuf);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 0, 0);
    const lineVao = vao();
    gl.bindVertexArray(lineVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, lineBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);

    const texRows = Math.max(1, Math.ceil(cellCount / TEX_W));
    const staging = new Uint8Array(TEX_W * texRows);
    const stateTex = this.makeTexture();
    gl.bindTexture(gl.TEXTURE_2D, stateTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8UI, TEX_W, texRows, 0, gl.RED_INTEGER, gl.UNSIGNED_BYTE, staging);
    const padded = new Uint8Array(TEX_W * texRows * 4);
    padded.set(cellColors);
    const colorTex = this.makeTexture();
    gl.bindTexture(gl.TEXTURE_2D, colorTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, TEX_W, texRows, 0, gl.RGBA, gl.UNSIGNED_BYTE, padded);

    this.current = {
      posBuf,
      cellBuf,
      lineBuf,
      fillVao,
      lineVao,
      stateTex,
      colorTex,
      texRows,
      staging,
      triVertCount: triVerts.length / 2,
      lineVertCount: totalVerts * 2,
      frameM: [1, 0, 0, 1],
      frameT: [0, 0],
    };
  }

  /** Replace the live window's per-cell color table (lens switches). */
  updateColors(cellColors: Uint8Array): void {
    const w = this.current;
    if (!w) return;
    const gl = this.gl;
    const padded = new Uint8Array(TEX_W * w.texRows * 4);
    padded.set(cellColors);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, w.colorTex);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, w.texRows, gl.RGBA, gl.UNSIGNED_BYTE, padded);
  }

  /** Set the CA state count k so the shader fades dying phases (k>2). */
  setStates(k: number): void {
    const gl = this.gl;
    gl.useProgram(this.fillProg);
    gl.uniform1f(this.fillStates, k);
  }

  /** Model frame of the live window: window-local -> world,
   *  `[m00, m01, m10, m11, tx, ty]` (a `FlightUniverse.frame()` array). */
  setFrame(f: Float64Array | number[]): void {
    if (!this.current) return;
    this.current.frameM = [f[0], f[1], f[2], f[3]];
    this.current.frameT = [f[4], f[5]];
  }

  /** Upload the live window's per-cell state byte (a fresh view into wasm
   *  memory, or a buffered copy, each frame). */
  updateState(state: Uint8Array): void {
    const w = this.current;
    if (!w) return;
    const gl = this.gl;
    w.staging.set(state); // copy into padded buffer so we always upload full rows
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, w.stateTex);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, TEX_W, w.texRows, gl.RED_INTEGER, gl.UNSIGNED_BYTE, w.staging);
  }

  draw(): void {
    const gl = this.gl;
    const [br, bg2, bb] = THEMES[this.theme].bg;
    gl.clearColor(br, bg2, bb, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const sx = (2 * this.ppu) / this.cssW;
    const sy = (2 * this.ppu) / this.cssH;
    const windows: [WindowGpu, number, number][] = [];
    for (const w of this.retired) windows.push([w, this.retiredFillFade, this.retiredLineFade]);
    if (this.current) windows.push([this.current, 0, 0]);
    if (windows.length === 0) return;

    gl.useProgram(this.fillProg);
    gl.uniform2f(this.fillScale, sx, sy);
    for (const [w, fade] of windows) {
      // Offset = frame translation − camera center, composed in f64 here
      // so the f32 uniform only carries a camera-relative (small) value.
      gl.uniform4f(this.fillFrame, w.frameM[0], w.frameM[1], w.frameM[2], w.frameM[3]);
      gl.uniform2f(this.fillOff, w.frameT[0] - this.cx, w.frameT[1] - this.cy);
      gl.uniform1f(this.fillFade, fade);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, w.stateTex);
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, w.colorTex);
      gl.bindVertexArray(w.fillVao);
      gl.drawArrays(gl.TRIANGLES, 0, w.triVertCount);
    }
    gl.useProgram(this.lineProg);
    gl.uniform3f(this.lineU, this.lineColor[0], this.lineColor[1], this.lineColor[2]);
    gl.uniform2f(this.lineScale, sx, sy);
    for (const [w, , lineFade] of windows) {
      gl.uniform4f(this.lineFrame, w.frameM[0], w.frameM[1], w.frameM[2], w.frameM[3]);
      gl.uniform2f(this.lineOff, w.frameT[0] - this.cx, w.frameT[1] - this.cy);
      gl.uniform1f(this.lineFade, lineFade);
      gl.bindVertexArray(w.lineVao);
      gl.drawArrays(gl.LINES, 0, w.lineVertCount);
    }
    gl.bindVertexArray(null);
  }

  /** Override the tile-outline colour (0..1 RGB); null restores the theme
   *  default. Takes effect on the next draw. */
  setLineColor(rgb: [number, number, number] | null): void {
    this.lineColor = rgb ?? ([...THEMES[this.theme].line] as [number, number, number]);
  }

  resize(): void {
    const dpr = window.devicePixelRatio || 1;
    this.cssW = Math.max(1, this.canvas.clientWidth);
    this.cssH = Math.max(1, this.canvas.clientHeight);
    this.canvas.width = Math.round(this.cssW * dpr);
    this.canvas.height = Math.round(this.cssH * dpr);
    this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
  }

  fitBounds(minX: number, minY: number, maxX: number, maxY: number): void {
    this.cx = (minX + maxX) / 2;
    this.cy = (minY + maxY) / 2;
    const spanX = Math.max(maxX - minX, 1e-6);
    const spanY = Math.max(maxY - minY, 1e-6);
    this.ppu = 0.92 * Math.min(this.cssW / spanX, this.cssH / spanY);
  }

  /** Recentre on a world point, keeping the current zoom (follow-cam). */
  centerOn(wx: number, wy: number): void {
    this.cx = wx;
    this.cy = wy;
  }

  /** Current pixels-per-world-unit; settable for camera presets. */
  get zoom(): number {
    return this.ppu;
  }

  set zoom(ppu: number) {
    this.ppu = Math.min(Math.max(ppu, 1e-3), 1e6);
  }

  /** Pan by a screen-space delta in CSS pixels (screen y down, world y up). */
  panBy(dxPx: number, dyPx: number): void {
    this.cx -= dxPx / this.ppu;
    this.cy += dyPx / this.ppu;
  }

  /** Zoom by `factor`, keeping the world point under (px, py) fixed. */
  zoomAt(px: number, py: number, factor: number): void {
    const [wx, wy] = this.screenToWorld(px, py);
    this.ppu = Math.min(Math.max(this.ppu * factor, 1e-3), 1e6);
    this.cx = wx - (px - this.cssW / 2) / this.ppu;
    this.cy = wy + (py - this.cssH / 2) / this.ppu;
  }

  /** Canvas-relative CSS pixel coords -> world coords. */
  screenToWorld(px: number, py: number): [number, number] {
    return [this.cx + (px - this.cssW / 2) / this.ppu, this.cy - (py - this.cssH / 2) / this.ppu];
  }
}
