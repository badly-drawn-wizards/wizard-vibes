-- What goes on in and around the orb, by the spell in hand (the lightning's
-- arcs are in lightning.fut): media samples at q, in the staff's rest frame,
-- so they go where the staff holds the orb. secs is the time in seconds.

import "vec3"
import "hash"
import "march"
import "scene"
import "world"

def no_media : media_sample = {cap = 1e9, radiance = splat 0, extinction = 0}

-- How far from the orb's centre anything here reaches.
def orbfx_reach : f32 = orb_radius + 0.5

-- Fire: flames licking upwards inside the glass, hottest low down, and
-- embers drifting up off the top of the orb, swaying as they rise and cool.
#[noinline]
def fire_media (secs: f32) (q: vec3) : media_sample =
  let v = q -^ orb_pos
  let r = length v
  in if r > orbfx_reach then no_media with cap = r - orbfx_reach + 0.02 else
  let flames =
    if r >= orb_radius then 0
    else let rise = vec 0 (-1.4 * secs) 0
         let f = vnoise3 (scale 18 v +^ rise) + 0.5 * vnoise3 (scale 37 v +^ scale 1.7 rise)
         let shape = 1.25 - 0.55 * (v.y / orb_radius) - 0.4 * (r / orb_radius)
         in smoothstep 0.9 1.3 (f * shape)
  let flame = scale (8 * flames) (mix (vec 1 0.3 0.04) (vec 1 0.85 0.45) flames)
  -- embers: each rises from the orb's top over its own cycle
  let (embers, near) = loop (acc, near) = (0f32, f32.inf) for j < 7 do
    let h (n: i32) = hash2 (j * 13 + n) 57
    let life = 1.2 + 0.8 * h 1
    let ph = fract (secs / life + h 2)
    let a = 6.283 * h 3 + 1.5 * ph
    let spread = 0.03 + 0.06 * ph
    let pos = orb_pos +^ vec (spread * f32.cos a) (0.8 * orb_radius + 0.38 * ph) (spread * f32.sin a)
    let d = length (q -^ pos)
    in (acc + (1 - ph) * f32.exp (-(d * d) / 0.00012), f32.min near d)
  in { cap = if r < orb_radius then 0.012 else f32.min (0.3 * near + 0.003) (0.3 * (r - orb_radius) + 0.012)
     , radiance = flame +^ scale (14 * embers) (vec 1 0.55 0.15)
     , extinction = 0 }

-- Wind: a vortex of pale streaks spinning inside the glass, and wisps
-- circling the orb on a tilted ring, each trailing a fading tail.
#[noinline]
def wind_media (secs: f32) (q: vec3) : media_sample =
  let v = q -^ orb_pos
  let r = length v
  in if r > orbfx_reach then no_media with cap = r - orbfx_reach + 0.02 else
  let swirl =
    if r >= orb_radius then 0
    else let th = f32.atan2 v.z v.x
         let h = v.y / orb_radius
         let rxz = f32.sqrt (v.x * v.x + v.z * v.z) / orb_radius
         let band = 0.5 + 0.5 * f32.sin (3 * th - 7 * secs + 6 * h + 2 * vnoise3 (scale 20 v))
         in smoothstep 0.7 1 band * smoothstep 0.05 0.45 rxz * (0.4 + 0.6 * rxz) + 0.08
  -- the ring, tilted, about 0.18 from the centre
  let w = vec v.x (v.y * 0.94 - v.z * 0.34) (v.z * 0.94 + v.y * 0.34)
  let ring_r = 0.18
  let rxz = f32.sqrt (w.x * w.x + w.z * w.z)
  let dt = f32.sqrt ((rxz - ring_r) * (rxz - ring_r) + w.y * w.y)
  let alpha = f32.atan2 w.z w.x
  let wisps = loop acc = 0f32 for j < 3 do
    let head = 4 * secs + 2.094 * f32.i32 j
    let da = alpha - head
    let da = da - 6.283 * f32.round (da / 6.283) -- (-pi, pi]: behind the head is negative
    let tail = if da > 0.05 || da < -1.6 then 0 else 1 + da / 1.6
    in acc + tail * tail
  let wisp = wisps * f32.exp (-(dt * dt) / 0.00015)
  in { cap = if r < orb_radius then 0.012 else 0.3 * dt + 0.004
     , radiance = scale (13 * swirl) (vec 0.6 1 0.8) +^ scale (5 * wisp) (vec 0.75 1 0.9)
     , extinction = 0 }
