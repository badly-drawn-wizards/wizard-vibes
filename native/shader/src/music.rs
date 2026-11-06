// fut/music.fut: the music, as Shadertoy does sound: each stereo sample is
// a pure function of time, and the app asks for a chunk of samples at a
// time (the `music` entry point in lib.rs, as sound_chunk).
//
// Wizard disco: 32 bars at 120 beats a minute in D minor, on the chords
// Dm, Gm, Bb, A (i iv VI V, two bars each). Its voices are modelled on
// those of Louie Zong's WIZARD DISCO (measured on its stems, as split by
// Demucs). Throughout: a four-on-the-floor kick with a snap of noise on
// every beat, open hi-hats on the offbeats and closed ones on the
// sixteenths, claps on two and four, a plucked bass in a syncopated
// octave figure and a harpsichord playing sixteenth arpeggios. In four
// sections of eight bars, each after a one-beat stop:
//   1  the groove;
//   2  with a pad and electric piano stabs;
//   3  a whistling synth lead plays the tune;
//   4  the tune again, with a harmony a third below, the pad and the stabs.
//
// Times are within the loop (the app wraps them in double precision), and
// every oscillator's phase is measured from the start of its note, so it
// stays small and exact.
//
// (Not in the prelude: some of its names, e.g. noise, are other modules'.)

use crate::prelude::*;

pub const bpm: f32 = 120.0;
pub const beat: f32 = 60.0 / bpm;
pub const eighth: f32 = beat / 2.0;
pub const sixteenth: f32 = beat / 4.0;
pub const bar: f32 = 4.0 * beat;
pub const chord_len: f32 = 2.0 * bar;
pub const chords: i32 = 4;
pub const period: f32 = 32.0 * bar; // the loop, in seconds

// The section (0..3) at time t.
#[inline]
pub fn section(t: f32) -> i32 {
    ftoi((t / (8.0 * bar)).floor())
}
// A gain fading in over `fade` seconds at the start of section k and out
// just before its end (for a part playing in sections k .. k2).
#[inline]
pub fn in_sections(k: i32, k2: i32, fade: f32, t: f32) -> f32 {
    let a = k as f32 * 8.0 * bar;
    let b = (k2 + 1) as f32 * 8.0 * bar;
    smoothstep(a, a + fade, t) * smoothstep(b, b - 0.3, t)
}

const tau: f32 = 2.0 * PI;
#[inline]
pub fn freq(midi: f32) -> f32 {
    440.0 * (2.0f32).powf((midi - 69.0) / 12.0)
}

// Wraps a time into [0, period).
#[inline]
pub fn wrap(t: f32) -> f32 {
    t - period * (t / period).floor()
}

// The chord playing at time t (0..3: Dm, Gm, Bb, A).
#[inline]
pub fn chord_at(t: f32) -> i32 {
    imod(ftoi((t / chord_len).floor()), chords)
}

// Chord tone k (0 root, 1 third, 2 fifth, 3 the root an octave up), as a
// MIDI note around the third octave.
#[inline]
pub fn chord_tone(c: i32, k: i32) -> f32 {
    let root = if c == 0 { 50 } else if c == 1 { 55 } else if c == 2 { 46 } else { 57 }; // D, G, Bb, A
    let third = if c >= 2 { 4 } else { 3 }; // (Bb and A are major; Dm and Gm minor)
    let interval = if k == 0 { 0 } else if k == 1 { third } else if k == 2 { 7 } else { 12 };
    (root + interval) as f32
}

// The stops: the last beat of each section.
#[inline]
pub fn stopped(t: f32) -> bool {
    t - 8.0 * bar * (t / (8.0 * bar)).floor() >= 8.0 * bar - beat
}

// Noise, one value per sample (hashed sample index), and a brighter version
// (differenced: its energy sits up where hi-hats live).
#[inline]
pub fn noise(rate: f32, t: f32) -> f32 {
    2.0 * hash2(ftoi(t * rate), 7) - 1.0
}
#[inline]
pub fn bright_noise(rate: f32, t: f32) -> f32 {
    let i = ftoi(t * rate);
    hash2(i, 7) - hash2(i.wrapping_sub(1), 7)
}

