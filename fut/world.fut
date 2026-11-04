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

-- fbm2_lod given its first octave's noise, v0 = vnoise2 x z (for a caller
-- that has it already).
def fbm2_lod_from (octaves: i32) (detail: f32) (v0: f32) (x: f32) (z: f32) : f32 =
  let (sum, norm, _, _, _) =
    loop (sum, norm, amp, x, z) = (0f32, 0f32, 0.5f32, x, z) for i < octaves do
      let k = saturate (detail - f32.i32 i)
      let v = if k > 0 then mix1 0.5 (if i == 0 then v0 else vnoise2 x z) k else 0.5
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

def terrain_max : f32 = 11 -- 9 + 6 - 4, and the alpine cap: no height exceeds this

---------------------------------------------------------------------
-- Biomes. One broad field b, a few hundred units across, sorts the land:
-- desert (low dunes) where it is low, alpine (higher, steeper mountains)
-- where it is high, and the hills and meadows between. Each biome's height
-- is a function of the same main noise, so mountains run on from meadow
-- into alpine and sink into dunes across the borders.

-- smoothstep e0 e1 x with its derivative in x
def smoothstep_d (e0: f32) (e1: f32) (x: f32) : (f32, f32) =
  let t = saturate ((x - e0) / (e1 - e0))
  in (t * t * (3 - 2 * t), 6 * t * (1 - t) / (e1 - e0))

