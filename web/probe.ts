// Reading the collision probe back without stalling: its few floats are
// copied into a mappable buffer along with the frame, and handed over
// whenever the GPU gets to them. While one read is pending, further probes
// are skipped.

import { type FutharkArray, type FutInstance, rawBuffer } from "./futhark-loader.ts";

interface Reader {
  buf: GPUBuffer;
  busy: boolean;
}

export class ProbeReader {
  private readers = new WeakMap<GPUDevice, Reader>();

  private reader(device: GPUDevice, bytes: number): Reader {
    let r = this.readers.get(device);
    if (!r || r.buf.size !== bytes) {
      if (r && !r.busy) r.buf.destroy();
      r = { buf: device.createBuffer({ size: bytes, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ }), busy: false };
      this.readers.set(device, r);
    }
    return r;
  }

  // Whether a probe of this size can be read on this device now.
  ready(device: GPUDevice, bytes: number): boolean {
    return !this.reader(device, bytes).busy;
  }

  // Records copying the probe's output (`used` bytes of a reader `bytes`
  // long) in `enc`. Once `enc` is submitted, call read() to have it handed
  // to `use` when it arrives.
  copy(i: FutInstance, enc: GPUCommandEncoder, out: FutharkArray, bytes: number, used = bytes): void {
    enc.copyBufferToBuffer(rawBuffer(i, out, "f32_1d"), 0, this.reader(i.device, bytes).buf, 0, used);
  }

  read(device: GPUDevice, bytes: number, use: (data: Float32Array) => void, used = bytes): void {
    const r = this.reader(device, bytes);
    r.busy = true;
    r.buf
      .mapAsync(GPUMapMode.READ, 0, used)
      .then(() => {
        const data = new Float32Array(r.buf.getMappedRange(0, used).slice(0));
        r.buf.unmap();
        use(data);
      })
      .catch(() => {}) // the device went away
      .finally(() => (r.busy = false));
  }
}