// ---------------------------------------------------------------------
// Harpsichord

// One string of one note, plucked at time 0 and let go at `held` (a quill
// at fraction b of its length; slightly stiff; damped on release). As
// measured on the Thomas Barton harpsichord of 1709 (St Cecilia's Hall,
// Edinburgh; its sample library, CC BY 4.0): the partials fall off more
// slowly than an ideal pluck's 1/k (the low ones are strong), each decays
// at about 12 dB/s plus 4 dB/s per kHz of its frequency, and the strings'
// inharmonicity is B ~ 1.5-4e-5.
#[inline]
pub fn harp_string(f: f32, b: f32, u: f32, held: f32) -> f32 {
    let released = (0.0f32).maximum(u - held);
    let mut s = 0.0f32;
    let mut k = 0;
    while k < 32 {
        let kf = (k + 1) as f32;
        let fk = kf * f * (1.0 + 0.00003 * kf * kf).sqrt();
        if !(fk > 12000.0) {
            let decay = (1.38 + 0.00046 * fk) * u + (25.0 + 2.0 * kf) * released;
            s = s + (PI * kf * b).sin() / kf.powf(0.8) * (-decay).exp() * (tau * fk * u).sin();
        }
        k += 1;
    }
    s
}

// Where the quill plucks a string of frequency f, as a fraction of its
// length: nearer the middle in the treble (as instruments are built: see
// Välimäki et al., "Sound synthesis of the harpsichord using a
// computationally efficient physical model", EURASIP JASP 2004). Here as
// the Barton's spectra's notches have it: about 1/7 at A3, 1/4 by G4.
#[inline]
pub fn harp_pluck(f: f32) -> f32 {
    0.14 + 0.11 * saturate((f / 207.0).log2() / (370.0f32 / 207.0).log2())
}

// One note: two registers' strings, plucked a little apart (the second
// nearer the nut) and a hair apart in tune (as the Barton's are, by 0.6 to
// 4 cents), one more to each side. The pluck is at once, then builds over
// some 30 ms as the soundboard joins in; the quill scrapes the string for
// some 20 ms (a twelfth of the onset's energy is noise), and clicks; the
// damper thumps faintly on release.
#[inline]
pub fn harp_note(rate: f32, t: f32, f: f32, u: f32, held: f32) -> (f32, f32) {
    if u < 0.0 {
        return (0.0, 0.0);
    }
    let attack = (1.0 - (-u * 4000.0).exp()) * (1.0 - 0.8 * (-u * 60.0).exp());
    let b = harp_pluck(f);
    let a = attack * harp_string(f, b, u, held);
    let c = attack * harp_string(f * 1.0012, 0.85 * b, u, held);
    let scrape = 0.1 * (-u * 150.0).exp() * noise(rate, t) + 0.25 * (-u * 900.0).exp() * bright_noise(rate, t);
    let released = u - held;
    let thump = if released < 0.0 { 0.0 } else { 0.03 * (-released * 30.0).exp() * noise(rate, t) };
    let n = scrape + thump;
    (0.6 * a + 0.4 * c + n, 0.4 * a + 0.6 * c + n)
}

// Which chord tone sixteenth s (0..7) of a half bar plays: 0 1 2 3 2 1 2 3.
#[inline]
pub fn arp_degree(s: i32) -> i32 {
    if s < 4 {
        s
    } else if s == 5 {
        1
    } else if s == 7 {
        3
    } else {
        2
    }
}

#[inline]
pub fn harp_at(n: i32) -> f32 {
    let start = n as f32 * sixteenth;
    freq(chord_tone(chord_at(start), arp_degree(imod(n, 8))) + 12.0)
}

