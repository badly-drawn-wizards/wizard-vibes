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

## How it works

- `fut/wizard.fut` holds the scene SDF, the ray marcher and the shading.
  `render` returns one frame as `[]u32` packed RGBA8.
- The WebGPU backend only exists on the `webgpu` branch of diku-dk/futhark,
  so `flake.nix` builds futhark from that branch. The first `nix develop`
  compiles it from source, which takes a while.
- The frame never goes through the CPU:
  - Futhark creates its own `GPUDevice`. `web/futhark-loader.ts` captures it
    and configures the canvas with it.
  - `fut/expose.js` maps the output array's `WGPUBuffer` handle
    (`futhark_values_raw_u32_1d`) to its JS `GPUBuffer`.
  - Each frame is then a single `copyBufferToTexture` into the canvas texture.
  - Rows are padded to 64 pixels so `bytesPerRow` is 256-byte aligned.
- `dev/server.ts` watches `fut/` and runs `futhark check`, then
  `futhark webgpu --library`, writing to a new `build/fut/v<N>/` directory.
  It notifies the page over server-sent events. Compile errors appear in an
  overlay while the last good build keeps running.
- The animation loops every 12 s (`loop_seconds`). Every animated term is a
  whole-number harmonic of the loop.

## Other scripts

| Command | Output |
|---|---|
| `npm run ref -- [w h t yaw pitch dist]` | CPU reference frame, `build/ref.png` (`futhark multicore`) |
| `npm run gif -- [w h fps yaw pitch dist]` | one loop as `build/wizard.gif` |
| `CHROME=$(which chromium) node dev/screenshot.ts [url] [out] [ms]` | headless screenshot of the live page |

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
