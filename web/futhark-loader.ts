// Loads a `futhark webgpu --library` build and exposes the GPUDevice it
// creates, so the harness can render with the same device and read the
// Futhark output buffers directly on the GPU.

/* eslint-disable @typescript-eslint/no-explicit-any */

export interface FutharkArray {
  arr: number;
  free(): void;
}

export interface FutInstance {
  version: number;
  m: any; // emscripten module
  fut: any; // FutharkModule
  device: GPUDevice;
}

// Futhark asks emdawnwebgpu for its own adapter/device, with no way to pass
// one in or read it back. Wrap requestDevice while it initialises to capture it.
async function captureDevice<T>(init: () => Promise<T>): Promise<[T, GPUDevice]> {
  const proto = GPUAdapter.prototype;
  const original = proto.requestDevice;
  let device: GPUDevice | undefined;
  proto.requestDevice = async function (this: GPUAdapter, desc?: GPUDeviceDescriptor) {
    const d = await original.call(this, desc);
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

  const [{ m, fut }, device] = await captureDevice(async () => {
    const m = await exports.Module({ locateFile: (f: string) => `${base}/${f}` });
    const fut = new exports.FutharkModule();
    await fut.init(m);
    return { m, fut };
  });
  return { version, m, fut, device };
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
