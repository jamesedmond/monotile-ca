// One wasm module instance shared by every essay panel (2D and 3D).
import init from './wasm/ui_wasm.js';

let memory: WebAssembly.Memory | null = null;
let ready: Promise<void> | null = null;

export function ensureWasm(): Promise<void> {
  ready ??= init(new URL('./wasm/ui_wasm_bg.wasm', import.meta.url)).then((wasm) => {
    memory = wasm.memory;
  });
  return ready;
}

/** Valid after ensureWasm() resolves. */
export function wasmMemory(): WebAssembly.Memory {
  if (!memory) throw new Error('wasm not initialised');
  return memory;
}
