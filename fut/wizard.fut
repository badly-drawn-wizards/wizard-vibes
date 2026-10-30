-- Sphere-traced wizard: camera, tone mapping and entry points. Compiled with
-- `futhark webgpu` for the browser harness and with `futhark multicore` for
-- the CPU reference image.

import "vec3"
import "anim"
import "shading"
import "scene"
import "spells"
import "world"
import "params"

def aces (x: f32) : f32 = saturate ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14))

def default_target : vec3 = vec 0 1.15 0

-- Camera orbits a target; returns the ray through the point (x, y) of the
-- image, in pixels from its top left (a pixel's centre is at + 0.5).
def camera_ray (target: vec3) (vw: i32) (h: i32) (yaw: f32) (pitch: f32) (dst: f32) (x: f32) (y: f32) : (vec3, vec3) =
  let ro = target +^ scale dst (vec (f32.cos pitch * f32.sin yaw) (f32.sin pitch) (f32.cos pitch * f32.cos yaw))
  let ro = {x = ro.x, y = f32.max ro.y 0.1, z = ro.z}
  let fw = normalise (target -^ ro)
  let rt = normalise (cross fw (vec 0 1 0))
  let up = cross rt fw
  let fh = f32.i32 h
  let u = (x - 0.5 * f32.i32 vw) / fh * 2
  let v = (0.5 * fh - y) / fh * 2
  let rd = normalise (scale 2.414 fw +^ scale u rt +^ scale v up)
  in (ro, rd)

-- The colour (before tone mapping) seen through image point (x, y), in
-- pixels from the image's top left, and how far the ray sees (infinity: the
-- sky). A real GPU function, like finish_pixel: the WebGL fallback calls
-- the two from a fragment shader (see dev/webgl.ts), so they take only
-- scalars and records, and their names must stay free of 'z' (see scene).
#[noinline]
def shade_pixel (prm: params) (dr: drive) (target: vec3) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) (x: f32) (y: f32) : (vec3, f32) =
  let (ro, rd) = camera_ray target vw h yaw pitch dst x y
  -- tm in seconds; the last argument is the angle one pixel spans
  in shade prm dr tm (2 / (f32.i32 h * 2.414)) ro rd

-- Tone mapping, gamma and vignette, for image point (x, y).
#[noinline]
def finish_pixel (prm: params) (vw: i32) (h: i32) (x: f32) (y: f32) (c: vec3) : vec3 =
  let c = if prm.view != 0 then c else vmap aces (scale prm.exposure c)
  let c = vmap (\v -> (v ** (1 / 2.2))) c
  let ux = (x / f32.i32 vw - 0.5)
  let uy = (y / f32.i32 h - 0.5)
  let vig = if prm.view != 0 then 1 else 1 - prm.vignette * (ux * ux + uy * uy)
  in scale vig c

-- The final colour seen through image point (x, y), with the spells, and
-- how far the ray sees.
def sample (prm: params) (dr: drive) (spells: []f32) (target: vec3) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) (x: f32) (y: f32) : (vec3, f32) =
  let (c, seen) = shade_pixel prm dr target vw h tm yaw pitch dst x y
  -- the spells shine in front of whatever the ray sees
  let (ro, rd) = camera_ray target vw h yaw pitch dst x y
  let fl = flight_of prm dr tm
  let c = c +^ spell_glow prm spells tm (to_world fl ro) (to_world_dir fl rd) seen
  in (finish_pixel prm vw h x y c, seen)

def pixel (prm: params) (dr: drive) (spells: []f32) (target: vec3) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) (x: i32) (y: i32) : vec3 =
  (sample prm dr spells target vw h tm yaw pitch dst (f32.i32 x + 0.5) (f32.i32 y + 0.5)).0

def to_u8 (v: f32) : u32 = u32.f32 (saturate v * 255 + 0.5)

def luma (c: vec3) : f32 = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z

---------------------------------------------------------------------
-- Anti-aliasing, as passes over the finished image.

-- FXAA, simplified (no search along the edge): where the local contrast
-- exceeds the threshold, find whether the edge runs across or along, and
-- blend towards the neighbour across it, by how much the pixel stands out
-- from its neighbourhood.
def fxaa [h][w] (threshold: f32) (img: [h][w]vec3) : [h][w]vec3 =
  let at (y: i64) (x: i64) = #[unsafe] img[i64.max 0 (i64.min (h - 1) y), i64.max 0 (i64.min (w - 1) x)]
  in tabulate_2d h w (\y x ->
       let c = at y x
       let (n, s, e, wst) = (at (y - 1) x, at (y + 1) x, at y (x + 1), at y (x - 1))
       let (ln, ls, le, lw, lc) = (luma n, luma s, luma e, luma wst, luma c)
       let hi = f32.max lc (f32.max (f32.max ln ls) (f32.max le lw))
       let lo = f32.min lc (f32.min (f32.min ln ls) (f32.min le lw))
       let range = hi - lo
       in if range < f32.max threshold (0.125 * hi) then c else
       let (lne, lnw, lse, lsw) = (luma (at (y - 1) (x + 1)), luma (at (y - 1) (x - 1)), luma (at (y + 1) (x + 1)), luma (at (y + 1) (x - 1)))
       -- an edge running along x has its contrast across y
       let along_x = f32.abs (ln + ls - 2 * lc) * 2 + f32.abs (lne + lse - 2 * le) + f32.abs (lnw + lsw - 2 * lw)
       let along_y = f32.abs (le + lw - 2 * lc) * 2 + f32.abs (lne + lnw - 2 * ln) + f32.abs (lse + lsw - 2 * ls)
       let horizontal = along_x >= along_y
       let (a, la, b, lb) = if horizontal then (n, ln, s, ls) else (wst, lw, e, le)
       let other = if f32.abs (la - lc) >= f32.abs (lb - lc) then a else b
       let avg = (2 * (ln + ls + le + lw) + lne + lnw + lse + lsw) / 12
       let sub = smoothstep 0 1 (saturate (f32.abs (avg - lc) / range))
       in mix c other (0.5 * sub * sub + 0.15))