-- The biome weights at (x, z): (desert, alpine), each in [0, 1] and never
-- both above 0 (the meadow's is the rest), with their gradients:
-- (wd, dwd/dx, dwd/dz, wa, dwa/dx, dwa/dz).
--
-- b is two crossing waves, each bent by a third and fourth: evaluated at
-- every step of the march, it has to be cheap (as a value noise it cost
-- 1-2 ms a frame at 1080p).
-- (shifted along the flight so it opens over meadow, with mountains ahead
-- after a minute and desert after seven)
def biome_field (x: f32) (z0: f32) : (f32, f32, f32) = -- (b, db/dx, db/dz)
  let z = z0 + 2850
  let w1 = 0.0047 * z - 0.0013 * x + 0.4
  let w2 = 0.0051 * x + 0.0017 * z + 2.1
  let (c1, c2) = (1.3 * f32.cos w1, 1.1 * f32.cos w2)
  let u = 0.0062 * x + 0.0021 * z + 1.3 * f32.sin w1
  let v = 0.0058 * z - 0.0024 * x + 1.1 * f32.sin w2
  let (cu, cv) = (0.2 * f32.cos u, 0.2 * f32.cos v)
  in ( 0.5 + 0.2 * (f32.sin u + f32.sin v)
     , cu * (0.0062 - 0.0013 * c1) + cv * (-0.0024 + 0.0051 * c2)
     , cu * (0.0021 + 0.0047 * c1) + cv * (0.0058 + 0.0017 * c2) )

def biome_grad (x: f32) (z: f32) : (f32, f32, f32, f32, f32, f32) =
  let (b, bx, bz) = biome_field x z
  let (d, dd) = smoothstep_d 0.42 0.32 b
  let (a, da) = smoothstep_d 0.58 0.68 b
  in (d, dd * bx, dd * bz, a, da * bx, da * bz)

def biome (x: f32) (z: f32) : (f32, f32) =
  let b = (biome_field x z).0
  in (smoothstep 0.42 0.32 b, smoothstep 0.58 0.68 b)

-- The desert's height from the main noise n (in [0, 1]): low ground with
-- dunes, long ridges bent by the noise. With its gradient from n's world-
-- space gradient (nx, nz).
def dune_height (x: f32) (z: f32) (n: f32) (nx: f32) (nz: f32) : (f32, f32, f32) =
  let a = 0.3 * (0.8 * x + 0.6 * z) + 6 * n
  let s = 0.45 * f32.cos a
  in ( -0.5 + 3 * n + 0.45 * (1 + f32.sin a)
     , 3 * nx + s * (0.24 + 6 * nx), 3 * nz + s * (0.18 + 6 * nz) )

-- The alpine height from the meadow's: steeper, and higher, but eased
-- under terrain_max (the flight path's rock hangs not far above it):
-- above alpine_knee, h becomes knee + r tanh ((h - knee) / r).
def alpine_scale : f32 = 1.25
def alpine_lift : f32 = 1
def alpine_knee : f32 = 7
def alpine_height (hm: f32) : (f32, f32) = -- (height, d height / d hm)
  let h = alpine_scale * hm + alpine_lift
  let r = terrain_max - alpine_knee
  in if h <= alpine_knee then (h, alpine_scale)
     else let t = f32.tanh ((h - alpine_knee) / r)
          in (alpine_knee + r * t, alpine_scale * (1 - t * t))

-- Blends the meadow's height hm (with gradient) into the desert's hd and
-- the alpine's by the weights (with gradients).
def biome_blend (hm: f32) (hmx: f32) (hmz: f32) (hd: f32) (hdx: f32) (hdz: f32)
                (wd: f32) (wdx: f32) (wdz: f32) (wa: f32) (wax: f32) (waz: f32) : (f32, f32, f32) =
  let (ha, dha) = alpine_height hm
  let (ed, ea) = (hd - hm, ha - hm)
  let (ax, az) = ((dha - 1) * hmx, (dha - 1) * hmz)
  in ( hm + wd * ed + wa * ea
     , hmx + wd * (hdx - hmx) + wa * ax + ed * wdx + ea * wax
     , hmz + wd * (hdz - hmz) + wa * az + ed * wdz + ea * waz )

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

-- The terrain's octaves (its main fbm's; 5 gave a little more fine
-- relief for 0.66 ms a frame at 1080p).
def terrain_octaves : i32 = 4

-- The first octaves' noise of the terrain's two fbms (the main one and the
-- ridges'), which terrain_upper needs as well.
def terrain_first_octaves (x: f32) (z: f32) : (f32, f32) =
  (vnoise2 (x * 0.035) (z * 0.035), vnoise2 (x * 0.02 + 5.2) (z * 0.02 - 1.7))

-- terrain_height_in given terrain_first_octaves at (x, z).
def terrain_height_from (l: land) (wd: f32) (wa: f32) (detail: f32) ((v0, r0): (f32, f32)) (x: f32) (z: f32) : f32 =
  let n = fbm2_lod_from terrain_octaves detail v0 (x * 0.035) (z * 0.035)
  -- (the desert has no ridges)
  let ridge = if wd >= 1 then 0
              else 1 - f32.abs (2 * fbm2_lod_from 3 (detail - 2) r0 (x * 0.02 + 5.2) (z * 0.02 - 1.7) - 1)
  let hm = 9 * n * n + 6 * ridge * ridge * ridge - 4
  let hd = if wd > 0 then (dune_height x z n 0 0).0 else 0
  let ha = if wa > 0 then (alpine_height hm).0 else 0
  let h = hm + wd * (hd - hm) + wa * (ha - hm)
  -- (rivers only carve ground above their bed, so lower ground needs no
  -- river noise)
  in if l.river <= 0 || h <= river_bed l then h else
  let f = fbm2_lod 2 2 (x * river_noise_scale + 31.7) (z * river_noise_scale - 12.3)
  in (carve l h 0 0 (river_weight l f).0 0 0).0

-- `detail` in octaves (terrain_octaves = full); far-away terrain needs fewer.
-- (given the biome weights there: see biome)
def terrain_height_in (l: land) (wd: f32) (wa: f32) (detail: f32) (x: f32) (z: f32) : f32 =
  terrain_height_from l wd wa detail (terrain_first_octaves x z) x z

def terrain_height_lod (l: land) (detail: f32) (x: f32) (z: f32) : f32 =
  let (wd, wa) = biome x z
  in terrain_height_in l wd wa detail x z

def terrain_height (l: land) (x: f32) (z: f32) : f32 = terrain_height_lod l 5 x z

-- terrain_height_lod with its gradient: (height, dh/dx, dh/dz).
-- h = 9 n^2 + 6 r^3 - 4 with r = 1 - |2f - 1|, then carved by the rivers.
def terrain_height_grad (l: land) (detail: f32) (x: f32) (z: f32) : (f32, f32, f32) =
  let (n, nx, nz) = fbm2_lod_grad terrain_octaves detail (x * 0.035) (z * 0.035)
  let (f, fx, fz) = fbm2_lod_grad 3 (detail - 2) (x * 0.02 + 5.2) (z * 0.02 - 1.7)
  let r = 1 - f32.abs (2 * f - 1)
  let dr = -2 * f32.sgn (2 * f - 1) -- dr/df
  let hn = 18 * n -- dh/dn
  let hr = 18 * r * r -- dh/dr
  let hm = 9 * n * n + 6 * r * r * r - 4
  let hmx = 0.035 * hn * nx + 0.02 * hr * dr * fx
  let hmz = 0.035 * hn * nz + 0.02 * hr * dr * fz
  let (wd, wdx, wdz, wa, wax, waz) = biome_grad x z
  let (hd, hdx, hdz) = if wd > 0 then dune_height x z n (0.035 * nx) (0.035 * nz) else (0, 0, 0)
  let (h, hx, hz) = biome_blend hm hmx hmz hd hdx hdz wd wdx wdz wa wax waz
  in if l.river <= 0 || h <= river_bed l then (h, hx, hz) else
  let (rf, rfx, rfz) = fbm2_lod_grad 2 2 (x * river_noise_scale + 31.7) (z * river_noise_scale - 12.3)
  let (m, dm) = river_weight l rf
  in carve l h hx hz m (dm * rfx * river_noise_scale) (dm * rfz * river_noise_scale)

-- The terrain's upward unit normal in world space.
def terrain_normal (l: land) (detail: f32) (x: f32) (z: f32) : vec3 =
  let (_, hx, hz) = terrain_height_grad l detail x z
  in normalise (vec (-hx) 1 (-hz))

-- A heightfield is not a distance: a point at height y over ground of slope
-- at most s is at least (y - h) / sqrt (1 + s^2) from it. Away from the
-- rivers' banks the slopes stay below about 1.55, and 1.85 in the
-- mountains (measured over 36 km^2), so scaling the height difference by
-- this (for the alpine weight wa), and the march's own 0.85, keeps sphere
-- tracing safe.
def terrain_lipschitz (wa: f32) : f32 = 0.6 - 0.1 * wa

-- An upper bound on terrain_height from the first octave of each fbm alone
-- (2 noise lookups instead of 8): the other octaves can add at most their
-- amplitudes. The height grows with n and with the ridge, so bounding those
-- bounds it. (Refining the bound octave by octave was tried and is slower on
-- the GPU: neighbouring pixels stop at different octaves and diverge.)
-- (given the biome weights there)
def terrain_upper_from (wd: f32) (wa: f32) ((v0, r0): (f32, f32)) : f32 =
  -- (the first octave has amplitude 0.5 of the total 1 - 0.5^octaves; the
  -- others together 0.5 - 0.5^octaves)
  let total = 1 - 0.5 ** f32.i32 terrain_octaves
  let n_hi = (0.5 * v0 + (total - 0.5)) / total
  let f_lo = 0.5 * r0 / 0.875
  let f_hi = f_lo + 0.375 / 0.875
  let ridge_hi = if f_lo <= 0.5 && f_hi >= 0.5 then 1
                 else 1 - f32.min (f32.abs (2 * f_lo - 1)) (f32.abs (2 * f_hi - 1))
  let hm = 9 * n_hi * n_hi + 6 * ridge_hi * ridge_hi * ridge_hi - 4
  -- (each biome's height grows with n and the meadow's, so bounding those
  -- bounds each; the weights are the same, never negative and sum to at
  -- most 1, so the blend of the bounds bounds the blend)
  let hd = -0.5 + 3 * n_hi + 0.9
  -- (the alpine height, a tanh, only where it has weight)
  in hm + wd * (hd - hm) + (if wa > 0 then wa * ((alpine_height hm).0 - hm) else 0)

def terrain_upper (wd: f32) (wa: f32) (x: f32) (z: f32) : f32 =
  terrain_upper_from wd wa (terrain_first_octaves x z)

-- The terrain's distance bound given terrain_first_octaves there (the march
-- evaluates the biome and those once per step for the bound and this).
def terrain_dist_from (l: land) (wd: f32) (wa: f32) (detail: f32) (o: (f32, f32)) (w: vec3) : f32 =
  terrain_lipschitz wa * (w.y - terrain_height_from l wd wa detail o w.x w.z)

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
  let px = (f32.i32 cx + 0.25 + 0.5 * hash2 (cx + 41) (cz - 17)) * struct_cell
  let pz = (f32.i32 cz + 0.25 + 0.5 * hash2 (cx - 23) (cz + 59)) * struct_cell
  -- pyramids in the desert, towers in the mountains, cottages between
  let (wd, wa) = biome px pz
  let kind = if wd > 0.5 then (if r < 0.8 then kind_pyramid else kind_tower)
             else if wa > 0.5 then (if r < 0.7 then kind_tower else kind_cottage)
             else if r < 0.7 then kind_cottage else kind_tower
  in (kind, px, pz, 2 * f32.pi * hash2 (cx + 5) (cz + 77))

-- Whether the cell holding (x, z) has a structure at all, for density in [0, 1].
def structure_present (density: f32) (x: f32) (z: f32) : bool =
  let cx = i32.f32 (f32.floor (x / struct_cell))
  let cz = i32.f32 (f32.floor (z / struct_cell))
  in hash2 (cx * 31 + 11) (cz * 17 + 3) < density

-- How far a ray from w along the unit direction d runs before leaving (in
-- xz) the square cell of the given size that holds w. At least the
-- distance to the cell's edge, so it can stand in for that in bounds that
-- are only used along this ray (see march_terrain).
def cell_exit (cell: f32) (w: vec3) (d: vec3) : f32 =
  let fx = w.x - cell * f32.floor (w.x / cell)
  let fz = w.z - cell * f32.floor (w.z / cell)
  -- (|d.x| keeps a zero component, of either sign, at +inf)
  let tx = (if d.x >= 0 then cell - fx else fx) / f32.abs d.x
  let tz = (if d.z >= 0 then cell - fz else fz) / f32.abs d.z
  in f32.min tx tz

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

-- A structure cell's candidate, as structures_in takes it: (present, kind,
-- centre x, z, yaw), without the ground under it (only needed close by).
type structure_cell = (bool, i32, f32, f32, f32)

