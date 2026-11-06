// Glitch Familiar: the fourth song, glitch-hop. 32 bars at 105 beats a
// minute in F minor, on Fm, Db, Ab, Eb (a bar each), the sixteenths swung.
// Its voices: a boom-bap kit (a punchy kick, a snare of noise and tone on
// two and four, swung hats, a rimshot); an FM growl bass whose index
// wobbles in eighths, over a sub; vocal chops (a formant voice going "ah"
// to "oh"); a pad, pumped by the kick; and a pulse arpeggio.
//
// A hook rides over sections 2 and 4 on a glitchy square lead, laser zaps
// in the second half, reverse cymbals swelling into each section's end,
// and in the last section a bass run closing each four bars.
//
// The glitches are edits of the song's own time, which every voice here is
// a pure function of: a stutter replays a slice of it (the last beat of
// each four bars: eighths, then sixteenths, then thirty-seconds); a tape
// stop slows it to a halt (the pitch falls with it: the last beat of each
// section); a bitcrush reads it at 6 kHz and in 16 levels (the third
// section, every other bar).
//
// In four sections of eight bars:
//   1  the kit (the kick from bar 3), the pad and the chops;
//   2  the growl bass and the arpeggio join;
//   3  the bass and the chops, bitcrushed every other bar;
//   4  everything.

use crate::chip::pulse;
use crate::music::{bright_noise, freq, noise};
use crate::prelude::*;
use crate::synth::{drive, formant, minor_degree, pump, tune_note};

pub const bpm: f32 = 105.0;
pub const beat: f32 = 60.0 / bpm;
pub const sixteenth: f32 = beat / 4.0;
pub const bar: f32 = 4.0 * beat;
pub const period: f32 = 32.0 * bar;
const tau: f32 = 2.0 * PI;
const swing: f32 = 0.18 * sixteenth; // the odd sixteenths' delay

#[inline]
fn section(t: f32) -> i32 {
    ftoi((t / (8.0 * bar)).floor())
}

// The chord at time t (0..3: Fm, Db, Ab, Eb): its root (a MIDI note in the
// second octave) and its third (minor on Fm).
#[inline]
fn root(c: i32) -> f32 {
    (if c == 0 { 41 } else if c == 1 { 37 } else if c == 2 { 44 } else { 39 }) as f32
}
#[inline]
fn third(c: i32) -> f32 {
    if c == 0 { 3.0 } else { 4.0 }
}
#[inline]
fn chord_at(t: f32) -> i32 {
    imod(ftoi((t / bar).floor()), 4)
}

// Swung sixteenths: the step sounding at t (its index, from 0) and the time
// since it began.
#[inline]
fn step_start(k: i32) -> f32 {
    k as f32 * sixteenth + if imod(k, 2) == 1 { swing } else { 0.0 }
}
#[inline]
fn step_at(t: f32) -> (i32, f32) {
    let k = ftoi((t / sixteenth).floor());
    if t < step_start(k) { (k - 1, t - step_start(k - 1)) } else { (k, t - step_start(k)) }
}

// ---------------------------------------------------------------------
// The kit

#[inline]
fn kicks(s: i32) -> bool {
    s == 0 || s == 7 || s == 10
}
#[inline]
fn snares(s: i32) -> bool {
    s == 4 || s == 12
}

// The kick: a sine swooping from 150 to 48 Hz, driven, with a click.
#[inline]
fn kick_at(rate: f32, t: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let p = tau * (48.0 * u + 102.0 / 28.0 * (1.0 - (-28.0 * u).exp()));
    drive((-u * 7.0).exp() * p.sin(), 2.5) + 0.25 * (-u * 700.0).exp() * noise(rate, t)
}

// The snare: a body at 190 Hz, falling, and noise.
#[inline]
fn snare_at(rate: f32, t: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let body = (-u * 25.0).exp() * (tau * (190.0 * u - 20.0 * u * u)).sin();
    let hiss = (-u * 13.0).exp() * (0.5 * noise(rate, t) + 0.8 * bright_noise(rate, t));
    0.6 * body + 0.7 * hiss
}

