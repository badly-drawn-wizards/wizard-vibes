// Renders frames on the CPU by compiling fut/wizard.fut with
// `futhark multicore` and calling its `render_rgb` entry point.

import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";

export interface View {
  width: number;
  height: number;
  yaw: number;
  pitch: number;
  dist: number;
}

const EXE = "build/ref/wizard";

export function compileCpu(): void {
  mkdirSync("build/ref", { recursive: true });
  execFileSync("futhark", ["multicore", "-o", EXE, "fut/wizard.fut"], { stdio: "inherit" });
}

// Returns packed RGB rows, top row first.
export function renderRgb(v: View, time: number): Uint8Array {
  const input = `${v.width}i32 ${v.height}i32 ${time}f32 ${v.yaw}f32 ${v.pitch}f32 ${v.dist}f32`;
  const out = execFileSync(EXE, ["-e", "render_rgb", "-b"], { input, maxBuffer: 1 << 30 });
  // Futhark binary data format: 'b', version, rank, 4-char type, i64 dims, payload.
  if (out[0] !== 0x62 || out[2] !== 3 || out.toString("latin1", 3, 7) !== "  u8") {
    throw new Error("unexpected futhark binary output");
  }
  return out.subarray(7 + 24);
}