// The arpeggio: this sixteenth's note, and the last two, each held a
// little under two sixteenths, then damped.
#[inline]
pub fn harpsichord(rate: f32, t: f32) -> (f32, f32) {
    let n = ftoi((t / sixteenth).floor());
    let u = t - n as f32 * sixteenth;
    let ring = |k: i32| {
        if n - k < 0 { (0.0, 0.0) } else { harp_note(rate, t, harp_at(n - k), u + k as f32 * sixteenth, 1.75 * sixteenth) }
    };
    let (l0, r0) = ring(0);
    let (l1, r1) = ring(1);
    let (l2, r2) = ring(2);
    (0.07 * (l0 + l1 + l2), 0.07 * (r0 + r1 + r2))
}

// ---------------------------------------------------------------------
// Bass, drums

// The pump: how far the kick ducks the bass, the strings and the
// harpsichord, after each beat (the disco and house sidechain).
#[inline]
pub fn duck(t: f32, depth: f32) -> f32 {
    let u = t - beat * (t / beat).floor();
    1.0 - depth * smoothstep(0.0, 0.004, u) * (-u * 9.0).exp()
}

// The bass's figure, a bar of sixteenths: whether a note starts on
// sixteenth s (0..15), how many semitones over the low root, and how many
// sixteenths it is held. Octave jumps, syncopated (as funk's and disco's).
#[inline]
pub fn bass_step(s: i32) -> (bool, f32, f32) {
    if s == 0 || s == 8 {
        (true, 0.0, 2.0)
    } else if s == 3 || s == 11 {
        (true, 12.0, 1.0)
    } else if s == 4 {
        (true, 0.0, 1.0)
    } else if s == 6 {
        (true, 12.0, 2.0)
    } else if s == 12 {
        (true, 7.0, 1.0)
    } else if s == 14 {
        (true, 12.0, 1.5)
    } else {
        (false, 0.0, 0.0)
    }
}

// The bass: the chord's root (and its octave and fifth) in the figure
// above, from the octave of D2, with a sine an octave under it; a band-limited sawtooth through a resonant filter that snaps shut
// (bright for its first 20 ms, round by 60 ms, as WIZARD DISCO's
// plucked bass measures), a soft clipper; as loud as the drums; pumped by
// the kick.
#[inline]
pub fn bass(t: f32) -> f32 {
    // (the note sounding: the latest to start, up to three sixteenths back)
    let n = ftoi((t / sixteenth).floor());
    let mut start = n;
    let mut k = 0;
    while k < 3 && !bass_step(imod(start, 16)).0 {
        start -= 1;
        k += 1;
    }
    let (on, semis, len) = bass_step(imod(start, 16));
    if !on {
        return 0.0;
    }
    let u = t - start as f32 * sixteenth;
    let held = len * sixteenth - 0.02;
    let root = chord_tone(chord_at(start as f32 * sixteenth), 0);
    let f = freq(root - 12.0 + semis);
    let env = (1.0 - (-u * 400.0).exp()) * (-u * 2.5).exp() * smoothstep(held + 0.015, held, u);
    // (the filter: harmonic k damped by how far it is above the cutoff,
    // which falls from 8 to 2 times the note's frequency, with a peak there)
    let cutoff = 2.0 + 6.0 * (-u * 40.0).exp();
    let mut saw = 0.0f32;
    let mut k = 0;
    while k < 24 {
        let kf = (k + 1) as f32;
        let d = (kf - cutoff) / 1.2;
        let keep = if kf * f > 8000.0 { 0.0 } else { 1.0 / (1.0 + (kf / cutoff).powf(4.0)) + 0.9 * (-d * d).exp() };
        saw = saw + keep * (tau * f * kf * u).sin() / kf;
        k += 1;
    }
    let drive = 1.5f32;
    let crunch = (drive * saw).tanh() / drive.tanh();
    let sub = (tau * f * 0.5 * u).sin();
    duck(t, 0.5) * env * (0.3 * crunch + 0.14 * sub)
}

