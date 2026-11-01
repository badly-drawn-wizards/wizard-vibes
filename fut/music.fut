-- Music, as Shadertoy does sound: each stereo sample is a pure function of
-- time, and the harness asks for a chunk of samples at a time (the `sound`
-- entry point; see web/music.ts).
--
-- Wizard disco: 32 bars at 120 beats a minute in D minor, on the chords
-- Dm, Gm, Bb, A (i iv VI V, two bars each: the A major chord's C# pulls
-- back to D). Throughout: a four-on-the-floor kick, open hi-hats on the
-- offbeats and closed ones on the sixteenths, claps on two and four, a
-- crunchy octave-jumping bass and a harpsichord playing sixteenth
-- arpeggios. In four sections of eight bars, each after a one-beat stop:
--   1  the groove;
--   2  with a string pad;
--   3  a synth lead plays the tune;
--   4  the tune again, with a harmony a third below, and the strings.
--
-- Times are within the loop (the harness wraps them in double precision: an
-- f32 time minutes in could not tell one sample from the next), and every
-- oscillator's phase is measured from the start of its note, so it stays
-- small and exact. (No arrays: the WebGPU backend rejects parallelism
-- inside the per-sample functions, so tables are written as arithmetic.)

import "vec3"
import "hash"

def bpm : f32 = 120
def beat : f32 = 60 / bpm
def eighth : f32 = beat / 2
def sixteenth : f32 = beat / 4
def bar : f32 = 4 * beat
def chord_len : f32 = 2 * bar
def chords : i32 = 4
def period : f32 = 32 * bar -- the loop, in seconds

-- The section (0..3) at time t, and whether t is in section k.
def section (t: f32) : i32 = i32.f32 (f32.floor (t / (8 * bar)))
-- A gain fading in over `fade` seconds at the start of section k and out
-- just before its end (for a part playing in sections k .. k2).
def in_sections (k: i32) (k2: i32) (fade: f32) (t: f32) : f32 =
  let a = f32.i32 k * 8 * bar
  let b = f32.i32 (k2 + 1) * 8 * bar
  in smoothstep a (a + fade) t * smoothstep b (b - 0.3) t

def tau : f32 = 2 * f32.pi
def freq (midi: f32) : f32 = 440 * 2 ** ((midi - 69) / 12)

-- Wraps a time into [0, period).
def wrap (t: f32) : f32 = t - period * f32.floor (t / period)

-- The chord playing at time t (0..3: Dm, Gm, Bb, A).
def chord_at (t: f32) : i32 = i32.f32 (f32.floor (t / chord_len)) % chords

-- Chord tone k (0 root, 1 third, 2 fifth, 3 the root an octave up), as a
-- MIDI note around the third octave.
def chord_tone (c: i32) (k: i32) : f32 =
  let root = if c == 0 then 50 else if c == 1 then 55 else if c == 2 then 46 else 57 -- D, G, Bb, A
  let third = if c >= 2 then 4 else 3 -- (Bb and A are major; Dm and Gm minor)
  let interval = if k == 0 then 0 else if k == 1 then third else if k == 2 then 7 else 12
  in f32.i32 (root + interval)

-- The stops: the last beat of each section.
def stopped (t: f32) : bool = t - 8 * bar * f32.floor (t / (8 * bar)) >= 8 * bar - beat

-- Noise, one value per sample (hashed sample index), and a brighter version
-- (differenced: its energy sits up where hi-hats live).
def noise (rate: f32) (t: f32) : f32 = 2 * hash2 (i32.f32 (t * rate)) 7 - 1
def bright_noise (rate: f32) (t: f32) : f32 =
  let i = i32.f32 (t * rate)
  in hash2 i 7 - hash2 (i - 1) 7

---------------------------------------------------------------------
-- Harpsichord

-- One plucked note: eight harmonics, each 1/k as loud and dying faster the
-- higher it is, after a near-instant attack: a bright, buzzy pluck that
-- fades within a second, with no sustain.
def harp_note (f: f32) (u: f32) : f32 =
  if u < 0 then 0 else
  let s = loop s = 0f32 for k < 8 do
            let kf = f32.i32 (k + 1)
            in s + f32.sin (tau * f * kf * u) / kf * f32.exp (-u * (3 + 2.5 * kf))
  in (1 - f32.exp (-u * 3000)) * s

-- Which chord tone sixteenth s (0..7) of a half bar plays: up, down, up
-- again: 0 1 2 3 2 1 2 3.
def arp_degree (s: i32) : i32 =
  if s < 4 then s else if s == 5 then 1 else if s == 7 then 3 else 2

def harp_at (n: i32) : f32 =
  let start = f32.i32 n * sixteenth
  in freq (chord_tone (chord_at start) (arp_degree (n % 8)) + 12)

-- The arpeggio: this sixteenth's note, and the last two still ringing.
def harpsichord (t: f32) : f32 =
  let n = i32.f32 (f32.floor (t / sixteenth))
  let u = t - f32.i32 n * sixteenth
  let ring (k: i32) = if n - k < 0 then 0 else harp_note (harp_at (n - k)) (u + f32.i32 k * sixteenth)
  in 0.075 * (ring 0 + ring 1 + ring 2)

