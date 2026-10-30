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
-- to hide the lattice; result in [0, 1]. `detail` (fractional) says how
-- many octaves to actually evaluate: the rest are faded out smoothly to
-- their average, so lowering it removes fine detail without moving the mean.
def fbm2_lod (octaves: i32) (detail: f32) (x: f32) (z: f32) : f32 =
  let (sum, norm, _, _, _) =
    loop (sum, norm, amp, x, z) = (0f32, 0f32, 0.5f32, x, z) for i < octaves do
      let k = saturate (detail - f32.i32 i)
      let v = if k > 0 then mix1 0.5 (vnoise2 x z) k else 0.5
      in ( sum + amp * v, norm + amp, amp * 0.5
         , 1.6 * x - 1.2 * z + 17.1, 1.2 * x + 1.6 * z - 9.3 )
  in sum / norm

-- vnoise2 with its gradient: (value, d/dx, d/dz). The smoothstep weight
-- u = f^2 (3 - 2f) has derivative 6 f (1 - f).
def vnoise2_grad (x: f32) (z: f32) : (f32, f32, f32) =
  let ix = i32.f32 (f32.floor x)
  let iz = i32.f32 (f32.floor z)
  let fx = fract x
  let fz = fract z
  let ux = fx * fx * (3 - 2 * fx)
  let uz = fz * fz * (3 - 2 * fz)
  let dux = 6 * fx * (1 - fx)
  let duz = 6 * fz * (1 - fz)
  let a = hash2 ix iz
  let b = hash2 (ix + 1) iz
  let c = hash2 ix (iz + 1)
  let d = hash2 (ix + 1) (iz + 1)
  let k = a - b - c + d
  in (a + (b - a) * ux + (c - a) * uz + k * ux * uz, dux * (b - a + k * uz), duz * (c - a + k * ux))

-- fbm2_lod with its gradient with respect to the *input* coordinates. Each
-- octave's coordinates are a linear map of the input (the rotation-scale
-- [1.6 -1.2; 1.2 1.6] applied i times), so its gradient is mapped back by
-- that matrix's transpose; the matrix is tracked as (j00, j01, j10, j11).
def fbm2_lod_grad (octaves: i32) (detail: f32) (x: f32) (z: f32) : (f32, f32, f32) =
  let (sum, gx, gz, norm, _, _, _, _, _, _, _) =
    loop (sum, gx, gz, norm, amp, x, z, j00, j01, j10, j11) =
      (0f32, 0f32, 0f32, 0f32, 0.5f32, x, z, 1f32, 0f32, 0f32, 1f32) for i < octaves do
      let k = saturate (detail - f32.i32 i)
      let (v, dx, dz) = if k > 0 then vnoise2_grad x z else (0.5, 0, 0)
      -- faded octave: 0.5 + k (v - 0.5)
      let w = amp * k
      in ( sum + amp * (0.5 + k * (v - 0.5))
         , gx + w * (dx * j00 + dz * j10), gz + w * (dx * j01 + dz * j11)
         , norm + amp, amp * 0.5
         , 1.6 * x - 1.2 * z + 17.1, 1.2 * x + 1.6 * z - 9.3
         , 1.6 * j00 - 1.2 * j10, 1.6 * j01 - 1.2 * j11
         , 1.2 * j00 + 1.6 * j10, 1.2 * j01 + 1.6 * j11 )
  in (sum / norm, gx / norm, gz / norm)

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

-- Density (extinction per unit length) at a world point, and a distance
-- that is safe to step without missing any cloud (0 inside one).
--
-- fbm3 4 = (0.5 o1 + 0.25 o2 + 0.125 o3 + 0.0625 o4) / 0.9375, each octave
-- in [0, 1], so the first octave alone bounds it from above. Where even that
-- bound is below the coverage threshold the sky is clear, and the other
-- octaves are skipped. Octave 1 changes by at most about 1.5 per lattice
-- unit, i.e. 0.165 per world unit, so the margin tells how far is clear.
def cloud_sample (secs: f32) (w: vec3) : (f32, f32) =
  let h = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
  let q = scale 0.11 (w -^ scale secs wind)
  let threshold = 0.66 - 0.14 * (4 * h * (1 - h)) -- thinner at top and bottom
  let o1 = vnoise3 q
  let upper = (0.5 * o1 + 0.4375) / 0.9375
  in if upper < threshold then (0, (threshold - upper) / (0.165 * 0.5 / 0.9375))
     else let n = fbm3 4 q
          in (1.2 * f32.max 0 (n - threshold), 0)