// The kick on every beat, as WIZARD DISCO's (Louie Zong's) measures: a
// body near 120 Hz for its first 30 ms, settling to a 58 Hz tail that
// rings some 250 ms (its weight in 60-250 Hz, little below), driven into a soft clipper for punch; a short click;
// and over it a snap of bright noise (1-12 kHz, some 100 ms), on every beat.
#[inline]
pub fn kick(rate: f32, t: f32) -> f32 {
    let u = t - beat * (t / beat).floor();
    let p = tau * (58.0 * u + 100.0 / 22.0 * (1.0 - (-22.0 * u).exp()));
    let body = (2.2 * (-u * 7.0).exp() * p.sin()).tanh() / (2.2f32).tanh();
    let click = (-u * 900.0).exp() * (0.5 + 0.5 * bright_noise(rate, t));
    let snap = smoothstep(0.0, 0.002, u) * (-u * 28.0).exp() * (noise(rate, t) + 0.3 * bright_noise(rate, t));
    0.42 * body + 0.08 * click + 0.35 * snap
}

// Claps on beats two and four: bright noise in three quick bursts, then a
// tail (and much of it sent to the room).
#[inline]
pub fn clap(rate: f32, t: f32) -> f32 {
    let b = ftoi((t / beat).floor());
    let u = t - b as f32 * beat;
    if imod(b, 2) == 0 {
        return 0.0;
    }
    let flam = if u < 0.025 {
        if fract(u / 0.0085) < 0.45 { 1.0 } else { 0.25 }
    } else {
        0.55 * (-(u - 0.025) * 16.0).exp()
    };
    0.22 * flam * (0.4 * noise(rate, t) + 1.2 * bright_noise(rate, t))
}

// A metallic hat: six square waves at the 808's inharmonic frequencies,
// differenced (a high-pass: their low end gone), at time t.
#[inline]
fn metal(rate: f32, t: f32) -> f32 {
    let sq = |f: f32, t: f32| if fract(f * t) < 0.5 { 1.0 } else { -1.0 };
    let m = |t: f32| sq(205.3 * 2.0, t) + sq(304.4 * 2.0, t) + sq(369.6 * 2.0, t) + sq(522.7 * 2.0, t) + sq(540.0 * 2.0, t) + sq(800.0 * 2.0, t);
    (m(t) - m(t - 1.0 / rate)) / 12.0
}

// Hi-hats: open on the offbeat eighths, closed on the other sixteenths; the
// 808's metal over a little noise; (left, right).
#[inline]
pub fn hats(rate: f32, t: f32) -> (f32, f32) {
    let n = ftoi((t / sixteenth).floor());
    let u = t - n as f32 * sixteenth;
    let (env, pan) = if imod(n, 4) == 2 {
        (0.24 * (-u * 8.0).exp(), 0.35)
    } else if imod(n, 2) == 1 {
        (0.16 * (-u * 60.0).exp(), 0.65)
    } else {
        (0.0, 0.5)
    };
    if env <= 0.0 {
        return (0.0, 0.0);
    }
    let v = env * (0.7 * metal(rate, t) + 0.5 * bright_noise(rate, t));
    (v * (1.0 - pan), v * pan)
}

// ---------------------------------------------------------------------
// Strings

// One chord of the pad: each tone an ensemble of voices slightly apart in
// tune (two a side), softened, with a slow vibrato. Each voice's even
// harmonics are strong and its odd ones weak (the octave above loud, the
// twelfth 16 dB down), as WIZARD DISCO's pad measures: hollow and organ-like
// more than a sawtooth's buzz.
#[inline]
fn voice(ch: i32, tt: f32, amp: f32) -> (f32, f32) {
    let (mut l, mut r) = (0.0f32, 0.0f32);
    // (the vibrato's phase offset, as a time warp: depth 0.25%, 5 Hz)
    let vib = 0.0025 * smoothstep(0.3, 1.0, tt) / (tau * 5.0) * (1.0 - (tau * 5.0 * tt).cos());
    let mut k = 0;
    while k < 3 {
        let f = freq(chord_tone(imod(ch, chords), k) + 12.0);
        let saw = |g: f32| {
            let mut s = 0.0f32;
            let mut h = 0;
            while h < 10 {
                let hf = (h + 1) as f32;
                let soft = 1.0 / (1.0 + (hf * g / 2200.0) * (hf * g / 2200.0));
                let weight = if h == 0 { 1.0 } else if imod(h, 2) == 1 { 1.1 } else { 0.5 };
                s = s + soft * weight * (tau * g * hf * (tt + vib)).sin() / hf;
                h += 1;
            }
            s
        };
        let nl = l + amp * (saw(f * 0.9960) + saw(f * 1.0017));
        let nr = r + amp * (saw(f * 1.0040) + saw(f * 0.9983));
        l = nl;
        r = nr;
        k += 1;
    }
    (l, r)
}

