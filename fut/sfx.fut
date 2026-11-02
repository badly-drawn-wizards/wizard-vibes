-- Sound effects, made as the music is: each stereo sample a pure function
-- of the time since the effect began. The harness renders each one once
-- (the `sfx` entry point) and keeps it (web/sfx.ts).
--
-- Without filters (they need state from sample to sample), noise is made of
-- many sine waves at random frequencies within a band, and a band can sweep
-- up or down by scaling those frequencies (each phase the integral of its
-- frequency). Bells are sums of a few inharmonic partials.

import "vec3"
import "hash"

def tau : f32 = 2 * f32.pi

def sfx_cast_fire : i32 = 0
def sfx_cast_strike : i32 = 1
def sfx_cast_wind : i32 = 2
def sfx_burst : i32 = 3
def sfx_thunder : i32 = 4
def sfx_gust : i32 = 5
def sfx_hurt : i32 = 6
def sfx_defeat : i32 = 7
def sfx_shield_up : i32 = 8
def sfx_block : i32 = 9
def sfx_shield_down : i32 = 10
def sfx_fizzle : i32 = 11
def sfx_mark : i32 = 12
def sfx_appear : i32 = 13
def sfx_count : i32 = 14

-- How long each lasts, in seconds.
def sfx_length (k: i32) : f32 =
  if k == sfx_cast_fire then 0.6 else if k == sfx_cast_strike then 0.5 else if k == sfx_cast_wind then 0.7
  else if k == sfx_burst then 1.6 else if k == sfx_thunder then 2.2 else if k == sfx_gust then 0.8
  else if k == sfx_hurt then 0.5 else if k == sfx_defeat then 1.6 else if k == sfx_shield_up then 0.7
  else if k == sfx_block then 0.9 else if k == sfx_shield_down then 0.5 else if k == sfx_fizzle then 0.3
  else if k == sfx_mark then 1.1 else 1

-- An envelope: up over `attack` seconds, then dying away by e every `decay`.
def env (attack: f32) (decay: f32) (t: f32) : f32 =
  if t < 0 then 0 else (1 - f32.exp (-t / attack)) * f32.exp (-t / decay)

-- Noise in the band [lo, hi] Hz (n sines, spread evenly in pitch), its
-- frequencies all scaled by e^(rate t) (a sweep; 0 for none). seed varies it.
def band (seed: i32) (n: i32) (lo: f32) (hi: f32) (rate: f32) (t: f32) : f32 =
  -- (the phase of a frequency f e^(rate t) is f (e^(rate t) - 1) / rate)
  let warp = if f32.abs rate < 1e-4 then t else (f32.exp (rate * t) - 1) / rate
  let s = loop s = 0f32 for k < n do
            let f = lo * (hi / lo) ** hash2 (seed + k) 3
            in s + f32.sin (tau * (f * warp + hash2 (seed + k) 5))
  in s / f32.sqrt (f32.i32 n)

-- A tone of frequency f0 e^(rate t) with `harmonics` harmonics at 1/k.
def tone (f0: f32) (rate: f32) (harmonics: i32) (t: f32) : f32 =
  let warp = if f32.abs rate < 1e-4 then t else (f32.exp (rate * t) - 1) / rate
  in loop s = 0f32 for k < harmonics do
       let kf = f32.i32 (k + 1)
       in s + f32.sin (tau * f0 * kf * warp) / kf

-- A bell struck at time 0: partials at the ratios of a struck bar.
def bell (f: f32) (decay: f32) (t: f32) : f32 =
  if t < 0 then 0 else
  let p (r: f32) (a: f32) (d: f32) = a * f32.sin (tau * f * r * t) * f32.exp (-t / (decay * d))
  in (1 - f32.exp (-t * 900)) * (p 1 1 1 + p 2.76 0.5 0.6 + p 5.4 0.25 0.35 + p 8.93 0.12 0.2)

-- Crackle: sparse clicks, `rate` a second on average.
def crackle (seed: i32) (rate: f32) (t: f32) : f32 =
  let bin = 0.004
  let i = i32.f32 (t / bin)
  let u = t - f32.i32 i * bin
  in if hash2 (seed + i) 11 > rate * bin then 0
     else (2 * hash2 (seed + i) 13 - 1) * f32.exp (-u * 900)

