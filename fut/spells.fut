-- Spells in flight, drawn as glows added along each camera ray: closed-form
-- falloff around points and segments, so they cost no extra marching. Their
-- state lives in the harness (web/spells.ts), which passes it each frame as
-- a flat array, `spell_stride` floats per slot:
--
--   kind, x, y, z, a, b, c, age (seconds)
--
--   spell_fire    a bolt at (x, y, z) moving at velocity (a, b, c)
--   spell_strike  lightning striking the ground at (x, y, z) from height b;
--                 a seeds its path
--   spell_wind    a blade at (x, y, z) moving at (a, b, c)
--   spell_burst   a fire bolt's explosion at (x, y, z)
--
-- Any other kind is an empty slot. Positions are in the world.

import "vec3"
import "hash"
import "params"
import "shading"

def spell_stride : i64 = 8
def spell_fire : i32 = 0
def spell_strike : i32 = 1
def spell_wind : i32 = 2
def spell_burst : i32 = 3

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

-- A glowing line: a hot core of radius r and a soft halo of radius h.
def glow_line (r: f32) (h: f32) (dist: f32) : (f32, f32) =
  (f32.exp (-(dist * dist) / (r * r)), h * h / (dist * dist + h * h))

-- The light the spells add along the world-space ray o + t d, t in
-- [0, tmax] (nearer than whatever the ray hits), dimmed by the fog.
def spell_glow [k] (prm: params) (spells: [k]f32) (secs: f32) (o: vec3) (d: vec3) (tmax: f32) : vec3 =
  let n = k / spell_stride
  let at (i: i64) (k: i64) = #[unsafe] spells[i * spell_stride + k]
  let tmax = f32.min tmax prm.view_distance
  -- (glows fade with the fog, and with depth behind what the ray hits, over
  -- about their size: a burst on the ground still shows, but nothing
  -- shines through the rock)
  let seen (t: f32) (behind: f32) (size: f32) = (1 - fog_at prm t) * f32.exp (-behind / size)
  in loop c = splat 0 for i < n do
       let kind = i32.f32 (at i 0)
       let p = vec (at i 1) (at i 2) (at i 3)
       let q = vec (at i 4) (at i 5) (at i 6)
       let age = at i 7
       in if kind == spell_fire then
            -- a ball with a short trail behind it, flickering
            let tail = p -^ scale 0.06 q
            let (dist, t, behind) = ray_segment o d tmax tail p
            let (core, halo) = glow_line 0.09 0.6 dist
            let flick = 0.85 + 0.15 * f32.sin (secs * 43 + f32.i64 i * 1.7)
            let hot = f32.exp (-(dist * dist) / 0.0016)
            in c +^ scale (flick * seen t behind 0.3) (scale (5 * core) (mix (vec 1 0.42 0.08) (vec 1 0.9 0.6) hot)
                                           +^ scale (0.5 * halo) (vec 1 0.35 0.05))
          else if kind == spell_burst then
            -- an expanding, fading ball of fire
            let r = 0.3 + 3.5 * age
            let (dist, t, behind) = ray_segment o d tmax p p
            let k = f32.exp (-6 * age)
            let (core, halo) = glow_line r (2 * r) dist
            in c +^ scale (k * seen t behind r) (scale (4 * core) (vec 1 0.5 0.12) +^ scale (0.6 * halo) (vec 1 0.3 0.05))
          else if kind == spell_wind then
            -- a pale crescent across its path, curving back
            let f = normalise q
            let side = normalise (cross f (vec 0 1 0) +^ vec 0 0.001 0)
            let up = cross side f
            -- (it rolls as it flies)
            let roll = 2.5 * age
            let s = scale (f32.cos roll) side +^ scale (f32.sin roll) up
            let fade = saturate (1.5 - age) * saturate (8 * age)
            let point (k: i32) = let th = (f32.i32 k / 4 - 0.5) * 2.2
                                 in p +^ scale (0.9 * f32.sin th) s -^ scale (0.55 * (1 - f32.cos th)) f
            let (dist, t, behind) = loop (best, bt, bb) = (f32.inf, 0f32, 0f32) for k < 4 do
                                      let (dd, tt, b) = ray_segment o d tmax (point k) (point (k + 1))
                                      in if dd < best then (dd, tt, b) else (best, bt, bb)
            let (core, halo) = glow_line 0.035 0.3 dist
            in c +^ scale (fade * seen t behind 0.2) (scale (3 * core) (vec 0.75 1 0.95) +^ scale (0.35 * halo) (vec 0.45 0.85 0.8))
          else if kind == spell_strike then
            -- a jagged bolt from the sky, re-forking several times a second,
            -- flashing as it fades, with a flash on the ground
            let seed = i32.f32 (at i 4)
            let top = vec p.x (at i 5) p.z
            let flash = i32.f32 (age * 14)
            let env = f32.exp (-5 * age) * (0.55 + 0.45 * hash2 (seed + flash) 17)
            let point (k: i32) =
              let u = f32.i32 k / 8
              let jx = if k == 0 || k == 8 then 0 else hash2 (seed * 13 + k) flash - 0.5
              let jz = if k == 0 || k == 8 then 0 else hash2 (seed * 7 - k) (flash + 3) - 0.5
              in mix top p u +^ vec (2.4 * jx) 0 (2.4 * jz)
            let (dist, t, behind) = loop (best, bt, bb) = (f32.inf, 0f32, 0f32) for k < 8 do
                                      let (dd, tt, b) = ray_segment o d tmax (point k) (point (k + 1))
                                      in if dd < best then (dd, tt, b) else (best, bt, bb)
            let (core, halo) = glow_line 0.06 0.9 dist
            let (gd, gt, gb) = ray_segment o d tmax p p
            let ground = 2.5 * 2.5 / (gd * gd + 2.5 * 2.5)
            in c +^ scale env (scale (seen t behind 0.5) (scale (9 * core) (vec 0.85 0.9 1) +^ scale (0.8 * halo) (vec 0.55 0.6 1))
                               +^ scale (1.2 * ground * seen gt gb 1.5) (vec 0.6 0.65 1))
          else c