// The pad on the chord, an octave up, swelling in; the previous
// chord fades under the next; pumped by the kick.
#[inline]
pub fn strings(t: f32) -> (f32, f32) {
    let c = ftoi((t / chord_len).floor());
    let lt = t - c as f32 * chord_len;
    let (l1, r1) = voice(c, lt, 0.03 * smoothstep(0.0, 0.6, lt));
    let fade = (0.0f32).maximum(1.0 - lt / 0.4);
    let (l0, r0) = if fade > 0.0 && c > 0 { voice(c - 1, lt + chord_len, 0.03 * fade) } else { (0.0, 0.0) };
    // (in sections 2 and 4)
    let g = duck(t, 0.4) * (in_sections(1, 1, 0.6, t) + in_sections(3, 3, 0.6, t));
    (g * (l1 + l0), g * (r1 + r0))
}

// ---------------------------------------------------------------------
// Electric piano

// Whether an electric piano stab starts on sixteenth s (0..15) of a bar,
// and how many sixteenths it is held: off the beat (the "and" of one and
// three, the last sixteenth of two, the "and" of four), as disco's
// rhythm parts play.
#[inline]
pub fn stab_step(s: i32) -> f32 {
    if s == 2 || s == 10 {
        1.5
    } else if s == 7 {
        1.0
    } else if s == 14 {
        2.0
    } else {
        0.0
    }
}

// One tine of an electric piano, struck at u = 0 and let go at `held`:
// two-operator FM (a Rhodes as the DX7 does it), its modulator on the
// carrier's frequency, its index falling fast, so the note is bright as
// it is struck (the second and third harmonics up with the first) and
// near a sine by 100 ms, as WIZARD DISCO's keys measure; and a faint
// metallic ping (the tine's own, some 14 times up).
#[inline]
pub fn tine(f: f32, u: f32, held: f32) -> f32 {
    let index = 0.35 + 2.2 * (-u * 30.0).exp();
    let p = tau * f * u;
    let env = (1.0 - (-u * 1500.0).exp()) * (-u * 3.0).exp() * smoothstep(held + 0.04, held, u);
    let ping = if 14.0 * f > 10000.0 { 0.0 } else { 0.12 * (-u * 80.0).exp() * (14.0 * p).sin() };
    env * ((p + index * p.sin()).sin() + ping)
}

// The stabs: the chord in the octave above the bass's, each stab's three
// tones (and the root an octave up) struck together; with the electric
// piano's stereo tremolo, left and right swapping at 4 Hz (left, right).
#[inline]
pub fn keys(t: f32) -> (f32, f32) {
    let n = ftoi((t / sixteenth).floor());
    let mut start = n;
    let mut k = 0;
    while k < 3 && stab_step(imod(start, 16)) == 0.0 {
        start -= 1;
        k += 1;
    }
    let len = stab_step(imod(start, 16));
    if len == 0.0 {
        return (0.0, 0.0);
    }
    let u = t - start as f32 * sixteenth;
    let c = chord_at(start as f32 * sixteenth);
    let held = len * sixteenth - 0.02;
    let mut s = 0.0f32;
    let mut j = 0;
    while j < 4 {
        s = s + tine(freq(chord_tone(c, j) + 12.0), u, held);
        j += 1;
    }
    let pan = 0.5 + 0.3 * (tau * 4.0 * t).sin();
    let g = 0.03 * (in_sections(1, 1, 0.05, t) + in_sections(3, 3, 0.05, t));
    (g * s * (1.0 - pan), g * s * pan)
}

// ---------------------------------------------------------------------
// The tune