-- Supersampling at edges: pixels on a geometric edge (where the distance
-- the ray sees jumps by over 4% from a neighbour's, or between something
-- and the sky) or of high contrast in luma (stars, glows) are shaded again
-- with four more rays on a rotated grid and averaged with their first
-- sample. Surface detail is not: textures are already filtered by the
-- pixel footprint. Only those pixels are traced again: they are gathered
-- into a list first, so the extra work scales with their number.
def edge_supersample [h][w] (threshold: f32) (shade_at: f32 -> f32 -> vec3) (img: [h][w](vec3, f32)) : [h][w]vec3 =
  let at (y: i64) (x: i64) = #[unsafe] img[i64.max 0 (i64.min (h - 1) y), i64.max 0 (i64.min (w - 1) x)]
  let edge (i: i64) =
    let (y, x) = (i / w, i % w)
    let (c, t) = at y x
    let lc = luma c
    let differs ((cn, tn): (vec3, f32)) =
      f32.abs (luma cn - lc) > threshold
      || (if f32.isinf t || f32.isinf tn then f32.isinf t != f32.isinf tn
          else f32.abs (t - tn) > 0.04 * f32.min t tn)
    in differs (at (y - 1) x) || differs (at (y + 1) x) || differs (at y (x - 1)) || differs (at y (x + 1))
  let [n] (edges: [n]i64) = filter edge (iota (h * w))
  -- (one thread per extra ray: edge pixels are the costliest to trace, so
  -- each thread tracing all four in turn left the GPU waiting on them)
  let offsets = [(0.125, -0.375), (0.375, 0.125), (-0.125, 0.375), (-0.375, -0.125)]
  let rays = tabulate (n * 4) (\k ->
               let i = #[unsafe] edges[k / 4]
               let (dx, dy) = #[unsafe] offsets[k % 4]
               in shade_at (f32.i64 (i % w) + 0.5 + dx) (f32.i64 (i / w) + 0.5 + dy))
  let resampled = tabulate n (\j ->
                    let i = #[unsafe] edges[j]
                    let r (k: i64) = #[unsafe] rays[4 * j + k]
                    in scale 0.2 (r 0 +^ r 1 +^ r 2 +^ r 3 +^ (at (i / w) (i % w)).0))
  in unflatten (scatter (map (.0) (flatten img)) edges resampled)

-- w is the padded row length (multiple of 64 so rows are 256-byte aligned);
-- columns x >= vw are padding. Pixels are packed BGRA8, little-endian, to
-- match a bgra8unorm canvas (the format every WebGPU implementation supports
-- for canvases; Firefox crashes copying buffers into rgba8unorm ones).
-- (The entry points are generated into render.fut, which passes the
-- uniforms in as a params record.)
-- With anti-aliasing (prm.aa: 1 FXAA, 2 edge supersampling) the image is
-- made first and then passed over.
def render_frame (prm: params) (dr: drive) (spells: []f32) (w: i32) (vw: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32) : []u32 =
  let pack (c: vec3) = to_u8 c.z | (to_u8 c.y << 8) | (to_u8 c.x << 16) | 0xff000000
  let shade_at = sample prm dr spells default_target vw h tm yaw pitch dst
  in if prm.aa == 0 then
       tabulate (i64.i32 w * i64.i32 h) (\i ->
         let x = i32.i64 (i % i64.i32 w)
         let y = i32.i64 (i / i64.i32 w)
         in if x >= vw then 0xff000000 else pack (shade_at (f32.i32 x + 0.5) (f32.i32 y + 0.5)).0)
     else
       let img = tabulate_2d (i64.i32 h) (i64.i32 vw) (\y x -> shade_at (f32.i64 x + 0.5) (f32.i64 y + 0.5))
       let img = if prm.aa == 1 then fxaa prm.aa_threshold (map (map (.0)) img)
                 else edge_supersample (3 * prm.aa_threshold) (\x y -> (shade_at x y).0) img
       in tabulate (i64.i32 w * i64.i32 h) (\i ->
            let x = i % i64.i32 w
            in if x >= i64.i32 vw then 0xff000000 else pack (#[unsafe] img[i / i64.i32 w, x]))

-- CPU reference image, orbiting an arbitrary target (for close-ups).
def render_rgb_frame (prm: params) (w: i32) (h: i32) (tm: f32) (yaw: f32) (pitch: f32) (dst: f32)
                     (tx: f32) (ty: f32) (tz: f32) : [][][3]u8 =
  tabulate_2d (i64.i32 h) (i64.i32 w) (\y x ->
    let c = pixel prm on_path [] (vec tx ty tz) w h tm yaw pitch dst (i32.i64 x) (i32.i64 y)
    in [u8.u32 (to_u8 c.x), u8.u32 (to_u8 c.y), u8.u32 (to_u8 c.z)])
