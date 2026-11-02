-- Collision probing for free flight (see web/flight.ts). The rock and the
-- wizard are covered by a few spheres; for each, the probe gives the land's
-- distance from its surface and the direction out of the land, so the harness
-- can push the wizard clear. It is a separate entry point with a tiny output,
-- which the harness copies out and reads back without stalling the frame.

import "vec3"
import "params"
import "world"
import "scene"

-- (centre in the wizard's frame, radius): the platform's top, middle and
-- tip (the carpet has only a top: its others shrink into the first), the
-- wizard's body and head, and the orb.
def collision_spheres (kind: i32) : [6](vec3, f32) =
  let carpet = kind == platform_carpet
  in [ (vec 0 (-0.5) 0, rock_radius)
     , if carpet then (vec 0 (-0.5) 0, 0) else (vec 0 (-1.7) 0, 0.95)
     , if carpet then (vec 0 (-0.5) 0, 0) else (vec 0 (-2.6) 0, 0.45)
     , (vec 0 1 0, 0.5)
     , (vec 0 2 0, 0.4)
     , (orb_pos, 0.2) ]

-- The land at a world point, in the visible layers: (distance, outward unit
-- normal). The ground's distance is its height above the terrain scaled by
-- the slope (exact for a plane); the water is solid.
def land_probe (prm: params) (w: vec3) : (f32, vec3) =
  let l = land_of prm
  let ground =
    if !(has prm.layers layers_terrain) then (f32.inf, vec 0 1 0)
    else let (h, hx, hz) = terrain_height_grad l 5 w.x w.z
         let n = vec (-hx) 1 (-hz)
         in ((w.y - h) / length n, normalise n)
  let water = if has prm.layers layers_water then (w.y - prm.sea_level, vec 0 1 0) else (f32.inf, vec 0 1 0)
  let built =
    if !(has prm.layers layers_structures) then (f32.inf, vec 0 1 0)
    -- (structures gives only a lower bound away from them, so use the exact
    -- distance to this cell's structure: any in another cell is further
    -- than the largest sphere's radius)
    else let (present, kind, o, yaw) = structure_at l prm.structure_density w
         let f (p: vec3) = if present then (sd_structure kind (yaw_inv yaw (p -^ o))).0 else f32.inf
         let d = f w
         let e = 0.01
         let n = vec (f (w +^ vec e 0 0) - f (w -^ vec e 0 0))
                     (f (w +^ vec 0 e 0) - f (w -^ vec 0 e 0))
                     (f (w +^ vec 0 0 e) - f (w -^ vec 0 0 e))
         in (d, if length n > 0 then normalise n else vec 0 1 0)
  let nearer (a: (f32, vec3)) (b: (f32, vec3)) = if a.0 <= b.0 then a else b
  in nearer ground (nearer water built)

-- Per sphere: [distance from its surface to the land (negative: inside),
-- normal x, y, z], in world space; then the same for each world point in
-- pts (x, y, z each; the spells, see web/spells.ts), as from a sphere of
-- radius 0.
def probe_frame [k] (prm: params) (dr: drive) (secs: f32) (pts: [k]f32) : []f32 =
  let fl = flight_of prm dr secs
  let spheres = map (\(c, r) -> (to_world fl c, r)) (collision_spheres prm.platform)
  let points = tabulate (k / 3) (\i -> (vec pts[3 * i] pts[3 * i + 1] pts[3 * i + 2], 0f32))
  in flatten (map (\(w, r) ->
                    let (d, n) = land_probe prm w
                    in [f32.min 1e6 d - r, n.x, n.y, n.z])
                  (spheres ++ points))
