// Cracktro: the second song, chiptune as the demoscene's cracktros and
// keygens had it (the C64's SID, the Amiga's trackers). 32 bars at 125
// beats a minute in A minor, on Am, F, C, G (a bar each). Its voices:
//   - an arpeggio: a pulse wave whose width sweeps slowly (PWM), stepping
//     through the chord's tones at 50 Hz (a tracker's tick: the chord
//     played on one channel), re-struck every sixteenth;
//   - a bass: a triangle and a narrow pulse, in octave-jumping eighths;
//   - drums as the SID made them: a kick that is a pulse sweeping down
//     from a click of noise, a snare that is a tone then noise, and hats of
//     sampled-and-held noise (metallic, as a low-rate noise channel is);
//   - a lead: a quarter-width pulse with delayed vibrato and portamento,
//     doubled a tracker's echo later, panned apart.
// In four sections of eight bars, the last beat of each a snare fill:
//   1  the arpeggio, the bass and the kick;
//   2  all the drums, and the lead plays the tune;
//   3  a breakdown: the arpeggio, the bass and hats, the tune an octave down;
//   4  everything, the tune with a harmony a third below.
//
// As the disco's (music.rs), every sample is a pure function of time
// within the loop; oscillators' phases are measured from their notes'
// starts.

use crate::music::{bright_noise, freq, noise};
use crate::prelude::*;

pub const bpm: f32 = 125.0;
pub const beat: f32 = 60.0 / bpm;
pub const eighth: f32 = beat / 2.0;
pub const sixteenth: f32 = beat / 4.0;
pub const bar: f32 = 4.0 * beat;
pub const period: f32 = 32.0 * bar;
const tick: f32 = 1.0 / 50.0;
const tau: f32 = 2.0 * PI;

#[inline]
fn section(t: f32) -> i32 {
    ftoi((t / (8.0 * bar)).floor())
}

// The chord at time t (0..3: Am, F, C, G), its root (a MIDI note in the
// second octave) and its third (minor on Am, major on the rest).
#[inline]
fn chord_at(t: f32) -> i32 {
    imod(ftoi((t / bar).floor()), 4)
}
#[inline]
fn root(c: i32) -> f32 {
    (if c == 0 { 45 } else if c == 1 { 41 } else if c == 2 { 48 } else { 43 }) as f32
}
#[inline]
fn third(c: i32) -> f32 {
    if c == 0 { 3.0 } else { 4.0 }
}