-- Effect k at time t: (left, right). (Each side has noise of its own, for
-- width.)
def sfx_at (k: i32) (t: f32) : (f32, f32) =
  let side (c: i32) =
    let sd = 1000 * c
    in if k == sfx_cast_fire then
         -- a rising whoosh, crackling
         0.5 * env 0.03 0.2 t * band (sd + 1) 20 250 2200 1.6 t + 0.25 * env 0.01 0.25 t * crackle (sd + 2) 120 t
       else if k == sfx_cast_strike then
         -- a zap: a buzzing tone diving in pitch
         let buzz = 0.6 + 0.4 * f32.sin (tau * 60 * t)
         in 0.3 * env 0.003 0.12 t * buzz * tone 1400 (-7) 8 t + 0.12 * env 0.002 0.05 t * band (sd + 3) 16 2000 7000 0 t
       else if k == sfx_cast_wind then
         -- air rushing past, from one side to the other
         let pan = if c == 0 then 1 - saturate (t / 0.6) else saturate (t / 0.6)
         in 0.4 * (0.4 + 0.6 * pan) * pow (f32.sin (f32.pi * saturate (t / 0.7))) 2 * band (sd + 4) 24 500 4500 0.8 t
       else if k == sfx_burst then
         -- a boom, a roar of fire, and crackling as it dies
         let boom = f32.sin (tau * (35 * t + 30 / 6 * (1 - f32.exp (-6 * t)))) * env 0.004 0.35 t
         in 0.55 * boom + 0.35 * env 0.01 0.4 t * band (sd + 5) 24 60 900 (-0.8) t
            + 0.18 * env 0.05 0.5 t * crackle (sd + 6) (60 * f32.exp (-t)) t
       else if k == sfx_thunder then
         -- a sharp crack, then rumbling rolls
         let crack = 0.5 * env 0.001 0.05 t * band (sd + 7) 24 300 9000 0 t
         let rolls = 0.6 + 0.4 * f32.sin (tau * 3.1 * t + 1) * f32.sin (tau * 1.3 * t)
         in crack + 0.55 * env 0.04 0.7 t * rolls * band (sd + 8) 20 30 220 0 t
       else if k == sfx_gust then
         -- the blade shattering: high chimes, scattered
         let chime (j: i32) = let at = 0.15 * hash2 (sd + j) 21
                                  let f = 2000 + 4000 * hash2 (sd + j) 23
                              in env 0.001 0.12 (t - at) * f32.sin (tau * f * (t - at))
         let chimes = loop s = 0f32 for j < 8 do s + chime j
         in 0.12 * chimes + 0.2 * env 0.005 0.15 t * band (sd + 9) 16 3000 9000 0 t
       else if k == sfx_hurt then
         -- a thud and a sour chord
         let thud = f32.sin (tau * (60 * t + 50 / 12 * (1 - f32.exp (-12 * t)))) * env 0.003 0.12 t
         in 0.6 * thud + 0.12 * env 0.005 0.25 t * (tone 330 0 4 t + tone 349 0 4 t)
       else if k == sfx_defeat then
         -- bells falling down a minor arpeggio, and a shimmer
         -- (semitones down: 0 3 7 12 15 19, as arithmetic: no arrays here)
         let down (j: i32) = 12 * (j / 3) + (if j % 3 == 0 then 0 else if j % 3 == 1 then 3 else 7)
         let note (j: i32) = bell (880 * 2 ** (-(f32.i32 (down j)) / 12)) 0.5 (t - 0.11 * f32.i32 j)
         let notes = loop s = 0f32 for j < 6 do s + note j
         in 0.16 * notes + 0.06 * env 0.2 0.5 t * band (sd + 10) 16 4000 10000 (-0.5) t
       else if k == sfx_shield_up || k == sfx_shield_down then
         -- a shimmer, rising or falling, trembling
         let rate = if k == sfx_shield_up then 1.6 else -2.4
         let base = if k == sfx_shield_up then 300 else 900
         let s = loop s = 0f32 for j < 6 do s + f32.sin (tau * base * (1 + 0.013 * f32.i32 j) * (2 ** (f32.i32 (j % 3))) * (f32.exp (rate * t) - 1) / rate)
         let len = sfx_length k
         in 0.07 * (0.75 + 0.25 * f32.sin (tau * 14 * t)) * pow (f32.sin (f32.pi * saturate (t / len))) 2 * s
       else if k == sfx_block then
         -- a ringing bell, with a hiss of sparks
         0.3 * bell 620 0.35 t + 0.15 * env 0.002 0.08 t * band (sd + 11) 16 2500 9000 0 t
       else if k == sfx_fizzle then
         -- a short, dull buzz: nothing there
         0.18 * env 0.005 0.1 t * (tone 110 (-1.5) 6 t + 0.5 * band (sd + 12) 12 100 600 0 t)
       else if k == sfx_mark then
         -- a whine rising to the strike, pulsing faster as it does
         let pulse = 0.5 + 0.5 * f32.sin (tau * (6 * t + 9 * t * t))
         in 0.12 * smoothstep 0 0.2 t * pulse * tone 420 1.1 3 t
       else
         -- (sfx_appear) a soft chord swelling in, and a breath of air
         let chord = tone 1046.5 0 2 t + tone 1318.5 0 2 t + tone 1568 0 2 t
         in 0.06 * env 0.08 0.4 t * chord + 0.05 * env 0.1 0.3 t * band (sd + 13) 16 2000 8000 0 t
  in (side 0, side 1)

-- n stereo samples of effect k at `rate` a second, interleaved (left, right).
def sfx_samples (k: i32) (rate: f32) (n: i64) : []f32 =
  tabulate (2 * n) (\j ->
    let (l, r) = sfx_at k (f32.i64 (j / 2) / rate)
    in if j % 2 == 0 then l else r)
