// The harpsichord's soundboard (and its undamped strings ringing along):
// a bank of modes, each a two-pole resonator, driven by the harpsichord
// alone and added to the mix. As measured on the Thomas Barton harpsichord
// of 1709 (St Cecilia's Hall, Edinburgh; its sample library), what rings on
// after the damper falls by 60 dB in about 1-1.5 s below 350 Hz, 4.5 s from
// 350 Hz to 1.4 kHz, 2.3 s to 3 kHz and 1.6 s above (as Välimäki et al.
// 2004 found on theirs: 0.5 s and 4.5 s), so a note's tail outlasts many
// chunks of music: the modes' state is carried from one chunk to the next
// (see the soundboard entry point in lib.rs).
//
// Each mode's state is its last two outputs: modes * 2 floats a slot.

use crate::prelude::*;

pub const modes: usize = 64;

// Mode m's frequency: spread evenly on a log scale from 90 Hz to 5 kHz, each
// moved by a hashed fraction of its step (so they do not sit in a pattern).
#[inline]
pub fn mode_freq(m: usize) -> f32 {
    let u = (m as f32 + hash2(m as i32, 61)) / modes as f32;
    90.0 * (5000.0f32 / 90.0).powf(u)
}

// How long the soundboard rings at frequency f (T60, in seconds),
// interpolated on a log scale between the measured bands' middles.
#[inline]
pub fn ring_time(f: f32) -> f32 {
    let x = f.log2();
    // (band middles: 110, 265, 495, 990, 2050, 4900 Hz)
    let (x0, x1, x2, x3, x4, x5) = (6.78f32, 8.05, 8.95, 9.95, 11.0, 12.26);
    let (t0, t1, t2, t3, t4, t5) = (1.1f32, 1.5, 4.1, 4.6, 2.3, 1.6);
    let lerp = |a: f32, b: f32, ta: f32, tb: f32| ta + (tb - ta) * saturate((x - a) / (b - a));
    if x < x1 {
        lerp(x0, x1, t0, t1)
    } else if x < x2 {
        lerp(x1, x2, t1, t2)
    } else if x < x3 {
        lerp(x2, x3, t2, t3)
    } else if x < x4 {
        lerp(x3, x4, t3, t4)
    } else {
        lerp(x4, x5, t4, t5)
    }
}

// The soundboard's overall level, against the harpsichord's: tuned so that
// what rings on half a second after a note's damper is some 30 dB under
// the note's peak, as on the Barton (see the test below).
pub const wet: f32 = 0.0013;

// Mode m's resonator at `rate`: y = a1 y' - a2 y'' + b x, and its pan
// weights (left, right).
#[inline]
pub fn mode_coeffs(m: usize, rate: f32) -> (f32, f32, f32, f32, f32) {
    let f = mode_freq(m);
    let r = (-6.9078 / (ring_time(f) * rate)).exp();
    let w = 2.0 * PI * f / rate;
    let a1 = 2.0 * r * w.cos();
    let a2 = r * r;
    // (normalised so each mode rings at about the same level whatever its
    // decay: 1 - r is the input a resonance of that decay holds steady)
    let b = wet * (1.0 - r) * 40.0;
    let p = hash2(m as i32, 67);
    (a1, a2, b, (1.0 - 0.6 * p).sqrt(), (0.4 + 0.6 * p).sqrt())
}

// The bank over a chunk on the CPU (as the soundboard and music_mix entry
// points do on the GPU): from `state` (modes * 2 floats, left updated),
// harp (n samples) to wet (n stereo frames, interleaved).
#[cfg(not(target_arch = "spirv"))]
pub fn run(rate: f32, harp: &[f32], state: &mut [f32], out: &mut [f32]) {
    out.iter_mut().for_each(|v| *v = 0.0);
    for m in 0..modes {
        let (a1, a2, b, pl, pr) = mode_coeffs(m, rate);
        let (mut y1, mut y2) = (state[2 * m], state[2 * m + 1]);
        for (i, &x) in harp.iter().enumerate() {
            let y = a1 * y1 - a2 * y2 + b * x;
            y2 = y1;
            y1 = y;
            out[2 * i] += pl * y;
            out[2 * i + 1] += pr * y;
        }
        state[2 * m] = y1;
        state[2 * m + 1] = y2;
    }
}

#[cfg(all(test, not(target_arch = "spirv")))]
mod tests {
    // One harpsichord note held 0.22 s (as the arpeggio's are), through the
    // bank: what rings half a second after the damper, against the note's
    // peak.
    #[test]
    fn rings_on_as_the_barton() {
        let rate = 48000.0;
        let n = (rate * 1.5) as usize;
        let harp: Vec<f32> = (0..n).map(|i| {
            let t = i as f32 / rate;
            let (l, r) = crate::music::harp_note(rate, t, 415.0, t, 0.22);
            0.5 * (l + r)
        }).collect();
        let mut state = vec![0.0; 2 * super::modes];
        let mut wet = vec![0.0; 2 * n];
        super::run(rate, &harp, &mut state, &mut wet);
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let peak = rms(&harp[(0.02 * rate) as usize..(0.07 * rate) as usize]);
        let at = ((0.22 + 0.5) * rate) as usize;
        let tail = rms(&wet[2 * at..2 * (at + 2400)]);
        let db = 20.0 * (tail / peak).log10();
        eprintln!("soundboard 0.5 s after the damper: {db:.1} dB under the note's peak");
        assert!((-36.0..-24.0).contains(&db), "{db}");
        assert!(state.iter().all(|v| v.is_finite()));
    }
}