// A band-limited sawtooth at phase p (cycles; fract taken) of a wave whose
// phase steps dt a sample (PolyBLEP: the jump's aliasing smoothed off), and
// a pulse of width w made of two.
#[inline]
fn blep(p: f32, dt: f32) -> f32 {
    if p < dt {
        let x = p / dt;
        x + x - x * x - 1.0
    } else if p > 1.0 - dt {
        let x = (p - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}
#[inline]
fn saw(p: f32, dt: f32) -> f32 {
    let p = fract(p);
    2.0 * p - 1.0 - blep(p, dt)
}
#[inline]
pub fn pulse(p: f32, w: f32, dt: f32) -> f32 {
    0.5 * (saw(p, dt) - saw(p - w, dt))
}
#[inline]
fn triangle(p: f32) -> f32 {
    let p = fract(p);
    4.0 * (p - 0.5).abs() - 1.0
}

// ---------------------------------------------------------------------
// The arpeggio

// Arpeggio step k (0..3): root, third, fifth, octave, two octaves up.
#[inline]
fn arp_tone(c: i32, k: i32) -> f32 {
    root(c) + 24.0 + if k == 0 { 0.0 } else if k == 1 { third(c) } else if k == 2 { 7.0 } else { 12.0 }
}

// The arpeggio: through the bar's chord, tick by tick, its phase carried
// on from tone to tone (so the steps do not click); struck every
// sixteenth and decaying, its pulse's width sweeping 10-50% over 8 s.
#[inline]
pub fn arpeggio(rate: f32, t: f32) -> f32 {
    let b = (t / bar).floor();
    let c = chord_at(t);
    let u = t - b * bar;
    let n = ftoi((u / tick).floor());
    let k = imod(n, 4);
    let (f0, f1, f2, f3) = (freq(arp_tone(c, 0)), freq(arp_tone(c, 1)), freq(arp_tone(c, 2)), freq(arp_tone(c, 3)));
    // (the phase: whole rounds of the four tones, the steps of this round
    // done, then this step so far; each part's fract taken, to stay exact)
    let round = fract(idiv(n, 4) as f32 * fract((f0 + f1 + f2 + f3) * tick));
    let done = (if k > 0 { f0 } else { 0.0 }) + (if k > 1 { f1 } else { 0.0 }) + (if k > 2 { f2 } else { 0.0 });
    let f = if k == 0 { f0 } else if k == 1 { f1 } else if k == 2 { f2 } else { f3 };
    let p = round + fract(done * tick) + f * (u - n as f32 * tick);
    let width = 0.3 + 0.2 * (tau * t / 8.0).sin();
    let s = u - sixteenth * (u / sixteenth).floor();
    let env = (1.0 - (-s * 2000.0).exp()) * (0.35 + 0.65 * (-s * 18.0).exp());
    env * pulse(p, width, f / rate)
}

// ---------------------------------------------------------------------
// The bass

// Octave-jumping eighths on the root; on the last eighth of the bar, the
// fifth, sliding down to the next bar's root.
#[inline]
pub fn bass(rate: f32, t: f32) -> f32 {
    let e = ftoi((t / eighth).floor());
    let u = t - e as f32 * eighth;
    let c = chord_at(e as f32 * eighth);
    let last = imod(e, 8) == 7;
    let note = root(c) - 12.0 + if last { 7.0 } else if imod(e, 2) == 1 { 12.0 } else { 0.0 };
    let f = freq(note);
    // (the slide on the last eighth: down to the next root over its second
    // half, as a phase offset, the integral of the difference)
    let next = freq(root(imod(c + 1, 4)) - 12.0);
    let h = 0.5 * eighth;
    let slid = if last && u > h { let x = (u - h) / h; 0.5 * (next - f) * h * x * x } else { 0.0 };
    let p = f * u + slid;
    let env = (1.0 - (-u * 1500.0).exp()) * (0.6 + 0.4 * (-u * 10.0).exp()) * smoothstep(eighth, 0.85 * eighth, u);
    env * (0.8 * triangle(p) + 0.25 * pulse(p, 0.125, f / rate))
}

// ---------------------------------------------------------------------
// Drums

// Noise held for `hold` seconds at a time: a low-rate noise channel's
// metallic hiss.
#[inline]
fn held_noise(t: f32, hold: f32) -> f32 {
    2.0 * hash2(ftoi(t / hold), 13) - 1.0
}

// The kick: two milliseconds of noise, then a pulse sweeping from 220 to
// 45 Hz.
#[inline]
fn kick_at(rate: f32, t: f32, u: f32) -> f32 {
    let p = 45.0 * u + 175.0 / 35.0 * (1.0 - (-35.0 * u).exp());
    let f = 45.0 + 175.0 * (-35.0 * u).exp();
    let tone = pulse(p, 0.5, f / rate) * (-u * 9.0).exp();
    let click = if u < 0.002 { noise(rate, t) } else { 0.0 };
    0.8 * tone + 0.5 * click
}

// The snare: a pulse at 190 Hz for 15 ms, then noise.
#[inline]
fn snare_at(rate: f32, t: f32, u: f32) -> f32 {
    let tone = if u < 0.015 { pulse(190.0 * u, 0.5, 190.0 / rate) } else { 0.0 };
    let hiss = held_noise(t, 1.0 / 16000.0) * (-u * 14.0).exp();
    0.7 * tone + 0.6 * hiss
}

// The drums in section k's pattern (sixteenths of a bar), and the fill on
// each section's last beat: snares on every sixteenth, rising.
#[inline]
pub fn drums(rate: f32, t: f32) -> f32 {
    let k = section(t);
    let n = ftoi((t / sixteenth).floor());
    let u = t - n as f32 * sixteenth;
    let s = imod(n, 16);
    let fill = imod(n, 128) >= 124;
    let mut v = 0.0f32;
    // the kick: on the beats in section 1, on one, three and the "and" of
    // three after; once a bar in the breakdown
    let kicked = if k == 0 {
        imod(s, 4) == 0
    } else if k == 2 {
        s == 0
    } else {
        s == 0 || s == 8 || s == 10
    };
    // (rung on into the next sixteenth: the kick and the snare outlast one)
    let back = u + sixteenth;
    let kicked_before = if k == 0 {
        imod(s - 1, 4) == 0
    } else if k == 2 {
        s == 1
    } else {
        s == 1 || s == 9 || s == 11
    };
    if !fill {
        if kicked {
            v += kick_at(rate, t, u);
        } else if kicked_before {
            v += kick_at(rate, t, back);
        }
    }
    // the snare on two and four (not in section 1 or the breakdown)
    if fill {
        v += (0.5 + 0.5 * imod(n, 4) as f32 / 3.0) * snare_at(rate, t, u);
    } else if k == 1 || k == 3 {
        if s == 4 || s == 12 {
            v += snare_at(rate, t, u);
        } else if s == 5 || s == 13 {
            v += snare_at(rate, t, back);
        }
    }
    // hats on the eighths, the offbeats open (after section 1)
    if k > 0 && !fill && imod(s, 2) == 0 {
        let open = imod(s, 4) == 2;
        let decay = if open { 25.0 } else { 70.0 };
        v += (if open { 0.35 } else { 0.22 }) * (0.6 * held_noise(t, 1.0 / 22000.0) + 0.4 * bright_noise(rate, t)) * (-u * decay).exp();
    }
    v
}

// ---------------------------------------------------------------------
// The tune

// Eight bars of eighths, coded as the disco's (music.rs's melody_code):
// 0 a rest, 15 hold the note before, else a degree of A minor (1 = A4).
#[inline]
fn melody_code(i: i32) -> i32 {
    let b = imod(idiv(i, 8), 8);
    let w: u32 = if b == 0 {
        0x457f8ff5
    } else if b == 1 {
        0xf431fff3
    } else if b == 2 {
        0xf35f7ff5
    } else if b == 3 {
        0x42fffff4
    } else if b == 4 {
        0xfa9f8ff5
    } else if b == 5 {
        0xf8fafff8
    } else if b == 6 {
        0xf3ff5ff7
    } else {
        0x0fff2ff4
    };
    ((w >> ((4 * imod(i, 8)) as u32)) & 15) as i32
}

#[inline]
fn degree_midi(d: i32) -> f32 {
    let o = idiv(d - 1, 7);
    let k = imod(d - 1, 7);
    // A natural minor: A B C D E F G
    let step = if k == 0 {
        0
    } else if k == 1 {
        2
    } else if k == 2 {
        3
    } else if k == 3 {
        5
    } else if k == 4 {
        7
    } else if k == 5 {
        8
    } else {
        10
    };
    (69 + 12 * o + step) as f32
}

// The note at eighth i: (degree, its first eighth, its last); 0 if none.
#[inline]
fn note_at(i: i32) -> (i32, i32, i32) {
    let k = if melody_code(i) == 0 { i - 1 } else { i };
    if k < 0 || melody_code(k) == 0 {
        return (0, 0, 0);
    }
    let (mut j, mut go) = (k, true);
    while go && j > 0 {
        if melody_code(j) == 15 {
            j -= 1;
        } else {
            go = false;
        }
    }
    let (mut e, mut go) = (k, true);
    while go && e < 63 {
        if melody_code(e + 1) == 15 {
            e += 1;
        } else {
            go = false;
        }
    }
    (melody_code(j), j, e)
}

// The lead's tone: a quarter-width pulse, sliding from the note before
// (pf) over 30 ms, vibrato coming in after 200 ms; tau_ since the note
// began, dur how long it is held.
#[inline]
fn lead_tone(rate: f32, f: f32, pf: f32, tau_: f32, dur: f32) -> f32 {
    let env = (1.0 - (-tau_ * 800.0).exp()) * (0.7 + 0.3 * (-tau_ * 8.0).exp()) * (if tau_ < dur { 1.0 } else { (-(tau_ - dur) * 40.0).exp() });
    let depth = 0.012 * smoothstep(0.2, 0.4, tau_);
    let wob = depth / (tau * 6.0) * (1.0 - (tau * 6.0 * tau_).cos());
    let g = 0.03f32;
    let slid = (pf / f - 1.0) * (if tau_ < g { tau_ - tau_ * tau_ / (2.0 * g) } else { 0.5 * g });
    env * pulse(f * (tau_ + wob + slid), 0.25, f / rate)
}

// The tune in sections 2-4 (an octave down in the breakdown), and in 4 a
// harmony two degrees below: (main, harmony).
#[inline]
fn tune_dry(rate: f32, t: f32) -> (f32, f32) {
    let k = section(t);
    if k == 0 || t < 0.0 {
        return (0.0, 0.0);
    }
    let i = imod(ftoi((t / eighth).floor()), 64);
    let u = t - eighth * (t / eighth).floor();
    let (d, j, e) = note_at(i);
    if d == 0 {
        return (0.0, 0.0);
    }
    let tau_ = (i - j) as f32 * eighth + u;
    let dur = (e - j + 1) as f32 * eighth - 0.02;
    let (pd, _, pe) = if j > 0 { note_at(j - 1) } else { (0, 0, 0) };
    let joined = pd != 0 && pe == j - 1;
    let shift = if k == 2 { -12.0 } else { 0.0 };
    let f = freq(degree_midi(d) + shift);
    let pf = if joined { freq(degree_midi(pd) + shift) } else { f };
    let main = lead_tone(rate, f, pf, tau_, dur);
    let harmony = if k == 3 && d > 2 {
        let hf = freq(degree_midi(d - 2));
        let hpf = if joined && pd > 2 { freq(degree_midi(pd - 2)) } else { hf };
        0.6 * lead_tone(rate, hf, hpf, tau_, dur)
    } else {
        0.0
    };
    // (the section's edges: no echo of the last section's tune into it)
    let fade = smoothstep(0.0, 0.01, t - k as f32 * 8.0 * bar) * smoothstep(0.0, 0.01, (k + 1) as f32 * 8.0 * bar - beat - t);
    (fade * main, fade * harmony)
}

// The tune and a tracker's echo: the lead again three sixteenths later,
// quieter, on the other side (left, right).
#[inline]
pub fn tune(rate: f32, t: f32) -> (f32, f32) {
    let (m, h) = tune_dry(rate, t);
    let (em, eh) = tune_dry(rate, t - 3.0 * sixteenth);
    let (fm, fh) = tune_dry(rate, t - 6.0 * sixteenth);
    let dry = m + h;
    let echo = em + eh;
    let echo2 = fm + fh;
    (0.8 * dry + 0.15 * echo2, 0.6 * dry + 0.4 * echo)
}

// ---------------------------------------------------------------------

// The mix (left, right) before the limit, and what is sent to the room.
#[inline]
pub fn parts(rate: f32, t: f32) -> (f32, f32, f32) {
    let k = section(t);
    let arp = (if k == 2 { 0.5 } else { 0.37 }) * arpeggio(rate, t);
    let low = 0.4 * bass(rate, t);
    let dr = 0.8 * drums(rate, t);
    let (ml, mr) = tune(rate, t);
    let (ml, mr) = (0.7 * ml, 0.7 * mr);
    // (the arpeggio a little left, the bass and drums in the middle)
    let l = 0.6 * arp + low + dr + ml;
    let r = 0.4 * arp + low + dr + mr;
    (l, r, 0.3 * arp + 0.5 * (ml + mr))
}
