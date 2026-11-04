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
open import "platform"
open import "enemies"

-- The enemies (see enemies.fut), in the layers of `mask`.
def enemies (mask: i32) (pose: pose) (p: vec3) : hit =
  if !(has mask layers_enemies) || pose.prm.enemies <= 0 then nothing
  else enemies_dist pose.prm pose.foes pose.secs pose.fl p

-- His platform, in the layer of the rock (see platform.fut).
def rock (pose: pose) (p: vec3) : hit = platform pose.prm.platform pose.secs p |> tag layers_rock

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
def land_layers : i32 = layers_terrain | layers_water | layers_structures | layers_trees

-- The ground and the water (a real GPU function taking just the flight and
-- the land, not the whole pose).
#[noinline]
def ground_at (mask: i32) (fl: flight) (l: land) (p: vec3) : hit =
  let w = to_world fl p
  let detail = terrain_detail p
  let ground =
    if !(has mask layers_terrain) then nothing
    else if w.y > terrain_max + 1.5 then (w.y - terrain_max, m_terrain) |> tag layers_terrain
    else
      -- the cheap bound clears most points near the wizard, his rock and
      -- the clouds; only close to the ground is the full terrain needed
      -- (rivers only lower the ground, so it stays an upper bound)
      let (wd, wa) = biome w.x w.z
      -- (the bound's noise is the full terrain's first octaves)
      let o = terrain_first_octaves w.x w.z
      let gap = w.y - terrain_upper_from wd wa o
      in (if gap > 1.5 then terrain_lipschitz wa * gap else terrain_dist_from l wd wa detail o w, m_terrain) |> tag layers_terrain
  let water = if has mask layers_water then (w.y - l.sea, m_water) |> tag layers_water else nothing
  in ground |> umin water

def land_ground (mask: i32) (pose: pose) (p: vec3) : hit = ground_at mask pose.fl (land_of pose.prm) p

def tree_near : f32 = 50
def tree_far : f32 = 75

-- The structures.
def land_structures (mask: i32) (pose: pose) (p: vec3) : hit =
  if !(has mask layers_structures) then nothing
  else let (d, part) = structures (land_of pose.prm) pose.prm.structure_density (to_world pose.fl p)
       in (d, m_wall + part) |> tag layers_structures

-- The same along a ray (see structures_in in world.fut), given the
-- structure of the cell holding p.
def land_structures_in (mask: i32) (pose: pose) (exit: f32) (c: structure_cell) (p: vec3) : hit =
  if !(has mask layers_structures) then nothing
  else let (d, part) = structures_in (land_of pose.prm) exit c (to_world pose.fl p)
       in (d, m_wall + part) |> tag layers_structures

-- The trees. They are drawn near the wizard only, where the camera always
-- is: they shrink away between tree_near and tree_far from him; farther
-- off, forests are a shade of the ground (see shading.fut).
-- (at the High quality preset only: draw_trees)
def land_trees (mask: i32) (pose: pose) (p: vec3) : hit =
  if !draw_trees || !(has mask layers_trees) then nothing
  else let r = length p
       let (d, part) = trees (land_of pose.prm) pose.prm.tree_density
                             (smoothstep tree_far tree_near r) (r - tree_far) (to_world pose.fl p)
       in (d, m_trunk + part) |> tag layers_trees

def no_tree_cell : tree_cell =
  {present = false, kind = 0, px = 0, pz = 0, size = 0, seed = 0, ground = f32.nan}

-- The same along a ray (exit: see structures in world.fut), given the tree
-- cell holding p.
def land_trees_in (mask: i32) (pose: pose) (exit: f32) (c: tree_cell) (p: vec3) : hit =
  if !draw_trees || !(has mask layers_trees) then nothing
  else let r = length p
       let (d, part) = trees_in (land_of pose.prm) pose.prm.tree_density
                                (smoothstep tree_far tree_near r) (r - tree_far) exit c (to_world pose.fl p)
       in (d, m_trunk + part) |> tag layers_trees

def land_built (mask: i32) (pose: pose) (p: vec3) : hit =
  land_structures mask pose p |> umin (land_trees mask pose p)