// The kit at t: the steps this one and the two before it (a hit rings on
// into the steps after), and the hats on every step, the offbeats open.
// (left, right, the time since the last kick, for the pump)
#[inline]
fn kit(rate: f32, t: f32, kick_on: bool) -> (f32, f32, f32) {
    let (k, u) = step_at(t);
    let mut v = 0.0f32;
    let mut since = 10.0f32;
    let mut j = 0;
    while j < 3 {
        let s = imod(k - j, 16);
        let back = t - step_start(k - j);
        if kick_on && kicks(s) {
            v += 0.9 * kick_at(rate, t, back);
            since = since.min(back);
        }
        if snares(s) {
            v += 0.55 * snare_at(rate, t, back);
        }
        j += 1;
    }
    // (a rimshot on the last step of the bar)
    let rim = if imod(k, 16) == 15 { 0.3 * (-u * 60.0).exp() * (tau * 1700.0 * u).sin() } else { 0.0 };
    let open = imod(k, 4) == 2;
    let accent = if open { 0.22 } else if imod(k, 2) == 1 { 0.08 } else { 0.13 };
    let hat = accent * (-u * if open { 18.0 } else { 70.0 }).exp() * bright_noise(rate, t);
    (v + rim + 0.7 * hat, v + rim + 1.0 * hat, since)
}

// ---------------------------------------------------------------------
// Tonal voices

// The growl: notes on the bar's steps 0, 3, 6, 8, 11 and 14 (held 3, 2,
// 2, 3, 2, 2 steps; the root, then its octave, fifth, root...): FM, the
// modulator on the carrier's frequency, its index wobbling in eighths;
// driven; over a sine an octave below.
#[inline]
fn growl(t: f32) -> f32 {
    let (k, _) = step_at(t);
    // (the note sounding: the latest started, up to two steps back)
    let mut start = k;
    let mut found = -1;
    let mut j = 0;
    while j < 3 && found < 0 {
        let s = imod(k - j, 16);
        if s == 0 || s == 3 || s == 6 || s == 8 || s == 11 || s == 14 {
            found = s;
            start = k - j;
        }
        j += 1;
    }
    // (in the last section, the second half of each four bars' last bar is
    // a run instead: sixteenths down from the octave, 12 10 7 5 3 0 -2 0)
    let run = section(t) == 3 && imod(idiv(k, 16), 4) == 3 && imod(k, 16) >= 8;
    if found < 0 && !run {
        return 0.0;
    }
    let (start, len, semis) = if run {
        let r = imod(k, 16) - 8;
        let semis = if r == 0 { 12.0 } else if r == 1 { 10.0 } else if r == 2 { 7.0 } else if r == 3 { 5.0 } else if r == 4 { 3.0 } else if r == 6 { -2.0 } else { 0.0 };
        (k, 1.0, semis)
    } else {
        let len = if found == 0 || found == 8 { 3.0 } else { 2.0 };
        (start, len, if found == 3 || found == 11 { 12.0 } else if found == 6 { 7.0 } else if found == 14 { 10.0 } else { 0.0 })
    };
    let t0 = step_start(start);
    let u = t - t0;
    let held = len * sixteenth - 0.015;
    let f = freq(root(chord_at(t0)) - 12.0 + semis);
    let env = (1.0 - (-u * 600.0).exp()) * smoothstep(held + 0.012, held, u);
    let wob = 0.5 - 0.5 * (tau * (2.0 / beat) * u).cos();
    let index = 0.8 + 3.2 * wob;
    let p = tau * f * u;
    let fm = (p + index * p.sin() + 0.5 * index * (2.0 * p).sin()).sin();
    env * (0.8 * drive(fm, 2.0) + 0.7 * (0.5 * p).sin())
}

// The chops: a formant voice, "ah" turning to "oh", on steps 2 and 11, the
// chord's third (or fifth on 11), two octaves above the bass.
#[inline]
fn chops(t: f32) -> f32 {
    let (k, u) = step_at(t);
    let s = imod(k, 16);
    let (on, back) = if s == 2 || s == 11 { (s, u) } else if s == 3 || s == 12 { (s - 1, u + sixteenth) } else { (-1, 0.0) };
    if on < 0 {
        return 0.0;
    }
    let c = chord_at(t);
    let f = freq(root(c) + 24.0 + if on == 2 { third(c) } else { 7.0 });
    let len = 1.6 * sixteenth;
    let env = smoothstep(0.0, 0.01, back) * smoothstep(len, len - 0.03, back);
    let m = saturate(back / len);
    formant(f, back, 800.0 - 350.0 * m, 1200.0 - 400.0 * m, 24) * env
}

