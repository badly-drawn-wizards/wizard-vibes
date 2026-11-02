-- Spells in flight, drawn as glows added along each camera ray: closed-form
-- falloff around points and segments, so they cost no extra marching. Their
-- state lives in the harness (web/spells.ts), which passes it each frame as
-- a flat array, `spell_stride` floats per slot:
--
--   kind, x, y, z, a, b, c, age, life (seconds), seed (an integer)
--
--   spell_fire    a bolt at (x, y, z) moving at velocity (a, b, c)
--   spell_strike  lightning striking the ground at (x, y, z) from height b
--   spell_wind    a blade at (x, y, z) moving at (a, b, c)
--   spell_burst   a fire bolt's explosion at (x, y, z)
--   spell_gust    a wind blade's burst into small blades at (x, y, z); it
--                 was moving at (a, b, c)
--   spell_mark    where an enemy's strike will land, (x, y, z), when its
--                 life is up (it then becomes the strike, from height b)
--
-- Every spell fades out over the last part of its life, so none vanishes
-- while still visible; the seed makes each one a little different.
--
-- Any other kind is an empty slot. Positions are in the world.

import "vec3"
import "hash"
import "params"
import "shading"
import "world"

def spell_stride : i64 = 10
def spell_fire : i32 = 0
def spell_strike : i32 = 1
def spell_wind : i32 = 2
def spell_burst : i32 = 3
def spell_gust : i32 = 4
def spell_mark : i32 = 5

-- Where the ray o + t d (t in [0, tmax]) comes closest to the segment a-b:
-- (distance, t, and how far beyond tmax the nearest point of the segment
-- lies along the ray: how deep behind what the ray hits).
def ray_segment (o: vec3) (d: vec3) (tmax: f32) (a: vec3) (b: vec3) : (f32, f32, f32) =
  let v = b -^ a
  let vv = f32.max 1e-8 (dot v v)
  let w0 = o -^ a
  let dv = dot d v
  let denom = vv - dv * dv
  let s = if denom > 1e-6 then saturate ((dot v w0 * 1 - dv * dot d w0) / denom) else 0
  let t = clamp 0 tmax (dot (a +^ scale s v -^ o) d)
  let s = saturate (dot (o +^ scale t d -^ a) v / vv)
  let q = a +^ scale s v
  in (length (o +^ scale t d -^ q), t, f32.max 0 (dot (q -^ o) d - tmax))

-- ray_segment for the polyline through point 0, 1, .., n: the nearest of
-- its segments.
def ray_polyline (o: vec3) (d: vec3) (tmax: f32) (n: i32) (point: i32 -> vec3) : (f32, f32, f32) =
  loop (best, bt, bb) = (f32.inf, 0f32, 0f32) for k < n do
    let (dd, tt, b) = ray_segment o d tmax (point k) (point (k + 1))
    in if dd < best then (dd, tt, b) else (best, bt, bb)

-- A glowing crescent centred at c, facing f (its tips curve back, away
-- from f) and spanning `width` either side along s: six segments, thickest
-- in the middle and tapering to the tips. (core, halo, t, behind) of its
-- brightest part along the ray (see ray_segment, glow_line).
def crescent (o: vec3) (d: vec3) (tmax: f32) (c: vec3) (f: vec3) (s: vec3) (width: f32) (bend: f32) (thick: f32) : (f32, f32, f32, f32) =
  let point (k: i32) = let th = (f32.i32 k / 6 - 0.5) * 2.4
                       in c +^ scale (width * f32.sin th) s -^ scale (bend * (1 - f32.cos th)) f
  in loop (core, halo, bt, bb) = (0f32, 0f32, 0f32, 0f32) for k < 6 do
       let (dd, tt, b) = ray_segment o d tmax (point k) (point (k + 1))
       let taper = f32.sin (f32.pi * (f32.i32 k + 0.5) / 6)
       let r = thick * (0.2 + 0.8 * taper)
       let cr = f32.exp (-(dd * dd) / (r * r))
       let hl = (5 * r) * (5 * r) / (dd * dd + (5 * r) * (5 * r))
       in if cr > core then (cr, f32.max halo hl, tt, b) else (core, f32.max halo hl, bt, bb)

-- A glowing line: a hot core of radius r and a soft halo of radius h.
def glow_line (r: f32) (h: f32) (dist: f32) : (f32, f32) =
  (f32.exp (-(dist * dist) / (r * r)), h * h / (dist * dist + h * h))

