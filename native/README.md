# Native client

The scene's per-pixel code, as Futhark compiles it to WGSL for the browser,
run natively by [wgpu](https://wgpu.rs) (Vulkan on Linux) in a window: on the
flight path, with the default parameters from `params.json`, and the frame
rate and GPU time per frame in the title bar (and on stdout each second).
It exists to compare with the browser.

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
```

`--bench` renders like the browser's benchmark (`window.wizard.bench`): a
warm-up frame, then frames at t0 + f / 30 with the benchmark daemon's
camera, nothing presented, the GPU time from timestamp queries.
