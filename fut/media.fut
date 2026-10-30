-- Everything the march integrates: the orb's glow cloud, the glowing rune
-- letters, the lightning, and a faint haze inside the orb's glass.

import "vec3"
import "sdf"
import "anim"
import "march"
import "scene"
import "lightning"
import "world"

def orb_glow_colour : vec3 = vec 0.35 0.55 1
def rune_glow_colour : vec3 = vec 0.25 0.5 1

-- A cloud around the orb whose density falls off exponentially with
-- distance from the orb's surface, flickering with the lightning. It is
-- outside the glass only: inside there is just the haze, so the orb's
-- refraction stays visible.
def orb_glow_density (g: glow) (od: f32) : f32 =
  if od < 0 then 0 else 0.95 * g.level * f32.exp (-od / 0.1)

-- The rune letters glow in a thin layer hugging the strokes, pulsing gently.
def rune_glow_density (tm: f32) (gd: f32) : f32 =
  4 * rune_pulse tm * f32.exp (-gd / 0.05)

-- Inside a glass orb, a faint glowing haze.
def globe_haze : f32 = 0.3

-- Clouds are lit by the moon, more brightly towards their tops, over a dim
-- ambient; they scatter what they absorb.
def cloud_colour (h: f32) : vec3 = vec 0.04 0.045 0.09 +^ scale (0.12 + 0.3 * h) (vec 0.42 0.46 0.62)

def media (pose: pose) (g: glow) (p: vec3) : media_sample =
  let tm = pose.tm
  let od = sd_orb p
  let gd = glyph_dist tm p
  let bolts = lightning_media tm p
  let haze = if od < 0 then globe_haze else 0
  in { cap = f32.min bolts.cap (f32.min (0.3 * f32.abs od + 0.015) (0.3 * gd + 0.01))
     , radiance = scale (orb_glow_density g od) orb_glow_colour
                  +^ scale (rune_glow_density tm gd) rune_glow_colour
                  +^ scale haze orb_glow_colour
                  +^ bolts.radiance
     , extinction = 0 }

-- The clouds are integrated separately from the march (see `clouds`): their
-- slab lies between two world heights, so each ray's way through it is known
-- in advance, and the march need not take small steps (each evaluating the
-- whole scene) to cross it.

-- Radiance and transmittance of the clouds along ro + t rd for t in
-- [0, tend]. About 24 samples, spread over the part of the ray inside the
-- slab; clear sky is skipped using the bound from cloud_sample.
def clouds (pose: pose) (ro: vec3) (rd: vec3) (tend: f32) : (vec3, f32) =
  let wo = to_world pose.fl ro
  let wd = to_world_dir pose.fl rd
  -- where the ray's world height crosses the slab's bottom and top
  let (t0, t1) =
    if f32.abs wd.y < 1e-5 then (if cloud_gap wo.y > 0 then (1, 0) else (0, tend))
    else let ta = (cloud_bottom - wo.y) / wd.y
         let tb = (cloud_top - wo.y) / wd.y
         in (f32.max 0 (f32.min ta tb), f32.min tend (f32.max ta tb))
  in if t1 <= t0 then (splat 0, 1) else
  let dt = f32.max 0.3 ((t1 - t0) / 24)
  let (_, _, rad, tr) =
    loop (t, i, rad, tr) = (t0 + 0.5 * dt, 0i32, splat 0, 1f32)
    while t < t1 && i < 64 && tr > 0.02 do
      let w = wo +^ scale t wd
      let (dens, clear) = cloud_sample pose.secs w
      in if dens <= 0 then (t + f32.max dt clear, i + 1, rad, tr)
         else let ch = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
              in (t + dt, i + 1, rad +^ scale (dens * dt * tr) (cloud_colour ch), tr * f32.exp (-dens * dt))
  in (rad, tr)
