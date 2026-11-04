// The native client's shader (see native/). As for the WebGL fallback
// (dev/webgl.ts), this adds an entry point to the Futhark program's WGSL
// that calls the per-pixel GPU functions, shade_pixel and finish_pixel, with
// the uniforms from a block of its own; here a compute shader, one thread a
// pixel in workgroups of 256 (Futhark's default block size, as the render
// kernel gets in the browser), packing the pixels BGRA8 into a buffer like
// the render entry point does. The other kernels and their globals are
// dropped, leaving only what the entry point reaches.
//
// Writes <build>/wizard.native.wgsl and <build>/wizard.native.json:
// { fields, workgroup } where fields lists the uniform block's members in
// order (as in wizard.gl.json), followed by `a_w`, the padded row length.
//
// With --rust, writes instead native/shader/src/generated.rs for the
// rust-gpu port: the uniforms as the same block (Uniforms) and as Futhark's
// records (Params, Drive), and the compile-time parameters and layer flags
// as constants, as fut/params.fut and fut/config.fut have them.
//
// usage: node dev/native.ts <build dir>
//        node dev/native.ts --rust

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { stripComments, topLevelItems } from "../web/wgsl-f32.ts";
import { makePortable } from "../web/wgsl-portable.ts";
import { allParams, compileValues, flagName, loadSchema, type Param, type Value } from "./params.ts";
import { DRIVE, DRIVE_BOOL, pixelProgram, type GlField } from "./webgl.ts";

const WORKGROUP = 256;

