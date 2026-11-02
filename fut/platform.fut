-- What a wizard flies on, and the magic circle on it: a floating rock, a
-- shard of ice or a magic carpet (platform_rock, platform_ice,
-- platform_carpet; the "platform" parameter picks his, and each enemy has
-- one of its own). Each is modelled with its flat top at y = 0, where the
-- wizard stands, facing +z, inside the sphere platform_centre, radius
-- platform_radius. Their tops are m_ground, the circle's material, whatever
-- the platform (see platform_top in shading.fut); their sides m_rock,
-- m_ice or m_carpet.

import "vec3"
import "sdf"
import "hash"
import "world"
open import "materials"

def platform_rock : i32 = 0
def platform_ice : i32 = 1
def platform_carpet : i32 = 2

def platform_centre : vec3 = vec 0 (-1.2) 0
def platform_radius : f32 = 2.3

---------------------------------------------------------------------
-- Rune circle: 14 glyphs on a circle of radius 1.125, turning once per loop.

def rune_radius : f32 = 1.125

-- 2D distance in the ground plane to the nearest rune stroke.
#[noinline]
def glyph_sdf (tm: f32) (x: f32) (z: f32) : f32 =
  let c = rune_radius
  let r = length2 (vec2 x z)
  -- Far from the band, a cheap lower bound. It is only used where the glow
  -- density is negligible, so the bound's looseness never shows.
  in if f32.abs (r - c) > 0.45 then f32.abs (r - c) - 0.12
  else
    let n = 14f32
    let sector = 2 * f32.pi / n
    let q = rot2 tm (vec2 x z)
    let a = f32.atan2 q.y q.x + f32.pi
    let idx = i32.f32 (f32.floor (a / sector))
    let lp = rep_angle n {x = q.x, y = 0, z = q.y}
    let h1 = hash2 idx 3
    let h2 = hash2 idx 7
    let seg (ax: f32) (az: f32) (bx: f32) (bz: f32) =
      sd_capsule lp (vec (c + ax) 0 az) (vec (c + bx) 0 bz) 0.008
    let g1 = seg (-0.08) (-0.05 + 0.1 * h1) 0.08 (0.05 - 0.1 * h1)
    let g2 = seg (-0.08 + 0.16 * h2) (-0.06) (0.08 - 0.16 * h1) 0.06
    let g3 = if h2 > 0.5 then seg 0 (-0.07) 0 0.07 else sd_torus (lp -^ vec c 0 0) 0.05 0.004
    in f32.min g1 (f32.min g2 g3)

-- Approximate 3D distance to the nearest letter: the in-plane distance and
-- the height above the ground, combined by Pythagoras.
def glyph_dist (tm: f32) (p: vec3) : f32 =
  if p.y > 0.35 then p.y -- high above the ground: skip the strokes
  else let d = f32.max 0 (glyph_sdf tm p.x p.z)
       in f32.sqrt (d * d + p.y * p.y)

-- Emission painted on the ground (before brightness): two rings and the glyphs.
#[noinline]
def rune_paint (tm: f32) (p: vec3) : f32 =
  let r = length2 (vec2 p.x p.z)
  let ring w c = 1 - smoothstep (w * 0.5) w (f32.abs (r - c))
  let rings = f32.max (ring 0.018 0.95) (ring 0.012 1.3)
  let glyph = 1 - smoothstep 0.0 0.008 (glyph_sdf tm p.x p.z)
  in f32.max rings glyph

---------------------------------------------------------------------
-- The floating rock: an inverted cone with a flat top (the magic circle's
-- outer ring, at radius 1.3, fits inside), roughened below by noise that
-- fades out towards the rim.

def rock_radius : f32 = 1.6
def rock_depth : f32 = 2.8

def rock_exact (p: vec3) : hit =
  let cone = sd_capped_cone (p -^ vec 0 (-rock_depth) 0) rock_depth 0.3 rock_radius
  -- (the weight is 0 at and above the top face, where the wizard is, so the
  -- noise is only evaluated below it)
  let rough = if p.y >= 0 then 0
              else saturate (-p.y * 2.5) * (0.22 * vnoise3 (scale 2.2 p) + 0.1 * vnoise3 (scale 5.3 p) - 0.16)
  -- the noise steepens the field; scale it back to keep tracing safe
  in (0.6 * (cone + rough), if p.y > -0.03 then m_ground else m_rock)

