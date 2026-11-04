# Wizard

> **AI disclaimer:** this project was written almost entirely by an AI
> (Claude, through Claude Code), directed and reviewed by a human. Expect
> the code, comments and docs to reflect that; check anything you rely on.

**Live demo:** <https://badly-drawn-wizards.github.io/wizard-vibes/> (needs a
browser with WebGPU).

A sphere-traced (distance-estimated) wizard flying over an endless world on
his rock, ice shard or magic carpet: the whole scene, its music and its sound
effects come from one shader crate written in Rust and compiled with
[rust-gpu](https://rust-gpu.github.io). The same app, Rust with an
[egui](https://github.com/emilk/egui) UI over [wgpu](https://wgpu.rs), runs
natively and in the browser. Edit the shader and the running app swaps it in;
edit the app and it is patched where it runs.

- Fly him freely, cast fire bolts, lightning and wind blades, and fight enemy
  wizards, each with its own looks, who cast back; a shield, mana, health.
- A world of desert dunes and pyramids, meadow hills with cottages and
  groves, and alpine mountains with towers, with rivers, clouds and fog.
- A glass orb that refracts and reflects, with lightning inside it; potions,
  a tome, runes that glow with his flight.
- Two songs, computed as pure functions of time: a disco (its voices
  modelled on a reference track's stems, its harpsichord on measurements
  of a 1709 instrument) and a chiptune cracktro. Synthesised sound effects.
- A drawer with every parameter (uniforms apply at once; shader settings
  rebuild it), quality levels with an automatic one, FXAA or edge
  supersampling, debug views, a game controller.

## Running

Everything builds in the flake's dev shell (the first `nix develop` fetches
rust-gpu's nightly toolchain).

```sh
nix develop -c cargo run --release -p wizard-dev   # dev server: http://localhost:8080
nix develop -c cargo run --release -p wizard       # the app in a window
```

The dev server rebuilds the shader (`native/shader`) when its sources,
`params.json` or `config.local.json` change, and the web app when
`client/` does; the page and the native app both pick up new shaders
without restarting. To hot-patch the native app's own code as well, run it
under Dioxus's [Subsecond](https://dioxuslabs.com/blog/release-070):

```sh
nix develop -c dx serve --hotpatch -p wizard --bin wizard --platform desktop
```

Headless, with the same shader and passes:

```sh
target/release/wizard --shot out.ppm --t0 3 [--size WxH] [--set aa=2]
target/release/wizard --bench 300 [--wgsl]   # GPU ms a 1080p frame; --wgsl: what the browser runs
```

`cargo run --release -p wizard-dev -- --static dist` builds a self-contained
site (the app, and the shader at its defaults and at each quality level),
which `.github/workflows/pages.yml` publishes.

## How it works

| Where | What |
|---|---|
| `native/shader` | the scene, music, sound effects and collisions (rust-gpu; ported from the Futhark in `fut/`: see `native/shader/PORTING.md`) |
| `native/shader-builder` | builds it: SPIR-V, naga's WGSL of it, and the uniform layout |
| `client` | the app: renderer, egui UI, the game, audio (cpal), native and wasm |
| `devserver` | rebuilds and serves, and tells the app over server-sent events |
| `params.json` | every parameter, its range and its group in the drawer |

**One shader crate, three ways.** rust-gpu compiles the crate to one SPIR-V
module, which the native app hands to the driver as it is (wgpu's
passthrough). For the browser, naga turns the same SPIR-V into WGSL after two
fixes (`shader-builder/src/fixup.rs`): structs outside buffers lose
rust-gpu's member offsets (a `Vec3` is 12 bytes to Rust and 16-aligned to
WGSL, so naga calls them overlapping), and infinite constants become calls
to functions returning them, as WGSL rejects non-finite constant
expressions. WGSL may also assume there are no NaNs, so the shader never
uses NaN as a sentinel. The client depends on the very same crate as a
normal Rust library, to run on the CPU what is cheaper there: the collision
probe each frame (no GPU readback to wait for) and the sound effects
(filters and reverb over whole buffers, made once). The music is a GPU
kernel, a chunk of samples at a time, read back and scheduled ahead.

**Hot reload.** `shader-builder` records each entry point's bindings and the
uniform block's fields in `shader.json`, so the app builds its pipelines and
fills its uniforms from that, and a shader with new parameters or passes
loads into an app that has not been rebuilt. One bind group layout covers
every entry point: SPIR-V run as it is is not checked against its layout,
and a layout per entry point broke the driver's pipelines. The app's own
code goes through `subsecond::call` each frame, which `dx` patches.

**The terrain march** steps over the land with over-relaxed sphere tracing,
and bounds the trees and structures along the ray rather than around the
point: how far the ray has to go to reach the treetops' or roofs' height,
or to leave its grid cell (if nothing stands near in it), instead of the
height or the distance to the cell's edge, which shallow rays towards the
horizon kept running into. Each cell's tree or structure (and the ground
under it) is worked out once per ray. Rays take a quarter fewer steps, and
far fewer run out of them before reaching distant land.

**rust-gpu and speed.** `f32::min` and `max` compile to NaN-aware compares
(the shader uses `minimum` and `maximum`, one instruction each); `as` from
f32 to i32 branches for NaN (`ftoi` clamps and converts); integer `/` and
`%` keep Rust's panics, which make rust-gpu inline every function that may
panic (`sdiv`, `srem`, `idiv`, `imod` use the SPIR-V instructions). On a
Radeon 8060S a 1080p frame takes about 19 ms as SPIR-V and 23 ms as naga's
WGSL; edge supersampling adds about 5.5 ms.

**Profiling.** `native/rgp.sh NAME [app args]` captures a frame for AMD's
Radeon GPU Profiler on RADV (`MESA_VK_TRACE=rgp`); its instruction timing
shows where the shader's time goes. `RADV_DEBUG=shaderstats,nocache` prints
its registers and spills.

## License

GPL-3.0-or-later; see [LICENSE](LICENSE).
