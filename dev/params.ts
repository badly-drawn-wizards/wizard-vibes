// Parameters: params.json describes every tweakable parameter and how the
// drawer lays it out. From it this generates
//   fut/params.fut  the uniforms' `params` record and their defaults;
//   fut/render.fut  the entry points, render taking one scalar argument per
//                   uniform value (Futhark puts scalar arguments in a
//                   uniform struct);
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

// The scalar entry-point arguments for a uniform (a colour takes three), and
// the expression rebuilding its value from them. Every argument is an f32,
// so the harness passes the values array as is.
function args(p: Param): { names: string[]; value: string } {
  if (p.kind === "color") {
    const names = ["r", "g", "b"].map((c) => `${p.id}_${c}`);
    return { names, value: `vec ${names.join(" ")}` };
  }
  const v = p.kind === "checkbox" ? `${p.id} > 0.5` : p.kind === "slider" ? p.id : `i32.f32 (f32.round ${p.id})`;
  return { names: [p.id], value: v };
}

function paramsFut(s: Schema): string {
  const ps = allParams(s).filter((p) => !p.compile);
  return `${HEADER}
import "vec3"

-- The uniforms. The harness passes them to render as scalar arguments (see
-- render.fut), which Futhark packs into each kernel's uniform struct.
type params =
  { ${ps.map((p) => `${p.id}: ${futType(p)}`).join("\n  , ")} }

def default_params : params =
  { ${ps.map((p) => `${p.id} = ${lit(p, p.default)}`).join("\n  , ")} }
`;
}

// The program's entry points (Futhark only takes entry points from the file
// it compiles): render takes one scalar per uniform value, in schema order.
function renderFut(s: Schema): string {
  const ps = allParams(s).filter((p) => !p.compile);
  const a = ps.map(args);
  const decls = a.flatMap((x) => x.names).map((n) => `(${n}: f32)`);
  const lines: string[] = [];
  for (let i = 0; i < decls.length; i += 4) lines.push("             " + decls.slice(i, i + 4).join(" "));
  return `${HEADER}
import "vec3"
import "params"
import "wizard"

entry render (w: i32) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
${lines.join("\n")}
             : []u32 =
  let prm : params =
    { ${ps.map((p, i) => `${p.id} = ${a[i].value}`).join("\n    , ")} }
  in render_frame prm w vw h tm yaw pitch dst

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
  const c = writeIfChanged(`${dir}/render.fut`, renderFut(s));
  return a || b || c;
}