def structure_cell_at (density: f32) (x: f32) (z: f32) : structure_cell =
  let (kind, px, pz, yaw) = structure_candidate x z
  in (density > 0 && structure_present density x z, kind, px, pz, yaw)

-- The same along a ray, given the candidate of the cell holding w (which a
-- march works out once per cell). `exit` is how far the ray runs before
-- leaving the cell (see cell_exit): other cells' structures are at least
-- that, plus the margin, along it. The result bounds the way along that
-- ray only.
#[noinline]
def structures_in (l: land) (exit: f32) ((present, kind, px, pz, yaw): structure_cell) (w: vec3) : (f32, i32) =
  let above = w.y - structure_top
  in if above > 0.3 then (above, part_wall) else
  let far = f32.max (cell_border w.x w.z) exit + (0.25 * struct_cell - struct_reach)
  in if !present then (f32.max above far, part_wall) else
  let own = length2 (vec2 (w.x - px) (w.z - pz)) - struct_reach
  let bound = f32.max above (f32.min far own)
  in if bound > 0.3 then (bound, part_wall) else
  let ground = terrain_height l px pz
  let dry = ground > l.sea + 0.3 && ground < structure_ground_max
  in if !dry then (f32.max above far, part_wall) else
  let (d, part) = sd_structure kind (yaw_inv yaw (w -^ vec px ground pz))
  in (f32.min d far, part)