-- Vertical distance from a world height to the slab (0 inside).
def cloud_gap (y: f32) : f32 = f32.max (cloud_bottom - y) (y - cloud_top) |> f32.max 0

---------------------------------------------------------------------
-- Terrain: rolling hills with ridged mountains, in world coordinates.

def terrain_max : f32 = 11 -- 9 + 6 - 4: no height exceeds this

-- `detail` in octaves (5 = full); far-away terrain needs fewer.
def terrain_height_lod (detail: f32) (x: f32) (z: f32) : f32 =
  let n = fbm2_lod 5 detail (x * 0.035) (z * 0.035)
  let ridge = 1 - f32.abs (2 * fbm2_lod 3 (detail - 2) (x * 0.02 + 5.2) (z * 0.02 - 1.7) - 1)
  in 9 * n * n + 6 * ridge * ridge * ridge - 4

def terrain_height (x: f32) (z: f32) : f32 = terrain_height_lod 5 x z

-- terrain_height_lod with its gradient: (height, dh/dx, dh/dz).
-- h = 9 n^2 + 6 r^3 - 4 with r = 1 - |2f - 1|.
def terrain_height_grad (detail: f32) (x: f32) (z: f32) : (f32, f32, f32) =
  let (n, nx, nz) = fbm2_lod_grad 5 detail (x * 0.035) (z * 0.035)
  let (f, fx, fz) = fbm2_lod_grad 3 (detail - 2) (x * 0.02 + 5.2) (z * 0.02 - 1.7)
  let r = 1 - f32.abs (2 * f - 1)
  let dr = -2 * f32.sgn (2 * f - 1) -- dr/df
  let hn = 18 * n -- dh/dn
  let hr = 18 * r * r -- dh/dr
  in ( 9 * n * n + 6 * r * r * r - 4
     , 0.035 * hn * nx + 0.02 * hr * dr * fx
     , 0.035 * hn * nz + 0.02 * hr * dr * fz )

-- The terrain's upward unit normal in world space.
def terrain_normal (detail: f32) (x: f32) (z: f32) : vec3 =
  let (_, hx, hz) = terrain_height_grad detail x z
  in normalise (vec (-hx) 1 (-hz))

-- An upper bound on terrain_height from the first octave of each fbm alone
-- (2 noise lookups instead of 8): the other octaves can add at most their
-- amplitudes. The height grows with n and with the ridge, so bounding those
-- bounds it. (Refining the bound octave by octave was tried and is slower on
-- the GPU: neighbouring pixels stop at different octaves and diverge.)
def terrain_upper (x: f32) (z: f32) : f32 =
  let n_hi = (0.5 * vnoise2 (x * 0.035) (z * 0.035) + 0.46875) / 0.96875
  let f_lo = 0.5 * vnoise2 (x * 0.02 + 5.2) (z * 0.02 - 1.7) / 0.875
  let f_hi = f_lo + 0.375 / 0.875
  let ridge_hi = if f_lo <= 0.5 && f_hi >= 0.5 then 1
                 else 1 - f32.min (f32.abs (2 * f_lo - 1)) (f32.abs (2 * f_hi - 1))
  in 9 * n_hi * n_hi + 6 * ridge_hi * ridge_hi * ridge_hi - 4

-- A heightfield is not a distance; its slopes stay below about 1.3, so
-- scaling the height difference keeps sphere tracing safe.
def terrain_dist (detail: f32) (w: vec3) : f32 = 0.6 * (w.y - terrain_height_lod detail w.x w.z)
