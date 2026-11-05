// Wub Ward: the fifth song, dubstep. 32 bars at 140 beats a minute, half
// time (the snare on three), in G minor, on Gm, Eb, Bb, F (a bar each in
// the intro and build; the drops ride the root). Its voices: a deep kick
// and a big snare (noise and a falling body, much of it sent to the room);
// hats; a pad and a plucked arpeggio; the build's snare roll (quarters,
// eighths, sixteenths, thirty-seconds, rising) and its riser (noise
// brightening and a tone sweeping up); the wobble bass (two sawtooths
// a little apart, a reese, through a resonant low-pass whose cutoff an LFO
// swings in time: quarters, eighths, triplets, sixteenths by the half
// bar); the talking bass (its harmonics weighted by formants that an LFO
// moves between "o" and "i": yoi); and a sub under both.
//
// In four sections of eight bars:
//   1  intro: the pad and the arpeggio, hats; the kick from bar 5;
//   2  build: the roll and the riser, the pad fading; a beat of silence;
//   3  the drop: the wobble bass, the kit in half time;
//   4  the second drop: the talking bass, the wobble answering it.

use crate::music::{bright_noise, freq, noise};
use crate::prelude::*;
use crate::synth::{drive, formant, pump, saw_lp};

pub const bpm: f32 = 140.0;
pub const beat: f32 = 60.0 / bpm;
pub const sixteenth: f32 = beat / 4.0;
pub const bar: f32 = 4.0 * beat;
pub const period: f32 = 32.0 * bar;
const tau: f32 = 2.0 * PI;

#[inline]
fn section(t: f32) -> i32 {
    ftoi((t / (8.0 * bar)).floor())
}

// The chord at time t (0..3: Gm, Eb, Bb, F): its root (a MIDI note in the
// second octave) and its third (minor on Gm).
#[inline]
fn root(c: i32) -> f32 {
    (if c == 0 { 43 } else if c == 1 { 39 } else if c == 2 { 46 } else { 41 }) as f32
}
#[inline]
fn third(c: i32) -> f32 {
    if c == 0 { 3.0 } else { 4.0 }
}
#[inline]
fn chord_at(t: f32) -> i32 {
    imod(ftoi((t / bar).floor()), 4)
}

// ---------------------------------------------------------------------
// Drums

#[inline]
fn kick_at(rate: f32, t: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let p = tau * (42.0 * u + 118.0 / 22.0 * (1.0 - (-22.0 * u).exp()));
    drive((-u * 5.0).exp() * p.sin(), 2.8) + 0.3 * (-u * 900.0).exp() * noise(rate, t)
}

#[inline]
fn snare_at(rate: f32, t: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let body = (-u * 16.0).exp() * (tau * (210.0 * u - 60.0 * u * u)).sin();
    let crack = (-u * 9.0).exp() * (0.6 * noise(rate, t) + 0.9 * bright_noise(rate, t));
    0.7 * body + 0.8 * crack
}

// The half-time kit (the drops): the kick on one and the last eighth of
// two, the snare on three; hats in eighths. (mono, the time since the last
// kick)
#[inline]
fn drop_kit(rate: f32, t: f32) -> (f32, f32) {
    let b = (t / bar).floor();
    let ub = t - b * bar;
    let mut v = 0.0f32;
    let mut since = 10.0f32;
    // (each hit, and its ring into the rest of the bar)
    let k1 = ub;
    let k2 = ub - 1.5 * beat;
    v += kick_at(rate, t, k1);
    since = since.min(k1);
    if k2 >= 0.0 {
        v += 0.8 * kick_at(rate, t, k2);
        since = k2;
    }
    v += 0.9 * snare_at(rate, t, ub - 2.0 * beat);
    // (the last bar's snare, ringing on)
    v += 0.9 * snare_at(rate, t, ub + 2.0 * beat);
    let e = ub / (0.5 * beat);
    let ue = (e - e.floor()) * 0.5 * beat;
    v += 0.12 * (-ue * 45.0).exp() * bright_noise(rate, t);
    (v, since)
}