---------------------------------------------------------------------
-- Trees: one cell of a tree_cell grid in every few holds a tree, in groves
-- with clearings between; broadleaf trees in the meadows and pines in the
-- mountains, none in the desert, and none above the tree line or on the
-- shore. Bounded like the structures: each tree stays within tree_reach of
-- its centre, which stays tree_inset cells inside its cell.
--
-- A broadleaf tree is an L-system (each branch ends in two shorter,
-- thinner branches) evaluated as a distance in time linear in its depth,
-- not in its 2^depth branches: at each level space is mirrored (|x|),
-- turned and moved to the branch's end, so one capsule per level stands for
-- every branch of that level. Mirrors and turns keep distances, so the
-- result is still (nearly) a distance; it can overestimate only where a
-- branch reaches across its mirror plane, and the branches lean away from it.

def tree_cell : f32 = 12
def tree_reach : f32 = 2.2 -- every tree fits within this of its centre (in xz)
def tree_inset : f32 = 0.38 -- (so trees in other cells are 2.36 further than the border)
def tree_height_max : f32 = 4.6
def tree_size_max : f32 = 1.1
def tree_tall : f32 = 3.82 * tree_size_max -- (measured: the tallest point of either kind, at size 1)

def tree_line : f32 = 4.5 -- no trees on ground higher than this (so they pass under the rock)
def tree_top : f32 = tree_line + tree_height_max -- under terrain_max

def kind_broadleaf : i32 = 0
def kind_pine : i32 = 1

-- Groves and clearings: 0 to 1.
def grove (x: f32) (z: f32) : f32 =
  smoothstep 0.3 0.55 (0.5 + 0.25 * (f32.sin (0.05 * x + 1.3 * f32.sin (0.031 * z)) + f32.sin (0.043 * z - 0.9 * f32.sin (0.037 * x) + 2)))