---------------------------------------------------------------------
-- The shard of ice: a seven-sided pyramid, point down, with three smaller
-- ones hanging beside its point and a crown of crystals round its rim.

-- A pyramid of n faces, apothem r at its base (y = 0) and its point h
-- below (h < 0: above), as the most a point lies outside any face (a lower
-- bound on the distance, exact off the edges).
def sd_pyramid (n: f32) (r: f32) (h: f32) (p: vec3) : f32 =
  let q = rep_angle n p
  let l = f32.sqrt (h * h + r * r)
  let face = (f32.abs h * q.x - r * (if h > 0 then q.y else -q.y) - f32.abs h * r) / l
  in f32.max face (if h > 0 then q.y else -q.y)

def ice_exact (p: vec3) : hit =
  let body = sd_pyramid 7 1.55 2.7 p
  -- the hanging shards, round the point
  let hang (k: i32) =
    let a = f32.i32 k * 2.1 + 0.4
    in sd_pyramid 5 (0.42 - 0.05 * f32.i32 k) (1.3 + 0.25 * f32.i32 k)
                  (p -^ vec (0.7 * f32.cos a) (-0.8 - 0.15 * f32.i32 k) (0.7 * f32.sin a))
  let below = f32.min (hang 0) (f32.min (hang 1) (hang 2))
  -- the crown: five crystals on the rim, of different heights
  let pr = vec (p.x * 0.96 - p.z * 0.28) p.y (p.z * 0.96 + p.x * 0.28)
  let c = rep_angle 5 pr
  let sector = i32.f32 (f32.floor (f32.atan2 pr.z pr.x / (2 * f32.pi) * 5 + 0.5))
  let tall = 0.25 + 0.3 * hash2 sector 41
  let crown = sd_pyramid 4 0.13 (-tall) (c -^ vec 1.4 (-0.05) 0)
  let d = f32.min body (f32.min below crown)
  in (0.9 * d, if p.y > -0.03 && crown > 0.01 then m_ground else m_ice)

---------------------------------------------------------------------
-- The magic carpet: a rug 2.3 by 3.2, rippling more towards its ends (it
-- is flat under his feet), with tassels along both ends.

def carpet_half : vec3 = vec 1.15 0.025 1.6

-- How far the carpet's surface is lifted at (x, z).
def carpet_lift (secs: f32) (x: f32) (z: f32) : f32 =
  0.07 * f32.sin (3 * z - 5 * secs + 0.8 * x) * smoothstep 0.5 1.6 (f32.abs z)
  + 0.03 * f32.sin (4 * x + 3 * secs) * smoothstep 0.5 1.15 (f32.abs x)

def carpet_exact (secs: f32) (p: vec3) : hit =
  let lift = carpet_lift secs p.x p.z
  let rug = sd_box (p -^ vec 0 (lift - carpet_half.y - 0.008) 0) carpet_half - 0.008
  -- the tassels: a short cord every 0.115 along each end
  let cell = 0.115
  let cx = clamp (-1.1) 1.1 (cell * f32.round (p.x / cell))
  let end_lift = carpet_lift secs cx (if p.z < 0 then -carpet_half.z else carpet_half.z)
  let az = f32.abs p.z
  let tassel = sd_capsule (vec p.x p.y az) (vec cx (end_lift - 0.03) (carpet_half.z - 0.02))
                          (vec cx (end_lift - 0.06) (carpet_half.z + 0.13)) 0.013
  -- (the ripple tilts the field by up to about 0.35, so it is scaled back)
  let d = f32.min (0.75 * rug) tassel
  in (d, if az < carpet_half.z && p.y > lift - 0.012 then m_ground else m_carpet)

---------------------------------------------------------------------

-- A platform of the given kind, at time secs.
def platform_exact (kind: i32) (secs: f32) (p: vec3) : hit =
  if kind == platform_ice then ice_exact p
  else if kind == platform_carpet then carpet_exact secs p
  else rock_exact p

#[noinline]
def platform (kind: i32) (secs: f32) (p: vec3) : hit =
  sphere_bounded_hit platform_centre platform_radius 0.3 m_rock (platform_exact kind secs) p
