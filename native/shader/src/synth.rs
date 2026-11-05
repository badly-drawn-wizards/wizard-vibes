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
