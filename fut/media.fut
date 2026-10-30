-- Everything the march integrates: the orb's glow cloud, the glowing rune
-- letters, the lightning, and a faint haze inside the orb's glass.

import "vec3"
import "params"
import "config"
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
def orb_glow_density (prm: params) (g: glow) (od: f32) : f32 =
  if od < 0 then 0 else prm.orb_glow * g.level * f32.exp (-od / prm.orb_glow_extent)

-- The rune letters glow in a thin layer hugging the strokes, pulsing gently.
def rune_glow_density (prm: params) (tm: f32) (gd: f32) : f32 =
  prm.rune_glow * rune_pulse prm.rune_pulse tm * f32.exp (-gd / 0.05)

-- Inside a glass orb, a faint glowing haze.
def globe_haze : f32 = 0.3

-- Clouds are lit by the moon, more brightly towards their tops, over a dim
-- ambient; they scatter what they absorb.
def cloud_colour (h: f32) : vec3 = vec 0.04 0.045 0.09 +^ scale (0.12 + 0.3 * h) (vec 0.42 0.46 0.62)

#[noinline] -- compiled once (see scene in scene.fut)
-- The colour a glowing material gives off, used both for its surface and
-- for the glow around it, so the two always match. 0 for materials that do
-- not glow.
def glow_colour (m: i32) : vec3 =
  if m == m_potion then vec 1 0.12 0.2
  else if m == m_potion + 1 then vec 0.2 1 0.35
  else if m == m_potion + 2 then vec 0.65 0.25 1
  else splat 0

-- The potions' liquids glow in a thin halo, coloured by whichever liquid is
-- nearest.
def potion_glow_density (prm: params) (ld: f32) : f32 = prm.potion_glow * f32.exp (-(f32.max ld 0) / 0.012)

def media (pose: pose) (g: glow) (p: vec3) : media_sample =
  let tm = pose.tm
  let vis = pose.prm.layers
  let glow = has vis layers_glow -- the glow clouds round the orb, runes and potions
  let orb = has vis layers_orb -- the haze and lightning inside the orb
  let od = sd_orb p
  let gd = if glow then glyph_dist tm p else 1e9
  let bolts = if orb then lightning_media tm p else {cap = 1e9, radiance = splat 0, extinction = 0}
  let haze = if orb && od < 0 then globe_haze else 0
  let od = if glow || orb then od else 1e9 -- (no step cap near a hidden orb)
  let (ld, lm) = if glow then nearest_liquid pose p else (1e9, m_potion)
  in { cap = f32.min bolts.cap (f32.min (0.3 * f32.abs od + 0.015) (0.3 * gd + 0.01))
             |> f32.min (0.3 * f32.abs ld + 0.004)
     , radiance = scale (if glow then orb_glow_density pose.prm g od else 0) orb_glow_colour
                  +^ scale (rune_glow_density pose.prm tm gd) rune_glow_colour
                  +^ scale haze orb_glow_colour
                  +^ bolts.radiance
                  +^ scale (potion_glow_density pose.prm ld) (glow_colour lm)
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
  let dt = f32.max 0.3 ((t1 - t0) / f32.i32 cloud_samples)
  let (_, _, rad, tr) =
    loop (t, i, rad, tr) = (t0 + 0.5 * dt, 0i32, splat 0, 1f32)
    while t < t1 && i < 64 && tr > 0.02 do
      let w = wo +^ scale t wd
      let (dens, clear) = cloud_sample pose.prm.cloud_cover pose.prm.cloud_density pose.secs w
      in if dens <= 0 then (t + f32.max dt clear, i + 1, rad, tr)
         else let ch = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
              in (t + dt, i + 1, rad +^ scale (dens * dt * tr) (cloud_colour ch), tr * f32.exp (-dens * dt))
  in (rad, tr)
