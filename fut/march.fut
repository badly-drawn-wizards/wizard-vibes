-- Scene-independent sphere tracing, parameterised by the scene's distance
-- function and by the participating media whose emission is integrated along
-- each ray.
--
-- Distance functions take a tolerance: `dist tol p` must be exact when the
-- true distance is below `tol`, and may otherwise return any lower bound that
-- is at least `tol`. That lets scenes skip detailed geometry far from the
-- point (see wizard_bound), while each consumer asks for the accuracy it needs.

import "vec3"
import "sdf"

-- At a point, media report a step cap (steps are kept below it, so the
-- integral is sampled finely where densities change fast) and the radiance
-- they emit per unit length.
type media_sample = {cap: f32, radiance: vec3}

type^ dist_fn = f32 -> vec3 -> f32

-- Returns (distance, material or -1, emission integrated up to the hit).
-- Stepping only needs exact distances near surfaces.
def march (scene: f32 -> vec3 -> hit) (media: vec3 -> media_sample) (ro: vec3) (rd: vec3) : (f32, i32, vec3) =
  let (t, _, m, acc) =
    loop (t, i, m, acc) = (0.02f32, 0i32, -1i32, splat 0)
    while i < 220 && m < 0 && t < 40 do
      let p = ro +^ scale t rd
      let (d, mat) = scene 0.1 p
      in if d < 0.0004 * t then (t, i + 1, mat, acc)
         else
           let s = media p
           let dt = f32.min (d * 0.85) s.cap
           in (t + dt, i + 1, -1, acc +^ scale dt s.radiance)
  in (t, m, acc)

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

-- Penumbra-estimating shadow ray towards a light at distance tmax. The
-- penumbra term k*h/t saturates once h >= t/k, so that is all the accuracy
-- the distance needs.
def soft_shadow (dist: dist_fn) (ro: vec3) (rd: vec3) (tmax: f32) : f32 =
  let k = 10
  let (res, _, _) =
    loop (res, t, i) = (1f32, 0.02f32, 0i32)
    while i < 48 && t < tmax && res > 0.002 do
      let h = dist (t / k) (ro +^ scale t rd)
      in (f32.min res (k * h / t), t + clamp 0.01 0.25 h, i + 1)
  in smoothstep 0 1 (saturate res)

-- Probes along the normal; each probe counts only how far its distance falls
-- short of its offset, so distances beyond the longest offset never matter.
def ambient_occlusion (dist: dist_fn) (p: vec3) (n: vec3) : f32 =
  let (occ, _) =
    loop (occ, sca) = (0f32, 1f32) for i < 5 do
      let h = 0.01 + 0.12 * f32.i32 i / 4
      let d = dist 0.13 (p +^ scale h n)
      in (occ + f32.max 0 (h - d) * sca, sca * 0.95)
  in saturate (1 - 3 * occ)
