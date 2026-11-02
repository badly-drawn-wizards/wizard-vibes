-- Enemies: other wizards flying round him, each on a platform of its own
-- (see platform.fut) with a magic circle of its own colour, and looks of
-- its own (robe, hat, beard and skin colours, the hat's height and droop,
-- the beard's length), all hashed from the seed and its number. Where they
-- are is a pure function of the time and his flight: each circles him in
-- the world at its own distance, height and speed, turned to face him.
--
-- Their fights are run by the harness (web/enemies.ts), which passes each
-- one's state as a float (see foe_of): how many times it has been
-- replaced (which re-rolls its looks and its place), and whether it is
-- appearing (growing in, flashing) or defeated (tumbling, falling, burnt
-- out, shrinking away). 0 is one there from the start.
--
-- They are drawn in the layer layers_enemies, with materials m_enemy +
-- enemy_parts * i + part (part_* below). A simpler figure than his: robe,
-- arms, head and nose, beard, hat and band, staff and orb; one arm on the
-- staff, the other raised and lowered as if casting.

import "vec3"
import "sdf"
import "hash"
import "world"
import "params"
open import "materials"
open import "platform"

def part_robe : i32 = 0
def part_skin : i32 = 1
def part_beard : i32 = 2
def part_hat : i32 = 3
def part_band : i32 = 4
def part_staff : i32 = 5
def part_orb : i32 = 6
def part_top : i32 = 7 -- the platform's top, with the circle
def part_side : i32 = 8 -- the rest of the platform

-- Everything about an enemy lies within this sphere of its own space
-- (the platform below, the hat's tip above).
def enemy_centre : vec3 = vec 0 (-0.1) 0
def enemy_radius : f32 = 2.8

-- The states of the enemies, from the harness: per enemy, 16 times its
-- generation (how many enemies have had its place before it) plus its
-- phase: under 8, the seconds it has still to appear; 8 and over, 8 plus
-- the seconds since it was defeated.
type foes = (f32, f32, f32, f32, f32, f32)

def foe_state (fs: foes) (i: i32) : f32 =
  if i == 0 then fs.0 else if i == 1 then fs.1 else if i == 2 then fs.2
  else if i == 3 then fs.3 else if i == 4 then fs.4 else fs.5

def appear_time : f32 = 1.5 -- seconds an enemy takes to appear
def fall_time : f32 = 3 -- and a defeated one to fall out of sight

-- Random number k (in [0, 1)) of enemy i, generation g.
def enemy_rnd (seed: i32) (g: i32) (i: i32) (k: i32) : f32 = hash3 (seed + 7919 * g) (i + 17) (k + 101)

-- Where enemy i is (in the world) and which way it faces (a yaw: its +z
-- points that way); its generation; how far it has still to appear (0..1)
-- and how long since it was defeated (or -1); its size, and how far it has
-- tumbled over (radians, about its x axis through tumble_axis).
type enemy = {pos: vec3, yaw: f32, gen: i32, appear: f32, dying: f32, size: f32, tumble: f32}

def tumble_axis : vec3 = vec 0 0.5 0

def enemy_at (seed: i32) (state: f32) (i: i32) (secs: f32) (fl: flight) : enemy =
  let gf = f32.floor (state / 16)
  let phase = state - 16 * gf
  let gen = i32.f32 gf
  let appear = if phase < 8 then saturate (phase / appear_time) else 0
  let dying = if phase >= 8 then phase - 8 else -1
  let r = enemy_rnd seed gen i
  let radius = 9 + 12 * r 0
  let speed = (0.04 + 0.06 * r 1) * (if r 2 < 0.5 then -1 else 1)
  -- (spread round him, then each drifting round at its own speed)
  let a = 2 * f32.pi * (f32.i32 i / f32.i32 max_enemies + 0.12 * r 3) + speed * secs
  let h = -1.5 + 4 * r 4 + 0.5 * f32.sin (0.6 * secs + 6.3 * r 5)
  let off = vec (radius * f32.sin a) h (radius * f32.cos a)
  -- (a defeated one falls, faster and faster, tumbling over, and shrinks
  -- away at the end; a new one grows in)
  let fall = if dying >= 0 then 2.5 * dying * dying else 0
  let size = if dying >= 0 then smoothstep fall_time (fall_time - 1) dying else 1 - 0.95 * appear * appear
  in { pos = fl.pos +^ off -^ vec 0 fall 0, yaw = f32.atan2 (-off.x) (-off.z), gen, appear, dying, size
     , tumble = if dying >= 0 then 1.4 * dying + 0.5 * dying * dying else 0 }

-- Whether it is drawn at all.
def shown (e: enemy) : bool = e.dying < fall_time

-- A point of his space in enemy e's (where it is modelled at full size,
-- upright), and how distances there scale back to his.
def enemy_space (fl: flight) (e: enemy) (p: vec3) : vec3 =
  let q = yaw_inv e.yaw (to_world fl p -^ e.pos) -^ tumble_axis
  let (c, s) = (f32.cos e.tumble, f32.sin e.tumble)
  let q = vec q.x (c * q.y + s * q.z) (c * q.z - s * q.y) +^ tumble_axis
  in scale (1 / f32.max 0.02 e.size) q

-- A colour from hue (0..1, round the wheel), saturation and value.
def hsv (h: f32) (s: f32) (v: f32) : vec3 =
  let k (n: f32) = let t = 6 * fract (n / 6 + h)
                   in v - v * s * saturate (f32.min t (f32.min (4 - t) 1))
  in vec (k 5) (k 3) (k 1)

type looks = { kind: i32, robe: vec3, hat: vec3, beard: vec3, skin: vec3, circle: vec3, carpet: vec3
             , hem: f32, hat_h: f32, droop: f32, beard_len: f32, phase: f32 }

def looks_of (seed: i32) (g: i32) (i: i32) : looks =
  let r = enemy_rnd seed g i
  let robe_hue = r 10
  let hat_hue = if r 11 < 0.5 then robe_hue else fract (robe_hue + 0.35 + 0.3 * r 12)
  let b = r 13
  let beard = if b < 0.3 then vec 0.85 0.85 0.85 else if b < 0.5 then vec 0.4 0.4 0.42
              else if b < 0.7 then vec 0.03 0.03 0.03 else if b < 0.85 then vec 0.5 0.17 0.05 else vec 0.18 0.09 0.04
  in { kind = i32.f32 (f32.min 2 (3 * r 14))
     , robe = hsv robe_hue (0.6 + 0.35 * r 15) (0.2 + 0.3 * r 16)
     , hat = hsv hat_hue (0.5 + 0.4 * r 17) (0.1 + 0.25 * r 18)
     , beard
     , skin = mix (vec 0.85 0.6 0.48) (vec 0.3 0.17 0.1) (r 19)
     , circle = hsv (r 20) (0.75 + 0.25 * r 21) 1
     , carpet = hsv (r 22) 0.85 (0.25 + 0.25 * r 23)
     , hem = 0.36 + 0.14 * r 24
     , hat_h = 0.5 + 0.45 * r 25
     , droop = 0.3 * r 26
     , beard_len = 0.2 + 0.4 * r 27
     , phase = 6.3 * r 28 }

-- The staff, the hand on it and the orb on its tip.
def enemy_grip : vec3 = vec 0.45 1.05 0.18
def enemy_orb : vec3 = vec 0.45 2.05 0.2

-- The figure, in its own space: (distance, part).
def sd_enemy_figure (lk: looks) (secs: f32) (q: vec3) : (f32, i32) =
  let robe = sd_round_cone_ab q (vec 0 0.06 0) (vec 0 1.32 0) lk.hem 0.15
  -- one hand on the staff, the other raised and lowered as if casting
  let cast = 0.5 + 0.5 * f32.sin (1.3 * secs + lk.phase)
  let hand = vec (-0.3) (1.12 + 0.38 * cast) (0.28 + 0.12 * cast)
  let arms = f32.min (sd_capsule q (vec 0.17 1.3 0) (enemy_grip -^ vec 0.06 0 0) 0.065)
                     (sd_capsule q (vec (-0.17) 1.3 0) (hand -^ scale 0.06 (normalise (hand -^ vec (-0.17) 1.3 0))) 0.065)
  let cloth = smin 0.05 robe arms
  let head = smin 0.03 (sd_sphere (q -^ vec 0 1.53 0.01) 0.15) (sd_sphere (q -^ vec 0 1.51 0.16) 0.04)
  let skin = f32.min head (f32.min (sd_sphere (q -^ enemy_grip) 0.05) (sd_sphere (q -^ hand) 0.05))
  let beard = sd_round_cone_ab q (vec 0 1.47 0.09) (vec 0 (1.47 - lk.beard_len) 0.15) 0.11 0.015
  let hq = q -^ vec 0 1.64 0
  let hat = f32.min (sd_cylinder hq 0.32 0.01 - 0.008)
                    (sd_round_cone_ab hq (vec 0 0 0) (vec 0 lk.hat_h (-lk.droop)) 0.21 0.012)
  let band = sd_torus (hq -^ vec 0 0.05 0) 0.2 0.024
  let staff = sd_capsule q (vec 0.45 0 0.18) (enemy_orb -^ vec 0 0.08 0) 0.025
  let orb = sd_sphere (q -^ enemy_orb) 0.09
  in (cloth, part_robe) |> umin (skin, part_skin) |> umin (beard, part_beard) |> umin (hat, part_hat)
     |> umin (band, part_band) |> umin (staff, part_staff) |> umin (orb, part_orb)

-- The enemies near his space's point p (n of them; tagged materials).
#[noinline]
def enemies_dist (n: i32) (seed: i32) (fs: foes) (secs: f32) (fl: flight) (p: vec3) : hit =
  tag layers_enemies <|
  loop best = nothing for i < i32.min n max_enemies do
    let e = enemy_at seed (foe_state fs i) i secs fl
    in if !(shown e) then best else
    let q = enemy_space fl e p
    let bound = e.size * (length (q -^ enemy_centre) - enemy_radius)
    let m = m_enemy + enemy_parts * i
    in if bound > f32.min 0.3 best.0 then umin best (bound, m) else
    let lk = looks_of seed e.gen i
    let (pd, pm) = platform_exact lk.kind secs q
    let (fd, fpart) = sd_enemy_figure lk secs q
    let h = if fd < pd then (e.size * fd, m + fpart) else (e.size * pd, m + if pm == m_ground then part_top else part_side)
    in umin best h

-- Sphere-traces the enemies alone along ro + t rd up to tmax: (distance,
-- tagged material), or infinity. Only the stretch of the ray through their
-- bounding spheres is marched (most rays miss them all, at the cost of a
-- ray-sphere test each).
def march_enemies (n: i32) (seed: i32) (fs: foes) (secs: f32) (fl: flight) (ro: vec3) (rd: vec3) (tmax: f32) : (f32, i32) =
  let (t0, t1) =
    loop (t0, t1) = (f32.inf, -f32.inf) for i < i32.min n max_enemies do
      let e = enemy_at seed (foe_state fs i) i secs fl
      -- (tumbling moves the centre round the axis: a wider sphere covers it)
      let c = to_local_dir fl (e.pos +^ tumble_axis -^ fl.pos)
      let (a, b) = ray_sphere c (e.size * enemy_radius + 0.6) ro rd
      in if !(shown e) || a > b || b < 0 then (t0, t1) else (f32.min t0 a, f32.max t1 b)
  let t1 = f32.min t1 tmax
  in if t0 >= t1 then (f32.inf, 0) else
  let (t, _, hit, m) =
    loop (t, i, hit, m) = (f32.max 0.02 t0, 0i32, false, 0i32)
    while i < 128 && t < t1 && !hit do
      let (d, mat) = enemies_dist n seed fs secs fl (ro +^ scale t rd)
      in if d < 0.0005 * t then (t, i + 1, true, mat) else (t + d, i + 1, false, m)
  in if hit then (t, m) else (f32.inf, 0)