// The melody: eight bars of eighth notes, each a degree of the scale (1 =
// D4 .. 12 = A5; 13 = C#5), - holding the note before, . a rest.
wizard_shader_macros::melody!(
    melody_code,
    8,
    "5 - 8 - 10 9 8 7 | 5 - - - . 8 9 10 | 11 - 10 9 8 - 6 - | 8 - - - . 4 5 6 |
     10 - 10 11 10 9 8 - | 8 - 7 8 - - . . | 9 - 13 - 5 - 13 9 | 12 - - - 11 10 9 13"
);

// Degree d of D natural minor (1 = D4), or 13: C#5 (the leading note).
#[inline]
pub fn degree_midi(d: i32) -> f32 {
    if d == 13 { 73.0 } else { crate::synth::minor_degree(62.0, d) }
}

// The note sounding at eighth i (or the one it is releasing from, after
// it): (degree, its first eighth, its last eighth); degree 0 if none.
#[inline]
pub fn note_at(i: i32) -> (i32, i32, i32) {
    crate::synth::tune_note(melody_code, i, 64)
}

// A synth lead: near a sine, its octave 8 dB down and its twelfth 22 dB
// (as WIZARD DISCO's whistling lead measures), two voices a hair apart,
// with vibrato growing in on long notes, gliding in from the note
// before (pf; f if none) over 40 ms. tau_: time since the note began; dur:
// how long it is held.
#[inline]
pub fn lead_tone(f: f32, pf: f32, tau_: f32, dur: f32) -> f32 {
    let env = (1.0 - (-tau_ * 120.0).exp())
        * (0.75 + 0.25 * (-tau_ * 6.0).exp())
        * (if tau_ < dur { 1.0 } else { (-(tau_ - dur) * 30.0).exp() });
    let at = crate::synth::sung_time(f, pf, tau_, 0.04, 0.005, 5.5, 0.15, 0.45);
    let osc = |h: f32| {
        let p = tau * h * at;
        p.sin() + 0.4 * (2.0 * p).sin() + 0.08 * (3.0 * p).sin()
    };
    env * (osc(f) + 0.6 * osc(f * 1.003))
}

// The tune in sections 3 and 4, an octave up; in section 4 also a third
// below (two scale degrees down). (main, harmony), mono.
#[inline]
pub fn tune_dry(t: f32) -> (f32, f32) {
    let start = 16.0 * bar;
    if t < start {
        return (0.0, 0.0);
    }
    let i = imod(ftoi(((t - start) / eighth).floor()), 64);
    let u = t - start - eighth * ((t - start) / eighth).floor();
    let (d, j, e) = note_at(i);
    if d == 0 {
        return (0.0, 0.0);
    }
    let tau_ = (i - j) as f32 * eighth + u;
    let dur = (e - j + 1) as f32 * eighth - 0.02;
    // (the note before, if it ran into this one: glided from)
    let (pd, _, pe) = if j > 0 { note_at(j - 1) } else { (0, 0, 0) };
    let joined = pd != 0 && pe == j - 1;
    let f = freq(degree_midi(d) + 12.0);
    let pf = if joined { freq(degree_midi(pd) + 12.0) } else { f };
    let main = 0.07 * lead_tone(f, pf, tau_, dur);
    let hd = if d == 13 { 5 } else { d - 2 };
    let phd = if pd == 13 { 5 } else { pd - 2 };
    let second = if section(t) == 3 && hd >= 1 {
        let hf = freq(degree_midi(hd) + 12.0);
        let hpf = if joined && phd >= 1 { freq(degree_midi(phd) + 12.0) } else { hf };
        0.045 * lead_tone(hf, hpf, tau_, dur)
    } else {
        0.0
    };
    let g = in_sections(2, 3, 0.05, t);
    (g * main, g * second)
}

