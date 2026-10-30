// Compiles fut/wizard.fut with the Futhark WebGPU backend into a fresh,
// versioned directory so the browser never sees a half-written build.

import { spawn } from "node:child_process";
import { mkdirSync, readdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

export const FUT_ROOT = "build/fut";
const ENTRY = "fut/wizard.fut";

export type BuildResult = { ok: true; version: number; ms: number } | { ok: false; message: string };

function run(cmd: string, args: string[], env: NodeJS.ProcessEnv = process.env): Promise<{ code: number; output: string }> {
  return new Promise((done) => {
    const p = spawn(cmd, args, { env });
    let output = "";
    p.stdout.on("data", (d) => (output += d));
    p.stderr.on("data", (d) => (output += d));
    p.on("error", (e) => done({ code: -1, output: String(e) }));
    p.on("close", (code) => done({ code: code ?? -1, output }));
  });
}

let counter = Date.now() % 100000;

export async function buildFuthark(): Promise<BuildResult> {
  const start = Date.now();

  // Fast typecheck first so errors show up in about a second.
  const check = await run("futhark", ["check", ENTRY]);
  if (check.code !== 0) return { ok: false, message: check.output };

  const version = ++counter;
  const dir = join(FUT_ROOT, `v${version}`);
  mkdirSync(dir, { recursive: true });
  const env = {
    ...process.env,
    CFLAGS: process.env.CFLAGS ?? "-O1",
    EMCFLAGS: [
      `--post-js ${resolve("fut/expose.js")}`,
      "-sDEFAULT_LIBRARY_FUNCS_TO_INCLUDE=$WebGPU",
      process.env.EMCFLAGS ?? "",
    ].join(" "),
  };
  const build = await run("futhark", ["webgpu", "--library", "-o", join(dir, "wizard"), ENTRY], env);
  if (build.code !== 0) {
    rmSync(dir, { recursive: true, force: true });
    return { ok: false, message: build.output };
  }
  prune(version);
  return { ok: true, version, ms: Date.now() - start };
}

// Keep the three newest builds; an open page may still be fetching the previous one.
function prune(latest: number): void {
  for (const name of readdirSync(FUT_ROOT)) {
    const v = Number(name.slice(1));
    if (name.startsWith("v") && v < latest - 2) rmSync(join(FUT_ROOT, name), { recursive: true, force: true });
  }
}