-- The candidate tree of the cell holding (x, z): (present, kind, centre x,
-- z, size, seed in [0, 1)).
def tree_candidate (density: f32) (x: f32) (z: f32) : (bool, i32, f32, f32, f32, f32) =
  let cx = i32.f32 (f32.floor (x / tree_cell))
  let cz = i32.f32 (f32.floor (z / tree_cell))
  let seed = hash2 (cx * 19 + 7) (cz * 23 + 1)
  let px = (f32.i32 cx + tree_inset + (1 - 2 * tree_inset) * hash2 (cx + 13) (cz - 29)) * tree_cell
  let pz = (f32.i32 cz + tree_inset + (1 - 2 * tree_inset) * hash2 (cx - 31) (cz + 37)) * tree_cell
  -- (most cells fail the hash, without the biome and grove fields)
  let h = hash2 (cx * 3 + 101) (cz * 5 - 7)
  let (present, kind) =
    if h >= density then (false, kind_broadleaf) else
    let (wd, wa) = biome px pz
    in ( h < density * (1 - wd) * grove px pz
       , if wa > 0.25 + 0.5 * seed then kind_pine else kind_broadleaf )
  in (present, kind, px, pz, 0.8 + (tree_size_max - 0.8) * hash2 (cx + 3) (cz + 9), seed)

-- Tree parts.
def part_trunk : i32 = 0
def part_leaves : i32 = 1
def part_needles : i32 = 2

-- A broadleaf tree, ground at y = 0, at most tree_reach / 1.2 across and
-- tree_height_max / 1.2 tall: (distance, part).
def sd_broadleaf (seed: f32) (q: vec3) : (f32, i32) =
  let trunk = sd_round_cone_ab q (vec 0 (-0.3) 0) (vec 0 1.3 0) 0.15 0.09
  let (wood, tip, _, _) =
    loop (d, p, len, rad) = (trunk, q -^ vec 0 1.3 0, 0.85f32, 0.085f32) for i < 3 do
      -- turn the branching plane (about 100 degrees a level, varied per tree),
      -- mirror, then lean the branch out from the mirror plane
      let p = yaw (1.75 + 0.6 * seed + f32.i32 i) p
      let p = p with x = f32.abs p.x
      let lean = 0.42 + 0.1 * f32.i32 i
      let r = rot2 lean (vec2 p.x p.y)
      let p = p with x = r.x with y = r.y
      in (f32.min d (sd_capsule p (vec 0 0 0) (vec 0 len 0) rad), p -^ vec 0 len 0, 0.78 * len, 0.62 * rad)
  -- a clump of leaves at each tip (lumpy: the bumps have slope under 0.45,
  -- so the 0.7 keeps it a bound), and one over the crown's middle
  let bumps = 0.04 * f32.sin (6 * q.x + 7 * seed) * f32.sin (6 * q.y) * f32.sin (6 * q.z + 3 * seed)
  let clump = sd_sphere (tip -^ vec 0 0.1 0) 0.55
  let middle = sd_ellipsoid (q -^ vec 0 2.55 0) (vec 0.95 0.75 0.95)
  let leaves = 0.7 * (smin 0.3 clump middle + bumps)
  in if wood < leaves then (wood, part_trunk) else (leaves, part_leaves)

-- A pine: a trunk and three tiers of needles, narrowing upwards.
def sd_pine (seed: f32) (q: vec3) : (f32, i32) =
  let trunk = sd_capsule q (vec 0 (-0.3) 0) (vec 0 3.4 0) 0.09
  let tier (y: f32) (r: f32) (h: f32) = sd_capped_cone (q -^ vec 0 y 0) h r 0.02
  let s = 0.1 * seed
  let needles = f32.min (tier 0.7 (1.25 + s) 1.6) (f32.min (tier 1.6 (0.95 + s) 1.4) (tier 2.4 0.65 1.4))
  -- (a ragged edge: needles in drooping bunches round each tier)
  let a = f32.atan2 q.z q.x
  let needles = needles + 0.05 * f32.sin (11 * a + 5 * q.y + 6 * seed) * saturate (length2 (vec2 q.x q.z) * 2)
  in if trunk < 0.8 * needles then (trunk, part_trunk) else (0.8 * needles, part_needles)

#[noinline]
def sd_tree (kind: i32) (seed: f32) (q: vec3) : (f32, i32) =
  if kind == kind_pine then sd_pine seed q else sd_broadleaf seed q