-- The light one spell (in slot i) adds along the ray. A real GPU function,
-- taking the slot's values as scalars (such a function cannot take the
-- array): spell_glow sums it over the slots.
#[noinline]
def spell_light (fog: f32) (view_distance: f32) (secs: f32) (o: vec3) (d: vec3) (tmax: f32)
                (i: i64) (kind: i32) (p: vec3) (q: vec3) (age: f32) (life: f32) (seed: i32) : vec3 =
       let fade = smoothstep life (0.6 * life) age
       let rnd (n: i32) = hash2 (seed + n * 7919) 23
       -- (glows fade with the fog, and with depth behind what the ray hits,
       -- over about their size: a burst on the ground still shows, but
       -- nothing shines through the rock)
       let seen (t: f32) (behind: f32) (size: f32) = (1 - fog_amount fog view_distance t) * f32.exp (-behind / size)
       in if kind == spell_fire then
            -- a ball with a short trail behind it, flickering
            let tail = p -^ scale 0.06 q
            let (dist, t, behind) = ray_segment o d tmax tail p
            let (core, halo) = glow_line 0.12 0.8 dist
            let flick = 0.85 + 0.15 * f32.sin (secs * 43 + f32.i64 i * 1.7)
            let hot = f32.exp (-(dist * dist) / 0.0016)
            in scale (fade * flick * seen t behind 0.3) (scale (5 * core) (mix (vec 1 0.42 0.08) (vec 1 0.9 0.6) hot)
                                           +^ scale (0.5 * halo) (vec 1 0.35 0.05))
          else if kind == spell_burst then
            -- An explosion: a billowing fireball that swells fast and cools
            -- from yellow-white through orange to a dull red, a shockwave
            -- ring racing out from it, and sparks thrown out and falling.
            let r = 3.2 * (1 - f32.exp (-5 * age)) + 0.2
            let (dist, t, behind) = ray_segment o d tmax p p
            let x = o +^ scale t d -- where the ray passes closest
            let turb = vnoise2 ((x.x - p.x) * 1.4 + (x.z - p.z) * 0.7 + 7 * f32.i64 i) ((x.y - p.y) * 1.4 - 3 * age)
            let heat = f32.exp (-2.2 * age)
            let body = smoothstep r (0.25 * r) (dist * (0.8 + 0.45 * turb))
            let colour = mix (vec 0.9 0.12 0.02) (mix (vec 1 0.45 0.08) (vec 1 0.9 0.6) (saturate (2 * heat - 0.6))) (saturate (1.5 * heat * (1 - dist / r)))
            let halo = (1.6 * r) * (1.6 * r) / (dist * dist + (1.6 * r) * (1.6 * r))
            let ring_r = 9 * age
            -- (squared by hand: WGSL's pow is undefined for negative bases)
            let ring = let u = (dist - ring_r) / (0.25 + 0.4 * age) in f32.exp (-(u * u)) * f32.exp (-4 * age)
            let fire = scale (7 * heat * body) colour +^ scale (0.55 * heat * halo) (vec 1 0.35 0.06)
                       +^ scale (1.6 * ring) (vec 1 0.75 0.45)
            -- sparks: hashed directions, mostly upwards, falling under gravity
            let sparks = loop acc = 0f32 for j < 8 do
              let h (n: i32) = hash2 (i32.i64 i * 31 + j * 7 + n) 91
              let dir = normalise (vec (h 1 - 0.5) (0.4 + h 2) (h 3 - 0.5))
              let speed = 7 + 6 * h 4
              let at_t (a: f32) = p +^ scale (speed * a) dir -^ vec 0 (4.9 * a * a) 0
              let (sd, _, _) = ray_segment o d tmax (at_t (f32.max 0 (age - 0.05))) (at_t age)
              in acc + f32.exp (-(sd * sd) / 0.0025)
            let ember = scale (5 * sparks * f32.exp (-2.5 * age)) (vec 1 0.6 0.2)
            in scale (fade * seen t behind r) (fire +^ ember)
          else if kind == spell_wind then
            -- a pale crescent across its path, its tips curving back, sharp
            -- at the ends; each its own width, curve and tilt, rolling
            -- slowly and wobbling a little; three fading echoes trail it
            let f = normalise q
            let side = normalise (cross f (vec 0 1 0) +^ vec 0 0.001 0)
            let up = cross side f
            let spin = if rnd 1 < 0.5 then -1 else 1
            let width = 0.7 + 0.45 * rnd 6
            let bend = 0.35 + 0.35 * rnd 7
            let fade = fade * saturate (8 * age)
            in loop c = splat 0 for k < 4 do
                 let a = age - 0.035 * f32.i32 k
                 let roll = (rnd 2 - 0.5) * 1.2 + spin * (0.25 + 0.35 * rnd 3) * a
                            + 0.12 * f32.sin (a * (5 + 4 * rnd 4) + 6.283 * rnd 5)
                 let s = scale (f32.cos roll) side +^ scale (f32.sin roll) up
                 let centre = p -^ scale (0.035 * f32.i32 k) q
                 let (core, halo, t, behind) = crescent o d tmax centre f s width bend (0.07 - 0.012 * f32.i32 k)
                 let k_gain = 0.45 ** f32.i32 k
                 in c +^ scale (k_gain * fade * seen t behind 0.2)
                                (scale (3.2 * core) (vec 0.85 1 0.95) +^ scale (0.4 * halo) (vec 0.4 0.85 0.75))
          else if kind == spell_gust then
            -- the blade's burst: eight small crescents flying out (mostly
            -- the way it was going), slowing, spinning and shrinking, and a
            -- faint ring of air spreading from where it broke
            -- (only the travel's horizontal part aims them: a blade bursting
            -- on the ground was heading into it)
            let travel = vec q.x 0 q.z
            let blades = loop c = splat 0 for j < 8 do
              let h (n: i32) = hash2 (seed * 7 + j * 31 + n) 41
              let dir = normalise (vec (h 1 - 0.5) (0.15 + 0.55 * h 2) (h 3 - 0.5) +^ scale 0.018 travel)
              let speed = 9 * (0.7 + 0.6 * h 4)
              let centre = p +^ scale (speed * (1 - f32.exp (-3 * age)) / 3) dir
              let side = normalise (cross dir (vec 0 1 0) +^ vec 0.001 0 0)
              let up = cross side dir
              let roll = 6.283 * h 5 + (if h 6 < 0.5 then -9 else 9) * age
              let s = scale (f32.cos roll) side +^ scale (f32.sin roll) up
              let size = 0.5 * (1 - 0.5 * age / life)
              let (core, halo, t, behind) = crescent o d tmax centre dir s size (0.45 * size) 0.05
              in c +^ scale (seen t behind 0.2) (scale (3.5 * core) (vec 0.85 1 0.95) +^ scale (0.45 * halo) (vec 0.4 0.85 0.75))
            let (gd, gt, gb) = ray_segment o d tmax p p
            let ring = let u = (gd - 5 * age) / (0.2 + 0.3 * age) in f32.exp (-(u * u)) * f32.exp (-4 * age)
            in scale fade (blades +^ scale (0.8 * ring * seen gt gb 1) (vec 0.6 0.95 0.85))
          else if kind == spell_strike then
            -- a jagged bolt from the sky, re-forking several times a second,
            -- flashing as it fades, with a flash on the ground
            let top = vec p.x q.y p.z
            let flash = i32.f32 (age * 14)
            let env = fade * f32.exp (-4 * age) * (0.55 + 0.45 * hash2 (seed + flash) 17)
            let point (k: i32) =
              let u = f32.i32 k / 8
              let jx = if k == 0 || k == 8 then 0 else hash2 (seed * 13 + k) flash - 0.5
              let jz = if k == 0 || k == 8 then 0 else hash2 (seed * 7 - k) (flash + 3) - 0.5
              in mix top p u +^ vec (2.4 * jx) 0 (2.4 * jz)
            let (dist, t, behind) = ray_polyline o d tmax 8 point
            let (core, halo) = glow_line 0.06 0.9 dist
            let (gd, gt, gb) = ray_segment o d tmax p p
            let ground = 3.5 * 3.5 / (gd * gd + 3.5 * 3.5)
            -- and a ring of light racing out over the ground where it hits
            let ring = let u = (gd - 7 * age) / (0.3 + 0.5 * age) in fade * f32.exp (-(u * u)) * f32.exp (-3 * age)
            in scale env (scale (seen t behind 0.5) (scale (9 * core) (vec 0.85 0.9 1) +^ scale (0.8 * halo) (vec 0.55 0.6 1))
                               +^ scale (1.5 * ground * seen gt gb 2) (vec 0.6 0.65 1))
                 +^ scale (2 * ring * seen gt gb 3) (vec 0.7 0.8 1)
          else if kind == spell_mark then
            -- a red ring closing in on where the strike will land, turning,
            -- brighter and faster to pulse as it comes; and a thread of
            -- light down to it from the sky
            let u = saturate (age / life)
            let r = 2.2 - 1.3 * u
            let point (k: i32) = let a = f32.i32 k * (2 * f32.pi / 20) + 2 * age
                                 in p +^ vec (r * f32.cos a) 0 (r * f32.sin a)
            let (dist, t, behind) = ray_polyline o d tmax 20 point
            let (core, halo) = glow_line 0.03 0.15 dist
            let (bd, bt, bb) = ray_segment o d tmax (vec p.x q.y p.z) p
            let (bcore, bhalo) = glow_line 0.012 0.08 bd
            let env = (0.4 + u) * (0.75 + 0.25 * f32.sin (age * (12 + 25 * u)))
            in scale env (scale (seen t behind 0.5) (scale (3 * core) (vec 1 0.25 0.3) +^ scale (0.25 * halo) (vec 1 0.1 0.15))
                          +^ scale (u * seen bt bb 0.5) (scale (1.5 * bcore) (vec 1 0.4 0.4) +^ scale (0.1 * bhalo) (vec 1 0.1 0.1)))
          else splat 0

-- The light the spells add along the world-space ray o + t d, t in
-- [0, tmax] (nearer than whatever the ray hits), dimmed by the fog.
def spell_glow [k] (prm: params) (spells: [k]f32) (secs: f32) (o: vec3) (d: vec3) (tmax: f32) : vec3 =
  let n = k / spell_stride
  let at (i: i64) (j: i64) = #[unsafe] spells[i * spell_stride + j]
  let tmax = f32.min tmax prm.view_distance
  in loop c = splat 0 for i < n do
       c +^ spell_light prm.fog prm.view_distance secs o d tmax i (i32.f32 (at i 0)) (vec (at i 1) (at i 2) (at i 3))
                        (vec (at i 4) (at i 5) (at i 6)) (at i 7) (at i 8) (i32.f32 (at i 9))
