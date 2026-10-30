-- The world the wizard flies over: the flight path, the frame that maps the
-- wizard's local space (where he and his rock stay at the origin, facing +z)
-- into the world, and the terrain below.

import "vec3"
import "hash"

---------------------------------------------------------------------
-- Flight

-- The path is closed-form: x wanders as a sum of sines while z advances at
-- a steady speed, so heading, turn rate and bank are exact at any time.
def forward_speed : f32 = 3
def altitude : f32 = 12 -- world height of the rock's flat top

def path_x (s: f32) : f32 = 14 * f32.sin (0.12 * s) + 6 * f32.sin (0.27 * s + 1)
def path_dx (s: f32) : f32 = 14 * 0.12 * f32.cos (0.12 * s) + 6 * 0.27 * f32.cos (0.27 * s + 1)
def path_ddx (s: f32) : f32 = -14 * 0.0144 * f32.sin (0.12 * s) - 6 * 0.0729 * f32.sin (0.27 * s + 1)

-- heading: radians from world +z towards +x; turn: its rate (rad/s), up to
-- about 0.2; bank: roll about the direction of travel, into the turn.
type flight = {pos: vec3, heading: f32, turn: f32, bank: f32}

def flight_at (s: f32) : flight =
  let dx = path_dx s
  let heading = f32.atan2 dx forward_speed
  let turn = forward_speed * path_ddx s / (dx * dx + forward_speed * forward_speed)
  in {pos = vec (path_x s) altitude (forward_speed * s), heading, turn, bank = clamp (-0.35) 0.35 (1.6 * turn)}

def yaw (a: f32) (v: vec3) : vec3 =
  {x = v.x * f32.cos a + v.z * f32.sin a, y = v.y, z = v.z * f32.cos a - v.x * f32.sin a}

def roll (a: f32) (v: vec3) : vec3 =
  {x = v.x * f32.cos a - v.y * f32.sin a, y = v.x * f32.sin a + v.y * f32.cos a, z = v.z}

-- Local -> world. Banking into a turn towards +x tips the local +x side down.
def to_world_dir (f: flight) (d: vec3) : vec3 = yaw f.heading (roll (-f.bank) d)
def to_world (f: flight) (p: vec3) : vec3 = f.pos +^ to_world_dir f p
def to_local_dir (f: flight) (d: vec3) : vec3 = roll f.bank (yaw (-f.heading) d)

-- The moon, fixed in the world.
def moon_world : vec3 = normalise (vec (-0.45) 0.65 0.6)

---------------------------------------------------------------------
-- Noise

-- Value noise in [0, 1], smoothly interpolated.
def vnoise2 (x: f32) (z: f32) : f32 =
  let ix = i32.f32 (f32.floor x)
  let iz = i32.f32 (f32.floor z)
  let fx = fract x
  let fz = fract z
  let ux = fx * fx * (3 - 2 * fx)
  let uz = fz * fz * (3 - 2 * fz)
  let a = hash2 ix iz
  let b = hash2 (ix + 1) iz
  let c = hash2 ix (iz + 1)
  let d = hash2 (ix + 1) (iz + 1)
  in a + (b - a) * ux + (c - a) * uz + (a - b - c + d) * ux * uz

def vnoise3 (p: vec3) : f32 =
  let i = vmap f32.floor p
  let f = p -^ i
  let u = vmap (\t -> t * t * (3 - 2 * t)) f
  let h (dx: i32) (dy: i32) (dz: i32) = hash3 (i32.f32 i.x + dx) (i32.f32 i.y + dy) (i32.f32 i.z + dz)
  let lerp a b t = a + (b - a) * t
  in lerp (lerp (lerp (h 0 0 0) (h 1 0 0) u.x) (lerp (h 0 1 0) (h 1 1 0) u.x) u.y)
          (lerp (lerp (h 0 0 1) (h 1 0 1) u.x) (lerp (h 0 1 1) (h 1 1 1) u.x) u.y) u.z

-- Fractal sum of `octaves` octaves of 2D value noise, rotating each octave
-- to hide the lattice; result roughly in [0, 1].
def fbm2 (octaves: i32) (x: f32) (z: f32) : f32 =
  let (sum, norm, _, _, _) =
    loop (sum, norm, amp, x, z) = (0f32, 0f32, 0.5f32, x, z) for _i < octaves do
      ( sum + amp * vnoise2 x z, norm + amp, amp * 0.5
      , 1.6 * x - 1.2 * z + 17.1, 1.2 * x + 1.6 * z - 9.3 )
  in sum / norm

-- Fractal 3D value noise, roughly in [0, 1].
def fbm3 (octaves: i32) (p: vec3) : f32 =
  let (sum, norm, _, _) =
    loop (sum, norm, amp, q) = (0f32, 0f32, 0.5f32, p) for _i < octaves do
      (sum + amp * vnoise3 q, norm + amp, amp * 0.5,
       {x = 1.6 * q.x - 1.2 * q.z + 7.1, y = 2 * q.y + 3.3, z = 1.2 * q.x + 1.6 * q.z - 4.9})
  in sum / norm

---------------------------------------------------------------------
-- Clouds: sparse wisps in a slab of world heights just below the rock,
-- drifting on the wind.

def cloud_bottom : f32 = 7.5
def cloud_top : f32 = 10
def wind : vec3 = vec 0.6 0 (-0.9)

-- Density (extinction per unit length) at a world point.
def cloud_density (secs: f32) (w: vec3) : f32 =
  let h = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
  in if h <= 0 || h >= 1 then 0 else
  let q = scale 0.11 (w -^ scale secs wind)
  let n = fbm3 4 q
  -- thinner towards the slab's top and bottom; mostly clear sky
  let cover = n - 0.66 + 0.14 * (4 * h * (1 - h))
  in 1.2 * f32.max 0 cover

-- Vertical distance from a world height to the slab (0 inside).
def cloud_gap (y: f32) : f32 = f32.max (cloud_bottom - y) (y - cloud_top) |> f32.max 0

---------------------------------------------------------------------
-- Terrain: rolling hills with ridged mountains, in world coordinates.

def terrain_max : f32 = 11 -- 9 + 6 - 4: no height exceeds this

def terrain_height (x: f32) (z: f32) : f32 =
  let n = fbm2 5 (x * 0.035) (z * 0.035)
  let ridge = 1 - f32.abs (2 * fbm2 3 (x * 0.02 + 5.2) (z * 0.02 - 1.7) - 1)
  in 9 * n * n + 6 * ridge * ridge * ridge - 4

-- A heightfield is not a distance; its slopes stay below about 1.3, so
-- scaling the height difference keeps sphere tracing safe.
def terrain_dist (w: vec3) : f32 = 0.6 * (w.y - terrain_height w.x w.z)