def terrain (mask: i32) (pose: pose) (p: vec3) : hit =
  land_ground mask pose p |> umin (land_built mask pose p)

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
  bounded_figure mask pose tol p |> umin (if has mask layers_rock then rock pose p else nothing)

def local_layers : i32 = figure_layers | layers_rock

-- What casts the shadows near the wizard: a simpler stand-in for the
-- figure (soft shadows need no fingers, beard locks, potions or tome),
-- each part inside the real one, so no surface starts its shadow ray inside
-- it: the robe and arms, the head as a sphere, the hat, the beard as a thin
-- capsule along its axis, and the staff as a plain rod; and the rock.
#[noinline]
def shadow_scene (mask: i32) (pose: pose) (tol: f32) (p: vec3) : f32 =
  let figure =
    if !(has mask layers_figure) then 1e9
    else let b = wizard_bound p
         in if b > tol then b
            else let hq = head_space pose p
                 in smin 0.06 (sd_robe pose (body_space pose p)) (sd_arms_shadow pose p)
                    |> f32.min (sd_sphere (hq -^ vec 0 1.6 0.02) 0.17)
                    |> f32.min (sd_hat pose p)
                    |> f32.min (sd_capsule hq beard_top beard_tip 0.05)
  let staff = if !(has mask layers_staff) then 1e9
              else sd_capsule (staff_rest pose p) staff_foot staff_top staff_radius
  let rock = if !(has mask layers_rock) then 1e9 else (rock pose p).0
  in f32.min figure (f32.min staff rock)

