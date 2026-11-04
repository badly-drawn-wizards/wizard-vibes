// fut/sfx.fut: sound effects, rendered once each on the host (sfx_samples)
// and kept by the app. Host only (not in the SPIR-V): filters and reverb
// over whole signals, plain Rust over Vecs. (The Futhark does the filters as
// scans of affine maps; done in order, as here, they are the recurrences.)
//
// Every effect is the same small synth, set up differently (patch, below):
//   two layers of white noise, each through a resonant filter (low-pass or
//   band-pass) whose frequency sweeps, each with its own envelope;
//   a tonal part, a pure function of time (zaps, booms, bells, chords,
//   crackle);
//   and a reverb, how much of it varying by effect.
// Each comes out with its loudest sample at its own level.
//
// (Not in the prelude: some of its names, e.g. tone, are other modules'.)

use crate::prelude::*;

const tau: f32 = 2.0 * PI;

pub const sfx_cast_fire: i32 = 0;
pub const sfx_cast_strike: i32 = 1;
pub const sfx_cast_wind: i32 = 2;
pub const sfx_burst: i32 = 3;
pub const sfx_thunder: i32 = 4;
pub const sfx_gust: i32 = 5;
pub const sfx_hurt: i32 = 6;
pub const sfx_defeat: i32 = 7;
pub const sfx_shield_up: i32 = 8;
pub const sfx_block: i32 = 9;
pub const sfx_shield_down: i32 = 10;
pub const sfx_fizzle: i32 = 11;
pub const sfx_mark: i32 = 12;
pub const sfx_appear: i32 = 13;
pub const sfx_count: i32 = 14;

// ---------------------------------------------------------------------
// Building blocks

// An envelope: up over `attack` seconds, then dying away by e every `decay`.
pub fn env(attack: f32, decay: f32, t: f32) -> f32 {
    if t < 0.0 { 0.0 } else { (1.0 - (-t / attack).exp()) * (-t / decay).exp() }
}

// A tone of frequency f0 e^(rate t) with `harmonics` harmonics at 1/k (its
// phase the integral of its frequency).
pub fn tone(f0: f32, rate: f32, harmonics: i32, t: f32) -> f32 {
    let warp = if rate.abs() < 1e-4 { t } else { ((rate * t).exp() - 1.0) / rate };
    let mut s = 0.0f32;
    for k in 0..harmonics {
        let kf = (k + 1) as f32;
        s = s + (tau * f0 * kf * warp).sin() / kf;
    }
    s
}

// A bell struck at time 0: partials at the ratios of a struck bar.
pub fn bell(f: f32, decay: f32, t: f32) -> f32 {
    if t < 0.0 {
        return 0.0;
    }
    let p = |r: f32, a: f32, d: f32| a * (tau * f * r * t).sin() * (-t / (decay * d)).exp();
    (1.0 - (-t * 900.0).exp()) * (p(1.0, 1.0, 1.0) + p(2.76, 0.5, 0.6) + p(5.4, 0.25, 0.35) + p(8.93, 0.12, 0.2))
}

// Crackle: sparse clicks, `rate` a second on average, each a tiny burst.
pub fn crackle(seed: i32, rate: f32, t: f32) -> f32 {
    let bin = 0.004f32;
    let i = ftoi(t / bin);
    let u = t - i as f32 * bin;
    if hash2(seed.wrapping_add(i), 11) > rate * bin {
        0.0
    } else {
        (2.0 * hash2(seed.wrapping_add(i), 13) - 1.0) * (-u * 900.0).exp() * (tau * (2000.0 + 3000.0 * hash2(seed.wrapping_add(i), 17)) * u).sin()
    }
}

// A biquad's coefficients (b0, b1, b2, a1, a2; normalised): a resonant
// low-pass, or a band-pass, at f Hz with quality q (after R. Bristow-
// Johnson's cookbook).
pub fn biquad_coeffs(lowpass: bool, f: f32, q: f32, rate: f32) -> (f32, f32, f32, f32, f32) {
    let w = tau * (0.45 * rate).min((10.0f32).max(f)) / rate;
    let (c, s) = (w.cos(), w.sin());
    let alpha = s / (2.0 * q);
    let a0 = 1.0 + alpha;
    if lowpass {
        ((1.0 - c) / 2.0 / a0, (1.0 - c) / a0, (1.0 - c) / 2.0 / a0, -2.0 * c / a0, (1.0 - alpha) / a0)
    } else {
        (alpha / a0, 0.0, -alpha / a0, -2.0 * c / a0, (1.0 - alpha) / a0)
    }
}