// The pad: the chord an octave above the bass, soft saws a little apart.
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
        l += crate::synth::saw_lp(f * 0.996, u, 1400.0, 0.0, 10);
        r += crate::synth::saw_lp(f * 1.004, u, 1400.0, 0.0, 10);
        j += 1;
    }
    let env = smoothstep(0.0, 0.08, u) * smoothstep(bar + 0.05, bar - 0.06, u);
    (env * l, env * r)
}

// The arpeggio: swung sixteenths of a quarter-width pulse up through the
// chord, two octaves above the bass.
#[inline]
fn arp(rate: f32, t: f32) -> f32 {
    let (k, u) = step_at(t);
    let c = chord_at(t);
    let j = imod(k, 4);
    let semis = if j == 0 { 0.0 } else if j == 1 { third(c) } else if j == 2 { 7.0 } else { 12.0 };
    let f = freq(root(c) + 36.0 + semis);
    (-u * 14.0).exp() * smoothstep(0.0, 0.002, u) * pulse(f * u, 0.25, f / rate)
}

// The hook: eight bars of eighths, each a degree of F minor (1 = F4), -
// holding the note before, . a rest; played in sections 2 and 4.
wizard_shader_macros::melody!(
    hook_code,
    8,
    "8 - 6 . 5 4 5 . | 3 - 4 . 5 - . . | 6 - 5 . 3 1 3 . | 2 - 3 . 4 - - . |
     8 - 9 . 10 9 8 . | 6 - 5 . 4 - . . | 3 - 4 5 6 5 3 . | 2 - - 1 - - - ."
);

// The lead playing it: a square wave, its width wobbling, sliding into
// each note joined to the one before (over 40 ms), a little vibrato once
// it is held, bitcrushed a touch (16 levels): glitchy, chippy.
#[inline]
fn lead(rate: f32, t: f32) -> f32 {
    let i = imod(ftoi((t / (0.5 * beat)).floor()), 64);
    let u = t - 0.5 * beat * (t / (0.5 * beat)).floor();
    let (d, j, e) = tune_note(hook_code, i, 64);
    if d == 0 {
        return 0.0;
    }
    let tau_ = (i - j) as f32 * 0.5 * beat + u;
    let dur = (e - j + 1) as f32 * 0.5 * beat - 0.03;
    let (pd, _, pe) = if j > 0 { tune_note(hook_code, j - 1, 64) } else { (0, 0, 0) };
    let joined = pd != 0 && pe == j - 1;
    let f = freq(minor_degree(65.0, d));
    let pf = if joined { freq(minor_degree(65.0, pd)) } else { f };
    let width = 0.5 + 0.15 * (tau * 0.7 * t).sin();
    let env = smoothstep(0.0, 0.006, tau_) * (0.75 + 0.25 * (-8.0 * tau_).exp()) * if tau_ < dur { 1.0 } else { (-(tau_ - dur) * 30.0).exp() };
    let v = env * pulse(f * crate::synth::sung_time(f, pf, tau_, 0.04, 0.004, 6.0, 0.2, 0.4), width, f / rate);
    (v * 8.0).round() / 8.0
}

// A laser zap: a sine diving from 2.4 kHz to 120 Hz over 90 ms, on the
// bar's last step every other bar.
#[inline]
fn zap(t: f32) -> f32 {
    let (k, u) = step_at(t);
    if imod(k, 32) != 31 {
        return 0.0;
    }
    let ph = tau * (120.0 * u + 2280.0 / 33.0 * (1.0 - (-33.0 * u).exp()));
    (-u * 18.0).exp() * smoothstep(0.0, 0.002, u) * ph.sin()
}

// A reverse cymbal: bright noise swelling over the two beats before each
// section's last (which the tape stop takes).
#[inline]
fn swell(rate: f32, t: f32) -> f32 {
    let into = t - (t / (8.0 * bar)).floor() * 8.0 * bar - (7.0 * bar + beat);
    if !(into >= 0.0 && into < 2.0 * beat) {
        return 0.0;
    }
    let x = into / (2.0 * beat);
    x * x * x * bright_noise(rate, t)
}