-- Everything in local_scene lies in this sphere (radius 3.3), and so do the
-- glowing media (the orb's cloud and tendrils, the rune circle; 3.6).
def occluder_centre : vec3 = vec 0 (-0.3) 0
def occluder_radius : f32 = 3.3
def local_radius : f32 = 3.6

#[noinline]
def scene (mask: i32) (pose: pose) (tol: f32) (p: vec3) : hit =
  local_scene mask pose tol p |> umin (if has mask land_layers then terrain mask pose p else nothing)
  |> umin (enemies mask pose p)

def dist (mask: i32) (pose: pose) (tol: f32) (p: vec3) : f32 = (scene mask pose tol p).0

-- The terrain pass: sphere-traces the land alone (the layers of `mask` among
-- land_layers) along ro + t rd, for t in [0, tmax], returning the hit's
-- distance and tagged material, or infinity. A ray heading upwards
-- from above the highest peak cannot hit it at all, and one heading down
-- jumps straight to that height before stepping.
--
-- The structures are rarely the nearest thing, so their distance is not
-- evaluated at every step: a bound taken at t still bounds them at t' > t
-- (less the way travelled, t' - t), so it is only evaluated again once
-- that could cut the step short. Their bounds are taken along the ray:
-- where it meets their tops' heights, and a cell with nothing near holds
-- the ray up only where it leaves the cell (see cell_exit), not wherever
-- the cell's edge is near; and what stands in a cell is worked out once
-- per cell.
--
-- The march is over-relaxed (Keinert et al. 2014, "Enhanced Sphere
-- Tracing"): each step goes terrain_relax times the safe distance, and
-- whenever the new point's bound shows that went too far (the two bounding
-- spheres do not overlap), it steps back and carries on unrelaxed.
#[noinline]
def terrain_relax : f32 = 1.4

-- Along a ray from height y going up at dy (a unit direction's), how far it
-- is to the height `top` (0 if not above it, inf if never reaching it).
def ray_to_plane (y: f32) (dy: f32) (top: f32) : f32 =
  if y <= top then 0 else if dy >= 0 then f32.inf else (y - top) / -dy

-- Along a ray from p along the unit d, how far it is into the sphere of
-- radius r round the origin (0 from inside, inf if it misses).
def ray_sphere_entry (r: f32) (p: vec3) (d: vec3) : f32 =
  let b = dot p d
  let c = dot p p - r * r
  in if c <= 0 then 0
     else let disc = b * b - c
          in if b >= 0 || disc < 0 then f32.inf else -b - f32.sqrt disc

def march_terrain (mask: i32) (pose: pose) (ro: vec3) (rd: vec3) (tmax: f32) : (f32, i32) =
  let wo = to_world pose.fl ro
  let wd = to_world_dir pose.fl rd
  let top = land_max + 1.5
  in if wd.y >= 0 && wo.y > land_max then (f32.inf, 0) else
  let t0 = if wo.y > top then (wo.y - top) / -wd.y else 0
  -- (prev: the safe step from the last point; step: the step taken from it)
  -- (sc, tc: the structure and tree cells last seen, as (x, z, what is
  -- there), each worked out once)
  let (t, _, hit, m, _, _, _, _, _) =
    loop (t, i, hit, m, (sd, st, sm), (td, tt, tm), sc, tc, (omega, prev, step)) =
      (f32.max 0.02 t0, 0i32, false, 0i32, (-1f32, 0f32, 0i32), (-1f32, 0f32, 0i32),
       (i32.lowest, 0i32, (false, 0i32, 0f32, 0f32, 0f32)), (i32.lowest, 0i32, no_tree_cell), (terrain_relax, 0f32, 0f32))
    while !hit && i < terrain_steps && t < tmax do
      let p = ro +^ scale t rd
      let ground = land_ground mask pose p
      -- the structures and the trees, each still bounded by what was found
      -- for it at st (tt), or again
      let w = to_world pose.fl p
      let (sd, st, sm, sc) =
        if sd - (t - st) >= omega * ground.0 then (sd, st, sm, sc) else
        -- (along the ray, how far it is to the roofs' height)
        let over = ray_to_plane w.y wd.y structure_top
        in if over > 0.3 then (over, t, sm, sc) else
        let key = (i32.f32 (f32.floor (w.x / struct_cell)), i32.f32 (f32.floor (w.z / struct_cell)))
        let sc = if (key.0 == sc.0 && key.1 == sc.1) || !(has mask layers_structures) then sc
                 else (key.0, key.1, structure_cell_at pose.prm.structure_density w.x w.z)
        let (d, part) = land_structures_in mask pose (cell_exit struct_cell w wd) sc.2 p
        in (d, t, part, sc)
      let (td, tt, tm, tc) =
        if td - (t - tt) >= omega * ground.0 then (td, tt, tm, tc) else
        -- (along the ray, how far it is to the trees' height and to where
        -- they are drawn round him)
        let over = f32.max (ray_to_plane w.y wd.y tree_top) (ray_sphere_entry tree_far p rd)
        in if over > 0.3 then (over, t, tm, tc) else
        -- (the cell's candidate and its ground, once per cell)
        let key = (i32.f32 (f32.floor (w.x / tree_cell)), i32.f32 (f32.floor (w.z / tree_cell)))
        let tc = if (key.0 == tc.0 && key.1 == tc.1) || !draw_trees || !(has mask layers_trees) then tc
                 else (key.0, key.1, tree_cell_grounded (land_of pose.prm) pose.prm.tree_density w.x w.z)
        let (d, part) = land_trees_in mask pose (cell_exit tree_cell w wd) tc.2 p
        in (d, t, part, tc)
      let (built, bm) = (sd - (t - st), sm) |> umin (td - (t - tt), tm)
      let (d, mat) = ground |> umin (built, bm)
      -- (the relaxation follows the ground alone; the structures and trees,
      -- whose bounds jump from cell to cell, would keep cutting it short,
      -- so they only cap each step at their own safe distance)
      let safe = 0.85 * ground.0
      let next = f32.min (omega * safe) (0.85 * built)
      in if omega > 1 && safe + prev < step
         -- overshot: back to where the last safe step would have ended
         -- (a bound taken here says nothing of the way back: forget it)
         then let back = t - step + prev
              let sd = if st > back then -1 else sd
              let td = if tt > back then -1 else td
              in (back, i + 1, false, m, (sd, st, sm), (td, tt, tm), sc, tc, (1, 0, 0))
         else if d < 0.0015 * t then (t, i + 1, true, mat, (sd, st, sm), (td, tt, tm), sc, tc, (omega, safe, 0))
         else (t + next, i + 1, false, m, (sd, st, sm), (td, tt, tm), sc, tc, (omega, safe, next))
  in if hit then (t, m) else (f32.inf, 0)