// x through a biquad whose coefficients change from sample to sample:
// y[i] = u[i] - a1 y[i-1] - a2 y[i-2], u[i] = b0 x[i] + b1 x[i-1] + b2 x[i-2]
// (as the Futhark's scan of affine maps computes it, in order).
pub fn biquad(coef: &[(f32, f32, f32, f32, f32)], x: &[f32]) -> Vec<f32> {
    let at = |j: isize| if j >= 0 { x[j as usize] } else { 0.0 };
    let (mut v0, mut v1) = (0.0f32, 0.0f32);
    let mut y = Vec::with_capacity(x.len());
    for (i, &(b0, b1, b2, a1, a2)) in coef.iter().enumerate() {
        let i = i as isize;
        let u = b0 * at(i) + b1 * at(i - 1) + b2 * at(i - 2);
        let n0 = -a1 * v0 + -a2 * v1 + u;
        v1 = v0;
        v0 = n0;
        y.push(v0);
    }
    y
}

// A feedback comb: y[i] = x[i] + g y[i - d].
pub fn comb(d: usize, g: f32, x: &[f32]) -> Vec<f32> {
    let mut y = Vec::with_capacity(x.len());
    for i in 0..x.len() {
        let prev = if i >= d { y[i - d] } else { 0.0 };
        y.push(g * prev + x[i]);
    }
    y
}

// An allpass: y[i] = -g x[i] + x[i - d] + g y[i - d].
pub fn allpass(d: usize, g: f32, x: &[f32]) -> Vec<f32> {
    let u: Vec<f32> = (0..x.len()).map(|i| -g * x[i] + (if i >= d { x[i - d] } else { 0.0 })).collect();
    comb(d, g, &u)
}

// A Schroeder reverb (Freeverb's delays, scaled to `rate`): four combs in
// parallel, then two allpasses; the two sides through allpasses of slightly
// different lengths. Fed through a low-pass first.
pub fn reverb(rate: f32, x: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let s = |k: usize| (k as f32 * rate / 44100.0) as usize;
    let n = x.len();
    let dark = biquad(&vec![biquad_coeffs(true, 3500.0, 0.7, rate); n], x);
    let (c1, c2, c3, c4) = (comb(s(1116), 0.84, &dark), comb(s(1188), 0.84, &dark), comb(s(1277), 0.84, &dark), comb(s(1356), 0.84, &dark));
    let room: Vec<f32> = (0..n).map(|i| 0.25 * (c1[i] + c2[i] + c3[i] + c4[i])).collect();
    (allpass(s(556), 0.5, &allpass(s(441), 0.5, &room)), allpass(s(579), 0.5, &allpass(s(464), 0.5, &room)))
}

// ---------------------------------------------------------------------
// The effects

// A noise layer: its filter (low-pass or band-pass), sweeping from f0 to
// f1 Hz over `sweep` seconds (exponentially), with quality q; its level;
// its envelope (attack and decay, or with `swell` rising and falling over
// most of the effect); a tremble (am_rate Hz, am_depth 0..1); and `pan`
// sweeping it from left to right.
#[derive(Clone, Copy)]
pub struct Layer {
    pub lowpass: bool,
    pub f0: f32,
    pub f1: f32,
    pub sweep: f32,
    pub q: f32,
    pub level: f32,
    pub attack: f32,
    pub decay: f32,
    pub swell: bool,
    pub am_rate: f32,
    pub am_depth: f32,
    pub pan: bool,
}

pub const quiet: Layer = Layer {
    lowpass: false,
    f0: 1000.0,
    f1: 1000.0,
    sweep: 1.0,
    q: 1.0,
    level: 0.0,
    attack: 0.01,
    decay: 0.1,
    swell: false,
    am_rate: 0.0,
    am_depth: 0.0,
    pan: false,
};

pub fn bp(f0: f32, f1: f32, sweep: f32, q: f32, level: f32, attack: f32, decay: f32) -> Layer {
    Layer { f0, f1, sweep, q, level, attack, decay, ..quiet }
}

pub fn lp(f0: f32, f1: f32, sweep: f32, q: f32, level: f32, attack: f32, decay: f32) -> Layer {
    Layer { lowpass: true, ..bp(f0, f1, sweep, q, level, attack, decay) }
}

// An effect: how long (with its reverb's tail), its two noise layers, how
// much reverb, and how loud its loudest sample.
#[derive(Clone, Copy)]
pub struct Patch {
    pub length: f32,
    pub a: Layer,
    pub b: Layer,
    pub wet: f32,
    pub peak: f32,
}

