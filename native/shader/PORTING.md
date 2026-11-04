# Porting fut/ to rust-gpu

Each Futhark module `fut/X.fut` becomes `src/X.rs` (vec3.fut is `src/vec.rs`),
a mechanical, line-by-line translation that computes exactly what the Futhark
does: the port is checked by diffing its image against the WGSL Futhark makes.
Keep the comments (shortened if you like), the order of definitions, and the
names.

## Names and namespaces

- Every module starts with `use crate::prelude::*;`, which re-exports every
  module: like Futhark, one namespace. Functions and constants keep their
  Futhark names exactly (snake_case; `'` becomes `_p`, e.g. `a'` -> `a_p`).
- Futhark types become CamelCase structs: `flight` -> `Flight`, `pose` ->
  `Pose`, `sky_in` -> `SkyIn`, `material` -> `Material`, `media_sample` ->
  `MediaSample`. Same field names, `pub`, `#[derive(Clone, Copy)]`. Type
  aliases of tuples stay aliases (`hit` is `Hit = (f32, i32)` in sdf.rs).
  A record type written inline (anonymous) gets a struct named for its use.
- A name defined in two modules (`sd_pyramid` in platform and world) is
  private (`fn`, not `pub fn`) in the module that uses it only itself.
- Already ported: vec.rs (vec3.fut, plus helpers below), hash.rs, anim.rs,
  foes.rs, materials.rs, sdf.rs, march.rs, wizard.rs (shade_pixel and
  finish_pixel), and generated.rs: `Params` (fut/params.fut's `params`
  record: colours are Vec3, checkboxes bool), `Drive` (rig.fut's `drive`,
  defined there, so do not define it again), the compile-time constants of
  fut/config.fut (`march_steps`, `full_enemies`, `leaf_colour`, ...: i32,
  bool, f32 or Vec3) and the layer flags (`layers_figure`, ...).
  Read those files for the conventions.

## Translation

- `vec3` is glam's `Vec3`, `vec2` is `Vec2`. Use the helpers in vec.rs under
  their Futhark names and argument orders: `vec x y z` -> `vec(x, y, z)`,
  `vec2 x y` -> `vec2(x, y)`, `scale s v`, `dot`, `cross`, `length`,
  `normalise`, `vmap`, `vabs`, `vmaxs`, `maxc`, `mix`, `reflect`, `length2`,
  `rot2`, `clamp lo hi x` (that order!), `saturate`, `smoothstep`, `fract`,
  `mix1`, `pow`, `splat`. `a +^ b` -> `a + b`, `-^` -> `-`, `*^` -> `*`.
  Record field access `v.x` is the same. Record update `p with y = 0` ->
  `vec(p.x, 0.0, p.z)` (or a struct update `Foo { y: 0.0, ..p }`).
- Scalars: `f32.min a b` -> `a.minimum(b)`, `f32.max` -> `.maximum` (as
  WGSL's min and max, one FMin or FMax each: rust-gpu compiles f32's `min`
  and `max` to compares and selects handling NaN), `f32.abs`,
  `f32.sqrt`, `f32.sin/cos/exp/floor/acos/asin/atan/tanh` -> methods,
  `f32.atan2 y x` -> `y.atan2(x)`, `x ** y` -> `x.powf(y)` (or `pow(x, y)`),
  `f32.pi` -> `PI`, `f32.inf` -> `INF`, `f32.isinf` -> `isinf`,
  `f32.sgn` -> `sgn` (0 at 0: NOT `signum`), `f32.round` -> `round_even`
  (ties to even: NOT `round`), `f32.i32 i` -> `i as f32`, `i32.f32 x` ->
  `ftoi(x)` (truncating and saturating as WGSL's; Rust's `as` adds NaN
  handling), `i32.bool b` -> `b as i32`,
  `i32.min` -> `.min` (on i32). Float literals need a `.0`: `2` -> `2.0`.
- Integer division and modulo: Futhark's i32 `/` and `%` round towards
  minus infinity: `idiv(a, b)` and `imod(a, b)`; `//` and `%%` truncate:
  `sdiv(a, b)` and `srem(a, b)`. Never Rust's `/` and `%` on integers: they
  check for zero and overflow even in release builds, and rust-gpu then
  inlines every function that may panic. Integer arithmetic wraps: use
  `wrapping_*` where it can overflow.
- `let` chains -> `let`; `if c then a else b` -> `if c { a } else { b }`.
- Loops: `loop (a, b) = (a0, b0) for i < n do body` and `... while c do
  body` -> `let (mut a, mut b) = (a0, b0); let mut i = 0; while i < n {
  ...; i += 1; }`. Use `while`, not `for` over ranges or iterators (rust-gpu
  compiles those less reliably). Keep the loop's exact update order: the
  Futhark body computes all the new values from the old ones, so compute
  them into fresh `let`s before assigning.
- Higher-order functions: a parameter `(f: vec3 -> f32)` becomes
  `f: impl Fn(Vec3) -> f32`; lambdas become closures (`move |p| ...` where
  they capture only Copy values, which is nearly always). Partial
  application becomes a closure.
- Tuples stay tuples (`.0`, `.1`). Arrays (rare in the scene code) become
  fixed-size Rust arrays.
- `#[noinline]` -> `#[inline(never)]`; leave other functions to the
  compiler (small ones may get `#[inline]`).
- Assertions, `#[unsafe]`, size annotations: drop.
- no_std: no `Vec`, `String`, `println!`, `panic!`, `assert!` or
  allocation; only core and `spirv_std::glam`.

## Checking

`nix develop ../..#gpu -c cargo check` in native/shader type-checks the
whole crate on the host (other modules may still be stubs while they are
being ported: errors about names other modules define are expected until
they are done; make sure every such name is spelled as the conventions say
the other module will define it). The SPIR-V build is
`cd native/shader-builder && nix develop ../..#gpu -c cargo run --release`.
