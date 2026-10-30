// Calling the build's entry points: render and probe take the uniforms as a
// params record (rebuilt per build only when they change), the flight state
// as a drive record and the spells or probe points as arrays. Builds from
// before an input existed take fewer, and are still called correctly (to
// benchmark against them).

import { buildRecord, type FutharkArray, type FutInstance } from "./futhark-loader.ts";
import type { Drive } from "./flight.ts";
import type { Params } from "./params-drawer.ts";

type Record_ = { ptr: number; free(): void };

// Whether an entry point of this build takes the named input.
export const takes = (i: FutInstance, entry: string, input: string): boolean =>
  !!i.fut.manifest.entry_points[entry]?.inputs.some((x: { name: string }) => x.name === input);

// A []f32 input (freed by the caller).
const f32Array = (i: FutInstance, data: Float32Array): FutharkArray => i.fut.types["[]f32"].from_data(data, data.length);

export class Entries {
  private records = new WeakMap<FutInstance, { version: number; rec: Record_ }>();
  private params: Params;

  constructor(params: Params) {
    this.params = params;
  }

  // The uniforms as this build's params record.
  paramsRecord(i: FutInstance): Record_ {
    const r = this.records.get(i);
    if (r && r.version === this.params.version) return r.rec;
    r?.rec.free();
    const rec = buildRecord(i, "params", this.params.values);
    this.records.set(i, { version: this.params.version, rec });
    return rec;
  }

  // Frees a build's params record (when it is unloaded).
  forget(i: FutInstance): void {
    this.records.get(i)?.rec.free();
    this.records.delete(i);
  }

  async render(i: FutInstance, w: number, vw: number, h: number, t: number, yaw: number, pitch: number, dist: number, d: Drive, spells: Float32Array = new Float32Array(0)): Promise<FutharkArray> {
    const prm = this.paramsRecord(i);
    if (!takes(i, "render", "dr")) return i.fut.entry.render(w, vw, h, t, yaw, pitch, dist, prm);
    const drive = buildRecord(i, "drive", d);
    const sp = takes(i, "render", "spells") ? f32Array(i, spells) : null;
    try {
      return await (sp
        ? i.fut.entry.render(w, vw, h, t, yaw, pitch, dist, prm, drive, sp)
        : i.fut.entry.render(w, vw, h, t, yaw, pitch, dist, prm, drive));
    } finally {
      drive.free();
      sp?.free();
    }
  }

  // The collision probe, from the same state, also at the given points.
  async probe(i: FutInstance, t: number, d: Drive, pts: Float32Array): Promise<FutharkArray> {
    const prm = this.paramsRecord(i);
    const drive = buildRecord(i, "drive", d);
    const p = takes(i, "probe", "pts") ? f32Array(i, pts) : null;
    try {
      return await (p ? i.fut.entry.probe(t, prm, drive, p) : i.fut.entry.probe(t, prm, drive));
    } finally {
      drive.free();
      p?.free();
    }
  }
}