pub fn patch(k: i32) -> Patch {
    let swell = |l: Layer| Layer { swell: true, ..l };
    let swell_pan = |l: Layer| Layer { swell: true, pan: true, ..l };
    let am = |l: Layer, am_rate: f32, am_depth: f32| Layer { am_rate, am_depth, ..l };
    if k == sfx_cast_fire {
        // a whoosh opening up as it goes, over a soft thump
        Patch { length: 0.9, a: bp(400.0, 2500.0, 0.25, 0.8, 0.6, 0.01, 0.25), b: lp(300.0, 120.0, 0.15, 0.7, 0.5, 0.005, 0.12), wet: 0.15, peak: 0.8 }
    } else if k == sfx_cast_strike {
        // a snap, and a buzzing hiss trembling at the mains' rate
        Patch { length: 0.8, a: bp(3000.0, 6000.0, 0.1, 2.0, 0.4, 0.001, 0.06), b: am(bp(1200.0, 1200.0, 1.0, 6.0, 0.25, 0.002, 0.15), 90.0, 0.8), wet: 0.2, peak: 0.75 }
    } else if k == sfx_cast_wind {
        // a resonant swoosh crossing from left to right, air above it
        Patch { length: 1.1, a: swell_pan(bp(500.0, 2200.0, 0.8, 3.0, 0.7, 0.0, 0.0)), b: swell_pan(bp(2500.0, 5000.0, 0.8, 1.0, 0.3, 0.0, 0.0)), wet: 0.25, peak: 0.7 }
    } else if k == sfx_burst {
        // a roar darkening as it dies, over a deep thump
        Patch { length: 2.2, a: lp(2500.0, 150.0, 0.8, 0.9, 0.8, 0.003, 0.5), b: lp(120.0, 120.0, 1.0, 1.2, 0.9, 0.002, 0.3), wet: 0.35, peak: 0.95 }
    } else if k == sfx_thunder {
        // a crack, then rolling rumble
        Patch { length: 3.0, a: bp(4000.0, 2500.0, 0.1, 0.5, 0.7, 0.0005, 0.05), b: am(lp(180.0, 180.0, 1.0, 0.8, 1.0, 0.05, 0.9), 2.3, 0.6), wet: 0.5, peak: 0.95 }
    } else if k == sfx_gust {
        // a glassy hiss falling, under the chimes
        Patch { length: 1.3, a: bp(6000.0, 3000.0, 0.3, 4.0, 0.3, 0.001, 0.2), b: bp(1500.0, 1500.0, 1.0, 1.0, 0.2, 0.001, 0.1), wet: 0.35, peak: 0.6 }
    } else if k == sfx_hurt {
        // a smack (with the thud and the sour chord)
        Patch { length: 0.7, a: lp(1500.0, 300.0, 0.1, 1.0, 0.6, 0.001, 0.08), b: quiet, wet: 0.1, peak: 0.85 }
    } else if k == sfx_defeat {
        // a whoosh falling away under the bells
        Patch { length: 2.2, a: bp(3000.0, 500.0, 1.2, 2.0, 0.25, 0.05, 0.6), b: quiet, wet: 0.45, peak: 0.8 }
    } else if k == sfx_shield_up {
        // a resonance rising, and air
        Patch { length: 1.1, a: swell(bp(400.0, 3000.0, 0.6, 5.0, 0.4, 0.0, 0.0)), b: swell(bp(6000.0, 6000.0, 1.0, 1.0, 0.15, 0.0, 0.0)), wet: 0.35, peak: 0.55 }
    } else if k == sfx_block {
        // a spray of sparks with the ringing bell
        Patch { length: 1.5, a: bp(5000.0, 5000.0, 1.0, 1.0, 0.4, 0.001, 0.03), b: quiet, wet: 0.4, peak: 0.75 }
    } else if k == sfx_shield_down {
        Patch { length: 0.9, a: swell(bp(3000.0, 300.0, 0.4, 5.0, 0.4, 0.0, 0.0)), b: quiet, wet: 0.3, peak: 0.5 }
    } else if k == sfx_fizzle {
        // a sputter
        Patch { length: 0.4, a: am(bp(800.0, 800.0, 1.0, 3.0, 0.4, 0.005, 0.1), 35.0, 0.9), b: quiet, wet: 0.05, peak: 0.5 }
    } else if k == sfx_mark {
        // a whistle rising to the strike
        Patch { length: 1.3, a: bp(600.0, 2500.0, 1.1, 8.0, 0.25, 0.3, 10.0), b: quiet, wet: 0.2, peak: 0.45 }
    } else {
        // (sfx_appear) a breath of air swelling in
        Patch { length: 1.6, a: swell(bp(1000.0, 6000.0, 0.8, 2.0, 0.3, 0.0, 0.0)), b: quiet, wet: 0.4, peak: 0.5 }
    }
}

