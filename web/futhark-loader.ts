// Loads a `futhark webgpu --library` build and exposes the GPUDevice it
// creates, so the harness can render with the same device and read the
// Futhark output buffers directly on the GPU.

/* eslint-disable @typescript-eslint/no-explicit-any */

import { forceF32Shaders } from "./wgsl-f32.ts";

export interface FutharkArray {
  arr: number;
  free(): void;
}

export interface FutInstance {
  version: number;
  m: any; // emscripten module
  fut: any; // FutharkModule
  device: GPUDevice;
  f32: boolean; // running with f16 stripped from the shaders
}

// Futhark asks emdawnwebgpu for its own adapter/device, with no way to pass
// one in or read it back. Wrap requestDevice while it initialises to capture
// it, and with `f32` set, drop the shader-f16 requirement (see wgsl-f32.ts).
async function captureDevice<T>(f32: boolean, init: () => Promise<T>): Promise<[T, GPUDevice]> {
  const proto = GPUAdapter.prototype;
  const original = proto.requestDevice;
  let device: GPUDevice | undefined;
  proto.requestDevice = async function (this: GPUAdapter, desc?: GPUDeviceDescriptor) {
    if (f32 && desc?.requiredFeatures) {
      desc = { ...desc, requiredFeatures: [...desc.requiredFeatures].filter((f) => f !== "shader-f16") };
    }
    const d = await original.call(this, desc);
    if (f32) forceF32Shaders(d);
    device ??= d;
    return d;
  };
  try {
    const result = await init();
    if (!device) throw new Error("Futhark did not request a WebGPU device");
    return [result, device];
  } finally {
    proto.requestDevice = original;
  }
}

export async function loadFuthark(version: number): Promise<FutInstance> {
  const base = `/fut/v${version}`;
  const src = await (await fetch(`${base}/wizard.js`)).text();
  // The generated script declares `var Module` and `class FutharkModule` at
  // top level; evaluating it in a function scope lets us load it repeatedly.
  const exports = new Function(`${src}\nreturn { Module, FutharkModule };\n//# sourceURL=${base}/wizard.js`)();

  // Futhark's backend requires shader-f16 even for f32-only programs. Without
  // it (or with ?f32 in the URL), run with f16 stripped from the shaders.
  const adapter = await navigator.gpu?.requestAdapter();
  if (!adapter) throw new Error("No WebGPU adapter available.");
  const f32 = !adapter.features.has("shader-f16") || new URLSearchParams(location.search).has("f32");

  // If the runtime exits (e.g. device creation fails) its promises never
  // settle, so turn the exit message into a rejection.
  let stderr = "";
  let onExit: (e: Error) => void = () => {};
  const exited = new Promise<never>((_, reject) => (onExit = reject));
  const [{ m, fut }, device] = await captureDevice(f32, async () => {
    const m = await exports.Module({
      locateFile: (f: string) => `${base}/${f}`,
      printErr: (line: string) => {
        console.error(line);
        stderr += line + "\n";
        if (line.startsWith("program exited")) onExit(new Error(stderr));
      },
    });
    const fut = new exports.FutharkModule();
    await Promise.race([fut.init(m), exited]);
    return { m, fut };
  });
  return { version, m, fut, device, f32 };
}

export function unloadFuthark(inst: FutInstance): void {
  inst.fut.free(); // needs a live device
  inst.device.destroy();
}

// The WGPUBuffer backing a []u32 result, as a JS GPUBuffer. No copy.
export function rawBuffer(inst: FutInstance, arr: FutharkArray): GPUBuffer {
  const handle = inst.m._futhark_values_raw_u32_1d(inst.fut.ctx, arr.arr);
  const buf = inst.m.wgpuGetJsObject(handle);
  if (!(buf instanceof GPUBuffer)) throw new Error("could not resolve Futhark output buffer");
  return buf;
}