// ---------------------------------------------------------------------

// The song at time t, before the edits: (left, right, the room's send).
#[inline]
fn dry(rate: f32, t: f32) -> (f32, f32, f32) {
    let k = section(t);
    let bar_n = imod(ftoi((t / bar).floor()), 8);
    let (dl, dr, since) = kit(rate, t, k > 0 || bar_n >= 2);
    let duck = pump(since, 0.55, 0.25);
    let g = if k == 1 || k >= 2 { growl(t) } else { 0.0 };
    let ch = chops(t);
    let (pl, pr) = pad(t);
    let pg = if k == 2 { 0.0 } else { 0.035 * duck };
    let a = if k == 1 || k == 3 { 0.05 * arp(rate, t) } else { 0.0 };
    let low = 0.32 * duck * g;
    // the hook (sections 2 and 4), the zaps (3 and 4), the swells into
    // each section's end
    let hook = if k == 1 || k == 3 { 0.9 * lead(rate, t) } else { 0.0 };
    let z = if k >= 2 { 0.3 * zap(t) } else { 0.0 };
    let sw = 0.25 * swell(rate, t);
    let l = 0.6 * dl + low + 0.11 * ch + pg * pl + 0.6 * a + 0.55 * hook + 0.7 * z + sw;
    let r = 0.6 * dr + low + 0.11 * ch + pg * pr + 0.4 * a + 0.45 * hook + 0.3 * z + sw;
    (l, r, 0.25 * (dl + dr) + 0.5 * ch + 0.5 * pg * (pl + pr) + 0.5 * a + 0.6 * hook + 0.5 * z + 0.5 * sw)
}

// The edits: the time to read the song at, and whether it is bitcrushed.
#[inline]
fn edit(t: f32) -> (f32, bool) {
    let b = ftoi((t / bar).floor());
    let in_bar = t - b as f32 * bar;
    let last_beat = in_bar >= 3.0 * beat;
    let bs = b as f32 * bar + 3.0 * beat;
    let u = t - bs;
    // the tape stop: the last beat of each section, time slowing to a halt
    if imod(b, 8) == 7 && last_beat {
        return (bs + u - u * u / (2.0 * beat), false);
    }
    // the stutter: the last beat of each four bars repeats its start, in
    // eighths, then sixteenths, then thirty-seconds
    if imod(b, 4) == 3 && last_beat {
        let len = if u < 0.5 * beat { 0.5 * beat * 0.5 } else if u < 0.75 * beat { sixteenth * 0.5 } else { sixteenth * 0.25 };
        let from = if u < 0.5 * beat { 0.0 } else if u < 0.75 * beat { 0.5 * beat } else { 0.75 * beat };
        let w = u - from;
        return (bs + w - len * (w / len).floor(), false);
    }
    (t, section(t) == 2 && imod(b, 2) == 1)
}

// The mix (left, right) before the limit, and the room's send.
#[inline]
pub fn parts(rate: f32, t: f32) -> (f32, f32, f32) {
    let (te, crush) = edit(t);
    if crush {
        // (read at 6 kHz, and in 16 levels)
        let held = (te * 6000.0).floor() / 6000.0;
        let (l, r, s) = dry(rate, held);
        let q = |x: f32| (x * 8.0).round() / 8.0;
        (q(l), q(r), s)
    } else {
        dry(rate, te)
    }
}

#[cfg(all(test, not(target_arch = "spirv")))]
mod tests {
    #[test]
    fn the_hook_sounds() {
        let rate = 48000.0;
        let t0 = 8.0 * super::bar;
        let (mut peak, mut zaps) = (0f32, 0f32);
        for i in 0..(8.0 * super::bar * rate) as usize {
            let t = t0 + i as f32 / rate;
            peak = peak.max(super::lead(rate, t).abs());
            zaps = zaps.max(super::zap(t + 8.0 * super::bar).abs());
        }
        eprintln!("hook peak {peak}, zap peak {zaps}");
        assert!(peak > 0.3 && zaps > 0.3);
    }
}
