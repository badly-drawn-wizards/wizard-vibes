-- The wizard scene: geometry, material ids, and the glowing media (orb cloud
-- and rune letters) that the march integrates.

import "vec3"
import "sdf"
import "hash"
import "anim"
import "march"

-- Material ids.
def m_robe : i32 = 1
def m_skin : i32 = 2
def m_beard : i32 = 3
def m_hat : i32 = 4
def m_gold : i32 = 5
def m_eye : i32 = 6
def m_wood : i32 = 7
def m_orb : i32 = 8
def m_ground : i32 = 9

---------------------------------------------------------------------
-- Wizard

-- The staff stands just outside the robe's hem and leans forward; the right
-- hand grips it at grip_height and the orb sits on its tip.
def staff_foot : vec3 = vec 0.6 0 0.25
def staff_top : vec3 = vec 0.52 2.0 0.46
def staff_dir : vec3 = normalise (staff_top -^ staff_foot)
def staff_radius : f32 = 0.028
def staff_at (y: f32) : vec3 = staff_foot +^ scale (y / staff_dir.y) staff_dir

def grip_height : f32 = 1.15
def right_elbow : vec3 = vec 0.36 1.07 0.14
-- The fist is a short capsule along the staff, offset towards the elbow so
-- the staff passes through its outer half and shows above and below it.
def fist_centre : vec3 =
  let g = staff_at grip_height
  let towards = right_elbow -^ g
  let off = towards -^ scale (dot towards staff_dir) staff_dir
  in g +^ scale 0.02 (normalise off)

def orb_radius : f32 = 0.11
def orb_pos : vec3 = staff_top +^ scale 0.1 staff_dir

def sd_orb (p: vec3) : f32 = sd_sphere (p -^ orb_pos) orb_radius

def sd_robe (tm: f32) (p: vec3) : f32 =
  let a = f32.atan2 p.z p.x
  let hem = 0.02 * f32.sin (9 * a + 2 * tm) * saturate (1 - p.y / 0.8)
  let cone = sd_capped_cone p 1.35 0.55 0.17 - 0.03 + hem
  let shoulders = sd_ellipsoid (p -^ vec 0 1.3 0) (vec 0.29 0.14 0.2)
  in smin 0.12 cone shoulders

def sd_arms (p: vec3) : f32 =
  let upper = sd_capsule p (vec 0.2 1.33 0) right_elbow 0.075
  let fore = sd_capsule p right_elbow (fist_centre -^ scale 0.05 (normalise (fist_centre -^ right_elbow))) 0.068
  let left = sd_capsule p (vec (-0.2) 1.33 0) (vec (-0.27) 1.02 0.24) 0.075
  in f32.min (smin 0.04 upper fore) left

def sd_hands (p: vec3) : f32 =
  let fist = sd_capsule p (fist_centre -^ scale 0.04 staff_dir) (fist_centre +^ scale 0.04 staff_dir) 0.05
  in f32.min fist (sd_sphere (p -^ vec (-0.27) 0.97 0.29) 0.055)

def sd_head (p: vec3) : f32 =
  let head = sd_sphere (p -^ vec 0 1.6 0.02) 0.17
  let nose = sd_ellipsoid (p -^ vec 0 1.58 0.19) (vec 0.035 0.05 0.05)
  in smin 0.03 head nose

def sd_eyes (p: vec3) : f32 =
  let q = {x = f32.abs p.x, y = p.y, z = p.z}
  in sd_sphere (q -^ vec 0.065 1.64 0.155) 0.022

def sd_beard (tm: f32) (p: vec3) : f32 =
  let sway = 0.03 * f32.sin (2 * tm) * saturate (1.55 - p.y)
  let q = p -^ vec sway 1.54 0.11
  let q = {x = q.x, y = -q.y, z = q.z - 0.15 * q.y}
  let wave = 0.012 * f32.sin (30 * p.y + 10 * p.x)
  in sd_round_cone q 0.13 0.025 0.55 + wave

