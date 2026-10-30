-- The whole scene: the wizard (from figure.fut, posed by rig.fut), the rune
-- circle and rock he stands on, and the land far below; the scene
-- functions the marches use, by layer. Re-exports materials, rig and figure,
-- so importing this gives all of it.

import "vec3"
import "params"
import "config"
import "sdf"
import "hash"
import "anim"
import "world"
import "march"
open import "materials"
open import "rig"
open import "figure"

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
-- The floating rock the wizard stands on: an inverted cone with a flat top
-- (the magic circle's outer ring, at radius 1.3, fits inside), roughened
-- below by noise that fades out towards the rim.

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

-- (the rock fits in a sphere of radius 2.3; its noise needs a wider margin)
def rock (p: vec3) : hit = sphere_bounded_hit (vec 0 (-1.2) 0) 2.3 0.3 m_rock rock_exact p |> tag layers_rock

-- The terrain far below, sampled through the flight frame. No peak is above
-- terrain_max, so well above it the height over terrain_max is a cheap lower
-- bound (it stays >= 1.5 here, so it can never register as a hit).
--
-- Terrain detail (octaves) is deliberately not reduced with distance. A
-- distance-based fade is centred on the wizard, who moves through the world,
-- so the ground changed shape as he flew past it: the terrain visibly swam.
-- Measured against a high-quality reference over consecutive flight frames,
-- the fade caused about three times the flicker of the cloud sampling, and
-- pushing it out to 30-60 units only halved that while costing the same as
-- no fade at all (17.2 ms at 1080p either way).
def terrain_detail (_p: vec3) : f32 = 5

def land_of (prm: params) : land = {sea = prm.sea_level, river = prm.river_width}

-- The world below, in the layers of `mask`: the ground, the water (sea and
-- rivers, a plane at sea level) and the structures.
def land_layers : i32 = layers_terrain | layers_water | layers_structures

def terrain (mask: i32) (pose: pose) (p: vec3) : hit =
  let prm = pose.prm
  let l = land_of prm
  let w = to_world pose.fl p
  let detail = terrain_detail p
  let ground =
    if !(has mask layers_terrain) then nothing
    else if w.y > terrain_max + 1.5 then (w.y - terrain_max, m_terrain) |> tag layers_terrain
    else
      -- the cheap bound clears most points near the wizard, his rock and
      -- the clouds; only close to the ground is the full terrain needed
      -- (rivers only lower the ground, so it stays an upper bound)
      let gap = w.y - terrain_upper w.x w.z
      in (if gap > 0.4 then 0.6 * gap else terrain_dist l detail w, m_terrain) |> tag layers_terrain
  let water = if has mask layers_water then (w.y - prm.sea_level, m_water) |> tag layers_water else nothing
  let built = if !(has mask layers_structures) then nothing
              else let (d, part) = structures l prm.structure_density w
                   in (d, m_wall + part) |> tag layers_structures
  in ground |> umin water |> umin built

-- #[noinline]: Futhark inlines everything by default, and the scene is used
-- by the march, normals, AO and both shadow loops, which made the shader
-- several megabytes (Firefox's compiler and runtime suffered most). The
-- figure, the scene, the occluders and the media are compiled once as real
-- GPU functions instead. The WebGPU backend's limits here:
-- - such functions may not use arrays;
-- - a GPU function whose name contains a 'z' cannot be called from another
--   GPU function: the call site escapes the name ('zz') but the definition
--   does not, so the call has no target. Hence bounded_figure, not
--   bounded_wizard. Fixed upstream; the pinned compiler predates the fix,
--   so keep the name until flake.lock is updated. See
--   dev/futhark-bugs/z-name-mangling.fut.
-- The scene near the wizard, in the layers of `mask`: the figure, its staff
-- and gear, and the rock. (Primary rays march this and the terrain in
-- separate passes, see follow in shading.fut.)
#[noinline]
def local_scene (mask: i32) (pose: pose) (tol: f32) (p: vec3) : hit =
  bounded_figure mask pose tol p |> umin (if has mask layers_rock then rock p else nothing)

def local_layers : i32 = figure_layers | layers_rock

-- Everything in local_scene lies in this sphere (radius 3.3), and so do the
-- glowing media (the orb's cloud and tendrils, the rune circle; 3.6).
def occluder_centre : vec3 = vec 0 (-0.3) 0
def occluder_radius : f32 = 3.3
def local_radius : f32 = 3.6

#[noinline]
def scene (mask: i32) (pose: pose) (tol: f32) (p: vec3) : hit =
  local_scene mask pose tol p |> umin (if has mask land_layers then terrain mask pose p else nothing)

def dist (mask: i32) (pose: pose) (tol: f32) (p: vec3) : f32 = (scene mask pose tol p).0

-- The terrain pass: sphere-traces the land alone (the layers of `mask` among
-- land_layers) along ro + t rd, for t in [0, tmax], returning the hit's
-- distance and tagged material, or infinity. A ray heading upwards
-- from above the highest peak cannot hit it at all, and one heading down
-- jumps straight to that height before stepping.
#[noinline]
def march_terrain (mask: i32) (pose: pose) (ro: vec3) (rd: vec3) (tmax: f32) : (f32, i32) =
  let wo = to_world pose.fl ro
  let wd = to_world_dir pose.fl rd
  let top = land_max + 1.5
  in if wd.y >= 0 && wo.y > land_max then (f32.inf, 0) else
  let t0 = if wo.y > top then (wo.y - top) / -wd.y else 0
  let (t, _, hit, m) =
    loop (t, i, hit, m) = (f32.max 0.02 t0, 0i32, false, 0i32)
    while !hit && i < terrain_steps && t < tmax do
      let (d, mat) = terrain mask pose (ro +^ scale t rd)
      in if d < 0.0004 * t then (t, i + 1, true, mat) else (t + d * 0.85, i + 1, false, m)
  in if hit then (t, m) else (f32.inf, 0)