// How long each lasts, in seconds.
pub fn sfx_length(k: i32) -> f32 {
    patch(k).length
}

// A layer's filter frequency, and its level on side c (0 left), at time t.
pub fn layer_freq(l: Layer, t: f32) -> f32 {
    l.f0 * (l.f1 / l.f0).powf(saturate(t / l.sweep))
}

pub fn layer_gain(l: Layer, length: f32, c: i32, t: f32) -> f32 {
    let shape = if l.swell { pow((PI * saturate(t / (0.75 * length))).sin(), 2.0) } else { env(l.attack, l.decay, t) };
    let tremble = 1.0 - l.am_depth * (0.5 + 0.5 * (tau * l.am_rate * t).sin());
    let side = if !l.pan {
        1.0
    } else {
        let u = saturate(t / (0.75 * length));
        0.3 + 0.7 * (if c == 0 { 1.0 - u } else { u })
    };
    l.level * shape * tremble * side
}

// The tonal part of effect k at time t, on side c.
pub fn tonal(k: i32, c: i32, t: f32) -> f32 {
    let sd = 1000 * c;
    if k == sfx_cast_fire {
        0.25 * env(0.01, 0.25, t) * crackle(sd + 2, 120.0, t)
    } else if k == sfx_cast_strike {
        // a zap: a buzzing tone diving in pitch
        let buzz = 0.6 + 0.4 * (tau * 60.0 * t).sin();
        0.3 * env(0.003, 0.12, t) * buzz * tone(1400.0, -7.0, 8, t)
    } else if k == sfx_burst {
        let boom = (tau * (35.0 * t + 30.0 / 6.0 * (1.0 - (-6.0 * t).exp()))).sin() * env(0.004, 0.35, t);
        0.5 * boom + 0.18 * env(0.05, 0.5, t) * crackle(sd + 6, 60.0 * (-t).exp(), t)
    } else if k == sfx_thunder {
        // a second, smaller crack a moment after the first
        0.3 * env(0.0005, 0.03, t - 0.12) * crackle(sd + 7, 200.0, t)
    } else if k == sfx_gust {
        // the blade shattering: high chimes, scattered
        let chime = |j: i32| {
            let at = 0.15 * hash2(sd + j, 21);
            let f = 2000.0 + 4000.0 * hash2(sd + j, 23);
            env(0.001, 0.12, t - at) * (tau * f * (t - at)).sin()
        };
        let mut s = 0.0f32;
        for j in 0..8 {
            s = s + chime(j);
        }
        0.12 * s
    } else if k == sfx_hurt {
        let thud = (tau * (60.0 * t + 50.0 / 12.0 * (1.0 - (-12.0 * t).exp()))).sin() * env(0.003, 0.12, t);
        0.6 * thud + 0.12 * env(0.005, 0.25, t) * (tone(330.0, 0.0, 4, t) + tone(349.0, 0.0, 4, t))
    } else if k == sfx_defeat {
        // bells falling down a minor arpeggio (semitones down 0 3 7 12 15 19)
        let down = |j: i32| 12 * idiv(j, 3) + (if imod(j, 3) == 0 { 0 } else if imod(j, 3) == 1 { 3 } else { 7 });
        let note = |j: i32| bell(880.0 * (2.0f32).powf(-(down(j) as f32) / 12.0), 0.5, t - 0.11 * j as f32);
        let mut s = 0.0f32;
        for j in 0..6 {
            s = s + note(j);
        }
        0.16 * s
    } else if k == sfx_shield_up || k == sfx_shield_down {
        // a shimmer, rising or falling, trembling
        let rate = if k == sfx_shield_up { 1.6 } else { -2.4 };
        let base = if k == sfx_shield_up { 300.0 } else { 900.0 };
        let mut s = 0.0f32;
        for j in 0..6 {
            s = s + (tau * base * (1.0 + 0.013 * j as f32) * ((2.0f32).powf(imod(j, 3) as f32)) * ((rate * t).exp() - 1.0) / rate).sin();
        }
        let len = if k == sfx_shield_up { 0.7 } else { 0.5 };
        0.07 * (0.75 + 0.25 * (tau * 14.0 * t).sin()) * pow((PI * saturate(t / len)).sin(), 2.0) * s
    } else if k == sfx_block {
        0.3 * bell(620.0, 0.35, t)
    } else if k == sfx_fizzle {
        0.18 * env(0.005, 0.1, t) * tone(110.0, -1.5, 6, t)
    } else if k == sfx_mark {
        // a whine rising to the strike, pulsing faster as it does
        let pulse = 0.5 + 0.5 * (tau * (6.0 * t + 9.0 * t * t)).sin();
        0.12 * smoothstep(0.0, 0.2, t) * smoothstep(1.1, 1.0, t) * pulse * tone(420.0, 1.1, 3, t)
    } else if k == sfx_appear {
        let chord = tone(1046.5, 0.0, 2, t) + tone(1318.5, 0.0, 2, t) + tone(1568.0, 0.0, 2, t);
        0.06 * env(0.08, 0.4, t) * chord
    } else {
        0.0
    }
}

