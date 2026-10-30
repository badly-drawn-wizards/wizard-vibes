-- The world the wizard flies over: the flight path, the frame that maps the
-- wizard's local space (where he and his rock stay at the origin, facing +z)
-- into the world, and the terrain below.

import "vec3"
import "hash"
import "sdf"

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

def flight_at (bank_gain: f32) (s: f32) : flight =
  let dx = path_dx s
  let heading = f32.atan2 dx forward_speed
  let turn = forward_speed * path_ddx s / (dx * dx + forward_speed * forward_speed)
  in {pos = vec (path_x s) altitude (forward_speed * s), heading, turn, bank = clamp (-0.35) 0.35 (bank_gain * turn)}

def yaw (a: f32) (v: vec3) : vec3 =
  {x = v.x * f32.cos a + v.z * f32.sin a, y = v.y, z = v.z * f32.cos a - v.x * f32.sin a}

def yaw_inv (a: f32) (v: vec3) : vec3 = yaw (-a) v

def roll (a: f32) (v: vec3) : vec3 =
  {x = v.x * f32.cos a - v.y * f32.sin a, y = v.x * f32.sin a + v.y * f32.cos a, z = v.z}

-- Local -> world. Banking into a turn towards +x tips the local +x side down.
def to_world_dir (f: flight) (d: vec3) : vec3 = yaw f.heading (roll (-f.bank) d)
def to_world (f: flight) (p: vec3) : vec3 = f.pos +^ to_world_dir f p
def to_local_dir (f: flight) (d: vec3) : vec3 = roll f.bank (yaw (-f.heading) d)

-- The moon's direction in the world, from its azimuth (degrees from +z
-- towards +x) and elevation.
def moon_dir (azimuth: f32) (elevation: f32) : vec3 =
  let a = azimuth * f32.pi / 180
  let e = elevation * f32.pi / 180
  in vec (f32.cos e * f32.sin a) (f32.sin e) (f32.cos e * f32.cos a)

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
def cloud_sample (cover: f32) (density: f32) (secs: f32) (w: vec3) : (f32, f32) =
  let h = (w.y - cloud_bottom) / (cloud_top - cloud_bottom)
  let q = scale 0.11 (w -^ scale secs wind)
  let threshold = 1 - cover - 0.14 * (4 * h * (1 - h)) -- thinner at top and bottom
  let o1 = vnoise3 q
  let upper = (0.5 * o1 + 0.4375) / 0.9375
  in if upper < threshold then (0, (threshold - upper) / (0.165 * 0.5 / 0.9375))
     else let n = fbm3 4 q
          in (density * f32.max 0 (n - threshold), 0)

-- Vertical distance from a world height to the slab (0 inside).
def cloud_gap (y: f32) : f32 = f32.max (cloud_bottom - y) (y - cloud_top) |> f32.max 0

---------------------------------------------------------------------
-- Terrain: rolling hills with ridged mountains, in world coordinates.

def terrain_max : f32 = 11 -- 9 + 6 - 4: no height exceeds this

-- The land's parameters: sea level, and the rivers' half-width in river-
-- noise units (0: no rivers).
type land = {sea: f32, river: f32}

-- Rivers run where a broad, low-frequency noise crosses its midpoint, which
-- traces long meandering lines; there the ground is carved down to a bed
-- below sea level. Carving only lowers the ground, so terrain_upper stays an
-- upper bound.
def river_noise_scale : f32 = 0.012
def river_bed (l: land) : f32 = l.sea - 0.6

-- (weight in [0, 1], 0 in the channel; d weight / d f)
def river_weight (l: land) (f: f32) : (f32, f32) =
  if l.river <= 0 then (1, 0) else
  let e0 = 0.5 * l.river
  let e1 = 1.5 * l.river
  let a = f32.abs (f - 0.5)
  let t = saturate ((a - e0) / (e1 - e0))
  let ds = if t <= 0 || t >= 1 then 0 else 6 * t * (1 - t) / (e1 - e0)
  in (t * t * (3 - 2 * t), ds * f32.sgn (f - 0.5))

-- Carves height h (with gradient hx, hz) by weight m (with gradient mx, mz):
-- h - (1 - m) max(0, h - bed).
def carve (l: land) (h: f32) (hx: f32) (hz: f32) (m: f32) (mx: f32) (mz: f32) : (f32, f32, f32) =
  let above = h > river_bed l
  let g = if above then h - river_bed l else 0
  let (gx, gz) = if above then (hx, hz) else (0, 0)
  in (h - (1 - m) * g, hx - (1 - m) * gx + g * mx, hz - (1 - m) * gz + g * mz)

