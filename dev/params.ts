// Parameters: params.json describes every tweakable parameter and how the
// drawer lays it out. From it this generates
//   fut/params.fut  the uniforms' `params` record and their defaults;
//   fut/render.fut  the entry points, render taking a params record;
//   fut/config.fut  the compile-time constants, from their defaults overlaid
//                   with the values the drawer saved in config.local.json.
// Both are generated (and gitignored); files are only rewritten when their
// contents change, so the file watcher does not rebuild needlessly.

import { existsSync, readFileSync, writeFileSync } from "node:fs";

export type Kind = "slider" | "int" | "color" | "checkbox" | "select";
export type Value = number | boolean | number[];
export interface Param {
  id: string;
  label: string;
  kind: Kind;
  default: Value;
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  compile?: boolean;
}
export interface Schema {
  groups: { name: string; params: Param[] }[];
}

export const SCHEMA = "params.json";
export const LOCAL = "config.local.json";

export const loadSchema = (): Schema => JSON.parse(readFileSync(SCHEMA, "utf8"));
export const allParams = (s: Schema): Param[] => s.groups.flatMap((g) => g.params);

// The compile-time values in force: defaults overlaid with saved changes.
export function compileValues(s: Schema): Record<string, Value> {
  const saved = existsSync(LOCAL) ? JSON.parse(readFileSync(LOCAL, "utf8")) : {};
  const out: Record<string, Value> = {};
  for (const p of allParams(s).filter((p) => p.compile)) out[p.id] = p.id in saved ? saved[p.id] : p.default;
  return out;
}

export function saveCompileValues(s: Schema, changes: Record<string, Value>): void {
  const known = new Set(allParams(s).filter((p) => p.compile).map((p) => p.id));
  const saved = existsSync(LOCAL) ? JSON.parse(readFileSync(LOCAL, "utf8")) : {};
  for (const [k, v] of Object.entries(changes)) if (known.has(k)) saved[k] = v;
  writeFileSync(LOCAL, JSON.stringify(saved, null, 2) + "\n");
}

// Number of floats a uniform occupies in the array passed to `render`.
export const width = (p: Param): number => (p.kind === "color" ? 3 : 1);

const futType = (p: Param): string =>
  p.kind === "color" ? "vec3" : p.kind === "checkbox" ? "bool" : p.kind === "slider" ? "f32" : "i32";

const lit = (p: Param, v: Value): string => {
  const f = (x: number) => (Number.isInteger(x) ? x.toFixed(1) : String(x));
  if (p.kind === "color") return `vec ${(v as number[]).map((x) => `(${f(x)})`).join(" ")}`;
  if (p.kind === "checkbox") return String(Boolean(v));
  if (p.kind === "slider") return `${f(v as number)}f32`;
  return `${Math.round(v as number)}i32`;
};

const HEADER = "-- Generated from params.json by dev/params.ts; do not edit.\n";

function paramsFut(s: Schema): string {
  const ps = allParams(s).filter((p) => !p.compile);
  return `${HEADER}
import "vec3"

-- The uniforms, passed to render as a record (see render.fut).
type params =
  { ${ps.map((p) => `${p.id}: ${futType(p)}`).join("\n  , ")} }

def default_params : params =
  { ${ps.map((p) => `${p.id} = ${lit(p, p.default)}`).join("\n  , ")} }
`;
}

// The program's entry points (Futhark only takes entry points from the file
// it compiles). render takes the uniforms as a params record: an opaque type
// in Futhark's API, which the harness builds from the manifest (see
// buildRecord in web/futhark-loader.ts); its scalars reach the GPU in each
// kernel's uniform struct.
function renderFut(): string {
  return `${HEADER}
import "vec3"
import "params"
import "wizard"

entry render (w: i32) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
             (prm: params) : []u32 =
  render_frame prm w vw h tm yaw pitch dst

-- CPU reference image, with the default uniforms (see dev/ref-render.ts).
entry render_rgb (w: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
                 (tx: f32) (ty: f32) (tz: f32) : [][][3]u8 =
  render_rgb_frame default_params w h tm yaw pitch dst tx ty tz
`;
}

function configFut(s: Schema): string {
  const vals = compileValues(s);
  const ps = allParams(s).filter((p) => p.compile);
  return `${HEADER}-- Compile-time constants (changing one rebuilds the shader).
${ps.map((p) => `\n-- ${p.label}\ndef ${p.id} : ${futType(p)} = ${lit(p, vals[p.id])}`).join("\n")}
`;
}

function writeIfChanged(path: string, text: string): boolean {
  if (existsSync(path) && readFileSync(path, "utf8") === text) return false;
  writeFileSync(path, text);
  return true;
}

// Regenerates both files; returns whether either changed.
export function generate(dir = "fut"): boolean {
  const s = loadSchema();
  const a = writeIfChanged(`${dir}/params.fut`, paramsFut(s));
  const b = writeIfChanged(`${dir}/config.fut`, configFut(s));
  const c = writeIfChanged(`${dir}/render.fut`, renderFut());
  return a || b || c;
}