// The tune with its echo: a dotted eighth, ping-ponging between the sides,
// each repeat quieter (the lead itself, delayed: no state to carry).
#[inline]
pub fn tune(t: f32) -> (f32, f32) {
    let (m, h) = tune_dry(t);
    let d = 0.75 * beat;
    let (e1m, e1h) = tune_dry(t - d);
    let (e2m, e2h) = tune_dry(t - 2.0 * d);
    let (e3m, e3h) = tune_dry(t - 3.0 * d);
    let (a, b) = (m + 0.7 * h, 0.8 * m + h);
    let (l1, l2, l3) = (e1m + e1h, e2m + e2h, e3m + e3h);
    (a + 0.10 * l2, b + 0.32 * l1 + 0.04 * l3)
}

// ---------------------------------------------------------------------

// The mix at time t (within the loop), softly limited.
#[inline(never)]
pub fn music_at(rate: f32, t: f32) -> (f32, f32) {
    let (l, r, _, _) = music_parts(rate, t);
    (soft(l), soft(r))
}

// The soft limit on the mix.
#[inline]
pub fn soft(x: f32) -> f32 {
    (1.1 * x).tanh() / 1.1
}

// The mix at time t before the limit (left, right), the harpsichord in it
// (mono), which drives the soundboard (see soundboard.rs), and what is sent
// to the room (mono; see room.rs).
#[inline]
pub fn music_parts(rate: f32, t: f32) -> (f32, f32, f32, f32) {
    let groove = if stopped(t) { 0.0 } else { 1.0 };
    let k = groove * kick(rate, t);
    let cl = groove * clap(rate, t);
    let (hhl, hhr) = hats(rate, t);
    let low = groove * bass(t);
    // (the harpsichord steps back while the tune plays, and pumps a little)
    let (hl, hr) = harpsichord(rate, t);
    let harp = (if section(t) >= 2 { 0.6 } else { 1.0 }) * duck(t, 0.25);
    let (sl, sr) = strings(t);
    let (ml, mr) = tune(t);
    let (el, er) = keys(t);
    let (el, er) = (groove * duck(t, 0.3) * el, groove * duck(t, 0.3) * er);
    let l = k + cl + low + groove * hhl + harp * hl + sl + ml + el;
    let r = k + cl + low + groove * hhr + harp * hr + sr + mr + er;
    let send = 0.9 * cl + 0.6 * (sl + sr) + 0.5 * (ml + mr) + 0.15 * harp * (hl + hr) + 0.3 * (el + er);
    (l, r, 0.5 * harp * (hl + hr), send)
}

// The songs: 0 this one (Wizard disco), 1 chip.rs's (Cracktro), 2
// stars.rs's (Starscape), 3 glitch.rs's (Glitch Familiar), 4 wub.rs's (Wub
// Ward).
pub const songs: i32 = 5;
#[cfg(not(target_arch = "spirv"))]
pub const song_names: [&str; 5] = ["Wizard disco", "Cracktro", "Starscape", "Glitch Familiar", "Wub Ward"];

// The beat a song is danced to (beats a minute): its tempo, but the
// dubstep's half time.
#[inline]
pub fn song_bpm(song: i32) -> f32 {
    if song == 1 {
        crate::chip::bpm
    } else if song == 2 {
        crate::stars::bpm
    } else if song == 3 {
        crate::glitch::bpm
    } else if song == 4 {
        0.5 * crate::wub::bpm
    } else {
        bpm
    }
}

// A song's loop, in seconds.
#[inline]
pub fn song_period(song: i32) -> f32 {
    if song == 1 {
        crate::chip::period
    } else if song == 2 {
        crate::stars::period
    } else if song == 3 {
        crate::glitch::period
    } else if song == 4 {
        crate::wub::period
    } else {
        period
    }
}

// Wraps a time into song's loop.
#[inline]
pub fn song_wrap(song: i32, t: f32) -> f32 {
    let p = song_period(song);
    t - p * (t / p).floor()
}

