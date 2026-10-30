-- Scene-independent sphere tracing, parameterised by the scene's distance
-- function and by the participating media whose emission is integrated along
-- each ray.
--
-- Distance functions take a tolerance: `dist tol p` must be exact when the
-- true distance is below `tol`, and may otherwise return any lower bound that
-- is at least `tol`. That lets scenes skip detailed geometry far from the
-- point (see wizard_bound), while each consumer asks for the accuracy it needs.

import "vec3"
import "config"
import "sdf"

-- At a point, media report a step cap (steps are kept below it, so the
-- integral is sampled finely where densities change fast), the radiance
-- they emit per unit length, and how much they absorb (extinction per unit
-- length, e.g. clouds; glows have none).
type media_sample = {cap: f32, radiance: vec3, extinction: f32}

type^ dist_fn = f32 -> vec3 -> f32

-- Marches up to distance tmax. Returns (distance, material or -1, emission
-- integrated up to the hit, transmittance to the hit). With no hit, the
-- distance is >= tmax unless the step budget ran out. Stepping only needs
-- exact distances near surfaces.
def march (scene: f32 -> vec3 -> hit) (media: vec3 -> media_sample) (ro: vec3) (rd: vec3) (tmax: f32) : (f32, i32, vec3, f32) =
  let (t, _, m, acc, tr) =
    loop (t, i, m, acc, tr) = (0.02f32, 0i32, -1i32, splat 0, 1f32)
    while i < 300 && m < 0 && t < tmax && tr > 0.01 do
      let p = ro +^ scale t rd
      let (d, mat) = scene 0.1 p
      in if d < 0.0004 * t then (t, i + 1, mat, acc, tr)
         else
           let s = media p
           let dt = f32.min (f32.min (d * 0.85) s.cap) (tmax - t + 1e-4)
           in (t + dt, i + 1, -1, acc +^ scale (dt * tr) s.radiance, tr * f32.exp (-s.extinction * dt))
  in (t, m, acc, tr)

-- Tetrahedral gradient, written as a loop so the scene is only inlined once.
def normal (dist: dist_fn) (p: vec3) : vec3 =
  let e = 0.0006
  let n = loop n = splat 0 for i < 4 do
            let k = if i == 0 then vec 1 (-1) (-1)
                    else if i == 1 then vec (-1) (-1) 1
                    else if i == 2 then vec (-1) 1 (-1)
                    else vec 1 1 1
            in n +^ scale (dist 0.1 (p +^ scale e k)) k
  in normalise n

def shadow_k : f32 = 10

-- Penumbra-estimating shadow ray towards a light at distance tmax. The
-- penumbra term k*h/t saturates once h >= t/k, so that is all the accuracy
-- the distance needs.
def soft_shadow (dist: dist_fn) (ro: vec3) (rd: vec3) (tmax: f32) : f32 =
  let k = shadow_k
  -- Improved penumbra (after Quilez): from the previous and current distance
  -- estimate where the ray passes closest to the occluder, rather than using
  -- the samples themselves, which bands at grazing angles.
  let (res, _, _, _) =
    loop (res, t, ph, i) = (1f32, 0.02f32, 1e10f32, 0i32)
    while i < shadow_steps && t < tmax && res > 0.002 do
      let h = dist (t / k) (ro +^ scale t rd)
      let y = h * h / (2 * ph)
      let d = f32.sqrt (f32.max 0 (h * h - y * y))
      -- Beyond t/k the distance may be only a bound, but there the penumbra
      -- term would be >= 1 anyway, so leave res alone.
      let res = if h < t / k then f32.min res (k * d / f32.max 1e-4 (t - y)) else res
      in (res, t + clamp 0.01 0.25 h, h, i + 1)
  in smoothstep 0 1 (saturate res)

-- True if a shadow ray cannot be darkened by occluders lying inside the
-- sphere (c, r): it never comes within tmax/k of it, so every penumbra
-- sample k*h/t is at least 1. Skipping such rays changes nothing.
def shadow_misses (c: vec3) (r: f32) (ro: vec3) (rd: vec3) (tmax: f32) : bool =
  let oc = c -^ ro
  let tc = clamp 0 tmax (dot oc rd)
  in length (oc -^ scale tc rd) - r > tmax / shadow_k

-- Where a ray leaves the sphere (c, r): beyond it, occluders inside the
-- sphere can only shade the ray through its penumbra, within the margin.
def sphere_exit (c: vec3) (r: f32) (ro: vec3) (rd: vec3) : f32 =
  let oc = c -^ ro
  let b = dot oc rd
  in b + f32.sqrt (f32.max 0 (b * b - (dot oc oc - r * r)))

-- Probes along the normal; each probe counts only how far its distance falls
-- short of its offset, so distances beyond the longest offset never matter.
def ambient_occlusion (dist: dist_fn) (p: vec3) (n: vec3) : f32 =
  let (occ, _) =
    loop (occ, sca) = (0f32, 1f32) for i < 5 do
      let h = 0.01 + 0.12 * f32.i32 i / 4
      let d = dist 0.13 (p +^ scale h n)
      in (occ + f32.max 0 (h - d) * sca, sca * 0.95)
  in saturate (1 - 3 * occ)
