# Futhark wizard

A sphere-tracing (distance-estimation) renderer written in Futhark, compiled
with Futhark's WebGPU backend and shown live in the browser. Saving any file in
`fut/` recompiles it and hot-swaps it into the running page.

```sh
nix develop -c npm install   # once
nix develop -c npm run dev   # http://localhost:8080
```

Drag to orbit, scroll to zoom, space to pause. The ¼ / ½ / 1× buttons set the
render scale.

F toggles free flight: W / S accelerate the way the camera looks and brake,
A / D accelerate to the camera's left and right, E / Q climb and descend,
Shift boosts. The camera then stays put in the world while he turns. He faces
the way he travels, banks into turns, points the way he accelerates, and his
runes brighten as he climbs and dim as he descends. The `probe` entry point
(fut/collide.fut) samples the land around a few spheres covering the rock and
him; the harness copies its 24 floats into a mappable buffer with each frame
and reads them back without waiting, to push him out of anything he hits
(web/flight.ts).

## How it works

- The Futhark sources, in `fut/`:

  | Module | Contents |
  |---|---|
  | `wizard.fut` | camera, tone mapping, entry points (`render` returns one frame as `[]u32`, packed BGRA8) |
  | `shading.fut` | materials, lights, glass (orb and potions), the per-ray passes |
  | `scene.fut` | geometry: the figure and its pose (IK, breathing, lean), rock, terrain bounds |
  | `march.fut` | sphere tracing, normals, soft shadows, AO |
  | `media.fut`, `lightning.fut` | glows, haze, potion halos, clouds, lightning |
  | `textures.fut` | bump-mapped bark, cloth, leather, hair, cork |
  | `world.fut` | flight path, terrain, noise |
  | `sdf.fut`, `vec3.fut`, `hash.fut`, `anim.fut` | primitives, ray–sphere and bounding-sphere helpers, maths, timing |
- The WebGPU backend only exists on the `webgpu` branch of diku-dk/futhark,
  so `flake.nix` builds futhark from that branch. The first `nix develop`
  compiles it from source, which takes a while.
- The frame never goes through the CPU:
  - Futhark creates its own `GPUDevice`. `web/futhark-loader.ts` captures it
    and configures the canvas with it.
  - `fut/expose.js` maps the output array's `WGPUBuffer` handle
    (`futhark_values_raw_u32_1d`) to its JS `GPUBuffer`.
  - Each frame is then a single `copyBufferToTexture` into the canvas texture
    (`bgra8unorm`; Futhark packs pixels as BGRA).
  - Rows are padded to 64 pixels so `bytesPerRow` is 256-byte aligned.
- `npm run dev` runs `dev/serve.ts`, which runs `dev/server.ts` and restarts it
  when the server's own code under `dev/` changes.
- `dev/server.ts` watches `fut/` and builds with `dev/build-fut.ts`.
  - Builds are content-addressed: `build/fut/<id>`, where the id hashes the
    sources, the shim, the compiler flags and the exact toolchain. Unchanged
    inputs reuse the build instantly.
  - The page is notified over server-sent events. Compile errors appear in
    an overlay while the last good build keeps running.
- WGSL compile errors are caught too. `web/shader-check.ts` wraps
  `createShaderModule` and reads `getCompilationInfo()` for every module.
  - Each error is reported with the module, the enclosing `fn`, line and column,
    and the source line with a caret.
  - An error while loading fails the hot swap and keeps the previous build.
  - An error after loading shows in the overlay.
  - Warnings go to the console.
- Time is in seconds. The flight never repeats; periodic motion (breathing,
  lightning, runes) is a whole-number harmonic of a 12 s loop
  (`loop_seconds`).

## Other scripts

| Command | Output |
|---|---|
| `npm run ref -- [w h t yaw pitch dist]` | CPU reference frame, `build/ref.png` (`futhark multicore`) |
| `npm run gif -- [w h fps yaw pitch dist]` | 12 s as `build/wizard.gif` |
| `dev/variant.sh name [file sed-expr]...` | builds a modified copy of `fut/` and prints its build id |
| `node dev/bench-daemon.ts`, then `dev/bench.sh id1,id2` | 1080p GPU ms per build in Chromium and Firefox, interleaved (set `CHROME`, `FIREFOX`) |
| `CHROME=$(which chromium) node dev/screenshot.ts [url] [out] [ms]` | headless Chromium screenshot of the live page |
| `FIREFOX=$(which firefox) node dev/firefox.ts [probe \| url] [out] [ms]` | the same in Firefox, via WebDriver BiDi (`DUMPIO=1` for its stderr) |

## f16

Futhark's WebGPU runtime always requires the `shader-f16` device feature
(`rts/c/backends/webgpu.h`) and puts `enable f16;` plus f16 helper functions
in front of every shader (`src/Futhark/CodeGen/RTS/WGSL.hs`), even for
f32-only programs like this one. When the adapter lacks `shader-f16`, or the
URL has `?f32`, `web/wgsl-f32.ts` handles it at load time:

- it drops the feature from Futhark's device request;
- it strips the f16 declarations from each shader before compiling it;
- if a kernel actually uses f16, it fails with an error saying so.

On a GPU that supports f16, the stripped shaders give bit-identical frames.
The HUD shows `f32` when this mode is active.

Chrome on Linux needs Vulkan for hardware WebGPU; see `dev/chrome-args.ts` for
the headless flags. The SwiftShader fallback adapter can compile the shaders
but Futhark's kernels don't run correctly on it (read-backs are aborted), so a
real GPU is needed.

## Firefox

Tested with Firefox 158 on Linux, which has hardware WebGPU. It renders
pixel-identical frames to Chrome, with hot reload working. Two things needed
handling:

- **Pointer parameters.** Futhark's WGSL helpers take `ptr<storage, …>`
  parameters, which need the `unrestricted_pointer_parameters` language
  feature. Firefox's naga lacks it, and also rejects Futhark's f16 helpers.
  When `navigator.gpu.wgslLanguageFeatures` lacks the feature, or the URL has
  `?portable`, `web/wgsl-portable.ts` rewrites each shader:
  - it drops functions no entry point reaches (about 270 of the prelude's
    ~290 per shader);
  - it specialises the remaining storage-pointer helpers per buffer, so
    `write_i8((&buf), i, v)` becomes `write_i8__buf(i, v)`.

  The HUD shows `portable` when this is active. On Chrome, `?portable` gives
  bit-identical frames.
- **Canvas format.** Firefox crashes when copying a buffer into an
  `rgba8unorm` canvas texture, so the canvas is `bgra8unorm` and Futhark packs
  pixels as BGRA.

Firefox may log that it can't create a shared-texture swapchain and falls
back to a slower presentation path. That's internal to Firefox and doesn't
affect correctness.

## The orb

The orb is a refracting glass sphere (`orb_ior` in `fut/shading.fut`).
`shade` follows each camera ray through up to three straight segments
(outside, inside the glass, outside again), bending it by Snell's law at each
crossing. The transmitted and reflected parts are weighted by Schlick's
Fresnel term, and the reflection is traced as a second path. Both paths go
through one loop, so the shading code is compiled only once. The march
integrates the haze and lightning inside the glass along the refracted
segment.

Compile-time switches (save the file and hot reload picks them up):

| Switch | File | Default |
|---|---|---|
| `orb_tendrils`: lightning inside the glass | `fut/lightning.fut` | on |
| `orb_arcs`: lightning leaping out of the orb | `fut/lightning.fut` | off |
| `orb_reflections`: reflect the scene in the glass | `fut/shading.fut` | on (about 0.1 ms at 1080p) |
