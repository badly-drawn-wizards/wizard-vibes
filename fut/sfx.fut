-- Sound effects, made by the build like the music, but rendered once each
-- (the `sfx` entry point) and kept by the harness (web/sfx.ts). So they can
-- afford real signal processing: filters and reverb, which need each
-- sample's predecessors, done as scans.
--
-- Every effect is the same small synth, set up differently (patch, below):
--   two layers of white noise, each through a resonant filter (low-pass or
--   band-pass) whose frequency sweeps, each with its own envelope;
--   a tonal part, a pure function of time (zaps, booms, bells, chords,
--   crackle);
--   and a reverb, how much of it varying by effect.
-- Each comes out with its loudest sample at its own level.
--
-- A filter's output depends on its last outputs, which looks sequential,
-- but each sample's step is an affine map of the filter's state, and
-- composing affine maps is associative: so the filter is a scan (biquad).
-- A reverb's comb filter (y[i] = x[i] + g y[i - d]) is d interleaved
-- recurrences of the same kind, one down each column of the signal laid
-- out d wide (comb).

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

---------------------------------------------------------------------
-- Building blocks

-- An envelope: up over `attack` seconds, then dying away by e every `decay`.
def env (attack: f32) (decay: f32) (t: f32) : f32 =
  if t < 0 then 0 else (1 - f32.exp (-t / attack)) * f32.exp (-t / decay)

-- A tone of frequency f0 e^(rate t) with `harmonics` harmonics at 1/k (its
-- phase the integral of its frequency).
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

-- Crackle: sparse clicks, `rate` a second on average, each a tiny burst.
def crackle (seed: i32) (rate: f32) (t: f32) : f32 =
  let bin = 0.004
  let i = i32.f32 (t / bin)
  let u = t - f32.i32 i * bin
  in if hash2 (seed + i) 11 > rate * bin then 0
     else (2 * hash2 (seed + i) 13 - 1) * f32.exp (-u * 900) * f32.sin (tau * (2000 + 3000 * hash2 (seed + i) 17) * u)

