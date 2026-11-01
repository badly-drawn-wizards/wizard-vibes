-- Music, as Shadertoy does sound: each stereo sample is a pure function of
-- time, and the harness asks for a chunk of samples at a time (the `sound`
-- entry point; see web/music.ts).
--
-- A loop of eight bars in D minor at 84 beats a minute: a pad holding the
-- chords (i - VI - III - VII: Dm, Bb, F, C, two bars each), a plucked
-- arpeggio with a ping-pong echo, a soft bass, a gentle kick and hats, and
-- now and then a bell.
--
-- Times are within the loop (the harness wraps them in double precision: an
-- f32 time minutes in could not tell one sample from the next), and every
-- oscillator's phase is measured from the start of its note, so it stays
-- small and exact.

import "vec3"
import "hash"

def bpm : f32 = 84
def beat : f32 = 60 / bpm
def bar : f32 = 4 * beat
def chord_bars : f32 = 2
def chord_len : f32 = chord_bars * bar
def chords : i32 = 4
def period : f32 = f32.i32 chords * chord_len -- the loop, in seconds

def tau : f32 = 2 * f32.pi
def freq (midi: f32) : f32 = 440 * 2 ** ((midi - 69) / 12)

-- Wraps a time into [0, period).
def wrap (t: f32) : f32 = t - period * f32.floor (t / period)

-- The chord at index c (0..3), as MIDI notes: root, third, fifth.
def chord_note (c: i32) (k: i32) : f32 =
  let c = c % chords
  let root = if c == 0 then 62 else if c == 1 then 58 else if c == 2 then 65 else 60 -- D, Bb, F, C
  let minor = c == 0
  let interval = if k == 0 then 0 else if k == 1 then (if minor then 3 else 4) else 7
  in f32.i32 (root + interval)

-- A bright but soft tone: a few harmonics falling off as 1/k^2.
def tone (f: f32) (t: f32) : f32 =
  let p = tau * f * t
  in f32.sin p + 0.3 * f32.sin (2 * p) + 0.12 * f32.sin (3 * p) + 0.05 * f32.sin (4 * p)

-- The pad: each chord's three notes an octave down, two slightly detuned
-- voices each, swelling in over a second; the previous chord fades out
-- under it.
def pad (t: f32) : (f32, f32) =
  let c = i32.f32 (f32.floor (t / chord_len))
  let lt = t - f32.i32 c * chord_len
  let voice (ch: i32) (tt: f32) (amp: f32) =
    loop (l, r) = (0f32, 0f32) for k < 3 do
      let f = freq (chord_note ch k - 12)
      let a = amp * 0.09 * tone (f * 1.003) tt
      let b = amp * 0.09 * tone (f * 0.997) tt
      in (l + a + 0.6 * b, r + b + 0.6 * a)
  let swell = 0.5 - 0.5 * f32.cos (f32.pi * f32.min 1 (lt / 1.2))
  let (l1, r1) = voice c lt swell
  -- (the previous chord, from its own start, fading over its first second
  -- in this one)
  let fade = f32.max 0 (1 - lt / 1.0)
  let (l0, r0) = if fade > 0 then voice ((c + chords - 1) % chords) (lt + chord_len) fade else (0, 0)
  in (l1 + l0, r1 + r0)

-- The arpeggio, in eighth notes: up and down the chord, two octaves up.
def arp_note (c: i32) (step: i32) : f32 =
  -- (the pattern 0 1 2 1 2 3 2 1, by arithmetic: an array here would put
  -- parallelism inside a GPU function, which the WebGPU backend rejects)
  let s = step % 8
  let j = if s == 0 then 0 else if s == 5 then 3 else if s % 2 == 1 then 1 else 2
  in if j == 3 then chord_note c 0 + 12 else chord_note c j

def eighth : f32 = beat / 2

-- One pluck: a decaying tone with a fast attack.
def pluck (f: f32) (u: f32) : f32 =
  if u < 0 then 0 else (1 - f32.exp (-u * 400)) * f32.exp (-u * 5) * tone f u