def hat_base : vec3 = vec 0 1.74 0
def hat_bend (tm: f32) : f32 = 0.9 + 0.15 * f32.sin tm

def hat_height : f32 = 0.72
def hat_radius (y: f32) : f32 = 0.2 - (0.2 - 0.012) * saturate (y / hat_height)

def sd_hat (tm: f32) (p: vec3) : f32 =
  let hp = p -^ hat_base
  let bp = bend (hat_bend tm) hp
  let cone = sd_capped_cone bp hat_height (hat_radius 0) (hat_radius hat_height)
  let brim = f32.min (sd_torus hp 0.3 0.022) (sd_cylinder hp 0.31 0.01)
  in smin 0.04 cone brim

def sd_band (p: vec3) : f32 = sd_torus (p -^ (hat_base +^ vec 0 0.05 0)) 0.186 0.025

def sd_staff (p: vec3) : f32 = sd_capsule p staff_foot staff_top staff_radius

def wizard (tm: f32) (p: vec3) : hit =
  (smin 0.06 (sd_robe tm p) (sd_arms p), m_robe)
  |> umin (sd_hands p, m_skin)
  |> umin (sd_head p, m_skin)
  |> umin (sd_eyes p, m_eye)
  |> umin (sd_beard tm p, m_beard)
  |> umin (sd_hat tm p, m_hat)
  |> umin (sd_band p, m_gold)
  |> umin (sd_staff p, m_wood)
  |> umin (sd_orb p, m_orb)

---------------------------------------------------------------------
-- Rune circle: 14 glyphs on a circle of radius 1.125, turning once per loop.

def rune_radius : f32 = 1.125

-- 2D distance in the ground plane to the nearest rune stroke.
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

-- Emission painted on the ground: two rings and the glyphs.
def rune_paint (tm: f32) (p: vec3) : f32 =
  let r = length2 (vec2 p.x p.z)
  let ring w c = 1 - smoothstep (w * 0.5) w (f32.abs (r - c))
  let rings = f32.max (ring 0.018 0.95) (ring 0.012 1.3)
  let glyph = 1 - smoothstep 0.0 0.008 (glyph_sdf tm p.x p.z)
  in f32.max rings glyph * pulse tm

---------------------------------------------------------------------
-- Whole scene

-- Every wizard part lies inside this capsule, so its distance is a lower
-- bound on the wizard's. Beyond the requested tolerance, return the bound and
-- skip evaluating the parts (see the distance contract in march.fut).
def wizard_bound (p: vec3) : f32 = sd_capsule p (vec 0 0.3 0) (vec 0 2.3 0) 0.95

def scene (tm: f32) (tol: f32) (p: vec3) : hit =
  let b = wizard_bound p
  let w = if b > tol then (b, m_robe) else wizard tm p
  in w |> umin (p.y, m_ground)

def dist (tm: f32) (tol: f32) (p: vec3) : f32 = (scene tm tol p).0

---------------------------------------------------------------------
-- Glowing media

def orb_glow_colour : vec3 = vec 0.35 0.55 1
def rune_glow_colour : vec3 = vec 0.25 0.5 1

-- A cloud around the orb whose density falls off exponentially with
-- distance from the orb's surface; its strength and extent breathe.
def orb_glow_density (tm: f32) (od: f32) : f32 =
  let s = f32.sin (3 * tm)
  let extent = 0.1 + 0.012 * s
  in (0.9 + 0.2 * s) * f32.exp (-(f32.max od 0) / extent)

-- The rune letters glow the same way, in a thin layer hugging the strokes.
def rune_glow_density (tm: f32) (gd: f32) : f32 =
  4 * pulse tm * f32.exp (-gd / 0.05)

def media (tm: f32) (p: vec3) : media_sample =
  let od = sd_orb p
  let gd = glyph_dist tm p
  in { cap = f32.min (0.3 * od + 0.015) (0.3 * gd + 0.01)
     , radiance = scale (orb_glow_density tm od) orb_glow_colour
                  +^ scale (rune_glow_density tm gd) rune_glow_colour }