-- A biquad's coefficients (b0, b1, b2, a1, a2; normalised): a resonant
-- low-pass, or a band-pass, at f Hz with quality q (after R. Bristow-
-- Johnson's cookbook).
def biquad_coeffs (lowpass: bool) (f: f32) (q: f32) (rate: f32) : (f32, f32, f32, f32, f32) =
  let w = tau * f32.min (0.45 * rate) (f32.max 10 f) / rate
  let (c, s) = (f32.cos w, f32.sin w)
  let alpha = s / (2 * q)
  let a0 = 1 + alpha
  in if lowpass then ((1 - c) / 2 / a0, (1 - c) / a0, (1 - c) / 2 / a0, -2 * c / a0, (1 - alpha) / a0)
     else (alpha / a0, 0, -alpha / a0, -2 * c / a0, (1 - alpha) / a0)

-- An affine map of a filter's state (its last two outputs): the matrix
-- [[m00 m01] [m10 m11]] and the vector (v0, v1).
type affine2 = (f32, f32, f32, f32, f32, f32)

-- a, then b: b (a s + va) + vb.
def compose ((a00, a01, a10, a11, av0, av1): affine2) ((b00, b01, b10, b11, bv0, bv1): affine2) : affine2 =
  ( b00 * a00 + b01 * a10, b00 * a01 + b01 * a11
  , b10 * a00 + b11 * a10, b10 * a01 + b11 * a11
  , b00 * av0 + b01 * av1 + bv0, b10 * av0 + b11 * av1 + bv1 )

-- x through a biquad whose coefficients change from sample to sample:
-- y[i] = u[i] - a1 y[i-1] - a2 y[i-2], u[i] = b0 x[i] + b1 x[i-1] + b2 x[i-2].
def biquad [n] (coef: [n](f32, f32, f32, f32, f32)) (x: [n]f32) : [n]f32 =
  let steps = tabulate n (\i ->
    let (b0, b1, b2, a1, a2) = #[unsafe] coef[i]
    let at (j: i64) = if j >= 0 then #[unsafe] x[j] else 0
    in (-a1, -a2, 1, 0, b0 * at i + b1 * at (i - 1) + b2 * at (i - 2), 0))
  in map (.4) (scan compose (1, 0, 0, 1, 0, 0) steps)

-- A feedback comb: y[i] = x[i] + g y[i - d].
def comb [n] (d: i64) (g: f32) (x: [n]f32) : [n]f32 =
  let m = (n + d - 1) / d
  let padded : [m * d]f32 = tabulate (m * d) (\i -> if i < n then #[unsafe] x[i] else 0)
  let rows : [m][d]f32 = unflatten padded
  -- (each column a first-order recurrence: (a, b) is y = a y' + b)
  let cols = map (\c -> map (.1) (scan (\(a1, b1) (a2, b2) -> (a1 * a2, a2 * b1 + b2)) (1, 0) (map (\v -> (g, v)) c)))
                 (transpose rows)
  in take n (flatten (transpose cols))

-- An allpass: y[i] = -g x[i] + x[i - d] + g y[i - d] (smears without
-- colouring).
def allpass [n] (d: i64) (g: f32) (x: [n]f32) : [n]f32 =
  -- (the attribute in parentheses: unbracketed, it takes the whole sum)
  comb d g (tabulate n (\i -> -g * (#[unsafe] x[i]) + (if i >= d then #[unsafe] x[i - d] else 0)))

-- A Schroeder reverb (the delays of Jezar's Freeverb, at its 44.1 kHz,
-- scaled to `rate`): four combs in parallel, then two allpasses; the two
-- sides through allpasses of slightly different lengths. Fed through a
-- low-pass first, so the room is dark rather than metallic.
def reverb [n] (rate: f32) (x: [n]f32) : ([n]f32, [n]f32) =
  let s (k: i64) = i64.f32 (f32.i64 k * rate / 44100)
  let dark = biquad (replicate n (biquad_coeffs true 3500 0.7 rate)) x
  let room = map4 (\a b c d -> 0.25 * (a + b + c + d))
                  (comb (s 1116) 0.84 dark) (comb (s 1188) 0.84 dark) (comb (s 1277) 0.84 dark) (comb (s 1356) 0.84 dark)
  in (allpass (s 556) 0.5 (allpass (s 441) 0.5 room), allpass (s 579) 0.5 (allpass (s 464) 0.5 room))

---------------------------------------------------------------------
-- The effects

-- A noise layer: its filter (low-pass or band-pass), sweeping from f0 to
-- f1 Hz over `sweep` seconds (exponentially), with quality q; its level;
-- its envelope (attack and decay, or with `swell` rising and falling over
-- most of the effect); a tremble (am_rate Hz, am_depth 0..1); and `pan`
-- sweeping it from left to right.
type layer = { lowpass: bool, f0: f32, f1: f32, sweep: f32, q: f32, level: f32
             , attack: f32, decay: f32, swell: bool, am_rate: f32, am_depth: f32, pan: bool }

def quiet : layer = { lowpass = false, f0 = 1000, f1 = 1000, sweep = 1, q = 1, level = 0
                    , attack = 0.01, decay = 0.1, swell = false, am_rate = 0, am_depth = 0, pan = false }

def bp (f0: f32) (f1: f32) (sweep: f32) (q: f32) (level: f32) (attack: f32) (decay: f32) : layer =
  quiet with f0 = f0 with f1 = f1 with sweep = sweep with q = q with level = level with attack = attack with decay = decay

def lp (f0: f32) (f1: f32) (sweep: f32) (q: f32) (level: f32) (attack: f32) (decay: f32) : layer =
  bp f0 f1 sweep q level attack decay with lowpass = true

-- An effect: how long (with its reverb's tail), its two noise layers, how
-- much reverb, and how loud its loudest sample.
type patch = {length: f32, a: layer, b: layer, wet: f32, peak: f32}

def patch (k: i32) : patch =
  if k == sfx_cast_fire then
    -- a whoosh opening up as it goes, over a soft thump
    {length = 0.9, a = bp 400 2500 0.25 0.8 0.6 0.01 0.25, b = lp 300 120 0.15 0.7 0.5 0.005 0.12, wet = 0.15, peak = 0.8}
  else if k == sfx_cast_strike then
    -- a snap, and a buzzing hiss trembling at the mains' rate
    {length = 0.8, a = bp 3000 6000 0.1 2 0.4 0.001 0.06, b = bp 1200 1200 1 6 0.25 0.002 0.15 with am_rate = 90 with am_depth = 0.8, wet = 0.2, peak = 0.75}
  else if k == sfx_cast_wind then
    -- a resonant swoosh crossing from left to right, air above it
    {length = 1.1, a = bp 500 2200 0.8 3 0.7 0 0 with swell = true with pan = true, b = bp 2500 5000 0.8 1 0.3 0 0 with swell = true with pan = true, wet = 0.25, peak = 0.7}
  else if k == sfx_burst then
    -- a roar darkening as it dies, over a deep thump
    {length = 2.2, a = lp 2500 150 0.8 0.9 0.8 0.003 0.5, b = lp 120 120 1 1.2 0.9 0.002 0.3, wet = 0.35, peak = 0.95}
  else if k == sfx_thunder then
    -- a crack, then rolling rumble
    {length = 3, a = bp 4000 2500 0.1 0.5 0.7 0.0005 0.05, b = lp 180 180 1 0.8 1 0.05 0.9 with am_rate = 2.3 with am_depth = 0.6, wet = 0.5, peak = 0.95}
  else if k == sfx_gust then
    -- a glassy hiss falling, under the chimes
    {length = 1.3, a = bp 6000 3000 0.3 4 0.3 0.001 0.2, b = bp 1500 1500 1 1 0.2 0.001 0.1, wet = 0.35, peak = 0.6}
  else if k == sfx_hurt then
    -- a smack (with the thud and the sour chord)
    {length = 0.7, a = lp 1500 300 0.1 1 0.6 0.001 0.08, b = quiet, wet = 0.1, peak = 0.85}
  else if k == sfx_defeat then
    -- a whoosh falling away under the bells
    {length = 2.2, a = bp 3000 500 1.2 2 0.25 0.05 0.6, b = quiet, wet = 0.45, peak = 0.8}
  else if k == sfx_shield_up then
    -- a resonance rising, and air
    {length = 1.1, a = bp 400 3000 0.6 5 0.4 0 0 with swell = true, b = bp 6000 6000 1 1 0.15 0 0 with swell = true, wet = 0.35, peak = 0.55}
  else if k == sfx_block then
    -- a spray of sparks with the ringing bell
    {length = 1.5, a = bp 5000 5000 1 1 0.4 0.001 0.03, b = quiet, wet = 0.4, peak = 0.75}
  else if k == sfx_shield_down then
    {length = 0.9, a = bp 3000 300 0.4 5 0.4 0 0 with swell = true, b = quiet, wet = 0.3, peak = 0.5}
  else if k == sfx_fizzle then
    -- a sputter
    {length = 0.4, a = bp 800 800 1 3 0.4 0.005 0.1 with am_rate = 35 with am_depth = 0.9, b = quiet, wet = 0.05, peak = 0.5}
  else if k == sfx_mark then
    -- a whistle rising to the strike
    {length = 1.3, a = bp 600 2500 1.1 8 0.25 0.3 10, b = quiet, wet = 0.2, peak = 0.45}
  else
    -- (sfx_appear) a breath of air swelling in
    {length = 1.6, a = bp 1000 6000 0.8 2 0.3 0 0 with swell = true, b = quiet, wet = 0.4, peak = 0.5}

-- How long each lasts, in seconds.
def sfx_length (k: i32) : f32 = (patch k).length

-- A layer's filter frequency, and its level on side c (0 left), at time t.
def layer_freq (l: layer) (t: f32) : f32 = l.f0 * (l.f1 / l.f0) ** saturate (t / l.sweep)

def layer_gain (l: layer) (length: f32) (c: i32) (t: f32) : f32 =
  let shape = if l.swell then pow (f32.sin (f32.pi * saturate (t / (0.75 * length)))) 2 else env l.attack l.decay t
  let tremble = 1 - l.am_depth * (0.5 + 0.5 * f32.sin (tau * l.am_rate * t))
  let side = if !l.pan then 1 else let u = saturate (t / (0.75 * length)) in 0.3 + 0.7 * (if c == 0 then 1 - u else u)
  in l.level * shape * tremble * side

-- The tonal part of effect k at time t, on side c.
def tonal (k: i32) (c: i32) (t: f32) : f32 =
  let sd = 1000 * c
  in if k == sfx_cast_fire then 0.25 * env 0.01 0.25 t * crackle (sd + 2) 120 t
  else if k == sfx_cast_strike then
    -- a zap: a buzzing tone diving in pitch
    let buzz = 0.6 + 0.4 * f32.sin (tau * 60 * t)
    in 0.3 * env 0.003 0.12 t * buzz * tone 1400 (-7) 8 t
  else if k == sfx_burst then
    let boom = f32.sin (tau * (35 * t + 30 / 6 * (1 - f32.exp (-6 * t)))) * env 0.004 0.35 t
    in 0.5 * boom + 0.18 * env 0.05 0.5 t * crackle (sd + 6) (60 * f32.exp (-t)) t
  else if k == sfx_thunder then
    -- a second, smaller crack a moment after the first
    0.3 * env 0.0005 0.03 (t - 0.12) * crackle (sd + 7) 200 t
  else if k == sfx_gust then
    -- the blade shattering: high chimes, scattered
    let chime (j: i32) = let at = 0.15 * hash2 (sd + j) 21
                         let f = 2000 + 4000 * hash2 (sd + j) 23
                         in env 0.001 0.12 (t - at) * f32.sin (tau * f * (t - at))
    in 0.12 * (loop s = 0f32 for j < 8 do s + chime j)
  else if k == sfx_hurt then
    let thud = f32.sin (tau * (60 * t + 50 / 12 * (1 - f32.exp (-12 * t)))) * env 0.003 0.12 t
    in 0.6 * thud + 0.12 * env 0.005 0.25 t * (tone 330 0 4 t + tone 349 0 4 t)
  else if k == sfx_defeat then
    -- bells falling down a minor arpeggio (semitones down 0 3 7 12 15 19,
    -- as arithmetic: no arrays here)
    let down (j: i32) = 12 * (j / 3) + (if j % 3 == 0 then 0 else if j % 3 == 1 then 3 else 7)
    let note (j: i32) = bell (880 * 2 ** (-(f32.i32 (down j)) / 12)) 0.5 (t - 0.11 * f32.i32 j)
    in 0.16 * (loop s = 0f32 for j < 6 do s + note j)
  else if k == sfx_shield_up || k == sfx_shield_down then
    -- a shimmer, rising or falling, trembling
    let rate = if k == sfx_shield_up then 1.6 else -2.4
    let base = if k == sfx_shield_up then 300 else 900
    let s = loop s = 0f32 for j < 6 do
              s + f32.sin (tau * base * (1 + 0.013 * f32.i32 j) * (2 ** f32.i32 (j % 3)) * (f32.exp (rate * t) - 1) / rate)
    let len = if k == sfx_shield_up then 0.7 else 0.5
    in 0.07 * (0.75 + 0.25 * f32.sin (tau * 14 * t)) * pow (f32.sin (f32.pi * saturate (t / len))) 2 * s
  else if k == sfx_block then 0.3 * bell 620 0.35 t
  else if k == sfx_fizzle then 0.18 * env 0.005 0.1 t * tone 110 (-1.5) 6 t
  else if k == sfx_mark then
    -- a whine rising to the strike, pulsing faster as it does
    let pulse = 0.5 + 0.5 * f32.sin (tau * (6 * t + 9 * t * t))
    in 0.12 * smoothstep 0 0.2 t * smoothstep 1.1 1.0 t * pulse * tone 420 1.1 3 t
  else if k == sfx_appear then
    let chord = tone 1046.5 0 2 t + tone 1318.5 0 2 t + tone 1568 0 2 t
    in 0.06 * env 0.08 0.4 t * chord
  else 0

-- n stereo samples of effect k at `rate` a second, interleaved (left,
-- right).
def sfx_samples (k: i32) (rate: f32) (n: i64) : []f32 =
  let p = patch k
  let ts = tabulate n (\i -> f32.i64 i / rate)
  -- the noise layers, (layer, side) by 2 * layer + side: each its own
  -- noise through its filter, brought to unit loudness, under its envelope
  let noise =
    map (\j ->
           let l = if j < 2 then p.a else p.b
           let c = i32.i64 (j % 2)
           let white = tabulate n (\i -> 2 * hash3 (i32.i64 i) (i32.i64 j + 17) (k + 3) - 1)
           let y = biquad (map (\t -> biquad_coeffs l.lowpass (layer_freq l t) l.q rate) ts) white
           let rms = f32.sqrt (f32.sum (map (\v -> v * v) y) / f32.i64 n) + 1e-6
           in map2 (\t v -> v / rms * layer_gain l p.length c t) ts y)
        (iota 4)
  let dry (c: i64) = map3 (\t a b -> tonal k (i32.i64 c) t + a + b) ts noise[c] noise[2 + c]
  let (dl, dr) = (dry 0, dry 1)
  let (wl, wr) = reverb rate (map2 (\a b -> 0.5 * (a + b)) dl dr)
  let l = map2 (\d w -> d + p.wet * w) dl wl
  let r = map2 (\d w -> d + p.wet * w) dr wr
  let loudest = f32.maximum (map f32.abs (l ++ r)) + 1e-6
  in tabulate (2 * n) (\j -> p.peak / loudest * (if j % 2 == 0 then #[unsafe] l[j / 2] else #[unsafe] r[j / 2]))
