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

-- The gold cradle holding the orb: a collar on the staff tip and prongs that
-- curve up around the glass with a helical twist. Angles are polar angles
-- from the orb's bottom, along the staff's axis.
def prongs : f32 = 5
def prong_start : f32 = 0.45
def prong_end : f32 = 1.37 -- 78 degrees: 40% of the way up the orb
def prong_twist : f32 = 1.2 -- radians around the orb per radian climbed
def prong_radius : f32 = 0.013

def sd_cradle (p: vec3) : f32 =
  -- The whole cradle fits in a sphere of radius 0.15 around this point; skip
  -- the detail beyond it (exact within 0.15, which march and AO need).
  let bound = length (p -^ (orb_pos -^ scale 0.04 staff_dir)) - 0.15
  in if bound > 0.15 then bound else
  -- a frame around the staff's axis
  let ax = staff_dir
  let e1 = normalise (cross ax (vec 0 0 1))
  let e2 = cross ax e1
  let v = p -^ orb_pos
  let rho = length v
  let theta = f32.acos (clamp (-1) 1 (-(dot v ax) / f32.max rho 1e-6))
  let phi = f32.atan2 (dot v e2) (dot v e1)
  -- prongs: tubes on a sphere just outside the glass, folded into one sector
  let r = orb_radius + 0.6 * prong_radius
  let th = clamp prong_start prong_end theta
  let f = (th - prong_start) / (prong_end - prong_start)
  let sector = 2 * f32.pi / prongs
  let dphi = (fract ((phi - prong_twist * th) / sector + 0.5) - 0.5) * sector
  let radial = rho - r
  let across = r * f32.sin th * dphi
  let along = r * (theta - th)
  -- the twist stretches space by at most this much, so divide it out
  let stretch = f32.sqrt (1 + (prong_twist * f32.sin th) ** 2)
  let prong = (f32.sqrt (radial * radial + across * across + along * along) - prong_radius * (1 - 0.4 * f)) / stretch
  let tip = (f32.sqrt (radial * radial + across * across + (r * (theta - prong_end)) ** 2) - 0.014) / stretch
  -- collar: a ring where the prongs start, on a cup flaring from the staff
  let ring_centre = orb_pos -^ scale (r * f32.cos prong_start) ax
  let ring_r = r * f32.sin prong_start
  let w = p -^ ring_centre
  let h = dot w ax
  let ring = length2 (vec2 (length (w -^ scale h ax) - ring_r) h) - 0.014
  let cup = sd_round_cone_ab p (staff_top -^ scale 0.05 ax) ring_centre staff_radius (0.85 * ring_r)
  in f32.min (smin 0.02 ring cup) (smin 0.01 prong tip)

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

-- The beard hangs from the cheeks to a forked tip resting on the chest.
def beard_top : vec3 = vec 0 1.52 0.11
def beard_tip : vec3 = vec 0 0.98 0.27

-- Beard space: swaying with the loop, and squashed sideways so the beard is
-- wider than deep (dividing x only contracts space, so distances stay safe).
def beard_space (tm: f32) (p: vec3) : vec3 =
  let sway = 0.03 * f32.sin (2 * tm) * saturate ((1.5 - p.y) / 0.5)
  in {x = (p.x - sway) / 1.45, y = p.y, z = p.z}

-- Strand grooves around the beard's axis, as a value in [0, 1] (0 = groove).
-- Shared by the shape and the material, so shading follows the strands.
def beard_strands (q: vec3) : f32 =
  let ba = beard_tip -^ beard_top
  let h = saturate (dot (q -^ beard_top) ba / dot ba ba)
  let rv = q -^ (beard_top +^ scale h ba)
  let phi = f32.atan2 rv.x rv.z
  in f32.abs (f32.sin (10 * phi + 5 * h + 0.6 * f32.sin (9 * h)))

def sd_beard (tm: f32) (p: vec3) : f32 =
  let q = beard_space tm p
  -- fork the lower half into two points
  let fork = 0.035 * smoothstep 1.3 0.98 q.y
  let fq = {x = f32.abs q.x - fork, y = q.y, z = q.z}
  let body = sd_round_cone_ab fq beard_top beard_tip 0.13 0.018
  -- grooves cut in proportion to the local radius, keeping the field smooth
  let ba = beard_tip -^ beard_top
  let h = saturate (dot (q -^ beard_top) ba / dot ba ba)
  let groove = 0.006 * (1 - h * 0.5) * (1 - beard_strands q)
  let jaw = sd_ellipsoid (p -^ vec 0 1.53 0.05) (vec 0.155 0.1 0.13)
  let moustache =
    let m = {x = f32.abs p.x, y = p.y, z = p.z}
    in sd_round_cone_ab m (vec 0.01 1.548 0.178) (vec 0.105 1.49 0.13) 0.024 0.01
  in smin 0.03 (smin 0.06 (body + groove) jaw) moustache

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
  |> umin (sd_cradle p, m_gold)

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

-- Emission painted on the ground (before brightness): two rings and the glyphs.
def rune_paint (tm: f32) (p: vec3) : f32 =
  let r = length2 (vec2 p.x p.z)
  let ring w c = 1 - smoothstep (w * 0.5) w (f32.abs (r - c))
  let rings = f32.max (ring 0.018 0.95) (ring 0.012 1.3)
  let glyph = 1 - smoothstep 0.0 0.008 (glyph_sdf tm p.x p.z)
  in f32.max rings glyph

---------------------------------------------------------------------
-- Whole scene

-- Every wizard part lies inside this capsule, so its distance is a lower
-- bound on the wizard's. Beyond the requested tolerance, return the bound and
-- skip evaluating the parts (see the distance contract in march.fut).
def wizard_bound (p: vec3) : f32 = sd_capsule p (vec 0 0.3 0) (vec 0 2.3 0) 0.95

def bounded_wizard (tm: f32) (tol: f32) (p: vec3) : hit =
  let b = wizard_bound p
  in if b > tol then (b, m_robe) else wizard tm p

-- The orb is glass, refracted analytically by the shading (see shading.fut),
-- so it is not part of the marched scene; what is inside it is drawn by the
-- media (see media.fut and lightning.fut).
def scene (tm: f32) (tol: f32) (p: vec3) : hit = bounded_wizard tm tol p |> umin (p.y, m_ground)

def dist (tm: f32) (tol: f32) (p: vec3) : f32 = (scene tm tol p).0

-- What casts shadows: only the wizard. Both lights are above the ground,
-- which therefore cannot block them (as an occluder it would only add
-- penumbra artefacts at grazing angles), and the orb is a light source
-- itself, which would otherwise darken everything it lights.
def occluder_dist (tm: f32) (tol: f32) (p: vec3) : f32 = (bounded_wizard tm tol p).0
