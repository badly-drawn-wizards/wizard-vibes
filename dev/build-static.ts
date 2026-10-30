// Builds the static site into dist/ (for GitHub Pages, or any static host):
//   index.html, web/main.js   the page and its bundled harness, built with
//                             STATIC defined (see web/static.ts)
//   params.json               the parameter schema, the presets and the
//                             build of each,
//                             in place of the dev server's /params
//   fut/<id>/wizard.{js,wasm,gl.json}
//                             the shader, once per quality preset
//                             (params.json's "quality"), compiled with the
//                             compile-time parameters at their defaults
//                             overlaid with the preset's (config.local.json
//                             is ignored)
// All paths are relative, so the site works under a sub-path.
//
// usage: node dev/build-static.ts [out-dir]   (default dist)

import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import * as esbuild from "esbuild";
import { buildFuthark, FUT_ROOT } from "./build-fut.ts";
import { compileValues, generate, loadSchema } from "./params.ts";

const out = process.argv[2] ?? "dist";
const schema = loadSchema();
const presets = schema.quality ?? { high: {} };

// The Futhark program once per quality preset (the page loads the one it
// needs: see web/quality.ts), generated with the default compile-time
// parameters overlaid with the preset's, built in parallel.
const built = await Promise.all(
  Object.entries(presets).map(async ([level, preset]) => {
    const src = `build/static-src/${level}`;
    rmSync(src, { recursive: true, force: true });
    mkdirSync(src, { recursive: true });
    for (const f of readdirSync("fut")) if (f.endsWith(".fut") || f.endsWith(".js")) cpSync(join("fut", f), join(src, f));
    generate(src, false, preset);
    const r = await buildFuthark(src);
    if (!r.ok) {
      console.error(`${level}: ${r.message}`);
      process.exit(1);
    }
    console.error(`${level}: shader ${r.version} ${r.cached ? "(cached)" : `built in ${(r.ms / 1000).toFixed(1)} s`}`);
    return [level, r.version] as const;
  }),
);
const builds = Object.fromEntries(built);

rmSync(out, { recursive: true, force: true });
for (const id of Object.values(builds)) {
  mkdirSync(join(out, "fut", id), { recursive: true });
  for (const f of ["wizard.js", "wizard.wasm", "wizard.gl.json"]) cpSync(join(FUT_ROOT, id, f), join(out, "fut", id, f));
}

// (build: the default level's, for anything that wants just one)
const fallback = builds.med ?? Object.values(builds)[0];
writeFileSync(join(out, "params.json"), JSON.stringify({ schema, compile: compileValues(schema, false), presets, builds, build: fallback }));

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
// the licence, and those of the code compiled into the shader module
for (const f of ["LICENSE", "THIRD_PARTY_LICENSES.txt"]) cpSync(f, join(out, f));

const size = (p: string) => Math.round(readFileSync(p).length / 1024);
console.error(`${out}/: page ${size(join(out, "web/main.js"))} KiB; per quality level ${size(join(out, "fut", fallback, "wizard.wasm"))} KiB wasm + ${size(join(out, "fut", fallback, "wizard.js"))} KiB js + ${size(join(out, "fut", fallback, "wizard.gl.json"))} KiB WebGL shader`);