// The build's roll: snares in quarters for four bars, eighths for two,
// sixteenths for one, thirty-seconds for the last (but its last beat,
// silent), swelling, their pitch rising.
#[inline]
fn roll(rate: f32, t: f32, b: i32, ub: f32) -> f32 {
    let step = if b < 4 { beat } else if b < 6 { 0.5 * beat } else if b < 7 { 0.25 * beat } else { 0.125 * beat };
    if b == 7 && ub >= 3.0 * beat {
        return 0.0;
    }
    let u = ub - step * (ub / step).floor();
    let progress = (b as f32 * bar + ub) / (8.0 * bar);
    let body = (-u * 22.0).exp() * (tau * (180.0 + 260.0 * progress) * u).sin();
    let crack = (-u * 18.0).exp() * bright_noise(rate, t);
    (0.25 + 0.75 * progress) * (0.5 * body + 0.7 * crack)
}

// ---------------------------------------------------------------------
// Tonal voices

// The pad: the chord an octave above the bass.
#[inline]
fn pad(t: f32) -> (f32, f32) {
    let b = (t / bar).floor();
    let u = t - b * bar;
    let c = chord_at(t);
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut j = 0;
    while j < 3 {
        let semis = if j == 0 { 0.0 } else if j == 1 { third(c) } else { 7.0 };
        let f = freq(root(c) + 24.0 + semis);
        l += saw_lp(f * 0.995, u, 1100.0, 0.0, 10);
        r += saw_lp(f * 1.005, u, 1100.0, 0.0, 10);
        j += 1;
    }
    let env = smoothstep(0.0, 0.3, u) * smoothstep(bar + 0.1, bar - 0.1, u);
    (env * l, env * r)
}

// The plucked arpeggio: sixteenths through the chord, an octave above the
// pad, their filter closing fast.
#[inline]
fn pluck(t: f32) -> f32 {
    let k = ftoi((t / sixteenth).floor());
    let u = t - k as f32 * sixteenth;
    let c = chord_at(t);
    let j = imod(k, 6);
    let semis = if j == 0 || j == 4 { 0.0 } else if j == 1 || j == 3 { third(c) + 12.0 } else if j == 2 { 7.0 + 12.0 } else { 7.0 };
    let f = freq(root(c) + 36.0 + semis);
    let fc = f * (1.5 + 10.0 * (-u * 30.0).exp());
    (-u * 9.0).exp() * smoothstep(0.0, 0.002, u) * saw_lp(f, u, fc, 0.6, 16)
}

// The drops' riff: per half bar, a note (G, G, Bb, F, G, G, D, F over the
// four bars' eight halves; G2 and round it) and an LFO rate in beats
// (1, 1/2, 1/3, 1/4; the triplets every third half bar).
#[inline]
fn riff(t: f32) -> (f32, f32, f32) {
    let h = ftoi((t / (2.0 * beat)).floor());
    let i = imod(h, 8);
    let note = if i == 2 { 46 } else if i == 3 || i == 7 { 41 } else if i == 6 { 38 } else { 43 };
    let per = if imod(i, 4) == 0 { 1.0 } else if imod(i, 4) == 1 { 0.5 } else if imod(i, 4) == 2 { 1.0 / 3.0 } else { 0.25 };
    let u = t - h as f32 * 2.0 * beat;
    (freq(note as f32), per * beat, u)
}

// The wobble: a reese (two saws, 0.4% apart) through a resonant low-pass,
// its cutoff swung from near the note up to 25 times it by the LFO (a
// period of `lfo` seconds), driven hard.
#[inline]
fn wobble(f: f32, u: f32, lfo: f32) -> f32 {
    let w = 0.5 - 0.5 * (tau * u / lfo).cos();
    let fc = f * (1.3 + 24.0 * w * w);
    let s = saw_lp(f * 0.998, u, fc, 1.6, 40) + saw_lp(f * 1.002, u, fc, 1.6, 40);
    drive(0.7 * s, 3.0)
}

