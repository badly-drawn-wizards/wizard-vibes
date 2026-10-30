-- Everything the march integrates: the orb's glow cloud, the glowing rune
-- letters, the lightning, and a faint haze inside the orb's glass.

import "vec3"
import "sdf"
import "anim"
import "march"
import "scene"
import "lightning"

def orb_glow_colour : vec3 = vec 0.35 0.55 1
def rune_glow_colour : vec3 = vec 0.25 0.5 1

-- A cloud around the orb whose density falls off exponentially with
-- distance from the orb's surface; its strength and extent breathe. It is
-- outside the glass only: inside there is just the haze, so the orb's
-- refraction stays visible.
def orb_glow_density (tm: f32) (od: f32) : f32 =
  let s = f32.sin (3 * tm)
  let extent = 0.1 + 0.012 * s
  in if od < 0 then 0 else (0.9 + 0.2 * s) * f32.exp (-od / extent)

-- The rune letters glow the same way, in a thin layer hugging the strokes.
def rune_glow_density (tm: f32) (gd: f32) : f32 =
  4 * pulse tm * f32.exp (-gd / 0.05)

-- Inside a glass orb, a faint glowing haze.
def globe_haze : f32 = 0.3

def media (tm: f32) (p: vec3) : media_sample =
  let od = sd_orb p
  let gd = glyph_dist tm p
  let bolts = lightning_media tm p
  let haze = if od < 0 then globe_haze else 0
  in { cap = f32.min bolts.cap (f32.min (0.3 * f32.abs od + 0.015) (0.3 * gd + 0.01))
     , radiance = scale (orb_glow_density tm od) orb_glow_colour
                  +^ scale (rune_glow_density tm gd) rune_glow_colour
                  +^ scale haze orb_glow_colour
                  +^ bolts.radiance }
