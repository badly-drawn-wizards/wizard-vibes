// The WebGL fallback's shader. Futhark's WebGPU backend writes the whole
// program as one WGSL module of compute kernels, which WebGL cannot run. The
// per-pixel work, though, is in two real GPU functions (shade_pixel and
// finish_pixel in fut/wizard.fut) that take only scalars: the uniforms and
// drive records' fields, flattened in field order, and plain arguments. So
// this adds a second entry point to that module, a fragment shader calling
// the two with values from a uniform block of its own, and has naga turn it
// into GLSL ES 3.00 for WebGL2 (only what the new entry point reaches).
//
// Writes <build>/wizard.gl.json: { glsl, block, fields } where fields lists
// the uniform block's members in order (each a 4-byte f32 or i32, at 4 * i),
// with where its value comes from. See web/webgl.ts.
//
// usage: node dev/webgl.ts <build dir>

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { stripF16 } from "../web/wgsl-f32.ts";
import { makePortable } from "../web/wgsl-portable.ts";
import { allParams, loadSchema, type Param } from "./params.ts";

// The drive record's fields (see fut/rig.fut), as Futhark orders them.
export const DRIVE = ["arena_x", "arena_y", "arena_z", "ax", "ay", "az", "climb", "flash", "foe0", "foe1", "foe2", "foe3", "foe4", "foe5", "free", "heading", "shield", "shield_hit", "spell", "staff", "sx", "sy", "sz", "turn", "x", "y", "z"];
export const DRIVE_BOOL = new Set(["free"]);

export interface GlField {
  name: string; // member of the uniform block
  type: "f32" | "i32";
  from: { kind: "param"; id: string; index?: number } | { kind: "drive"; id: string } | { kind: "arg"; id: string };
}

// The program's WGSL, from the C source the backend writes (one string
// literal per chunk, in an array named gpu_program).
function programWgsl(c: string): string {
  const m = /static const char \*gpu_program\[\] = \{([\s\S]*?)NULL\};/.exec(c);
  if (!m) throw new Error("no gpu_program in the generated C");
  let out = "";
  for (const lit of m[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
    out += lit[1].replace(/\\(x[0-9a-fA-F]{2}|[0-7]{1,3}|.)/g, (_, e: string) => {
      if (e[0] === "x") return String.fromCharCode(parseInt(e.slice(1), 16));
      if (/^[0-7]/.test(e)) return String.fromCharCode(parseInt(e, 8));
      return ({ n: "\n", t: "\t", r: "\r" } as Record<string, string>)[e] ?? e;
    });
  }
  return out;
}

// A GPU function's name and parameter names, without Futhark's suffixes and
// z escaping.
function signature(wgsl: string, fn: string): { name: string; params: string[] } {
  const m = new RegExp(`\\bfn (futrts_${fn}_\\d+)\\(([^)]*)\\)`).exec(wgsl);
  if (!m) throw new Error(`no GPU function ${fn} in the program (is it #[noinline]?)`);
  // (split at commas outside <...>: a pointer's type has one)
  const params = m[2]
    .split(/,(?![^<]*>)/)
    .map((p) => p.split(":")[0].trim())
    .filter(Boolean)
    .map((p) => (p.endsWith("_out") ? "@out" : p.replace(/_\d+$/, "").replace(/zz/g, "z")));
  return { name: m[1], params };
}

// The uniforms' fields, as the params record flattens them.
function paramFields(ps: Param[]): GlField[] {
  const fields: GlField[] = [];
  for (const p of [...ps].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))) {
    if (p.kind === "color") for (let k = 0; k < 3; k++) fields.push({ name: `p_${p.id}_${k}`, type: "f32", from: { kind: "param", id: p.id, index: k } });
    else fields.push({ name: `p_${p.id}`, type: p.kind === "slider" ? "f32" : "i32", from: { kind: "param", id: p.id } });
  }
  return fields;
}

