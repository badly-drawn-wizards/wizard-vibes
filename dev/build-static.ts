// Builds the static site into dist/ (for GitHub Pages, or any static host):
//   index.html, web/main.js   the page and its bundled harness, built with
//                             STATIC defined (see web/static.ts)
//   params.json               the parameter schema and the build to load,
//                             in place of the dev server's /params
//   fut/<id>/wizard.{js,wasm} the shader, compiled with every compile-time
//                             parameter at its default (config.local.json
//                             is ignored), which the page cannot change
// All paths are relative, so the site works under a sub-path.
//
// usage: node dev/build-static.ts [out-dir]   (default dist)

import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import * as esbuild from "esbuild";
import { buildFuthark, FUT_ROOT } from "./build-fut.ts";
import { compileValues, generate, loadSchema } from "./params.ts";

const out = process.argv[2] ?? "dist";
const src = "build/static-src";

// The Futhark program, generated with the default compile-time parameters.
rmSync(src, { recursive: true, force: true });
mkdirSync(src, { recursive: true });
for (const f of readdirSync("fut")) if (f.endsWith(".fut") || f.endsWith(".js")) cpSync(join("fut", f), join(src, f));
generate(src, false);
const r = await buildFuthark(src);
if (!r.ok) {
  console.error(r.message);
  process.exit(1);
}
console.error(`shader ${r.version} ${r.cached ? "(cached)" : `built in ${(r.ms / 1000).toFixed(1)} s`}`);

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "fut", r.version), { recursive: true });
for (const f of ["wizard.js", "wizard.wasm"]) cpSync(join(FUT_ROOT, r.version, f), join(out, "fut", r.version, f));

const schema = loadSchema();
writeFileSync(join(out, "params.json"), JSON.stringify({ schema, compile: compileValues(schema, false), build: r.version }));

await esbuild.build({
  entryPoints: ["web/main.ts"],
  bundle: true,
  format: "esm",
  target: "es2022",
  minify: true,
  define: { STATIC: "true" },
  outdir: join(out, "web"),
  logLevel: "warning",
});
cpSync("web/index.html", join(out, "index.html"));
writeFileSync(join(out, ".nojekyll"), ""); // serve files as they are

const size = (p: string) => Math.round(readFileSync(p).length / 1024);
console.error(`${out}/: page ${size(join(out, "web/main.js"))} KiB, shader ${size(join(out, "fut", r.version, "wizard.wasm"))} KiB wasm + ${size(join(out, "fut", r.version, "wizard.js"))} KiB js`);