// Song's music_parts: the mix before the limit (left, right), the
// harpsichord in it (for the soundboard), the room's send.
#[inline]
pub fn song_parts(song: i32, rate: f32, t: f32) -> (f32, f32, f32, f32) {
    if song == 1 {
        let (l, r, s) = crate::chip::parts(rate, t);
        (l, r, 0.0, s)
    } else if song == 2 {
        let (l, r, s) = crate::stars::parts(rate, t);
        (l, r, 0.0, s)
    } else if song == 3 {
        let (l, r, s) = crate::glitch::parts(rate, t);
        (l, r, 0.0, s)
    } else if song == 4 {
        let (l, r, s) = crate::wub::parts(rate, t);
        (l, r, 0.0, s)
    } else {
        music_parts(rate, t)
    }
}

// Value j of sound_chunk: sample j / 2 from time t0 at `rate` samples a
// second, left (j even) or right.
#[inline]
pub fn sound_value(t0: f32, rate: f32, j: i32) -> f32 {
    let (l, r) = music_at(rate, wrap(t0 + sdiv(j, 2) as f32 / rate));
    if srem(j, 2) == 0 { l } else { r }
}

// n stereo samples, interleaved (left, right), from time t0 (on the host),
// without the soundboard.
#[cfg(not(target_arch = "spirv"))]
pub fn sound_chunk(t0: f32, rate: f32, n: usize) -> Vec<f32> {
    (0..2 * n as i32).map(|j| sound_value(t0, rate, j)).collect()
}

// What carries from one chunk of the music to the next (on the host).
#[cfg(not(target_arch = "spirv"))]
pub struct State {
    // the soundboard's modes (modes * 2 floats)
    pub board: Vec<f32>,
    // the room's two slots (see room.rs)
    pub room: Vec<f32>,
}

#[cfg(not(target_arch = "spirv"))]
impl State {
    pub fn new() -> State {
        State { board: vec![0.0; 2 * crate::soundboard::modes], room: vec![0.0; 2 * crate::room::slot] }
    }
}

// The room's level in the mix.
pub const room_level: f32 = 0.9;

// A chunk of song `song` as the GPU makes it (the music, soundboard and
// music_mix entry points): n stereo frames from loop time t0 (wrapped),
// chunk number `chunk` (from 0), carried on from `state`, which is left as
// at the chunk's end.
#[cfg(not(target_arch = "spirv"))]
pub fn render_chunk(song: i32, t0: f32, rate: f32, n: usize, chunk: u32, state: &mut State) -> Vec<f32> {
    let mut dry = Vec::with_capacity(4 * n);
    for i in 0..n {
        let (l, r, h, s) = song_parts(song, rate, song_wrap(song, t0 + i as f32 / rate));
        dry.extend_from_slice(&[l, r, h, s]);
    }
    let harp: Vec<f32> = (0..n).map(|i| dry[4 * i + 2]).collect();
    let mut wet = vec![0.0; 2 * n];
    crate::soundboard::run(rate, &harp, &mut state.board, &mut wet);
    let mixed: Vec<f32> = (0..n).flat_map(|i| [dry[4 * i] + wet[2 * i], dry[4 * i + 1] + wet[2 * i + 1], dry[4 * i + 3]]).collect();
    finish_chunk(&mixed, rate, chunk, &mut state.room)
}

// The end of a chunk, on the host (after the GPU's music_mix, as its 3
// floats a frame: left, right, the room's send): the room (its two slots
// in `room`, carried on from the last chunk; see room.rs), then the limit;
// interleaved stereo.
#[cfg(not(target_arch = "spirv"))]
pub fn finish_chunk(mixed: &[f32], rate: f32, chunk: u32, room: &mut [f32]) -> Vec<f32> {
    let n = mixed.len() / 3;
    let mut rows = vec![0.0; crate::room::combs * n];
    for k in 0..crate::room::combs {
        crate::room::comb_run(k, rate, n, chunk as i32, mixed, 3, 2, room, &mut rows);
    }
    let mut wet = vec![0.0; 2 * n];
    for side in 0..2 {
        crate::room::allpass_run(side, rate, n, chunk as i32, &rows, room, &mut wet);
    }
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        out.push(soft(mixed[3 * i] + room_level * wet[2 * i]));
        out.push(soft(mixed[3 * i + 1] + room_level * wet[2 * i + 1]));
    }
    out
}