// The talking bass: a formant voice whose vowel the LFO moves from "o" (450,
// 800 Hz) to "i" (300, 2200 Hz) and back: yoi; driven.
#[inline]
fn talk(f: f32, u: f32, lfo: f32) -> f32 {
    let w = 0.5 - 0.5 * (tau * u / lfo).cos();
    let f1 = 450.0 - 150.0 * w;
    let f2 = 800.0 + 1400.0 * w;
    drive(0.5 * formant(f, u, f1, f2, 40), 2.5)
}

// ---------------------------------------------------------------------

// The mix (left, right) before the limit, and what is sent to the room.
#[inline]
pub fn parts(rate: f32, t: f32) -> (f32, f32, f32) {
    let k = section(t);
    let b = imod(ftoi((t / bar).floor()), 8);
    let ub = t - (t / bar).floor() * bar;
    if k <= 1 {
        let (pl, pr) = pad(t);
        let pg = if k == 0 { 0.03 } else { 0.03 * (1.0 - smoothstep(4.0 * bar, 8.0 * bar, b as f32 * bar + ub)) };
        let pk = if k == 0 { 0.06 * pluck(t) } else { 0.06 * (1.0 - b as f32 / 8.0) * pluck(t) };
        // (the intro's kick on one from bar 5, its hats in eighths)
        let kick = if k == 0 && b >= 4 { 0.7 * kick_at(rate, t, ub) } else { 0.0 };
        let e = ub / (0.5 * beat);
        let ue = (e - e.floor()) * 0.5 * beat;
        let hats = if k == 0 { 0.07 * (-ue * 60.0).exp() * bright_noise(rate, t) } else { 0.0 };
        // (the build: the roll, and the riser: noise brightening, a tone
        // sweeping from 200 Hz to 1.6 kHz; silent on the last beat)
        let (rl, riser) = if k == 1 {
            let x = (b as f32 * bar + ub) / (8.0 * bar);
            let gap = b == 7 && ub >= 3.0 * beat;
            let sweep = 200.0 * 8f32.powf(x);
            // (the sweep's phase: the integral of 200 * 8^x over the build)
            let ph = 200.0 * 8.0 * bar / 8f32.ln() * (8f32.powf(x) - 1.0);
            let r = if gap { 0.0 } else { x * x * (0.10 * (0.4 * noise(rate, t) + bright_noise(rate, t)) + 0.04 * (tau * ph).sin() * smoothstep(0.0, 1.0, sweep / 400.0)) };
            (0.5 * roll(rate, t, b, ub), 2.2 * r)
        } else {
            (0.0, 0.0)
        };
        let l = pg * pl + 0.6 * pk + kick + hats + rl + riser;
        let r = pg * pr + 0.4 * pk + kick + hats + rl + riser;
        return (l, r, 0.6 * pg * (pl + pr) + 0.5 * pk + 0.6 * rl + 0.4 * riser);
    }
    // the drops
    let (dk, since) = drop_kit(rate, t);
    let duck = pump(since, 0.7, 0.3);
    let (f, lfo, u) = riff(t);
    let env = smoothstep(0.0, 0.004, u) * smoothstep(2.0 * beat, 2.0 * beat - 0.02, u);
    // (the sub: an octave below the bass, G1)
    let sub = (tau * 0.5 * f * u).sin();
    let (bass, sw) = if k == 2 {
        (wobble(f, u, lfo), 0.0)
    } else {
        // (the second drop: the talking bass on the even half bars, the
        // wobble answering on the odd)
        let h = imod(ftoi((t / (2.0 * beat)).floor()), 2);
        if h == 0 { (talk(f, u, lfo), 0.0) } else { (wobble(f, u, lfo), 0.2) }
    };
    let low = duck * env * (0.45 * bass + 0.16 * sub);
    // (a little stereo: the bass leaning left and right by the half bar,
    // the answering wobble further)
    let wide = 1.0 + (0.1 + sw) * if imod(ftoi((t / (2.0 * beat)).floor()), 2) == 0 { 1.0 } else { -1.0 };
    let l = 0.55 * dk + 0.7 * low * wide;
    let r = 0.55 * dk + 0.7 * low * (2.0 - wide);
    (l, r, 0.25 * dk + 0.07 * low)
}
