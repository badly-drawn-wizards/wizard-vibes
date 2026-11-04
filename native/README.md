# Native client

The scene's per-pixel code run natively by [wgpu](https://wgpu.rs) (Vulkan
on Linux) in a window: on the flight path, with the default parameters from
`params.json`, and the frame rate and GPU time per frame in the title bar
(and on stdout each second). It exists to compare with the browser. The
shader is either the WGSL Futhark compiles for the browser, or a port of the
Futhark to Rust compiled with [rust-gpu](https://rust-gpu.github.io)
(`--rust-gpu`; see below).

`dev/native.ts` adds a compute entry point, `native_main`, to a build's WGSL
that calls `shade_pixel` and `finish_pixel` (the functions the WebGL
fallback calls from its fragment shader) with the uniforms from a block of
its own, and writes `wizard.native.wgsl` and `wizard.native.json` next to
the build. The client runs it for the newest build under `build/fut`, or
the one given, the first time it sees that build.

```sh
nix develop .#native
cd native && cargo run --release                 # window; Esc quits
cargo run --release -- --vsync --scale 0.5       # (default: no vsync, full size)
cargo run --release -- --bench 120               # 1920x1080, as dev/bench.sh times the browser
cargo run --release -- --shot out.ppm --t0 3     # one frame
cargo run --release -- --build ../build/fut/<id> # a specific build
cargo run --release -- --unchecked               # WGSL without wgpu's bounds checks
cargo run --release -- --set view=1 --set platform=2   # override parameters
```

`--bench` renders like the browser's benchmark (`window.wizard.bench`): a
warm-up frame, then frames at t0 + f / 30 with the benchmark daemon's
camera, nothing presented, the GPU time from timestamp queries.

## The rust-gpu port

`native/shader` is the scene's per-pixel code ported from `fut/` to Rust,
module for module and line for line (see `shader/PORTING.md`), compiled to
SPIR-V by rust-gpu and handed to the driver as is (wgpu's passthrough).
`native/shader-builder` builds it; it needs rust-gpu's pinned nightly, from
the `gpu` dev shell, and first regenerates `shader/src/generated.rs` (the
parameters, from `params.json`, as `node dev/native.ts --rust`):

```sh
nix develop .#gpu -c sh -c 'cd native/shader-builder && cargo run --release'   # -> native/shader.spv
nix develop .#native -c sh -c 'cd native && cargo run --release -- --rust-gpu'
```

It renders what the browser does: against Chrome's image of the same build,
99.86% of pixels are identical and none differ by more than 4 levels (of
255), and it matches the WGSL as closely over other times, the three
platforms, the enemies and the debug views. A port of the Futhark must
match its sources, so changes to `fut/` need making here too.

Three things rust-gpu does differently cost time, and the port avoids them:
`f32::min` and `max` become compares and selects that handle NaN (the port
uses `minimum` and `maximum`: one FMin or FMax each, as WGSL's); `as` from
f32 to i32 branches for NaN (`ftoi` clamps and converts, as naga does); and
integer `/` and `%` keep Rust's checks for zero and overflow, and rust-gpu
inlines every function that may panic, `#[inline(never)]` or not (`sdiv`,
`srem` and Futhark's flooring `idiv`, `imod` use the SPIR-V instructions).

## Frame times

GPU time per 1920x1080 frame (`--bench 120`, dev/bench.sh for Chrome), the
same build, on a Radeon 8060S (RADV):

| shader                                   | ms          |
|------------------------------------------|-------------|
| Chrome (WebGPU, Futhark's kernels)       | 21.4 - 21.5 |
| native, Futhark's WGSL                   | 22.1 - 23.0 |
| native, Futhark's WGSL, unchecked        | 20.9 - 21.5 |
| native, rust-gpu port                    | 22.0 - 22.2 |

The same shader runs about as fast natively as in Chrome: the GPU time is
the shader's, and both reach the same driver through SPIR-V. wgpu's bounds
checks (on WGSL) cost about 0.7 ms; rust-gpu's code, which has none, is
about as fast as the checked WGSL.

## Profiling

On RADV, `native/rgp.sh NAME [args]` captures one 640x360 bench frame for
the Radeon GPU Profiler into `build/rgp/NAME.rgp` (`native/rgp.sh wgsl`,
`native/rgp.sh rust-gpu --rust-gpu`). nixpkgs' `rgp` 2.7 fails to build
(patchelf crashes on AMD's binary), but its source tarball, AMD's Radeon
Developer Tool Suite, runs unpatched through nix-ld given Qt's system
libraries in `NIX_LD_LIBRARY_PATH` (libglvnd, fontconfig, libX11, libxcb
and its utils, libxkbcommon, freetype, libpng, dbus, glib, wayland, zlib,
libstdc++).
Its instruction timing shows where each kernel's time goes in the ISA.
`RADV_DEBUG=shaderstats,nocache` prints registers, spills and code size
(without `nocache` a cached pipeline prints nothing): both kernels use 108
SGPRs and 120 VGPRs at 12 waves a SIMD, spilling 234 (WGSL) and 213
(rust-gpu) SGPRs and no VGPRs.
In the WGSL's instruction timing (default view, 640x360) the terrain march
took 45% of the time, the march near the wizard 29%, the clouds 7% and the
shadow rays 7%; the code is ALU bound (memory waits 3%, SGPR spills under
3%). The terrain march's trees and structures now bound it along the ray
(their tops' heights, cell exits, and what stands in a cell worked out once
per cell), which cut its steps by a quarter (and the rays that ran out of
steps at the horizon from 31% to 9%) and the frame by about 1 ms.
