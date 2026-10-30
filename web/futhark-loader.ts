// Loads a `futhark webgpu --library` build and exposes the GPUDevice it
// creates, so the harness can render with the same device and read the
// Futhark output buffers directly on the GPU.

/* eslint-disable @typescript-eslint/no-explicit-any */

import { instrumentShaders, type ShaderLog } from "./shader-check.ts";

export interface FutharkArray {
  arr: number;
  values(): Promise<ArrayLike<number>>; // a copy, read back from the GPU
  free(): void;
}

export interface FutInstance {
  version: string; // build id (content hash, see dev/build-fut.ts)
  m: any; // emscripten module
  fut: any; // FutharkModule
  device: GPUDevice;
  f32: boolean; // running with f16 stripped from the shaders
  portable: boolean; // WGSL rewritten for implementations like naga
  shaders: ShaderLog; // WGSL compiler diagnostics
}

// Futhark asks emdawnwebgpu for its own adapter/device, with no way to pass
// one in or read it back. Wrap requestDevice while it initialises to capture
// it, and with `f32` set, drop the shader-f16 requirement (see wgsl-f32.ts).
// `setup` runs on the device before Futhark creates any shaders.
async function captureDevice<T>(
  f32: boolean,
  setup: (d: GPUDevice) => void,
  init: () => Promise<T>,
): Promise<[T, GPUDevice]> {
  const proto = GPUAdapter.prototype;
  const original = proto.requestDevice;
  let device: GPUDevice | undefined;
  proto.requestDevice = async function (this: GPUAdapter, desc?: GPUDeviceDescriptor) {
    if (f32 && desc?.requiredFeatures) {
      desc = { ...desc, requiredFeatures: [...desc.requiredFeatures].filter((f) => f !== "shader-f16") };
    }
    const d = await original.call(this, desc);
    if (!device) {
      device = d;
      setup(d);
    }
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

export async function loadFuthark(version: string): Promise<FutInstance> {
  const base = `/fut/${version}`;
  const src = await (await fetch(`${base}/wizard.js`)).text();
  // The generated script declares `var Module` and `class FutharkModule` at
  // top level; evaluating it in a function scope lets us load it repeatedly.
  const exports = new Function(`${src}\nreturn { Module, FutharkModule };\n//# sourceURL=${base}/wizard.js`)();

  // Futhark's backend requires shader-f16 even for f32-only programs. Without
  // it (or with ?f32 in the URL), run with f16 stripped from the shaders.
  const adapter = await navigator.gpu?.requestAdapter();
  if (!adapter) throw new Error("No WebGPU adapter available.");
  const params = new URLSearchParams(location.search);
  const f32 = !adapter.features.has("shader-f16") || params.has("f32");
  // Futhark's helpers take storage pointers as parameters, which needs this
  // WGSL language feature (missing in Firefox); otherwise rewrite them away.
  const portable = !navigator.gpu.wgslLanguageFeatures?.has("unrestricted_pointer_parameters") || params.has("portable");

  // If the runtime exits (e.g. device creation fails) its promises never
  // settle, so turn the exit message into a rejection.
  let stderr = "";
  let onExit: (e: Error) => void = () => {};
  const exited = new Promise<never>((_, reject) => (onExit = reject));
  let shaders: ShaderLog | undefined;
  let captured: GPUDevice | undefined;
  let compileFailed = () => {};
  const failed = new Promise<Error>((resolve) => (compileFailed = () => resolve(new Error("WGSL compilation failed"))));
  const setup = (d: GPUDevice) => {
    captured = d;
    shaders = instrumentShaders(d, { f32, portable });
    shaders.onError = () => compileFailed();
  };
  const loading = captureDevice(f32, setup, async () => {
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

  // WGSL compile errors make loading fail. Futhark may panic, or hang, on the
  // invalid pipeline, so stop at the first error and report the diagnostics
  // in place of whatever init produced.
  const result = await Promise.race([loading.catch((e: unknown) => e as Error), failed]);
  await shaders?.settled();
  if (shaders?.errors.length) {
    if (!(result instanceof Error)) result[0].fut.free();
    captured?.destroy();
    const shown = shaders.errors.slice(0, 4);
    const more = shaders.errors.length - shown.length;
    throw new Error(`WGSL compilation failed:\n\n${shown.join("\n\n")}${more > 0 ? `\n\n…and ${more} more (see console)` : ""}`);
  }
  if (result instanceof Error) throw result;
  const [{ m, fut }, device] = result;
  return { version, m, fut, device, f32, portable, shaders: shaders! };
}

export function unloadFuthark(inst: FutInstance): void {
  inst.fut.free(); // needs a live device
  inst.device.destroy();
}

// The WGPUBuffer backing a result array (of Futhark type `type`, e.g. a
// []f32 is "f32_1d"), as a JS GPUBuffer. No copy.
export function rawBuffer(inst: FutInstance, arr: FutharkArray, type = "u32_1d"): GPUBuffer {
  const handle = inst.m[`_futhark_values_raw_${type}`](inst.fut.ctx, arr.arr);
  const buf = inst.m.wgpuGetJsObject(handle);
  if (!(buf instanceof GPUBuffer)) throw new Error("could not resolve Futhark output buffer");
  return buf;
}

// Builds a value of a Futhark record type (an opaque in its API) from a JS
// value, using the manifest: each field in the manifest's order, nested
// records recursively (given as objects keyed by field name, or as arrays in
// field order, e.g. a vec3 as [x, y, z]). The wrapper has no constructor for
// records, so this calls the exported C one directly. Returns the opaque,
// which the caller frees; nested records are copied into their parent, so
// they are freed as soon as it exists.
export function buildRecord(inst: FutInstance, type: string, value: unknown): { ptr: number; free(): void } {
  const { m, fut } = inst;
  const info = fut.manifest.types[type];
  if (!info?.record) throw new Error(`${type} is not a record type`);
  const children: { free(): void }[] = [];
  const args = info.record.fields.map((f: { name: string; type: string }, i: number) => {
    const v = Array.isArray(value) ? value[i] : (value as Record<string, unknown>)[f.name];
    if (v === undefined) throw new Error(`${type}: no value for field ${f.name}`);
    if (fut.manifest.types[f.type]?.record) {
      const child = buildRecord(inst, f.type, v);
      children.push(child);
      return child.ptr;
    }
    return typeof v === "boolean" ? Number(v) : v;
  });
  const out = m._malloc(4);
  try {
    const err = m["_" + info.record.new](fut.ctx, out, ...args);
    if (err !== 0) throw new Error(`${info.record.new} failed (${err})`);
    return fut.types[type].from_native(m.HEAPU32[out >> 2]);
  } finally {
    m._free(out);
    for (const c of children) c.free();
  }
}
