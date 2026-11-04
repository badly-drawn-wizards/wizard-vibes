-- Materials, sky and lighting for the wizard scene.

import "vec3"
import "params"
import "config"
import "sdf"
import "hash"
import "anim"
import "march"
import "scene"
import "lightning"
import "media"
import "world"
import "textures"
import "glass"


---------------------------------------------------------------------
-- Surface detail

-- What the sky depends on: projected out of the pose, so that sky_colour,
-- a real GPU function (#[noinline]; it is called from many places), takes a
-- few scalars rather than the whole pose's.
type sky_in = {horizon: vec3, zenith: vec3, moon: vec3, moon_size: f32, moon_halo: f32, moon_brightness: f32,
               star_density: f32, star_brightness: f32, twinkle: f32, secs: f32}

def sky_in_of (pose: pose) : sky_in =
  { horizon = pose.prm.sky_horizon, zenith = pose.prm.sky_zenith, moon = pose.moon_world
  , moon_size = pose.prm.moon_size, moon_halo = pose.prm.moon_halo, moon_brightness = pose.prm.moon_brightness
  , star_density = pose.prm.star_density, star_brightness = pose.prm.star_brightness
  , twinkle = pose.prm.twinkle, secs = pose.secs }

-- The night sky in a world direction: a gradient, the moon, and stars.
#[noinline]
def sky_colour (sk: sky_in) (rd: vec3) : vec3 =
  let up = saturate (rd.y * 0.5 + 0.5)
  let base = mix sk.horizon sk.zenith (f32.sqrt up)
  -- The moon: a disc of angular radius moon_size, with darker maria and
  -- limb darkening, in a soft halo.
  let m = sk.moon
  let r = sk.moon_size
  let md = dot rd m
  let ang = f32.acos (clamp (-1) 1 md) -- angle from the moon's centre
  let rr = ang / r -- 0 at the centre, 1 at the limb
  let moon =
    if rr > 1.05 then 0 else
    let e1 = normalise (cross m (vec 0 1 0))
    let e2 = cross e1 m
    let u = dot rd e1 / r
    let v = dot rd e2 / r
    let maria = smoothstep 0.5 0.65 (0.6 * vnoise2 (u * 1.8 + 3.1) (v * 1.8 - 1.7) + 0.4 * vnoise2 (u * 4.3) (v * 4.3 + 5))
    let craters = smoothstep 0.72 0.8 (vnoise2 (u * 9 + 11) (v * 9 - 7))
    let limb = f32.sqrt (f32.max 0 (1 - rr * rr)) ** 0.4
    in smoothstep 1 0.96 rr * (0.55 + 0.45 * limb) * (1 - 0.45 * maria) * (1 - 0.2 * craters)
  let halo = 0.15 * f32.exp (-(1 - md) * 60) + sk.moon_halo * f32.exp (-f32.max 0 (ang - r) / 0.06)
  -- Stars: a few cells of a direction grid each hold a small round star at a
  -- random point (kept off the cell's edges), twinkling at its own rate.
  let q = scale 220 rd
  let c = vmap f32.floor q
  let (ix, iy, iz) = (i32.f32 c.x, i32.f32 c.y, i32.f32 c.z)
  let s = hash3 ix iy iz
  let star =
    let thr = 1 - sk.star_density
    in if s <= thr || rd.y <= 0.02 then 0 else
    let at = c +^ vec (0.2 + 0.6 * hash3 ix iy (iz + 71)) (0.2 + 0.6 * hash3 (ix + 13) iy iz) (0.2 + 0.6 * hash3 ix (iy + 29) iz)
    let d = length (q -^ at) / 0.22 -- in units of the star's size (~1.3 pixels at 1080p)
    let rate = 2 + 5 * hash3 (ix + 5) (iy + 3) iz
    let twinkle = 1 - sk.twinkle + sk.twinkle * f32.sin (sk.secs * rate + 6.28 * hash3 ix (iy + 7) (iz + 3))
    in (s - thr) / 0.0025 * sk.star_brightness * 0.0025 * f32.exp (-(d * d)) * twinkle * saturate (rd.y * 4) * smoothstep 1.2 2 rr
  in base +^ scale (moon * sk.moon_brightness + halo) (vec 0.85 0.9 1) +^ splat star

def sky (pose: pose) (rd: vec3) : vec3 = sky_colour (sky_in_of pose) rd

---------------------------------------------------------------------
-- Materials

-- Metals tint their highlights with their albedo and have little diffuse.
-- sheen: a cloth-like glow at grazing angles, tinted by the albedo.
type material = {albedo: vec3, emission: vec3, ks: f32, shininess: f32, rim: bool, metal: bool, sheen: f32}

def plain (albedo: vec3) : material =
  {albedo, emission = splat 0, ks = 0.08, shininess = 8, rim = true, metal = false, sheen = 0}

-- How far the sky shows through at distance t: the fog, and towards the view
-- distance (where rays stop) a fade into the sky, so there is no edge.
def fog_amount (fog: f32) (view_distance: f32) (t: f32) : f32 =
  f32.max (1 - f32.exp (-fog * t * t)) (1 - view_fade_at view_distance t)

def fog_at (prm: params) (t: f32) : f32 = fog_amount prm.fog prm.view_distance t

-- The frame of the structure at local point p: (kind, origin, yaw). (It
-- looks up the ground there, so work it out once per pixel.)
def structure_frame (pose: pose) (p: vec3) : (i32, vec3, f32) =
  let (_, kind, o, yaw) = structure_at (land_of pose.prm) pose.prm.structure_density (to_world pose.fl p)
  in (kind, o, yaw)

-- Local point p in that structure's own frame.
def structure_q (pose: pose) (o: vec3) (yaw: f32) (p: vec3) : vec3 = yaw_inv yaw (to_world pose.fl p -^ o)

-- Stone bricks in running bond, `size` high and twice as long, on a wall
-- with coordinates (u along it, v up): (mortar: 1 in a joint .. 0 on a
-- brick, faded out as the pixel footprint fp nears the brick size; a random
-- value per brick).
def bricks (size: f32) (fp: f32) (u: f32) (v: f32) : (f32, f32) =
  let row = f32.floor (v / size)
  let x = u / (2 * size) + 0.5 * (row - 2 * f32.floor (row / 2))
  let dx = (0.5 - f32.abs (fract x - 0.5)) * 2 * size
  let dy = (0.5 - f32.abs (fract (v / size) - 0.5)) * size
  let joint = 0.07 * size
  let mortar = smoothstep joint (0.5 * joint) (f32.min dx dy) * keep fp size
  in (mortar, hash2 (i32.f32 (f32.floor x)) (i32.f32 row))

-- Wall coordinates on a cottage (along whichever face is nearest) or round
-- a tower (whole bricks round it, so no seam).
def wall_uv (size: f32) (kind: i32) (q: vec3) : (f32, f32) =
  if kind == kind_tower
  then let around = f32.max 1 (f32.round (f32.pi / size)) -- bricks round the tower
       in (f32.atan2 q.z q.x / (2 * f32.pi) * around * 2 * size, q.y)
  else if f32.abs q.x / 1.8 > f32.abs q.z / 1.3 then (q.z, q.y) else (q.x, q.y)

def is_brick (m: i32) : bool = m == m_wall || m == m_tower

-- The bricks' relief: the mortar sits back in the joints.
def brick_height (pose: pose) (fp: f32) (kind: i32) (o: vec3) (yaw: f32) (p: vec3) : f32 =
  let size = pose.prm.brick_size
  let (u, v) = wall_uv size kind (structure_q pose o yaw p)
  in -0.06 * size * (bricks size fp u v).0

-- Brick colours from a base colour: each brick a little different, the
-- mortar greyer and darker.
def brick_colour (size: f32) (fp: f32) (kind: i32) (q: vec3) (base: vec3) : vec3 =
  let (u, v) = wall_uv size kind q
  let (mortar, r) = bricks size fp u v
  let brick = scale (0.75 + 0.4 * r) base
  in mix brick (scale 0.6 (mix base (splat 0.5) 0.5)) mortar

-- What the structures' materials depend on, projected out of the pose (see
-- sky_in).
type structure_in = {fl: flight, land: land, density: f32, brick_size: f32, wall: vec3, thatch: vec3, tower: vec3,
                     slate: vec3, sandstone: vec3, window: vec3, window_glow: f32}

-- The structures' materials: stone bricks with lit windows on cottages and
-- towers.
#[noinline]
def structure_shade (si: structure_in) (fp: f32) (m: i32) (p: vec3) : material =
  let w = to_world si.fl p
  let (_, kind, o, yaw) = structure_at si.land si.density w
  let q = yaw_inv yaw (w -^ o)
  let grain = 0.85 + 0.3 * vnoise2 (w.x * 3 + w.z) (w.y * 3)
  let lit (window: bool) = if window then scale si.window_glow si.window else splat 0
  in if m == m_wall then
       -- windows on the long walls (two each) and the gable ends (one each),
       -- and a door on the front
       let long = f32.abs q.z > 1.25 && f32.abs (f32.abs q.x - 0.9) < 0.22 && f32.abs (q.y - 1.25) < 0.28
       let gable = f32.abs q.x > 1.75 && f32.abs q.z < 0.22 && f32.abs (q.y - 1.25) < 0.28
       let door = q.z > 1.25 && f32.abs q.x < 0.3 && q.y < 1.5
       let window = (long || gable) && kind == kind_cottage
       let c = if door then vec 0.18 0.1 0.05 else brick_colour si.brick_size fp kind q (scale grain si.wall)
       in plain (if window then splat 0.05 else c) with emission = lit window
     else if m == m_thatch then plain (scale (0.7 + 0.5 * vnoise2 (q.x * 9) (q.y * 20)) si.thatch)
     else if m == m_tower then
       -- four narrow lit windows round the tower, on two floors
       let a = f32.atan2 q.z q.x / (2 * f32.pi) * 4
       let slot = f32.abs (fract a - 0.5) < 0.06
       let floor = (q.y > 2.3 && q.y < 3) || (q.y > 4 && q.y < 4.6)
       let window = slot && floor
       in plain (if window then splat 0.05 else brick_colour si.brick_size fp kind q (scale grain si.tower))
          with emission = lit window
     else if m == m_slate then plain si.slate with ks = 0.3 with shininess = 30
     else plain (scale grain si.sandstone)

def structure_material (pose: pose) (fp: f32) (m: i32) (p: vec3) : material =
  let prm = pose.prm
  in structure_shade { fl = pose.fl, land = land_of prm, density = prm.structure_density, brick_size = prm.brick_size
                     , wall = wall_colour, thatch = thatch_colour, tower = tower_colour, slate = slate_colour
                     , sandstone = sandstone_colour, window = prm.window_colour, window_glow = prm.window_glow }
                     fp m p

-- What the water depends on, projected out of the pose (see sky_in).
type water_in = {sky: sky_in, fl: flight, moon: vec3, pix: f32, colour: vec3, fog: f32, view_distance: f32}

-- The water: rippled (the ripples fade with the pixel footprint), dark below,
-- reflecting the sky by Fresnel, with a glint of the moon.
#[noinline]
def water_shade (wi: water_in) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  let w = to_world wi.fl p
  let s = wi.sky.secs
  let k = keep (t * wi.pix) 0.6
  let (_, ax, az) = vnoise2_grad (w.x * 1.3 + s * 0.3) (w.z * 1.3 - s * 0.2)
  let (_, bx, bz) = vnoise2_grad (w.x * 3.1 - s * 0.5) (w.z * 3.1 + s * 0.4)
  let gx = k * (0.04 * ax * 1.3 + 0.015 * bx * 3.1)
  let gz = k * (0.04 * az * 1.3 + 0.015 * bz * 3.1)
  let n = to_local_dir wi.fl (normalise (vec (-gx) 1 (-gz)))
  let cosi = saturate (-(dot n rd))
  let f = 0.02 + 0.98 * pow (1 - cosi) 5
  let r = reflect rd n
  let glint = scale (1.5 * pow (saturate (dot r wi.moon)) 400) (vec 0.85 0.9 1)
  let c = mix wi.colour (sky_colour wi.sky (to_world_dir wi.fl r)) f +^ glint
  let fog = fog_amount wi.fog wi.view_distance t
  in mix c (sky_colour wi.sky (to_world_dir wi.fl rd)) fog

def water_surface (pose: pose) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  water_shade { sky = sky_in_of pose, fl = pose.fl, moon = pose.moon, pix = pose.pix, colour = pose.prm.water_colour
              , fog = pose.prm.fog, view_distance = pose.prm.view_distance } p rd t

-- A platform's surface (see platform.fut): m is m_ground (its top),
-- m_rock, m_ice or m_carpet. Its colours are passed in (each enemy has its
-- own); the circle's runes are added by the caller. (rock: dark, light and
-- top colours, as rock_dark etc.)
def platform_surface (kind: i32) (rock: (vec3, vec3, vec3)) (moss: vec3) (carpet: vec3) (fp: f32) (m: i32) (p: vec3) : material =
  if m == m_ground then
    if kind == platform_ice then plain (frost_albedo ice_colour fp p) with ks = 0.5 with shininess = 30 with rim = false
    else if kind == platform_carpet then plain (carpet_albedo carpet carpet_trim fp p) with ks = 0.02 with sheen = 0.5 with rim = false
    else let (edge, id) = flagstone p
         let grain = 0.8 + 0.35 * vnoise3 (scale 9 p) * keep fp 0.1
         let stone = scale ((0.72 + 0.45 * id) * grain) rock.2
         let moss_part = smoothstep 0.03 0.006 edge * keep fp 0.06 * (0.4 + 0.6 * vnoise3 (scale 5 p))
         in plain (mix stone (scale 0.8 moss) moss_part) with ks = 0.3 with shininess = 40 with rim = false
  else if m == m_ice then
    -- glossy, with a faint glow from within
    plain (ice_albedo ice_colour fp p) with ks = 1.2 with shininess = 90 with emission = scale 0.03 ice_colour
  else if m == m_carpet then plain (carpet_albedo carpet carpet_trim fp p) with ks = 0.02 with sheen = 0.5 with rim = false
  else
    -- red-orange sandstone, banded and mottled (strongly saturated, since
    -- the blue moonlight mutes warm colours)
    let band = 0.5 + 0.5 * f32.sin (p.y * 9 + 2 * vnoise3 (scale 1.5 p))
    let c = mix rock.0 rock.1 band
    in plain (scale (0.6 + 0.7 * vnoise3 (scale 4 p)) c) with ks = 0.1