// n stereo samples of effect k at `rate` a second, interleaved (left,
// right).
pub fn sfx_samples(k: i32, rate: f32, n: usize) -> Vec<f32> {
    let p = patch(k);
    let ts: Vec<f32> = (0..n).map(|i| i as f32 / rate).collect();
    // the noise layers, (layer, side) by 2 * layer + side: each its own
    // noise through its filter, brought to unit loudness, under its envelope
    let noise: Vec<Vec<f32>> = (0..4)
        .map(|j: i32| {
            let l = if j < 2 { p.a } else { p.b };
            let c = j % 2;
            let white: Vec<f32> = (0..n).map(|i| 2.0 * hash3(i as i32, j + 17, k + 3) - 1.0).collect();
            let coef: Vec<_> = ts.iter().map(|&t| biquad_coeffs(l.lowpass, layer_freq(l, t), l.q, rate)).collect();
            let y = biquad(&coef, &white);
            let mut sum = 0.0f32;
            for v in &y {
                sum += v * v;
            }
            let rms = (sum / n as f32).sqrt() + 1e-6;
            ts.iter().zip(&y).map(|(&t, &v)| v / rms * layer_gain(l, p.length, c, t)).collect()
        })
        .collect();
    let dry = |c: usize| -> Vec<f32> { (0..n).map(|i| tonal(k, c as i32, ts[i]) + noise[c][i] + noise[2 + c][i]).collect() };
    let (dl, dr) = (dry(0), dry(1));
    let mono: Vec<f32> = (0..n).map(|i| 0.5 * (dl[i] + dr[i])).collect();
    let (wl, wr) = reverb(rate, &mono);
    let l: Vec<f32> = (0..n).map(|i| dl[i] + p.wet * wl[i]).collect();
    let r: Vec<f32> = (0..n).map(|i| dr[i] + p.wet * wr[i]).collect();
    let loudest = l.iter().chain(&r).fold(f32::NEG_INFINITY, |m, v| m.max(v.abs())) + 1e-6;
    (0..2 * n).map(|j| p.peak / loudest * (if j % 2 == 0 { l[j / 2] } else { r[j / 2] })).collect()
}

#[cfg(test)]
mod tests {
    // y[i] = -g x[i] + x[i - d] + g y[i - d], as Futhark computes it.
    // Every effect is finite and peaks where its patch says, not silent.
    #[test]
    fn effects_are_sound() {
        for k in 0..super::sfx_count {
            let n = (super::sfx_length(k) * 48000.0).ceil() as usize;
            let y = super::sfx_samples(k, 48000.0, n);
            assert!(y.iter().all(|v| v.is_finite()), "effect {k} is not finite");
            let peak = y.iter().fold(0f32, |m, v| m.max(v.abs()));
            let rms = (y.iter().map(|v| v * v).sum::<f32>() / y.len() as f32).sqrt();
            eprintln!("effect {k}: peak {peak:.3}, rms {rms:.4}");
            assert!(peak > 0.01 && peak <= 1.0, "effect {k} peaks at {peak}");
        }
    }

    #[test]
    fn allpass_is_the_textbook_one() {
        let y = super::allpass(2, 0.5, &[1.0, 0.0, 0.0, 2.0, 0.0, -1.0, 0.0, 0.0]);
        assert_eq!(y, [-0.5, 0.0, 0.75, -1.0, 0.375, 2.0, 0.1875, 0.0]);
    }
}
