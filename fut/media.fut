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
import "orbfx"
import "world"


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
def cloud_colour (colour: vec3) (h: f32) : vec3 = vec 0.04 0.045 0.09 +^ scale (0.12 + 0.3 * h) colour

#[noinline] -- compiled once (see scene in scene.fut)
-- The colour a glowing material gives off, used both for its surface and
-- for the glow around it, so the two always match. 0 for materials that do
-- not glow.
def glow_colour (prm: params) (m: i32) : vec3 =
  if m == m_potion then potion_colour_1
  else if m == m_potion + 1 then potion_colour_2
  else if m == m_potion + 2 then potion_colour_3
  else splat 0

-- The potions' liquids glow in a thin halo, coloured by whichever liquid is
-- nearest.
def potion_glow_density (prm: params) (ld: f32) : f32 = prm.potion_glow * f32.exp (-(f32.max ld 0) / 0.012)

#[noinline] -- (few call sites: the whole pose is passed)
def media (pose: pose) (g: glow) (p: vec3) : media_sample =
  let tm = pose.tm
  let vis = pose.prm.layers
  let glow = has vis layers_glow -- the glow clouds round the orb, runes and potions
  let orb = has vis layers_orb -- the haze and lightning inside the orb
  -- (the orb and its arcs go where the staff holds them)
  let q = staff_rest pose p
  let od = sd_orb q
  let gd = if glow then glyph_dist tm p else 1e9
  -- in the orb: lightning arcs, flames or a whirlwind, by the spell in hand
  let bolts = if !orb then no_media
              else if pose.spell == 0 then fire_media pose.secs q
              else if pose.spell == 2 then wind_media pose.secs q
              else let l = lightning_media tm q in l with radiance = scale pose.prm.lightning l.radiance
  let haze = if orb && od < 0 then globe_haze else 0
  let od = if glow || orb then od else 1e9 -- (no step cap near a hidden orb)
  let (ld, lm) = if glow then nearest_liquid pose p else (1e9, m_potion)
  in { cap = f32.min bolts.cap (f32.min (0.3 * f32.abs od + 0.015) (0.3 * gd + 0.01))
             |> f32.min (0.3 * f32.abs ld + 0.004)
     , radiance = scale (if glow then pose.orb_boost * orb_glow_density pose.prm g od else 0) pose.orb_tint
                  +^ scale (pose.runes * rune_glow_density pose.prm tm gd) pose.prm.rune_colour
                  +^ scale haze pose.orb_tint
                  +^ bolts.radiance
                  +^ scale (potion_glow_density pose.prm ld) (glow_colour pose.prm lm)
     , extinction = 0 }

-- The clouds are integrated separately from the march (see `clouds`): their
-- slab lies between two world heights, so each ray's way through it is known
-- in advance, and the march need not take small steps (each evaluating the
-- whole scene) to cross it.

-- Towards the view distance, where rays stop, everything fades into the sky
-- (1 near .. 0 at the view distance), so there is no edge.
def view_fade_at (view_distance: f32) (t: f32) : f32 = smoothstep view_distance (0.7 * view_distance) t
def view_fade (prm: params) (t: f32) : f32 = view_fade_at prm.view_distance t

-- What the clouds depend on, projected out of the pose (so clouds_through,
-- a real GPU function, takes a few scalars rather than the whole pose's).
type cloud_in = {fl: flight, secs: f32, cover: f32, density: f32, colour: vec3, view_distance: f32}

-- Radiance and transmittance of the clouds along ro + t rd for t in
-- [0, tend]. About 24 samples, spread over the part of the ray inside the
-- slab; clear sky is skipped using the bound from cloud_sample.
#[noinline]
def clouds_through (ci: cloud_in) (ro: vec3) (rd: vec3) (tend: f32) : (vec3, f32) =
  let wo = to_world ci.fl ro
  let wd = to_world_dir ci.fl rd
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
      let (dens, clear) = cloud_sample ci.cover ci.density ci.secs w
      let dens = dens * view_fade_at ci.view_distance t
      in if dens <= 0 then (t + f32.max dt clear, i + 1, rad, tr)
         else let ch = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
              in (t + dt, i + 1, rad +^ scale (dens * dt * tr) (cloud_colour ci.colour ch), tr * f32.exp (-dens * dt))
  in (rad, tr)

def clouds (pose: pose) (ro: vec3) (rd: vec3) (tend: f32) : (vec3, f32) =
  let prm = pose.prm
  in clouds_through { fl = pose.fl, secs = pose.secs, cover = prm.cloud_cover, density = prm.cloud_density
                    , colour = prm.cloud_colour, view_distance = prm.view_distance } ro rd tend
