-- Sphere-traced wizard: camera, tone mapping and entry points. Compiled with
-- `futhark webgpu` for the browser harness and with `futhark multicore` for
-- the CPU reference image.

import "vec3"
import "anim"
import "shading"

def aces (x: f32) : f32 = saturate ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14))

def default_target : vec3 = vec 0 1.15 0

-- Camera orbits a target; returns the ray for pixel (x, y) with y down.
def camera_ray (target: vec3) (vw: i32) (h: i32) (yaw: f32) (pitch: f32) (dst: f32) (x: i32) (y: i32) : (vec3, vec3) =
  let ro = target +^ scale dst (vec (f32.cos pitch * f32.sin yaw) (f32.sin pitch) (f32.cos pitch * f32.cos yaw))
  let ro = {x = ro.x, y = f32.max ro.y 0.1, z = ro.z}
  let fw = normalise (target -^ ro)
  let rt = normalise (cross fw (vec 0 1 0))
  let up = cross rt fw
  let fh = f32.i32 h
  let u = (f32.i32 x + 0.5 - 0.5 * f32.i32 vw) / fh * 2
  let v = (0.5 * fh - f32.i32 y - 0.5) / fh * 2
  let rd = normalise (scale 2.414 fw +^ scale u rt +^ scale v up)
  in (ro, rd)

def pixel (target: vec3) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) (x: i32) (y: i32) : vec3 =
  let (ro, rd) = camera_ray target vw h yaw pitch dst x y
  let c = shade tm ro rd -- tm in seconds
  let c = vmap aces (scale 1.1 c)
  let c = vmap (\v -> (v ** (1 / 2.2))) c
  let ux = (f32.i32 x / f32.i32 vw - 0.5)
  let uy = (f32.i32 y / f32.i32 h - 0.5)
  let vig = 1 - 0.6 * (ux * ux + uy * uy)
  in scale vig c

def to_u8 (v: f32) : u32 = u32.f32 (saturate v * 255 + 0.5)

-- w is the padded row length (multiple of 64 so rows are 256-byte aligned);
-- columns x >= vw are padding. Pixels are packed BGRA8, little-endian, to
-- match a bgra8unorm canvas (the format every WebGPU implementation supports
-- for canvases; Firefox crashes copying buffers into rgba8unorm ones).
entry render (w: i32) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) : []u32 =
  tabulate (i64.i32 w * i64.i32 h) (\i ->
    let x = i32.i64 (i % i64.i32 w)
    let y = i32.i64 (i / i64.i32 w)
    in if x >= vw then 0xff000000
       else let c = pixel default_target vw h tm yaw pitch dst x y
            in to_u8 c.z | (to_u8 c.y << 8) | (to_u8 c.x << 16) | 0xff000000)

-- CPU reference image, orbiting an arbitrary target (for close-ups).
entry render_rgb (w: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
                 (tx: f32) (ty: f32) (tz: f32) : [][][3]u8 =
  tabulate_2d (i64.i32 h) (i64.i32 w) (\y x ->
    let c = pixel (vec tx ty tz) w h tm yaw pitch dst (i32.i64 x) (i32.i64 y)
    in [u8.u32 (to_u8 c.x), u8.u32 (to_u8 c.y), u8.u32 (to_u8 c.z)])
