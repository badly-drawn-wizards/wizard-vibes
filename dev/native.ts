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
// usage: node dev/native.ts <build dir>

import { writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { stripComments, topLevelItems } from "../web/wgsl-f32.ts";
import { makePortable } from "../web/wgsl-portable.ts";
import { pixelProgram, type GlField } from "./webgl.ts";

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

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  nativeShader(process.argv[2]);
  console.error(`wrote ${join(process.argv[2], "wizard.native.wgsl")}`);
}