// Drops the module-scope declarations (variables, overrides, structs,
// constants) that nothing kept refers to: the other kernels' bindings,
// whose workgroup arrays are sized by overrides wgpu would have to resolve.
function dropUnusedGlobals(wgsl: string): string {
  const items = topLevelItems(stripComments(wgsl)).filter((s) => s.trim());
  const nameOf = (item: string) => (/\bfn\s+\w+\s*\(/.test(item) ? null : (/\b(?:alias|struct|const|override|var(?:<[^>]*>)?)\s+([A-Za-z_]\w*)/.exec(item)?.[1] ?? null));
  const names = items.map(nameOf);
  const keep = items.map((_, i) => names[i] === null);
  // what the kept items mention, until nothing more is reached
  let changed = true;
  while (changed) {
    changed = false;
    const text = items.filter((_, i) => keep[i]).join("\n");
    const used = new Set(text.match(/\b[A-Za-z_]\w*\b/g));
    names.forEach((n, i) => {
      if (!keep[i] && n !== null && used.has(n)) {
        keep[i] = true;
        changed = true;
      }
    });
  }
  return items.filter((_, i) => keep[i]).join("\n");
}

export function nativeShader(buildDir: string): void {
  const { wgsl, fields: pixelFields, shade, finish } = pixelProgram(buildDir);
  const fields: GlField[] = [...pixelFields, { name: "a_w", type: "i32", from: { kind: "arg", id: "w" } }];
  const entry = `
struct NativeUniforms {
${fields.map((f) => `  ${f.name}: ${f.type},`).join("\n")}
}
@group(0) @binding(0) var<uniform> u: NativeUniforms;
@group(0) @binding(1) var<storage, read_write> native_out: array<u32>;

fn native_byte(v: f32) -> u32 {
  return u32(clamp(v, 0.0, 1.0) * 255.0 + 0.5);
}

@compute @workgroup_size(${WORKGROUP})
fn native_main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = i32(gid.x);
  if (i >= u.a_w * u.a_h) { return; }
  let px = i % u.a_w;
  if (px >= u.a_vw) { native_out[i] = 0xff000000u; return; }
  let x = f32(px) + 0.5;
  let y = f32(i / u.a_w) + 0.5;
  var r: f32; var g: f32; var b: f32; var seen: f32;
  ${shade}
  var fr: f32; var fg: f32; var fb: f32;
  ${finish}
  native_out[i] = native_byte(fb) | (native_byte(fg) << 8u) | (native_byte(fr) << 16u) | 0xff000000u;
}
`;
  // (the program's other bindings are in group 0 too: dropped as unused)
  const portable = makePortable(wgsl.replace(/@group\(0\)\s*@binding\(\d+\)\s*/g, "") + entry, ["native_main"]);
  writeFileSync(join(buildDir, "wizard.native.wgsl"), dropUnusedGlobals(portable));
  writeFileSync(join(buildDir, "wizard.native.json"), JSON.stringify({ fields, workgroup: WORKGROUP }));
}

const RUST_OUT = "native/shader/src/generated.rs";

const rustType = (p: Param): string => (p.kind === "color" ? "Vec3" : p.kind === "checkbox" ? "bool" : p.kind === "slider" ? "f32" : "i32");
const rustLit = (p: Param, v: Value): string => {
  const f = (x: number) => (Number.isInteger(x) ? x.toFixed(1) : String(x));
  if (p.kind === "color") return `Vec3::new(${(v as number[]).map(f).join(", ")})`;
  if (p.kind === "checkbox") return String(Boolean(v));
  if (p.kind === "slider") return f(v as number);
  return String(Math.round(v as number));
};

export function rustGenerated(): string {
  const s = loadSchema();
  const all = allParams(s);
  const uniforms = all.filter((p) => !p.compile).sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const compile = all.filter((p) => p.compile);
  const values = compileValues(s);
  // the uniform block, as dev/native.ts's WGSL entry point has it
  const block: { name: string; ty: string; from: string }[] = [];
  for (const p of uniforms) {
    if (p.kind === "color") for (let k = 0; k < 3; k++) block.push({ name: `p_${p.id}_${k}`, ty: "f32", from: p.id });
    else block.push({ name: `p_${p.id}`, ty: p.kind === "slider" ? "f32" : "i32", from: p.id });
  }
  for (const id of DRIVE) block.push({ name: `d_${id}`, ty: DRIVE_BOOL.has(id) ? "i32" : "f32", from: id });
  for (const id of ["vw", "h"]) block.push({ name: `a_${id}`, ty: "i32", from: id });
  for (const id of ["tm", "yaw", "pitch", "dst"]) block.push({ name: `a_${id}`, ty: "f32", from: id });
  block.push({ name: "a_w", ty: "i32", from: "w" });
  const unpack = (p: Param): string =>
    p.kind === "color" ? `Vec3::new(u.p_${p.id}_0, u.p_${p.id}_1, u.p_${p.id}_2)` : p.kind === "checkbox" ? `u.p_${p.id} != 0` : `u.p_${p.id}`;
  return `// Generated from params.json by dev/native.ts --rust; do not edit.
// (fut/params.fut and fut/config.fut for the rust-gpu port)

use crate::prelude::*;
${all
  .filter((p) => p.kind === "flags")
  .map((p) => `\n// ${p.label}\n` + p.options!.map((o, i) => `pub const ${flagName(p, o)}: i32 = ${1 << i};`).join("\n") + `\npub const ${p.id}_all: i32 = ${(1 << p.options!.length) - 1};`)
  .join("\n")}

// Compile-time parameters (with config.local.json's values)
${compile.map((p) => `pub const ${p.id}: ${rustType(p)} = ${rustLit(p, values[p.id])}; // ${p.label}`).join("\n")}

// The uniforms (fut/params.fut's params record)
#[derive(Clone, Copy)]
pub struct Params {
${uniforms.map((p) => `    pub ${p.id}: ${rustType(p)},`).join("\n")}
}

// The flight state (fut/rig.fut's drive record)
#[derive(Clone, Copy)]
pub struct Drive {
${DRIVE.map((id) => `    pub ${id}: ${DRIVE_BOOL.has(id) ? "bool" : "f32"},`).join("\n")}
}

// The uniform block, as the client fills it (wizard.native.json's fields)
#[repr(C)]
pub struct Uniforms {
${block.map((f) => `    pub ${f.name}: ${f.ty},`).join("\n")}
}

pub fn unpack(u: &Uniforms) -> (Params, Drive) {
    (
        Params {
${uniforms.map((p) => `            ${p.id}: ${unpack(p)},`).join("\n")}
        },
        Drive {
${DRIVE.map((id) => `            ${id}: ${DRIVE_BOOL.has(id) ? `u.d_${id} != 0` : `u.d_${id}`},`).join("\n")}
        },
    )
}
`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === "--rust") {
    const text = rustGenerated();
    if (!existsSync(RUST_OUT) || readFileSync(RUST_OUT, "utf8") !== text) writeFileSync(RUST_OUT, text);
    console.error(`wrote ${RUST_OUT}`);
  } else {
    nativeShader(process.argv[2]);
    console.error(`wrote ${join(process.argv[2], "wizard.native.wgsl")}`);
  }
}
