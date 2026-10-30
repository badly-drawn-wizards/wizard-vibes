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

-- The rune letters glow the same way, in a thin layer hugging the strokes.
def rune_glow_density (g: glow) (gd: f32) : f32 =
  4 * g.level * f32.exp (-gd / 0.05)

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
  -- clouds, in the world
  let w = to_world pose.fl p
  let gap = cloud_gap w.y
  let dens = if gap > 0 then 0 else cloud_density pose.secs w
  let ch = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
  in { cap = f32.min bolts.cap (f32.min (0.3 * f32.abs od + 0.015) (0.3 * gd + 0.01))
             |> f32.min (if gap > 0 then gap + 0.05 else 0.25)
     , radiance = scale (orb_glow_density g od) orb_glow_colour
                  +^ scale (rune_glow_density g gd) rune_glow_colour
                  +^ scale haze orb_glow_colour
                  +^ bolts.radiance
                  +^ scale dens (cloud_colour ch)
     , extinction = dens }