-- (inlined into its one caller, surface: as a GPU function of its own, the
-- whole pose it takes came to more than WGSL's 255 parameters)
def material (pose: pose) (_g: glow) (fp: f32) (m: i32) (p: vec3) (n: vec3) : material =
  let tm = pose.tm in
  if m == m_robe then plain (cloth_albedo pose p) with rim = false with sheen = pose.prm.sheen with ks = 0.03
  else if m == m_staff then plain (bark_albedo pose.prm fp (staff_rest pose p)) with ks = 0.05
  else if m == m_skin then plain pose.prm.skin_colour with ks = 0.15 with shininess = 16
  else if m == m_beard then plain (hair_albedo pose p) with ks = 0
  else if m == m_hat then
    let s = (hat_stars pose fp p).0
    in plain (mix pose.prm.hat_colour pose.prm.star_colour s) with emission = scale (pose.prm.star_glow * s) pose.prm.star_colour
  else if m == m_gold then plain gold_colour with ks = 1.6 with shininess = 48 with metal = true
  else if m == m_band then plain band_colour with ks = 1.6 with shininess = 48 with metal = true
  else if m == m_eye then plain (eye_albedo pose p) with ks = 1 with shininess = 150 with rim = false
  else if m == m_brow then plain brow_colour
  else if m == m_wood then
    plain (scale (0.75 + 0.25 * f32.sin (p.y * 45 + 3 * f32.sin (p.x * 40))) (vec 0.36 0.21 0.1))
  else if m == m_rock || m == m_ice || m == m_carpet then
    platform_surface pose.prm.platform (pose.prm.rock_dark, pose.prm.rock_light, pose.prm.rock_top) pose.prm.moss_colour carpet_colour fp m p
  else if m == m_terrain then
    -- moss in the valleys, bare rock on steep slopes, snow on high flats;
    -- sand in the desert, and in the mountains more rock and snow lower down
    -- (moss: broad drier and lusher patches, and up close the grass's
    -- blades; stone: strata, warped; far off, where trees would stand, the
    -- forest's canopy as clumps of leaves)
    let w = to_world pose.fl p
    let up = (to_world_dir pose.fl n).y
    let (wd, wa) = biome w.x w.z
    let grain = 0.75 + 0.5 * vnoise2 (w.x * 1.7) (w.z * 1.7)
    let dry = smoothstep 0.45 0.85 (vnoise2 (w.x * 0.11 + 3.1) (w.z * 0.11 - 7.7))
    let moss = mix pose.prm.moss_colour (pose.prm.moss_colour *^ vec 1.7 1.4 0.75) dry
    let moss = if fp < 0.014 then scale (0.75 + 0.6 * grass_blades fp w) moss else scale 1.05 moss
    let strata = 0.5 + 0.5 * f32.sin (w.y * 5 + 3 * vnoise2 (w.x * 0.3) (w.z * 0.3))
    let stone = scale (0.7 + 0.45 * strata * keep fp 0.4) pose.prm.stone_colour
    let snow = pose.prm.snow_colour
    let c = mix stone moss (smoothstep (0.7 + 0.12 * wa) (0.85 + 0.08 * wa) up)
    let canopy = (if draw_trees then smoothstep tree_near tree_far (length p) else 1)
                 * pose.prm.tree_density * (1 - wd) * grove w.x w.z
                 * smoothstep (tree_line + 0.5) (tree_line - 0.5) w.y * smoothstep (pose.prm.sea_level + 0.4) (pose.prm.sea_level + 0.9) w.y
    let clumps = vnoise2 (w.x * 0.45) (w.z * 0.45)
    let leaves = if canopy > 0 then scale (0.6 + 0.8 * vnoise2 (w.x * 1.9) (w.z * 1.9)) leaf_colour else leaf_colour
    let c = mix c leaves (canopy * smoothstep 0.3 0.55 clumps * smoothstep 0.6 0.8 up)
    let c = mix c pose.prm.sand_colour (wd * smoothstep 0.5 0.75 up)
    let snow_line = pose.prm.snow_line - 0.5 * wa
    let c = mix c snow (smoothstep snow_line (snow_line + 1.5) w.y * smoothstep (0.6 + 0.12 * wa) (0.8 + 0.1 * wa) up)
    in plain (scale grain c) with rim = false
  else if m >= m_wall && m <= m_sandstone then structure_material pose fp m p
  else if m == m_leather then plain (leather_albedo pose fp p) with ks = 0.35 with shininess = 28
  else if m == m_cork then plain (cork_albedo fp p) with ks = 0.05
  else if m == m_tome then plain tome_colour with ks = 0.15 with shininess = 16
  else if m == m_pages then plain (vec 0.85 0.8 0.66)
  else if m >= m_potion && m < m_potion + 3 then
    -- the glowing liquid (its glass is drawn by `glass`)
    let c = glow_colour pose.prm m
    in plain (scale 0.3 c) with emission = scale 0.8 c with ks = 0.6 with shininess = 60
  else if m == m_ground then
    platform_surface pose.prm.platform (pose.prm.rock_dark, pose.prm.rock_light, pose.prm.rock_top) pose.prm.moss_colour carpet_colour fp m p
       with emission = scale (0.9 * pose.runes * rune_pulse pose.prm.rune_pulse tm * rune_paint tm p) pose.prm.rune_colour
  else plain (splat 0.5)

-- The trees' materials, at world point w. (Apart from `material`, whose
-- GPU function would otherwise pass more than WGSL's 255 parameters.)
def is_tree (m: i32) : bool = m == m_trunk || m == m_leaves || m == m_needles

#[noinline]
def tree_material (trunk: vec3) (leaf: vec3) (needle: vec3) (m: i32) (w: vec3) : material =
  if m == m_trunk
  then plain (scale (0.7 + 0.6 * vnoise2 (w.y * 9) (f32.atan2 w.z w.x * 3)) trunk) with ks = 0.02
  else
    -- clumps of lighter and darker leaves, a little sheen on the lit side
    let v = 0.6 * vnoise3 (scale 3 w) + 0.4 * vnoise3 (scale 11 w)
    in plain (scale (0.55 + 0.9 * v) (if m == m_leaves then leaf else needle)) with ks = 0.12 with shininess = 12 with rim = false

-- The enemies' materials (see enemies.fut), at point p of his space.
def is_enemy (m: i32) : bool =
  (m >= m_enemy && m < m_enemy + enemy_parts * max_enemies)
  || (full_enemies && m >= m_enemy_full && m < m_enemy_full + full_parts * max_enemies)

-- An enemy's part (in its space, q), before what happens to it.
def enemy_surface (lk: looks) (sheen: f32) (glow: f32) (tm: f32) (rock: (vec3, vec3, vec3)) (moss: vec3) (fp: f32) (part: i32) (q: vec3) : material =
  if part == part_robe then plain lk.robe with rim = false with sheen = sheen with ks = 0.03
  else if part == part_skin then plain lk.skin with ks = 0.15 with shininess = 16
  else if part == part_beard then plain lk.beard with ks = 0.05
  else if part == part_hat then plain lk.hat with ks = 0.05
  else if part == part_band then plain (vec 1 0.6 0.14) with ks = 1.6 with shininess = 48 with metal = true
  else if part == part_staff then plain (scale (0.75 + 0.25 * f32.sin (q.y * 40)) (vec 0.3 0.19 0.1)) with ks = 0.05
  else if part == part_orb then plain (scale 0.3 lk.circle) with emission = scale 2.5 lk.circle with ks = 1 with shininess = 80
  else let side = if lk.kind == platform_ice then m_ice else if lk.kind == platform_carpet then m_carpet else m_rock
       let mat = platform_surface lk.kind rock moss lk.carpet fp (if part == part_top then m_ground else side) q
       -- (the circle turns the other way to his; without his glowing
       -- media, a halo round its strokes is painted on instead)
       in if part != part_top then mat
          else let halo = f32.exp (-(f32.max 0 (glyph_sdf (-tm) q.x q.z)) / 0.05)
               in mat with emission = scale ((0.25 + 0.06 * glow) * (5 * rune_paint (-tm) q + halo)) lk.circle

-- (with full_enemies, the parts of his figure are shaded as his are, in
-- the enemy's pose)
#[noinline]
def enemy_material (prm: params) (g: glow) (fs: foes) (secs: f32) (tm: f32) (fl: flight) (fp: f32) (m: i32) (p: vec3) (n: vec3) : material =
  let full = m >= m_enemy_full
  let i = if full then (m - m_enemy_full) / full_parts else (m - m_enemy) / enemy_parts
  let e = enemy_at prm.enemy_seed fs i secs fl
  let lk = looks_of prm.enemy_seed e.gen i
  let q = enemy_space fl e p
  let mat =
    if full_enemies && full
    then let n = yaw_inv e.yaw (to_world_dir fl n)
         in material (enemy_pose prm lk secs) g fp ((m - m_enemy_full) % full_parts) q n
    else enemy_surface lk prm.sheen prm.rune_glow tm (prm.rock_dark, prm.rock_light, prm.rock_top) prm.moss_colour
                       fp ((m - m_enemy) % enemy_parts) q
  -- appearing, it flashes in its circle's colour; defeated, it chars and
  -- smoulders as it falls, its circle going out
  in if e.appear > 0 then mat with emission = mat.emission +^ scale (2 * e.appear * e.appear) lk.circle
     else if e.dying < 0 then mat
     else let burnt = smoothstep 0 0.6 e.dying
              let ember = smoothstep 0.62 0.85 (vnoise3 (scale 16 q +^ vec 0 (-4 * secs) 0))
          in mat with albedo = scale (1 - 0.85 * burnt) mat.albedo
                 with emission = scale (1 - burnt) mat.emission
                                 +^ scale (0.9 * burnt * smoothstep fall_time 0.5 e.dying * ember) (vec 1 0.35 0.08)

---------------------------------------------------------------------
-- Lighting

-- Diffuse plus Blinn specular from one light in direction l. The colours
-- carry the light's intensity (including any attenuation).
def light (mat: material) (n: vec3) (rd: vec3) (l: vec3) (shadow: f32) (diffuse: vec3) (specular: vec3) : vec3 =
  let lit = saturate (dot n l) * shadow
  let spec = mat.ks * pow (saturate (dot n (normalise (l -^ rd)))) mat.shininess * lit
  in if mat.metal then scale (0.25 * lit) (mat.albedo *^ diffuse) +^ scale spec (mat.albedo *^ specular)
     else scale lit (mat.albedo *^ diffuse) +^ scale spec specular

---------------------------------------------------------------------
-- Lights

-- A light: directional (pos is then its direction) or a point light whose
-- intensity falls off as 1 / (1 + falloff d^2), and smoothly to nothing at
-- `range` (cheap lights only reach nearby surfaces). Point lights of some
-- size stop their shadow rays short of it. Which layers it involves:
--   casters             block it (local layers only: shadow rays are only
--                       traced through the sphere around the wizard);
--   receivers           are lit by it;
--   shadowed_receivers  of those, take its shadows.
type light = { pos: vec3, directional: bool
             , diffuse: vec3, specular: vec3
             , falloff: f32, range: f32, radius: f32
             , casters: i32, receivers: i32, shadowed_receivers: i32 }

-- The moon, the orb, and a weak light in each potion.
def n_lights : i32 = 5

#[noinline] -- (few call sites: the whole pose is passed)
def light_at (pose: pose) (g: glow) (k: i32) : light =
  if k == 0 then
    { pos = pose.moon, directional = true
    , diffuse = scale pose.prm.moon_strength pose.prm.moon_colour
    , specular = scale pose.prm.moon_strength (vec 0.7 0.75 1)
    , falloff = 0, range = f32.inf, radius = 0
    , casters = local_layers, receivers = layers_all, shadowed_receivers = layers_all }
  else if k == 1 then
    -- The orb's light flickers with the lightning, whitening in flashes.
    -- Its shadow rays stop short of it: the staff pokes into its underside,
    -- and a penumbra estimate taken right next to the light bands badly.
    -- The staff holds the orb on its tip, so its shadow rays towards the
    -- orb run along its own surface and graze every swelling and knot; a
    -- branch cannot really shadow a light at its own tip, so the staff
    -- layer takes no shadows from it.
    let c = scale (pose.prm.orb_light * g.level) (mix (vec 0.6 0.5 1) (vec 0.9 0.95 1) (0.6 * g.flash) *^ pose.bolt_tint)
    in { pos = pose.orb, directional = false, diffuse = c, specular = c
       , falloff = 1.5, range = f32.inf, radius = orb_radius
       , casters = local_layers, receivers = layers_all, shadowed_receivers = layers_all & !layers_staff }
  else
    -- The potions: weak, unshadowed, reaching about half a unit and only
    -- lighting the wizard's things, each with a slight flicker of its own
    -- (whole multiples of the loop).
    let i = k - 2
    let flicker = 1 + 0.12 * f32.sin (f32.i32 (7 + 3 * i) * pose.tm + f32.i32 i * 2.1)
    let c = scale (pose.prm.potion_light * flicker) (glow_colour pose.prm (m_potion + i))
    in { pos = from_body_space pose (potion_centre (gear_of pose.prm) i), directional = false, diffuse = c, specular = scale 0.3 c
       , falloff = 1 / (0.12 * 0.12), range = 0.5, radius = 0.04
       , casters = 0, receivers = figure_layers, shadowed_receivers = 0 }

-- The lit colour of a surface hit at distance t along rd (fog included);
-- m is the hit's tagged material (with its layer).
#[noinline] -- (few call sites: the whole pose is passed)
def surface (pose: pose) (g: glow) (tagged: i32) (p: vec3) (rd: vec3) (t: f32) : vec3 =
  let layer = hit_layer tagged
  let m = hit_material tagged
  let vis = pose.prm.layers -- only visible layers occlude, or show in AO
  -- (the enemies are far from everything but themselves: leaving them out
  -- spares the normal and AO probes elsewhere a loop over them)
  let vis = if layer == layers_enemies then vis else vis & !layers_enemies
  in if m == m_water then water_surface pose p rd t else
  -- The terrain's normal comes from its height gradient, worked out
  -- analytically (one evaluation instead of four of the whole scene), and it
  -- gets no AO: at its scale the probes see nothing.
  let n = if m == m_terrain
          then let w = to_world pose.fl p
               in to_local_dir pose.fl (terrain_normal (land_of pose.prm) (terrain_detail p) w.x w.z)
          else normal (dist vis pose) p
  -- textures tilt the normal used for lighting; shadows and AO keep the
  -- geometric one
  let fp = t * pose.pix
  let ng = n
  let n = if has_bump m then bumped (bump_height pose fp m) p ng
          else if is_brick m && keep fp pose.prm.brick_size > 0
          then let (kind, o, yaw) = structure_frame pose p
               in bumped (brick_height pose fp kind o yaw) p ng
          else ng
  let mat = if is_tree m
            then tree_material trunk_colour leaf_colour needle_colour m (to_world pose.fl p)
            else if is_enemy m
            then enemy_material pose.prm g pose.foes pose.secs pose.tm pose.fl fp m p n
            else material pose g fp m p n
  let occ = if m == m_terrain then 1 else ambient_occlusion (dist vis pose) p ng
  let po = p +^ scale 0.002 ng
  -- (shadow rays that pass well clear of the figure and rock are fully lit)
  let shadow (casters: i32) (l: vec3) (tmax: f32) =
    let casters = casters & vis & local_layers
    in if casters == 0 || shadow_misses occluder_centre occluder_radius po l tmax then 1
    -- (and they end shortly after leaving that sphere, and start where
    -- they come near it: nothing lies outside it, and the shadow loop's
    -- steps are capped at 0.25, so marching the rest of the way only
    -- crawled through empty space)
    else let tmax = f32.min tmax (sphere_exit occluder_centre occluder_radius po l + 1)
         in soft_shadow (\tol q -> shadow_scene casters pose tol q) po l
                        (shadow_start occluder_centre occluder_radius po l tmax) tmax
  -- All the lights, in one loop so the lighting and shadow code is
  -- compiled once. The moon's shadow is kept for the cloth sheen.
  let (lit, moon_shadow) =
    loop (lit, moon_shadow) = (splat 0, 0f32) for k < n_lights do
      let L = light_at pose g k
      let (l, d) = if L.directional then (L.pos, f32.inf)
                   else let v = L.pos -^ p
                        let d = length v
                        in (scale (1 / d) v, d)
      -- (an endless range does not fade: smoothstep of infinities would be
      -- NaN, which GPUs' min and max resolve differently)
      let atten = if L.directional then 1
                  else (if f32.isinf L.range then 1 else smoothstep L.range (0.6 * L.range) d) / (1 + L.falloff * d * d)
      -- (skipped where the light is negligible, e.g. the orb on the far terrain)
      in if atten < 0.01 || !(has L.receivers layer) then (lit, moon_shadow) else
      -- (nor where the light is behind the surface: the light is then
      -- nothing, and so is what the moon's shadow is kept for, the sheen
      -- and hair lit round to 0.4 past the terminator)
      let facing = dot n l > (if k == 0 then -0.4 else 0)
      let sh = if !(has L.shadowed_receivers layer) || d < L.radius || !facing then 1
               else shadow L.casters l (if L.directional then 12 else d - L.radius - 0.15)
      let c = light mat n rd l sh (scale atten L.diffuse) (scale atten L.specular)
      in (lit +^ c, if k == 0 then sh else moon_shadow)
  let to_orb = pose.orb -^ p
  let od = length to_orb
  let ol = scale (1 / od) to_orb
  let fill = scale (occ * (0.5 + 0.5 * n.y) * pose.prm.fill) (mat.albedo *^ vec 0.2 0.22 0.45)
  let rim = if mat.rim then scale (occ * pow (1 - saturate (-(dot n rd))) 3 * pose.prm.rim) (vec 0.35 0.3 0.8)
            else splat 0
  -- Metals mirror their surroundings: the sky, and the glowing orb where
  -- the reflected ray points at it (it is right next to the gold cradle).
  let mirror =
    if !mat.metal then splat 0
    else let r = reflect rd n
         let cos_orb = dot r ol
         let disc = f32.cos (f32.asin (f32.min 1 (orb_radius / od)))
         let orb_seen = smoothstep (disc - 0.05) disc cos_orb + 0.25 * pow (saturate cos_orb) 6
         in mat.albedo *^ (scale 0.6 (sky pose (to_world_dir pose.fl r)) +^ scale (1.6 * g.level * orb_seen) (vec 0.75 0.9 1))
  -- cloth sheen: brightest where the surface turns away from view, lit by
  -- the moon where it reaches and a little by the sky everywhere
  let sheen =
    if mat.sheen <= 0 then splat 0
    else let f = pow (1 - saturate (-(dot n rd))) 4
         let lit = 0.3 * occ + 0.7 * moon_shadow * saturate (dot n pose.moon + 0.4)
         in scale (mat.sheen * f * lit) (scale 2.2 mat.albedo +^ vec 0.05 0.05 0.1)
  -- hair: a Kajiya-Kay highlight, a band running across the strands
  let hair_spec =
    if m != m_beard then splat 0
    else let hh = normalise (pose.moon -^ rd)
         let th = dot hair_tangent hh
         let k = pow (f32.sqrt (f32.max 0 (1 - th * th))) 60
         in scale (0.35 * k * moon_shadow * saturate (dot n pose.moon + 0.2)) (vec 0.75 0.8 1)
  let c = lit +^ fill +^ rim +^ mirror +^ sheen +^ hair_spec +^ mat.emission
  let fog = fog_at pose.prm t
  in match pose.prm.view
     case 1 -> scale 0.5 (n +^ splat 1)
     case 2 -> mat.albedo
     case 3 -> splat occ
     case _ -> mix c (sky pose (to_world_dir pose.fl rd)) fog

---------------------------------------------------------------------
-- The glass orb

-- (whether the orb reflects, and its index of refraction, are uniforms:
-- orb_reflections and orb_ior in params.json)
def glass_tint : vec3 = vec 0.93 0.97 1

-- Distances (entry, exit) where the ray meets the orb; entry > exit on a miss.
def orb_hit (pose: pose) (ro: vec3) (rd: vec3) : (f32, f32) = ray_sphere pose.orb orb_radius ro rd

-- What a glass surface at p, met as fr describes, reflects towards the
-- eye: the sky, and (by `shine`) highlights of the moon and the orb.
def glass_reflection (pose: pose) (g: glow) (shine: f32) (p: vec3) (fr: interface) : vec3 =
  let rr = fr.reflected
  let to_orb = normalise (pose.orb -^ p)
  let highlights = scale (2 * shine * pow (saturate (dot rr pose.moon)) 300) (vec 0.8 0.85 1)
                   +^ scale (2.5 * shine * g.level * pow (saturate (dot rr to_orb)) 120) (vec 0.7 0.85 1)
  in scale fr.f (sky pose (to_world_dir pose.fl rr)) +^ highlights

def potion_ior : f32 = 1.5

-- The potions' glass, a thin wall rays pass through. Finds where the ray
-- (up to t_end) first meets it and returns what the wall adds there (its
-- reflection and a faint rim) and what it lets through (what neither of
-- its faces reflects, slightly tinted). Only rays passing near the potions
-- are traced.
#[noinline] -- (few call sites: the whole pose is passed)
def glass (pose: pose) (g: glow) (ro: vec3) (rd: vec3) (t_end: f32) : (vec3, vec3) =
  -- the potions' bounding sphere, in local space (body space leans)
  let c = from_body_space pose (potions_at (gear_of pose.prm))
  let (enter, exit) = ray_sphere c (potions_radius + 0.02) ro rd
  in if enter > exit then (splat 0, splat 1) else
  let (hit, t, n) = glass_hit (potion_glass pose) 64 ro rd (f32.max 0.02 enter) (f32.min t_end exit)
  in if !hit then (splat 0, splat 1) else
  let p = ro +^ scale t rd
  let fr = fresnel_at potion_ior rd n
  let cosi = saturate (-(dot fr.n rd))
  let added = glass_reflection pose g 1 p fr +^ scale (0.08 * pow (1 - cosi) 2) (vec 0.6 0.7 0.9)
  in (added, scale ((1 - fr.f) * (1 - fr.f)) (vec 0.9 0.95 0.97))

---------------------------------------------------------------------
-- The shield: a bubble round him, raised from the harness (pose.shield,
-- 0 down .. 1 up; web/shield.ts). Glass-like (each face reflects by
-- Fresnel), it glows at its rim and in a faint honeycomb, and flares and
-- ripples each time it stops a spell (pose.shield_hit: the seconds since).

def shield_centre : vec3 = vec 0 0.85 0
def shield_radius : f32 = 1.85
def shield_ior : f32 = 1.33

def shield_sdf (pose: pose) (p: vec3) : f32 =
  let q = p -^ shield_centre
  let r = shield_radius * (0.25 + 0.75 * smoothstep 0 1 pose.shield)
  -- (the ripple's slope stays under 0.25, so the field stays safe to trace)
  let ripple = 0.02 * f32.exp (-4 * pose.shield_hit) * f32.sin (12 * q.y - 40 * pose.shield_hit)
  in length q - r - ripple

-- Distance to the edges of a grid of hexagons (apothem 0.5, one across
-- per unit of u).
def hex_edge (u: f32) (v: f32) : f32 =
  let m (a: f32) (b: f32) = a - b * f32.floor (a / b)
  let h = 0.8660254 -- (rows are sqrt 3 apart)
  let (ax, ay) = (m u 1 - 0.5, m v (2 * h) - h)
  let (bx, by) = (m (u - 0.5) 1 - 0.5, m (v - h) (2 * h) - h)
  let (gx, gy) = if ax * ax + ay * ay < bx * bx + by * by then (ax, ay) else (bx, by)
  in 0.5 - f32.max (0.5 * f32.abs gx + h * f32.abs gy) (f32.abs gx)

#[noinline] -- (few call sites: the whole pose is passed)
def shield_layer (pose: pose) (g: glow) (ro: vec3) (rd: vec3) (t_end: f32) : (vec3, vec3) =
  let (enter, exit) = ray_sphere shield_centre (shield_radius + 0.05) ro rd
  in if pose.shield <= 0.005 || enter > exit || exit < 0 then (splat 0, splat 1) else
  let sdf = shield_sdf pose
  let t1 = f32.min t_end exit
  -- what one face adds: its reflection, and its glow
  let face (t: f32) (n: vec3) =
    let p = ro +^ scale t rd
    let fr = fresnel_at shield_ior rd n
    let q = normalise (p -^ shield_centre)
    -- (30 cells round, so the grid meets itself)
    let k = 30 / (2 * f32.pi)
    let cells = smoothstep 0.05 0 (hex_edge (k * f32.atan2 q.z q.x) (k * f32.asin q.y)) * keep (t * pose.pix) 0.2
    let rim = pow (1 - saturate (-(dot fr.n rd))) 3
    let flare = 2 * f32.exp (-5 * pose.shield_hit)
    let glow = scale (pose.shield * (0.02 + 0.6 * rim + 0.09 * cells + flare * (0.15 + rim))) shield_colour
    -- (an energy skin, not glass: sharp highlights of the moon and the
    -- orb, which is inside, only made blots on it)
    in (glass_reflection pose g 0.1 p fr +^ glow, 1 - fr.f)
  -- the near face, then the far one seen through it (a thin skin: each
  -- face reflects, neither bends the ray)
  let (front, tf, nf) = glass_hit sdf 32 ro rd (f32.max 0.02 enter) t1
  in if !front then (splat 0, splat 1) else
  let (a1, k1) = face tf nf
  let (back, tb, nb) = glass_hit sdf 32 ro rd (tf + 0.02) t1
  let (a2, k2) = if back then face tb nb else (splat 0, 1)
  in (a1 +^ scale k1 a2, splat (k1 * k2))

-- Where a reflected path starts: (exists, point, direction, weight).
type reflection = (bool, vec3, vec3, f32)

-- Follows a ray through at most three straight segments - outside, inside
-- the orb, outside again - refracting at the glass, and returns the colour
-- it gathers, weighted by w. Also returns the reflection spawned where the
-- ray first enters the orb, and how far the ray sees before anything opaque
-- or the orb's glass (infinity if nothing).
#[noinline] -- (few call sites: the whole pose is passed)
def follow (pose: pose) (g: glow) (ro0: vec3) (rd0: vec3) (w0: f32) : (vec3, reflection, f32) =
  let (_, _, _, acc, _, _, refl, first) =
    loop (ro, rd, w, acc, inside, done, refl, first) =
      (ro0, rd0, splat w0, splat 0, false, false, (false, ro0, rd0, 0f32), f32.inf)
    for seg < 3 do
      if done then (ro, rd, w, acc, inside, done, refl, first) else
      let vis = pose.prm.layers
      -- (a hidden orb is no glass at all)
      let (t_in, t_out) = if has vis layers_orb then orb_hit pose ro rd else (f32.inf, -f32.inf)
      let enters = !inside && t_in > 1e-3 && t_in < t_out
      let tmax = if inside then t_out else if enters then t_in else pose.prm.view_distance
      -- Two passes: the terrain alone over the whole segment, then the scene
      -- near the wizard up to the terrain hit or the edge of the sphere that
      -- holds it (the peaks reach into that sphere, so the terrain goes
      -- first). Neither pass evaluates the other's geometry at every step.
      let (t_terrain, m_land) = if inside || enters || !(has vis land_layers) then (f32.inf, 0) else march_terrain vis pose ro rd tmax
      -- the enemies, in front of the land (a pass of their own, which most
      -- rays skip: see march_enemies)
      let (t_terrain, m_land) =
        if inside || enters || !(has vis layers_enemies) || pose.prm.enemies <= 0 then (t_terrain, m_land)
        else let (te, me) = march_enemies pose.prm pose.foes pose.secs pose.fl ro rd (f32.min tmax t_terrain)
             in if te < t_terrain then (te, me) else (t_terrain, m_land)
      let local_end = f32.max 0 (ray_sphere occluder_centre local_radius ro rd).1
      let local_max = f32.min tmax (f32.min t_terrain local_end)
      let (t, m, emitted, tr) = march (local_scene vis pose) (media pose g) ro rd local_max
      let (t, m) = if m < 0 && t_terrain < tmax then (t_terrain, m_land) else (t, m)
      -- the glows are above the cloud slab, so in front of the clouds
      let (cloud_rad, cloud_tr) = if has vis layers_clouds then clouds pose ro rd (if m >= 0 then t else tmax)
                                  else (splat 0, 1)
      -- the potions' glass, in front of whatever this segment reaches
      let (glass_added, glass_through) =
        if inside || !(has vis layers_gear) then (splat 0, splat 1) else glass pose g ro rd (if m >= 0 then t else local_max)
      let acc = acc +^ (w *^ glass_added)
      let w = w *^ glass_through
      -- his shield, if raised (outside the orb, the potions and him)
      let (shield_added, shield_through) =
        if inside then (splat 0, splat 1) else shield_layer pose g ro rd (if m >= 0 then t else tmax)
      let acc = acc +^ (w *^ shield_added)
      let w = w *^ shield_through
      let first = if seg > 0 then first else if m >= 0 then t else if enters then t_in else f32.inf
      let acc = acc +^ (w *^ emitted) +^ (scale tr w *^ cloud_rad)
      -- whatever lies beyond this segment is seen through its media
      let w = scale (tr * cloud_tr) w
      in if m >= 0 then
           (ro, rd, w, acc +^ (w *^ surface pose g m (ro +^ scale t rd) rd t), inside, true, refl, first)
         else if enters && t >= t_in then
           -- into the glass
           let p = ro +^ scale t_in rd
           let n = normalise (p -^ pose.orb)
           let fr = fresnel_at pose.prm.orb_ior rd n
           let refl = if refl.0 then refl else (true, p, fr.reflected, w.x * fr.f)
           in (p, fr.refracted, scale (1 - fr.f) (w *^ glass_tint), acc, true, false, refl, first)
         else if inside && t >= t_out then
           -- out of the glass
           let p = ro +^ scale t_out rd
           let n = normalise (p -^ pose.orb)
           in (p, (fresnel_at (1 / pose.prm.orb_ior) rd n).refracted, w *^ glass_tint, acc, false, false, refl, first)
         else (ro, rd, w, acc +^ (w *^ sky pose (to_world_dir pose.fl rd)), inside, true, refl, first)
  in (acc, refl, first)

-- The colour seen along a camera ray, and how far it sees (see follow).
-- (#[noinline]: render calls it from more than one kernel when it
-- anti-aliases, which would otherwise each hold a copy of all the shading.)
#[noinline]
def shade (prm: params) (dr: drive) (secs: f32) (pix: f32) (ro: vec3) (rd: vec3) : (vec3, f32) =
  -- worked out once per pixel, not per march step
  let pose = pose_at secs prm dr with pix = pix
  let g = glow_now pose.tm
  -- Path 0 is the camera ray; path 1 its reflection off the orb, if any.
  -- A loop, so that follow (and all the shading) is compiled only once.
  let paths = if prm.orb_reflections then 2 else 1
  let (c, _, first) =
    loop (c, refl, first) = (splat 0, (false, ro, rd, 0f32), f32.inf) for k < paths do
      if k == 1 && !refl.0 then (c, refl, first) else
      let (o, d, w) = if k == 0 then (ro, rd, 1) else (refl.1, refl.2, refl.3)
      let (col, r, f) = follow pose g o d w
      in (c +^ col, if k == 0 then r else refl, if k == 0 then f else first)
  in (c, first)
