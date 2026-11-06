// Shared synthesis for the bass-music songs (glitch.rs, wub.rs): filtered
// sawtooths and talking (formant) voices built from their harmonics, as
// every voice here is a pure function of time (no filter state to carry).

use crate::prelude::*;

const tau: f32 = 2.0 * PI;

#[inline]
pub fn gauss(x: f32) -> f32 {
    (-x * x).exp()
}

// A sawtooth of frequency f at time u (since its note began) through a
// resonant low-pass at fc: harmonic k kept by 1 / (1 + (k f / fc)^4), and
// lifted by `reso` round the cutoff; up to n harmonics (and none above
// 12 kHz).
#[inline]
pub fn saw_lp(f: f32, u: f32, fc: f32, reso: f32, n: i32) -> f32 {
    let p = tau * f * u;
    let mut s = 0.0f32;
    let mut k = 1;
    while k <= n {
        let fk = k as f32 * f;
        if fk < 12000.0 {
            let x = fk / fc;
            let w = 1.0 / (1.0 + x * x * x * x) + reso * gauss((fk - fc) / (0.35 * fc + 40.0));
            s = s + w * (k as f32 * p).sin() / k as f32;
        }
        k += 1;
    }
    s
}

// A voice of frequency f at time u whose harmonics are weighted by two
// formants (f1, f2: a vowel), the talking bass's and the chops' "yoi" and
// "ah": up to n harmonics.
#[inline]
pub fn formant(f: f32, u: f32, f1: f32, f2: f32, n: i32) -> f32 {
    let p = tau * f * u;
    let mut s = 0.0f32;
    let mut k = 1;
    while k <= n {
        let fk = k as f32 * f;
        if fk < 9000.0 {
            let w = gauss((fk - f1) / 140.0) + 0.7 * gauss((fk - f2) / 220.0) + 0.12 / k as f32;
            s = s + w * (k as f32 * p).sin();
        }
        k += 1;
    }
    s
}

// The note sounding at step i of a tune written out with melody!() (its
// code function, and how many steps the tune has): (its code, its first
// step, its last); code 0 if none (a rest, or the release after a note:
// the note it releases is given, so it can ring out).
#[inline]
pub fn tune_note(code: impl Fn(i32) -> i32, i: i32, steps: i32) -> (i32, i32, i32) {
    let k = if code(i) == 0 { i - 1 } else { i };
    if k < 0 || code(k) == 0 {
        return (0, 0, 0);
    }
    let (mut j, mut go) = (k, true);
    while go && j > 0 {
        if code(j) == 15 {
            j -= 1;
        } else {
            go = false;
        }
    }
    let (mut e, mut go) = (k, true);
    while go && e < steps - 1 {
        if code(e + 1) == 15 {
            e += 1;
        } else {
            go = false;
        }
    }
    (code(j), j, e)
}

// Degree d of a scale (1 = the tonic `root`, a MIDI note), as a MIDI note:
// major (0 2 4 5 7 9 11) or natural minor (0 2 3 5 7 8 10).
#[inline]
pub fn scale_degree(root: f32, d: i32, major: bool) -> f32 {
    let o = idiv(d - 1, 7);
    let k = imod(d - 1, 7);
    let step = if k == 0 {
        0
    } else if k == 1 {
        2
    } else if k == 2 {
        if major { 4 } else { 3 }
    } else if k == 3 {
        5
    } else if k == 4 {
        7
    } else if k == 5 {
        if major { 9 } else { 8 }
    } else if major {
        11
    } else {
        10
    };
    root + (12 * o + step) as f32
}

#[inline]
pub fn minor_degree(root: f32, d: i32) -> f32 {
    scale_degree(root, d, false)
}

// The time to read a sung (or played) note's oscillator at, u seconds into
// a note of frequency f: slid into from frequency pf over `glide` seconds
// (the frequency moving linearly, as a phase offset: the integral of the
// difference), with vibrato of `depth` (a fraction) at `rate` Hz coming in
// between `from` and `to` seconds into the note.
#[inline]
pub fn sung_time(f: f32, pf: f32, u: f32, glide: f32, depth: f32, rate: f32, from: f32, to: f32) -> f32 {
    let slid = (pf / f - 1.0) * if u < glide { u - u * u / (2.0 * glide) } else { 0.5 * glide };
    let w = 2.0 * PI * rate;
    let wob = depth * smoothstep(from, to, u) / w * (1.0 - (w * u).cos());
    u + slid + wob
}

// Drive into a soft clipper, normalised so a full-scale sine stays near 1.
#[inline]
pub fn drive(x: f32, amount: f32) -> f32 {
    (amount * x).tanh() / amount.tanh()
}

// The sidechain pump: 1 - depth just after a kick `since` seconds ago,
// recovering over `time`.
#[inline]
pub fn pump(since: f32, depth: f32, time: f32) -> f32 {
    if since < 0.0 {
        return 1.0;
    }
    1.0 - depth * smoothstep(0.0, 0.004, since) * (-since / time * 3.0).exp()
}
