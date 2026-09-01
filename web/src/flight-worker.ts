// Flight worker: owns the wasm FlightUniverse and races ahead of the
// display. The expensive window hop (patch generation) happens here,
// off the main thread; the panel consumes a buffered stream of
// per-generation states, so the on-screen glider never stalls while
// the next window is computed. Backpressure: the worker pumps only
// while it is fewer than `lookahead` generations ahead of the last
// generation the display acknowledged.
import { FlightUniverse } from './wasm/ui_wasm.js';
import { ensureWasm, wasmMemory } from './wasm-shared';

export interface LaunchMsg {
  type: 'launch';
  record: string;
  windowRadius: number;
  launchRadius: number;
  launchGens: number;
  selectHeading?: number;
  lookahead: number;
}
export interface AckMsg {
  type: 'ack';
  gen: number;
}
export type WorkerIn = LaunchMsg | AckMsg;

/** Geometry + palette for one window; applies from generation `gen`. */
export interface WindowMsg {
  type: 'window';
  gen: number;
  cellCount: number;
  states: number;
  windowRadius: number;
  triVertices: Float32Array;
  triCells: Uint32Array;
  polyXy: Float32Array;
  polyOffsets: Uint32Array;
  cellClasses: Uint16Array;
  classInfoJson: string;
  frame: Float64Array;
}
export interface GenMsg {
  type: 'gen';
  gen: number;
  state: Uint8Array;
  global: [number, number];
  pathRings: number;
  hops: number;
  heading: number;
  population: number;
}
export interface FailMsg {
  type: 'fail';
  message: string;
}
export type WorkerOut = WindowMsg | GenMsg | FailMsg;

let flight: FlightUniverse | null = null;
let lookahead = 64;
let ackGen = 0;
let centroids = new Float32Array(0); // window-local, for the glider position
let frame: number[] = [1, 0, 0, 1, 0, 0];
let pumping = false;

function post(msg: WorkerOut, transfer: Transferable[] = []): void {
  (self as unknown as Worker).postMessage(msg, transfer);
}

/** Send the current window's geometry bundle; applies from `gen`. */
function postWindow(): void {
  if (!flight) return;
  const cellCount = flight.cellCount();
  const polyXy = new Float32Array(flight.polygonXy());
  const polyOffsets = new Uint32Array(flight.polygonOffsets());
  frame = Array.from(flight.frame());

  // Window-local per-cell centroids (mean of polygon vertices).
  centroids = new Float32Array(cellCount * 2);
  for (let c = 0; c < cellCount; c++) {
    const s = polyOffsets[c];
    const e = polyOffsets[c + 1];
    let sx = 0;
    let sy = 0;
    for (let i = s; i < e; i++) {
      sx += polyXy[2 * i];
      sy += polyXy[2 * i + 1];
    }
    centroids[2 * c] = sx / (e - s);
    centroids[2 * c + 1] = sy / (e - s);
  }

  const msg: WindowMsg = {
    type: 'window',
    gen: flight.generation(),
    cellCount,
    states: flight.states(),
    windowRadius: flight.windowRadius(),
    triVertices: new Float32Array(flight.triVertices()),
    triCells: new Uint32Array(flight.triCells()),
    polyXy,
    polyOffsets,
    cellClasses: new Uint16Array(flight.cellClasses()),
    classInfoJson: flight.classInfoJson(),
    frame: new Float64Array(frame),
  };
  post(msg, [
    msg.triVertices.buffer,
    msg.triCells.buffer,
    msg.polyXy.buffer,
    msg.polyOffsets.buffer,
    msg.cellClasses.buffer,
    msg.frame.buffer,
  ]);
}

/** Send the current generation's state + glider position + odometer. */
function postGen(): void {
  if (!flight) return;
  const cellCount = flight.cellCount();
  const view = new Uint8Array(wasmMemory().buffer, flight.statePtr(), cellCount);
  const state = new Uint8Array(view); // copy out of wasm memory
  let sx = 0;
  let sy = 0;
  let n = 0;
  for (let c = 0; c < cellCount; c++) {
    if (state[c] !== 0) {
      sx += centroids[2 * c];
      sy += centroids[2 * c + 1];
      n++;
    }
  }
  const lx = n > 0 ? sx / n : 0;
  const ly = n > 0 ? sy / n : 0;
  const msg: GenMsg = {
    type: 'gen',
    gen: flight.generation(),
    state,
    global: [
      frame[0] * lx + frame[1] * ly + frame[4],
      frame[2] * lx + frame[3] * ly + frame[5],
    ],
    pathRings: flight.pathRings(),
    hops: flight.hops(),
    heading: flight.windowedHeadingDeg(),
    population: flight.population(),
  };
  post(msg, [msg.state.buffer]);
}

/** Step while under the lookahead budget, yielding regularly so ack
 *  messages keep flowing. */
function pump(): void {
  if (pumping) return;
  pumping = true;
  const run = (): void => {
    if (!flight) {
      pumping = false;
      return;
    }
    let steps = 0;
    try {
      while (flight.generation() - ackGen < lookahead && steps < 32) {
        if (flight.step(1) > 0) postWindow();
        postGen();
        steps++;
      }
    } catch (err) {
      post({ type: 'fail', message: err instanceof Error ? err.message : String(err) });
      flight = null;
      pumping = false;
      return;
    }
    if (steps === 32) {
      setTimeout(run, 0); // budget filled a slice; keep going next tick
    } else {
      pumping = false; // caught up to the lookahead; awoken by the next ack
    }
  };
  run();
}

self.onmessage = (e: MessageEvent<WorkerIn>): void => {
  const msg = e.data;
  if (msg.type === 'launch') {
    void (async (): Promise<void> => {
      await ensureWasm();
      try {
        flight = FlightUniverse.launch(
          msg.record,
          msg.windowRadius,
          msg.launchRadius,
          msg.launchGens,
          msg.selectHeading,
        );
      } catch (err) {
        post({ type: 'fail', message: err instanceof Error ? err.message : String(err) });
        return;
      }
      lookahead = msg.lookahead;
      ackGen = 0;
      postWindow();
      postGen(); // generation 0 (the seed)
      pump();
    })();
  } else if (msg.type === 'ack') {
    ackGen = msg.gen;
    pump();
  }
};