-- `detail` in octaves (5 = full); far-away terrain needs fewer.
def terrain_height_lod (l: land) (detail: f32) (x: f32) (z: f32) : f32 =
  let n = fbm2_lod 5 detail (x * 0.035) (z * 0.035)
  let ridge = 1 - f32.abs (2 * fbm2_lod 3 (detail - 2) (x * 0.02 + 5.2) (z * 0.02 - 1.7) - 1)
  let h = 9 * n * n + 6 * ridge * ridge * ridge - 4
  -- (rivers only carve ground above their bed, so lower ground needs no
  -- river noise)
  in if l.river <= 0 || h <= river_bed l then h else
  let f = fbm2_lod 2 2 (x * river_noise_scale + 31.7) (z * river_noise_scale - 12.3)
  in (carve l h 0 0 (river_weight l f).0 0 0).0

def terrain_height (l: land) (x: f32) (z: f32) : f32 = terrain_height_lod l 5 x z

-- terrain_height_lod with its gradient: (height, dh/dx, dh/dz).
-- h = 9 n^2 + 6 r^3 - 4 with r = 1 - |2f - 1|, then carved by the rivers.
def terrain_height_grad (l: land) (detail: f32) (x: f32) (z: f32) : (f32, f32, f32) =
  let (n, nx, nz) = fbm2_lod_grad 5 detail (x * 0.035) (z * 0.035)
  let (f, fx, fz) = fbm2_lod_grad 3 (detail - 2) (x * 0.02 + 5.2) (z * 0.02 - 1.7)
  let r = 1 - f32.abs (2 * f - 1)
  let dr = -2 * f32.sgn (2 * f - 1) -- dr/df
  let hn = 18 * n -- dh/dn
  let hr = 18 * r * r -- dh/dr
  let h = 9 * n * n + 6 * r * r * r - 4
  let hx = 0.035 * hn * nx + 0.02 * hr * dr * fx
  let hz = 0.035 * hn * nz + 0.02 * hr * dr * fz
  in if l.river <= 0 || h <= river_bed l then (h, hx, hz) else
  let (rf, rfx, rfz) = fbm2_lod_grad 2 2 (x * river_noise_scale + 31.7) (z * river_noise_scale - 12.3)
  let (m, dm) = river_weight l rf
  in carve l h hx hz m (dm * rfx * river_noise_scale) (dm * rfz * river_noise_scale)

-- The terrain's upward unit normal in world space.
def terrain_normal (l: land) (detail: f32) (x: f32) (z: f32) : vec3 =
  let (_, hx, hz) = terrain_height_grad l detail x z
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
def terrain_dist (l: land) (detail: f32) (w: vec3) : f32 = 0.6 * (w.y - terrain_height_lod l detail w.x w.z)

