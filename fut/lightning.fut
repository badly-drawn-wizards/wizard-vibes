-- Crackling lightning on the orb, as emissive media integrated by the march.
--
-- Two kinds of bolt: arcs that leap from the orb's surface into the air, and
-- tendrils inside the orb from its core to its glass (plasma-globe style).
-- Each bolt is a jagged polyline. It is re-seeded `lives` times per loop
-- (phase-shifted per bolt), and within a life its brightness flickers and
-- part of its jaggedness is re-randomised, which makes it crackle. All timing
-- is in whole slots of the loop, so the animation still loops seamlessly.

import "vec3"
import "sdf"
import "hash"
import "march"
import "scene"

-- Compile-time switches: arcs leaping out from the orb, and tendrils inside
-- its glass.
def orb_arcs : bool = false
def orb_tendrils : bool = true

def kind_arc : i32 = 0
def kind_tendril : i32 = 1
def kind_enabled (kind: i32) : bool =
  if kind == kind_arc then orb_arcs else orb_tendrils

def bolts_per_kind : i32 = 4
def segments : i32 = 7
def lives : f32 = 30 -- per loop: 2.5 per second
def flickers : f32 = 5 -- per life

def arc_reach : f32 = 0.26 -- arcs end at most this far from the orb's surface

def rand_dir (a: f32) (b: f32) : vec3 =
  let z = 2 * a - 1
  let r = f32.sqrt (f32.max 0 (1 - z * z))
  let phi = 2 * f32.pi * b
  in vec (r * f32.cos phi) z (r * f32.sin phi)

-- A bolt's current life: (seed slot, flicker slot, position in [0,1) within
-- its life). Each bolt is phase-shifted so they don't all change at once.
type life = {slot: i32, flick: i32, u: f32}

def bolt_life (tm: f32) (id: i32) : life =
  let x = tm / (2 * f32.pi) * lives + hash2 id 101 * lives
  let s = f32.floor x
  let u = x - s
  let slot = i32.f32 s % i32.f32 lives
  in {slot, flick = slot * 64 + i32.f32 (u * flickers), u}

-- Brightness over a life: a sharp strike that decays, flickering.
def envelope (id: i32) (l: life) : f32 =
  let strike = if l.u < 0.06 then l.u / 0.06 else f32.exp (-(l.u - 0.06) * 3)
  in strike * (0.45 + 0.55 * hash3 id l.flick 7)

-- A bolt's two ends and length, fixed for its life.
type ends = {a: vec3, b: vec3, len: f32}

def bolt_ends (kind: i32) (id: i32) (l: life) : ends =
  let h (j: i32) = hash3 id l.slot j
  in if kind == kind_arc then
       let d0 = normalise (rand_dir (h 1) (h 2) +^ vec 0 0.6 0)
       let d1 = normalise (d0 +^ scale 0.9 (rand_dir (h 3) (h 4)))
       let len = 0.1 + (arc_reach - 0.12) * h 5
       in {a = orb_pos +^ scale orb_radius d0, b = orb_pos +^ scale (orb_radius + len) d1, len}
     else
       let d = rand_dir (h 1) (h 2)
       in {a = orb_pos +^ scale (0.12 * orb_radius) d, b = orb_pos +^ scale (0.96 * orb_radius) d, len = orb_radius}

-- Jagged offsets are at most this fraction of the bolt's length.
def jag : f32 = 0.3 * 1.4

-- Control point i (0..segments) of a bolt. The jagged offsets are mostly
-- fixed for the life, with a part re-drawn each flicker.
def bolt_point (kind: i32) (id: i32) (l: life) (e: ends) (i: i32) : vec3 =
  let h (j: i32) = hash3 id l.slot j
  let {a, b, len} = e
  let f = f32.i32 i / f32.i32 segments
  let fixed = rand_dir (h (10 + i)) (h (30 + i))
  let crackle = rand_dir (hash3 id l.flick (50 + i)) (hash3 id l.flick (70 + i))
  let offset = scale (0.3 * len * f32.sin (f32.pi * f)) (fixed +^ scale 0.4 crackle)
  let v = (mix a b f +^ offset) -^ orb_pos
  let r = length v
  -- arcs stay outside the orb, tendrils inside its glass
  let r' = if kind == kind_arc then f32.max r orb_radius else f32.min r (0.95 * orb_radius)
  in orb_pos +^ scale (r' / f32.max r 1e-6) v

-- Distance to the nearest point of a bolt's polyline.
def bolt_dist (kind: i32) (id: i32) (l: life) (e: ends) (p: vec3) : f32 =
  let (d, _) =
    loop (d, prev) = (1e9f32, bolt_point kind id l e 0) for i < segments do
      let next = bolt_point kind id l e (i + 1)
      in (f32.min d (sd_capsule p prev next 0), next)
  in d

-- Beyond this distance from a bolt its halo is negligible, so it is skipped.
def bolt_margin : f32 = 0.12

def bolt_core_colour : vec3 = vec 1 1 1
def bolt_halo_colour : vec3 = vec 0.3 0.75 1

-- Lightning media at p: a thin white-hot core and a cyan halo around each
-- bolt. Only evaluated near the orb.
def lightning_media (tm: f32) (p: vec3) : media_sample =
  let reach = orb_radius + arc_reach + 0.15
  in if length (p -^ orb_pos) > reach then {cap = length (p -^ orb_pos) - reach + 0.02, radiance = splat 0}
     else
       loop acc = {cap = 1e9f32, radiance = splat 0} for kind < 2 do
            if !(kind_enabled kind) then acc else
            loop acc for b < bolts_per_kind do
              let id = kind * 100 + b
              let l = bolt_life tm id
              let e = bolt_ends kind id l
              -- the whole bolt lies within this sphere
              let c = mix e.a e.b 0.5
              let r = 0.5 * length (e.b -^ e.a) + jag * e.len
              let dc = length (p -^ c) - r
              in if dc > bolt_margin then acc with cap = f32.min acc.cap (0.3 * (dc + bolt_margin) + 0.002)
                 else
                   let d = bolt_dist kind id l e p
                   let k = envelope id l
                   let core = 220 * f32.exp (-d / 0.003)
                   let halo = 1.4 * f32.exp (-d / 0.025)
                   in { cap = f32.min acc.cap (0.3 * d + 0.002)
                      , radiance = acc.radiance +^ scale k (scale core bolt_core_colour +^ scale halo bolt_halo_colour) }

-- Overall brightness of the bolts right now, in [0, 1]: drives the flicker
-- of the orb's light on the scene.
def flash (tm: f32) : f32 =
  let n = f32.i32 ((i32.bool orb_arcs + i32.bool orb_tendrils) * bolts_per_kind)
  let total =
    loop t = 0f32 for kind < 2 do
      if !(kind_enabled kind) then t else
      loop t for b < bolts_per_kind do
        let id = kind * 100 + b
        in t + envelope id (bolt_life tm id)
  in if n > 0 then total / n else 0