---------------------------------------------------------------------
-- Bass, drums

-- The bass: the chord's root in eighths, low then an octave up. Crunchy:
-- a sawtooth (harmonics 1/k, band-limited to below 8 kHz) whose upper
-- harmonics fade quickly after each note starts, as if a filter closed
-- (bright then mellow), overdriven through a soft clipper (which adds more
-- harmonics: the grit), over a clean sine an octave below for weight.
def bass (t: f32) : f32 =
  let e = i32.f32 (f32.floor (t / eighth))
  let u = t - f32.i32 e * eighth
  let root = chord_tone (chord_at (f32.i32 e * eighth)) 0
  let f = freq (root - 12 + (if e % 2 == 1 then 12 else 0))
  let env = (1 - f32.exp (-u * 400)) * f32.exp (-u * 4) * smoothstep (0.92 * eighth) (0.8 * eighth) u
  -- (the filter: harmonic k is damped by how far it is above the cutoff,
  -- which falls from 30 to 6 times the note's frequency)
  let cutoff = 6 + 24 * f32.exp (-u * 18)
  let saw = loop s = 0f32 for k < 24 do
              let kf = f32.i32 (k + 1)
              let keep = if kf * f > 8000 then 0 else 1 / (1 + (kf / cutoff) ** 4)
              in s + keep * f32.sin (tau * f * kf * u) / kf
  let drive = 3.5
  let crunch = f32.tanh (drive * saw) / f32.tanh drive
  let sub = f32.sin (tau * f * 0.5 * u)
  in env * (0.13 * crunch + 0.12 * sub)

-- The kick on every beat: a sine falling from 150 to 50 Hz (its phase is
-- the integral of that frequency), with a little click on top.
def kick (t: f32) : f32 =
  let u = t - beat * f32.floor (t / beat)
  let p = tau * (50 * u + 100 / 25 * (1 - f32.exp (-25 * u)))
  in 0.42 * f32.exp (-u * 7) * f32.sin p + 0.05 * f32.exp (-u * 300)

-- Claps on beats two and four: noise in three quick bursts (several hands,
-- not quite together), then a short tail.
def clap (rate: f32) (t: f32) : f32 =
  let b = i32.f32 (f32.floor (t / beat))
  let u = t - f32.i32 b * beat
  in if b % 2 == 0 then 0 else
     let flam = if u < 0.025 then (if fract (u / 0.0085) < 0.45 then 1 else 0.25) else 0.55 * f32.exp (-(u - 0.025) * 14)
     in 0.11 * flam * noise rate t

-- Hi-hats: open on the offbeat eighths, closed on the other sixteenths.
def hats (rate: f32) (t: f32) : f32 =
  let n = i32.f32 (f32.floor (t / sixteenth))
  let u = t - f32.i32 n * sixteenth
  let env = if n % 4 == 2 then 0.09 * f32.exp (-u * 9) else if n % 2 == 1 then 0.035 * f32.exp (-u * 70) else 0
  in env * bright_noise rate t

---------------------------------------------------------------------
-- Strings

-- A string pad on the chord, two octaves up: each tone two detuned voices
-- (one per side), a few harmonics, swelling in; the previous chord fades
-- under the next.
def strings (t: f32) : (f32, f32) =
  let c = i32.f32 (f32.floor (t / chord_len))
  let lt = t - f32.i32 c * chord_len
  let voice (ch: i32) (tt: f32) (amp: f32) =
    loop (l, r) = (0f32, 0f32) for k < 3 do
      let f = freq (chord_tone (ch % chords) k + 24)
      let tone (g: f32) = f32.sin (tau * g * tt) + 0.45 * f32.sin (2 * tau * g * tt) + 0.2 * f32.sin (3 * tau * g * tt)
      in (l + amp * tone (f * 1.004), r + amp * tone (f * 0.996))
  let (l1, r1) = voice c lt (0.035 * smoothstep 0 0.5 lt)
  let fade = f32.max 0 (1 - lt / 0.4)
  let (l0, r0) = if fade > 0 && c > 0 then voice (c - 1) (lt + chord_len) (0.035 * fade) else (0, 0)
  -- (in sections 2 and 4)
  let g = in_sections 1 1 0.6 t + in_sections 3 3 0.6 t
  in (g * (l1 + l0), g * (r1 + r0))

---------------------------------------------------------------------
-- The tune

-- The melody: eight bars of eighth notes, each a code: 0 a rest, 15 hold
-- the note before, otherwise a degree of the scale (1 = D4 .. 12 = A5;
-- 13 = C#5, the A chord's leading note). Each bar's eight codes are packed
-- as hex digits (the first eighth in the lowest), since the WebGPU
-- backend allows no arrays here.
--   Dm  A4 - D5 - F5 E5 D5 C5 | A4 - - - . D5 E5 F5
--   Gm  G5 - F5 E5 D5 - Bb4 - | D5 - - - . G4 A4 Bb4
--   Bb  F5 - F5 G5 F5 E5 D5 - | D5 - C5 D5 - - . .
--   A   E5 - C#5 - A4 - C#5 E5 | A5 - - - G5 F5 E5 C#5
def melody_code (i: i32) : i32 =
  let b = (i / 8) % 8
  let w : u32 =
    if b == 0 then 0x789af8f5 else if b == 1 then 0xa980fff5
    else if b == 2 then 0xf6f89afb else if b == 3 then 0x6540fff8
    else if b == 4 then 0xf89abafa else if b == 5 then 0x00ff87f8
    else if b == 6 then 0x9df5fdf9 else 0xd9abfffc
  in i32.u32 ((w >> u32.i32 (4 * (i % 8))) & 15)

def degree_midi (d: i32) : f32 =
  f32.i32 (if d == 13 then 73
           else let o = (d - 1) / 7
                let k = (d - 1) % 7
                -- D natural minor: D E F G A Bb C
                let step = if k == 0 then 0 else if k == 1 then 2 else if k == 2 then 3 else if k == 3 then 5
                           else if k == 4 then 7 else if k == 5 then 8 else 10
                in 62 + 12 * o + step)

-- The note sounding at eighth i (or the one it is releasing from, after
-- it): (degree, its first eighth, its last eighth); degree 0 if none.
def note_at (i: i32) : (i32, i32, i32) =
  let k = if melody_code i == 0 then i - 1 else i
  in if k < 0 || melody_code k == 0 then (0, 0, 0) else
  let (j, _) = loop (j, go) = (k, true) while go && j > 0 do
                 if melody_code j == 15 then (j - 1, true) else (j, false)
  let (e, _) = loop (e, go) = (k, true) while go && e < 63 do
                 if melody_code (e + 1) == 15 then (e + 1, true) else (e, false)
  in (melody_code j, j, e)

-- A synth lead: a square-ish wave (odd harmonics, 1/k), two voices a hair
-- apart, with vibrato growing in on long notes. tau: time since the note
-- began; dur: how long it is held.
def lead_tone (f: f32) (tau_: f32) (dur: f32) : f32 =
  let env = (1 - f32.exp (-tau_ * 120)) * (0.75 + 0.25 * f32.exp (-tau_ * 6))
            * (if tau_ < dur then 1 else f32.exp (-(tau_ - dur) * 30))
  let depth = 0.005 * smoothstep 0.15 0.45 tau_
  let wob = depth / (2 * f32.pi * 5.5) * (1 - f32.cos (2 * f32.pi * 5.5 * tau_))
  let osc (g: f32) =
    loop s = 0f32 for k < 6 do
      let kf = f32.i32 (2 * k + 1)
      in if kf * g > 9000 then s else s + f32.sin (tau * g * kf * (tau_ + wob)) / kf
  in env * (osc f + 0.6 * osc (f * 1.003))

-- The tune in sections 3 and 4, an octave up; in section 4 also a third
-- below (two scale degrees down).
def tune (t: f32) : (f32, f32) =
  let start = 16 * bar
  in if t < start then (0, 0) else
  let i = i32.f32 (f32.floor ((t - start) / eighth)) % 64
  let u = t - start - eighth * f32.floor ((t - start) / eighth)
  let (d, j, e) = note_at i
  in if d == 0 then (0, 0) else
  let tau_ = f32.i32 (i - j) * eighth + u
  let dur = f32.i32 (e - j + 1) * eighth - 0.02
  let main = 0.07 * lead_tone (freq (degree_midi d + 12)) tau_ dur
  let harmony_degree = if d == 13 then 5 else d - 2
  let second = if section t == 3 && harmony_degree >= 1
               then 0.045 * lead_tone (freq (degree_midi harmony_degree + 12)) tau_ dur else 0
  let g = in_sections 2 3 0.05 t
  in (g * (main + 0.7 * second), g * (0.8 * main + second))

---------------------------------------------------------------------

-- The mix at time t (within the loop), softly limited.
def music_at (rate: f32) (t: f32) : (f32, f32) =
  let groove = if stopped t then 0 else 1
  let drums = groove * (kick t + clap rate t)
  let hh = groove * hats rate t
  let low = groove * bass t
  -- (the harpsichord steps back while the tune plays)
  let harp = (if section t >= 2 then 0.6 else 1) * harpsichord t
  let (sl, sr) = strings t
  let (ml, mr) = tune t
  let soft (x: f32) = f32.tanh (1.1 * x) / 1.1
  in (soft (drums + low + 0.6 * hh + harp + sl + ml), soft (drums + low + hh + 0.7 * harp + sr + mr))

-- n stereo samples, interleaved (left, right), from time t0 at `rate`
-- samples a second. (One thread per value, each computing its sample's
-- whole mix and keeping its side: a two-element array per sample made the
-- WebGPU backend reject the kernel.)
def sound_chunk (t0: f32) (rate: f32) (n: i64) : []f32 =
  tabulate (2 * n) (\j ->
    let (l, r) = music_at rate (wrap (t0 + f32.i64 (j / 2) / rate))
    in if j % 2 == 0 then l else r)