---------------------------------------------------------------------
-- Structures: one cell of a 30-unit grid in every few holds a cottage, a
-- wizard's tower or a pyramid, at a random spot kept well inside its cell,
-- standing on the ground there with a random rotation. Only dry ground
-- below structure_ground_max is built on, so no structure rises above
-- land_max (the land's early-out bound).

def struct_cell : f32 = 30
def struct_reach : f32 = 3.2 -- every structure fits within this of its centre (in xz)
def structure_ground_max : f32 = 2.8 -- so structure_top = terrain_max
def structure_top : f32 = structure_ground_max + 8.2 -- the tallest: a tower's roof tip
def land_max : f32 = f32.max terrain_max structure_top -- nothing on land is higher

def kind_cottage : i32 = 0
def kind_tower : i32 = 1
def kind_pyramid : i32 = 2

-- The candidate structure of the cell holding (x, z): (kind, centre x, z, yaw).
def structure_candidate (x: f32) (z: f32) : (i32, f32, f32, f32) =
  let cx = i32.f32 (f32.floor (x / struct_cell))
  let cz = i32.f32 (f32.floor (z / struct_cell))
  let r = hash2 (cx * 7 + 3) (cz * 13 + 5)
  let kind = if r < 0.45 then kind_cottage else if r < 0.75 then kind_tower else kind_pyramid
  let px = (f32.i32 cx + 0.25 + 0.5 * hash2 (cx + 41) (cz - 17)) * struct_cell
  let pz = (f32.i32 cz + 0.25 + 0.5 * hash2 (cx - 23) (cz + 59)) * struct_cell
  in (kind, px, pz, 2 * f32.pi * hash2 (cx + 5) (cz + 77))

-- Whether the cell holding (x, z) has a structure at all, for density in [0, 1].
def structure_present (density: f32) (x: f32) (z: f32) : bool =
  let cx = i32.f32 (f32.floor (x / struct_cell))
  let cz = i32.f32 (f32.floor (z / struct_cell))
  in hash2 (cx * 31 + 11) (cz * 17 + 3) < density

-- Distance from (x, z) to the edge of its cell.
def cell_border (x: f32) (z: f32) : f32 =
  let fx = x - struct_cell * f32.floor (x / struct_cell)
  let fz = z - struct_cell * f32.floor (z / struct_cell)
  in f32.min (f32.min fx (struct_cell - fx)) (f32.min fz (struct_cell - fz))

-- Square pyramid with base 1 x 1 at y = 0 and apex at height h (after Quilez).
def sd_pyramid (p: vec3) (h: f32) : f32 =
  let m2 = h * h + 0.25
  let (px, pz) = (f32.abs p.x, f32.abs p.z)
  let (px, pz) = if pz > px then (pz, px) else (px, pz)
  let (px, pz) = (px - 0.5, pz - 0.5)
  let qx = pz
  let qy = h * p.y - 0.5 * px
  let qz = h * px + 0.5 * p.y
  let s = f32.max (-qx) 0
  let t = saturate ((qy - 0.5 * pz) / (m2 + 0.25))
  let a = m2 * (qx + s) * (qx + s) + qy * qy
  let b = m2 * (qx + 0.5 * t) * (qx + 0.5 * t) + (qy - m2 * t) * (qy - m2 * t)
  let d2 = if f32.min qy (-qx * m2 - qy * 0.5) > 0 then 0 else f32.min a b
  in f32.sqrt ((d2 + qz * qz) / m2) * f32.sgn (f32.max qz (-p.y))

-- Structure parts (for materials).
def part_wall : i32 = 0
def part_roof : i32 = 1
def part_stone : i32 = 2
def part_slate : i32 = 3
def part_sandstone : i32 = 4

-- A structure in its own frame (ground at y = 0): (distance, part). Each is
-- sunk into the ground on a foundation, so slopes leave no gap beneath it.
#[noinline]
def sd_structure (kind: i32) (q: vec3) : (f32, i32) =
  if kind == kind_cottage then
    let walls = sd_box (q -^ vec 0 0.1 0) (vec 1.8 2.1 1.3)
    -- a gable roof along x: from the eaves at y 2.2 (z = +-1.6) to the ridge at 3.4
    let gable = (f32.abs q.z * 1.2 + (q.y - 3.4) * 1.6) / 2
    let roof = f32.max (f32.max gable (2.15 - q.y)) (f32.abs q.x - 2)
    let chimney = sd_box (q -^ vec 1.1 3.2 0.5) (vec 0.22 0.6 0.22)
    in if walls < f32.min roof chimney then (walls, part_wall) else (f32.min roof chimney, part_roof)
  else if kind == kind_tower then
    let body = sd_cylinder (q -^ vec 0 1.75 0) 1 3.75
    let ring = sd_cylinder (q -^ vec 0 5.45 0) 1.15 0.15
    let roof = sd_capped_cone (q -^ vec 0 5.6 0) 2.6 1.35 0.03
    let stone = f32.min body ring
    in if stone < roof then (stone, part_stone) else (roof, part_slate)
  else
    -- 5.2 across, 3.6 high, its base sunk 0.6 into the ground
    let s = 5.2
    in (sd_pyramid (scale (1 / s) (q +^ vec 0 0.6 0)) (4.2 / s) * s, part_sandstone)

-- The structure of the cell holding a world point, if there is one: (present,
-- kind, its frame's origin and yaw). Evaluates the ground there.
#[noinline]
def structure_at (l: land) (density: f32) (w: vec3) : (bool, i32, vec3, f32) =
  let (kind, px, pz, yaw) = structure_candidate w.x w.z
  let ground = terrain_height l px pz
  let dry = ground > l.sea + 0.3 && ground < structure_ground_max
  in (structure_present density w.x w.z && dry, kind, vec px ground pz, yaw)

-- The structures' distance at a world point: (distance, part). Every bound
-- returned is a true lower bound, so it is safe to step by:
-- - above every roof, the height above structure_top;
-- - structures in other cells lie at least (cell_border + margin) away, as
--   each stays struct_reach of its centre and its centre stays a quarter
--   cell inside its cell;
-- - the own cell's structure lies at least (xz distance - struct_reach) away.
-- The ground (8 noise lookups) is only evaluated close to a structure.
#[noinline]
def structures (l: land) (density: f32) (w: vec3) : (f32, i32) =
  let above = w.y - structure_top
  in if above > 0.3 then (above, part_wall) else
  let far = cell_border w.x w.z + (0.25 * struct_cell - struct_reach)
  -- (cells without a structure stop here, after one hash)
  in if density <= 0 || !(structure_present density w.x w.z) then (f32.max above far, part_wall) else
  let (kind, px, pz, yaw) = structure_candidate w.x w.z
  let own = length2 (vec2 (w.x - px) (w.z - pz)) - struct_reach
  let bound = f32.max above (f32.min far own)
  in if bound > 0.3 then (bound, part_wall) else
  let ground = terrain_height l px pz
  let dry = ground > l.sea + 0.3 && ground < structure_ground_max
  in if !dry then (f32.max above far, part_wall) else
  let (d, part) = sd_structure kind (yaw_inv yaw (w -^ vec px ground pz))
  in (f32.min d far, part)