// The program's WGSL (portable, without f16) and the per-pixel GPU
// functions' calls: an entry point added to it passes them the uniforms
// from a block `u` with members `fields` (each a 4-byte f32 or i32, at 4 *
// i), and image point (x, y) as f32s named `x` and `y`. `shade` leaves the
// colour in r, g, b (and the distance seen in `seen`), `finish` the final
// colour in fr, fg, fb; all are declared by the caller. Shared with the
// native client's shader (dev/native.ts).
export function pixelProgram(buildDir: string): { wgsl: string; fields: GlField[]; shade: string; finish: string } {
  const schema = loadSchema();
  const uniforms = allParams(schema).filter((p) => !p.compile);
  const bools = new Set(uniforms.filter((p) => p.kind === "checkbox").map((p) => `p_${p.id}`));
  const pfields = paramFields(uniforms);
  const dfields: GlField[] = DRIVE.map((id) => ({ name: `d_${id}`, type: DRIVE_BOOL.has(id) ? "i32" : "f32", from: { kind: "drive", id } }));
  const args: GlField[] = [
    { name: "a_vw", type: "i32", from: { kind: "arg", id: "vw" } },
    { name: "a_h", type: "i32", from: { kind: "arg", id: "h" } },
    ...["tm", "yaw", "pitch", "dst"].map((id): GlField => ({ name: `a_${id}`, type: "f32", from: { kind: "arg", id } })),
  ];
  const fields = [...pfields, ...dfields, ...args];

  const wgsl = makePortable(stripF16(programWgsl(readFileSync(join(buildDir, "wizard.c"), "utf8"))));
  const shade = signature(wgsl, "shade_pixel");
  const finish = signature(wgsl, "finish_pixel");

  // the arguments, in the order the functions take them (checked against
  // their actual parameters, so a change to either shows up here)
  // (the target is camera_ray's default_target, (0, 1.15, 0))
  const pnames = pfields.map((f) => f.from.id); // (a colour's three components all carry its name)
  const value = (f: GlField) => (bools.has(f.name) || (f.from.kind === "drive" && DRIVE_BOOL.has(f.from.id)) ? `(u.${f.name} != 0)` : `u.${f.name}`);
  const shadeArgs = [...pfields.map(value), ...dfields.map(value), "0.0", "1.15", "0.0", "u.a_vw", "u.a_h", "u.a_tm", "u.a_yaw", "u.a_pitch", "u.a_dst", "x", "y", "&r", "&g", "&b", "&seen"];
  const shadeWant = [...pnames, ...DRIVE, "x", "y", "z", "vw", "h", "tm", "yaw", "pitch", "dst", "x", "y", "@out", "@out", "@out", "@out"];
  const finishArgs = [...pfields.map(value), "u.a_vw", "u.a_h", "x", "y", "r", "g", "b", "&fr", "&fg", "&fb"];
  const finishWant = [...pnames, "vw", "h", "x", "y", "x", "y", "z", "@out", "@out", "@out"];
  for (const [fn, want] of [[shade, shadeWant], [finish, finishWant]] as const) {
    if (fn.params.join(",") !== want.join(",")) {
      throw new Error(`${fn.name} takes (${fn.params.join(", ")})\nbut the entry point would pass (${want.join(", ")})`);
    }
  }
  return { wgsl, fields, shade: `${shade.name}(${shadeArgs.join(", ")});`, finish: `${finish.name}(${finishArgs.join(", ")});` };
}

export function glShader(buildDir: string): void {
  const { wgsl, fields, shade, finish } = pixelProgram(buildDir);
  const entry = `
struct GlUniforms {
${fields.map((f) => `  ${f.name}: ${f.type},`).join("\n")}
}
@group(1) @binding(0) var<uniform> u: GlUniforms;

@fragment
fn gl_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
  // (WebGL's fragment y runs up the image; the image's runs down)
  let x = pos.x;
  let y = f32(u.a_h) - pos.y;
  var r: f32; var g: f32; var b: f32; var seen: f32;
  ${shade}
  var fr: f32; var fg: f32; var fb: f32;
  ${finish}
  return vec4<f32>(fr, fg, fb, 1.0);
}
`;
  const src = join(buildDir, "gl.wgsl");
  const out = join(buildDir, "gl.frag");
  // (only what the fragment shader reaches: naga validates the whole module)
  writeFileSync(src, makePortable(wgsl + entry, ["gl_main"]));
  execFileSync("naga", ["--compact", "--entry-point", "gl_main", "--profile", "es300", "--keep-coordinate-space", src, out], { stdio: ["ignore", "pipe", "pipe"] });
  const glsl = readFileSync(out, "utf8");
  const block = /uniform\s+(\w+)\s*\{/.exec(glsl)?.[1];
  if (!block) throw new Error("no uniform block in naga's GLSL");
  writeFileSync(join(buildDir, "wizard.gl.json"), JSON.stringify({ glsl, block, fields }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  glShader(process.argv[2]);
  console.error(`wrote ${join(process.argv[2], "wizard.gl.json")}`);
}