-- The tree of the cell holding a world point: (present, kind, origin on the
-- ground, size, seed). Evaluates the ground there.
def tree_at (l: land) (density: f32) (w: vec3) : (bool, i32, vec3, f32, f32) =
  let (present, kind, px, pz, size, seed) = tree_candidate density w.x w.z
  let ground = if present then terrain_height l px pz else 0
  let ok = present && ground > l.sea + 0.4 && ground < tree_line
  in (ok, kind, vec px ground pz, size, seed)

-- Distance from (x, z) to the edge of its tree cell.
def tree_cell_border (x: f32) (z: f32) : f32 =
  let fx = x - tree_cell * f32.floor (x / tree_cell)
  let fz = z - tree_cell * f32.floor (z / tree_cell)
  in f32.min (f32.min fx (tree_cell - fx)) (f32.min fz (tree_cell - fz))

-- A tree cell's candidate (see tree_candidate), as `trees_in` takes it:
-- whether a tree may stand there, and the ground under it if `known`
-- (worked out). (Not NaN for unknown: WGSL may assume there are no NaNs.)
type tree_cell = {present: bool, kind: i32, px: f32, pz: f32, size: f32, seed: f32, known: bool, ground: f32}

def tree_ground_ok (l: land) (ground: f32) : bool = ground > l.sea + 0.4 && ground < tree_line

def tree_cell_at (density: f32) (x: f32) (z: f32) : tree_cell =
  let (present, kind, px, pz, size, seed) = tree_candidate density x z
  in {present, kind, px, pz, size, seed, known = false, ground = 0}

-- The same with its ground worked out (8 noise lookups), so whether the tree
-- stands at all (most candidates fail on their ground) is known: for a
-- march, which keeps it while it crosses the cell, so a cell without a tree
-- need not hold it up, and nothing about the cell is worked out twice.
def tree_cell_grounded (l: land) (density: f32) (x: f32) (z: f32) : tree_cell =
  let c = tree_cell_at density x z
  in if !c.present then c
     else let g = terrain_height l c.px c.pz
          in c with known = true with ground = g with present = tree_ground_ok l g

-- The trees' distance at a world point: (distance, part), with the same
-- kinds of bounds as the structures', and one more: a tree is no taller
-- than its ground's cheap upper bound (capped by the tree line) plus its
-- height, so the ground under it (8 noise lookups) is only needed close by.
-- `grown` in [0, 1] scales every tree (towards the edge of where they are
-- drawn at all: see land_built); shrinking only shortens distances. Where
-- none are drawn, `outside` is the distance to where they are. `exit` is
-- as for the structures; `c` is the tree cell holding w.
#[noinline]
def trees_in (l: land) (density: f32) (grown: f32) (outside: f32) (exit: f32) (c: tree_cell) (w: vec3) : (f32, i32) =
  let above = f32.max (w.y - tree_top) outside
  in if above > 0.3 then (above, part_trunk) else
  let far = f32.max (tree_cell_border w.x w.z) exit + (tree_inset * tree_cell - tree_reach)
  in if density <= 0 || grown <= 0 || !c.present then (f32.max above far, part_trunk) else
  let known = c.known
  let size = c.size * grown
  let own = length2 (vec2 (w.x - c.px) (w.z - c.pz)) - tree_reach * size / tree_size_max
  let b = f32.max above (f32.min far own)
  in if b > 0.3 then (b, part_trunk) else
  let top = if known then c.ground
            else let (wd, wa) = biome c.px c.pz in f32.min (terrain_upper wd wa c.px c.pz) tree_line
  let tall = w.y - (top + tree_tall * size / tree_size_max)
  in if tall > 0.3 then (f32.max tall (f32.min far own), part_trunk) else
  let ground = if known then c.ground else terrain_height l c.px c.pz
  in if !(tree_ground_ok l ground) then (f32.max above far, part_trunk) else
  let (d, part) = sd_tree c.kind c.seed (scale (1 / size) (yaw (6.28 * c.seed) (w -^ vec c.px ground c.pz)))
  in (f32.min (d * size) far, part)

-- trees_in for the tree cell holding w (its candidate is only worked out
-- below the trees' tops).
#[noinline]
def trees (l: land) (density: f32) (grown: f32) (outside: f32) (w: vec3) : (f32, i32) =
  let above = f32.max (w.y - tree_top) outside
  in if above > 0.3 then (above, part_trunk)
     else trees_in l density grown outside 0 (tree_cell_at density w.x w.z) w