def arp_dry (t: f32) : f32 =
  let t = wrap t
  let n = i32.f32 (f32.floor (t / eighth))
  let u = t - f32.i32 n * eighth
  let c = i32.f32 (f32.floor (t / chord_len))
  -- (the previous note still rings a little)
  let prev = if n > 0 then pluck (freq (arp_note (i32.f32 (f32.floor ((t - u - 0.001) / chord_len))) (n - 1) + 12)) (u + eighth) else 0
  in 0.11 * (pluck (freq (arp_note c n + 12)) u + 0.5 * prev)

-- Ping-pong echo: three repeats, three eighths apart, alternating sides.
def arp (t: f32) : (f32, f32) =
  loop (l, r) = (arp_dry t, arp_dry t) for k < 3 do
    let g = 0.45 ** f32.i32 (k + 1)
    let e = g * arp_dry (t - f32.i32 (k + 1) * 3 * eighth)
    in if k % 2 == 0 then (l + 0.3 * e, r + e) else (l + e, r + 0.3 * e)

-- The bass: the chord's root two octaves down, on beats 1 and 3, and a
-- pickup on the last eighth.
def bass (t: f32) : f32 =
  let c = i32.f32 (f32.floor (t / chord_len))
  let f = freq (chord_note c 0 - 24)
  let b = t - beat * f32.floor (t / beat)
  let beat_no = i32.f32 (f32.floor (t / beat)) % 4
  let on = beat_no == 0 || beat_no == 2
  let u = if on then b else if beat_no == 3 && b > eighth then b - eighth else -1
  in if u < 0 then 0 else
     let env = (1 - f32.exp (-u * 200)) * f32.exp (-u * 2.5)
     let p = tau * f * u
     in 0.22 * env * (f32.sin p + 0.25 * f32.sin (2 * p))

-- Drums: a soft kick on 1 and 3 (a falling sine), hats on the offbeats
-- (hashed noise).
def kick (t: f32) : f32 =
  let u = t - 2 * beat * f32.floor (t / (2 * beat))
  -- (the phase of a sweep from 150 to 45 Hz: the integral of its frequency)
  let p = tau * (45 * u + 105 / 30 * (1 - f32.exp (-30 * u)))
  in 0.3 * f32.exp (-u * 9) * f32.sin p

def hat (t: f32) (rate: f32) : f32 =
  let u = t - eighth * f32.floor (t / eighth)
  let off = i32.f32 (f32.floor (t / eighth)) % 2 == 1
  in if !off then 0 else
     let n = hash2 (i32.f32 (t * rate)) 7 - hash2 (i32.f32 (t * rate) - 1) 7 -- (differenced: brighter)
     in 0.035 * f32.exp (-u * 45) * n

-- A bell (FM: a sine modulating another's phase) at the start of every
-- other bar, on a note of the chord chosen by hash.
def bell (t: f32) : (f32, f32) =
  let b = i32.f32 (f32.floor (t / (2 * bar)))
  let u = t - f32.i32 b * 2 * bar
  let c = i32.f32 (f32.floor (t / chord_len))
  let f = freq (chord_note c (i32.f32 (3 * hash2 b 3)) + 24)
  let env = (1 - f32.exp (-u * 300)) * f32.exp (-u * 1.6)
  let s = 0.07 * env * f32.sin (tau * f * u + 2.0 * f32.exp (-u * 3) * f32.sin (tau * f * 3.5 * u))
  let pan = hash2 b 9
  in (s * (1 - pan * 0.6), s * (0.4 + pan * 0.6))

-- The mix at time t (within the loop), softly limited.
def music_at (rate: f32) (t: f32) : (f32, f32) =
  let (pl, pr) = pad t
  let (al, ar) = arp t
  let (bl, br) = bell t
  let mono = bass t + kick t + hat t rate
  let soft (x: f32) = f32.tanh (1.2 * x) / 1.2
  in (soft (pl + al + bl + mono), soft (pr + ar + br + mono))

-- n stereo samples, interleaved (left, right), from time t0 at `rate`
-- samples a second. (One thread per value, each computing its sample's
-- whole mix and keeping its side: a two-element array per sample made the
-- WebGPU backend reject the kernel.)
def sound_chunk (t0: f32) (rate: f32) (n: i64) : []f32 =
  tabulate (2 * n) (\j ->
    let (l, r) = music_at rate (wrap (t0 + f32.i64 (j / 2) / rate))
    in if j % 2 == 0 then l else r)
