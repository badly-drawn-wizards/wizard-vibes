// GPU time per kernel, for benchmarking: with the device's timestamp-query
// feature, every compute pass Futhark encodes while profiling gets
// timestamps written at its start and end, and the passes are totalled by
// the kernel (pipeline) they ran. Futhark encodes through the JS WebGPU API
// (emdawnwebgpu), so wrapping the prototypes reaches its passes.
//
// Only active on a device created with timestamp-query (the harness asks for
// it with ?profile in the URL) and between start() and stop().

const MAX_PASSES = 2048;

export interface GpuProfile {
  start(): void;
  // Ends profiling; resolves to the total GPU milliseconds per kernel.
  stop(): Promise<Map<string, number>>;
}

let active: { device: GPUDevice; querySet: GPUQuerySet; names: string[] } | null = null;
let wrapped = false;

// Names each pipeline after its entry point, so passes can be told apart,
// and times passes while profiling. Call before Futhark creates its
// pipelines (they are created as it initialises).
export function prepareGpuProfile(): void {
  if (wrapped) return;
  wrapped = true;
  const dev = GPUDevice.prototype;
  const createPipeline = dev.createComputePipeline;
  dev.createComputePipeline = function (this: GPUDevice, d: GPUComputePipelineDescriptor) {
    const p = createPipeline.call(this, d);
    if (!p.label) p.label = d.compute.entryPoint ?? "kernel";
    return p;
  };
  const createAsync = dev.createComputePipelineAsync;
  dev.createComputePipelineAsync = async function (this: GPUDevice, d: GPUComputePipelineDescriptor) {
    const p = await createAsync.call(this, d);
    if (!p.label) p.label = d.compute.entryPoint ?? "kernel";
    return p;
  };
  const begin = GPUCommandEncoder.prototype.beginComputePass;
  GPUCommandEncoder.prototype.beginComputePass = function (this: GPUCommandEncoder, d?: GPUComputePassDescriptor) {
    if (!active || active.names.length >= MAX_PASSES || d?.timestampWrites) return begin.call(this, d);
    const k = active.names.length;
    active.names.push("?");
    const pass = begin.call(this, {
      ...d,
      timestampWrites: { querySet: active.querySet, beginningOfPassWriteIndex: 2 * k, endOfPassWriteIndex: 2 * k + 1 },
    });
    // the pass's kernel is the pipeline it sets
    const names = active.names;
    const setPipeline = pass.setPipeline;
    pass.setPipeline = function (this: GPUComputePassEncoder, p: GPUComputePipeline) {
      names[k] = p.label || "kernel";
      return setPipeline.call(this, p);
    };
    return pass;
  };
}

export function gpuProfile(device: GPUDevice): GpuProfile | null {
  if (!device.features.has("timestamp-query")) return null;
  prepareGpuProfile();
  return {
    start() {
      active = { device, querySet: device.createQuerySet({ type: "timestamp", count: 2 * MAX_PASSES }), names: [] };
    },
    async stop() {
      const a = active;
      active = null;
      const totals = new Map<string, number>();
      if (!a || a.names.length === 0) return totals;
      const n = 2 * a.names.length;
      const resolve = device.createBuffer({ size: n * 8, usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC });
      const read = device.createBuffer({ size: n * 8, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
      const enc = device.createCommandEncoder();
      enc.resolveQuerySet(a.querySet, 0, n, resolve, 0);
      enc.copyBufferToBuffer(resolve, 0, read, 0, n * 8);
      device.queue.submit([enc.finish()]);
      await read.mapAsync(GPUMapMode.READ);
      const t = new BigInt64Array(read.getMappedRange().slice(0));
      read.unmap();
      a.names.forEach((name, k) => {
        const ms = Number(t[2 * k + 1] - t[2 * k]) / 1e6;
        if (ms >= 0 && ms < 1e4) totals.set(name, (totals.get(name) ?? 0) + ms);
      });
      for (const b of [resolve, read, a.querySet]) b.destroy();
      return totals;
    },
  };
}
