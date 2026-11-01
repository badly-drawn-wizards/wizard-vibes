// Parameters: params.json describes every tweakable parameter and how the
// drawer lays it out. From it this generates
//   fut/params.fut  the uniforms' `params` record and their defaults;
//   fut/render.fut  the entry points, render taking a params record;
//   fut/reference.fut  the CPU reference renderer's entry point;
//   fut/config.fut  the compile-time constants, from their defaults overlaid
//                   with the values the drawer saved in config.local.json.
// Both are generated (and gitignored); files are only rewritten when their
// contents change, so the file watcher does not rebuild needlessly.

import { existsSync, readFileSync, writeFileSync } from "node:fs";

export type Kind = "slider" | "int" | "color" | "checkbox" | "select" | "flags";
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
// Groups nest: each holds parameters, subgroups, or both.
export interface Group {
  name: string;
  params?: Param[];
  groups?: Group[];
}
export interface Schema {
  groups: Group[];
  // quality presets: compile-time parameter values by level (low, med, high)
  quality?: Record<string, Record<string, Value>>;
}

export const SCHEMA = "params.json";
export const LOCAL = "config.local.json";

export const loadSchema = (): Schema => JSON.parse(readFileSync(SCHEMA, "utf8"));
const groupParams = (g: Group): Param[] => [...(g.params ?? []), ...(g.groups ?? []).flatMap(groupParams)];
export const allParams = (s: Schema): Param[] => s.groups.flatMap(groupParams);

// The compile-time values in force: defaults overlaid with saved changes
// (unless `local` is false: the defaults alone, as for the static build),
// then with `overrides` (e.g. a quality preset).
export function compileValues(s: Schema, local = true, overrides: Record<string, Value> = {}): Record<string, Value> {
  const saved = { ...(local && existsSync(LOCAL) ? JSON.parse(readFileSync(LOCAL, "utf8")) : {}), ...overrides };
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

// A flags option's constant: e.g. "layers" + "Figure" -> layers_figure.
export const flagName = (p: Param, option: string): string => `${p.id}_${option.toLowerCase().replace(/[^a-z0-9]+/g, "_")}`;

function flagDefs(s: Schema): string {
  return allParams(s)
    .filter((p) => p.kind === "flags")
    .map((p) => `\n-- ${p.label}\n` + p.options!.map((o, i) => `def ${flagName(p, o)} : i32 = ${1 << i}`).join("\n") +
                 `\ndef ${p.id}_all : i32 = ${(1 << p.options!.length) - 1}\n`)
    .join("");
}

function paramsFut(s: Schema): string {
  const ps = allParams(s).filter((p) => !p.compile);
  return `${HEADER}
import "vec3"
${flagDefs(s)}
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
// kernel's uniform struct. The flight state (drive) is passed the same way.
function renderFut(): string {
  return `${HEADER}
import "vec3"
import "params"
import "wizard"
import "scene"
import "collide"
import "config"
import "music"

-- dr: free flight's state, from the harness (or on the flight path);
-- spells: the spells in flight (see spells.fut).
entry render (w: i32) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
             (prm: params) (dr: drive) (spells: []f32) : []u32 =
  render_frame prm dr spells w vw h tm yaw pitch dst

-- The collision probe, for free flight and the spells (see collide.fut).
entry probe (tm: f32) (prm: params) (dr: drive) (pts: []f32) : []f32 =
  probe_frame prm dr tm pts

-- How many spells may be in flight at once (a compile-time parameter).
entry spell_limit (_: i32) : i32 = max_spells

-- Music: n stereo samples (interleaved) from time t0 within the loop, at
-- rate samples a second; and the loop's length (see music.fut).
entry sound (t0: f32) (rate: f32) (n: i32) : []f32 = sound_chunk t0 rate (i64.i32 n)
entry music_period (_: i32) : f32 = period
`;
}

// The CPU reference renderer's entry point, in a program of its own: in the
// browser's it would be one more pipeline holding all the shading, which
// Chrome compiles at load for nothing.
function referenceFut(): string {
  return `${HEADER}
import "params"
import "wizard"

-- CPU reference image, with the default uniforms (see dev/cpu-render.ts).
entry render_rgb (w: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
                 (tx: f32) (ty: f32) (tz: f32) : [][][3]u8 =
  render_rgb_frame default_params w h tm yaw pitch dst tx ty tz
`;
}

function configFut(s: Schema, local: boolean, overrides: Record<string, Value>): string {
  const vals = compileValues(s, local, overrides);
  const ps = allParams(s).filter((p) => p.compile);
  return `${HEADER}-- Compile-time constants (changing one rebuilds the shader).
import "vec3"
${ps.map((p) => `\n-- ${p.label}\ndef ${p.id} : ${futType(p)} = ${lit(p, vals[p.id])}`).join("\n")}
`;
}

function writeIfChanged(path: string, text: string): boolean {
  if (existsSync(path) && readFileSync(path, "utf8") === text) return false;
  writeFileSync(path, text);
  return true;
}

// Regenerates the files in `dir`; returns whether any changed. With `local`
// false the compile-time parameters take their defaults, ignoring
// config.local.json; `overrides` sets some of them (e.g. a quality preset).
export function generate(dir = "fut", local = true, overrides: Record<string, Value> = {}): boolean {
  const s = loadSchema();
  const a = writeIfChanged(`${dir}/params.fut`, paramsFut(s));
  const b = writeIfChanged(`${dir}/config.fut`, configFut(s, local, overrides));
  const c = writeIfChanged(`${dir}/render.fut`, renderFut());
  writeIfChanged(`${dir}/reference.fut`, referenceFut());
  return a || b || c;
}
