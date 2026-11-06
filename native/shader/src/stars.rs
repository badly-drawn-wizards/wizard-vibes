// Starscape: the third song, in the manner of RuneScape's (RS2-era) music,
// the General MIDI orchestra of its soundtrack: a slow waltz under the
// night sky, 32 bars of 3/4 at 84 beats a minute in D major, lifted to the
// Lydian by an E major over D: Dmaj7, E/D, Bm7, G, Em7, A, Gmaj7, Asus4 (a
// bar each). Its voices:
//   - a celesta (a struck bar: its fundamental, a little of the double
//     octave, a glint of inharmonic overtones) in rising and falling
//     arpeggios of eighths, each note ringing on under the next;
//   - a harp in a waltz: the bass on one, the chord on two and three;
//   - a string pad, swelling in and crossfading chord to chord;
//   - a choir singing "aah" (harmonics weighted by the vowel's formants);
//   - a flute for the tune: near a sine, breathy, with a chiff on each
//     note, delayed vibrato and legato slides;
//   - a glockenspiel doubling the celesta for the tune in section 3;
//   - timpani (struck membranes' inharmonic modes) and a tambourine.
// In four sections of eight bars:
//   1  the celesta and the strings;
//   2  the harp's waltz, the flute's tune;
//   3  the tune on the glockenspiel and celesta, the choir, the flute
//      below in long notes, the tambourine; a timpani roll into...
//   4  everything: the flute's tune, a third above it on the glockenspiel.
//
// As the other songs, every sample is a pure function of time within the
// loop; the room (room.rs) gets much of it: the stars want a big hall.

use crate::music::{bright_noise, freq, noise};
use crate::prelude::*;

pub const bpm: f32 = 84.0;
pub const beat: f32 = 60.0 / bpm;
pub const eighth: f32 = beat / 2.0;
pub const bar: f32 = 3.0 * beat;
pub const period: f32 = 32.0 * bar;
const tau: f32 = 2.0 * PI;

#[inline]
fn section(t: f32) -> i32 {
    ftoi((t / (8.0 * bar)).floor())
}

// A gain fading in over `fade` seconds from section k's start, and out
// over the last 0.3 s of section k2.
#[inline]
fn in_sections(k: i32, k2: i32, fade: f32, t: f32) -> f32 {
    let a = k as f32 * 8.0 * bar;
    let b = (k2 + 1) as f32 * 8.0 * bar;
    smoothstep(a, a + fade, t) * smoothstep(b, b - 0.3, t)
}

// Chord c (0..7): its root (a MIDI note, second-third octave), and tone k
// (0..3) of it, in semitones over the root.
#[inline]
fn root(c: i32) -> f32 {
    (if c == 0 {
        50 // D
    } else if c == 1 || c == 4 {
        52 // E
    } else if c == 2 {
        47 // B
    } else if c == 3 || c == 6 {
        43 // G
    } else {
        45 // A
    }) as f32
}
#[inline]
fn tone(c: i32, k: i32) -> f32 {
    if k == 0 {
        return 0.0;
    }
    // (Dmaj7, E, Bm7, G, Em7, A, Gmaj7, Asus4)
    let minor = c == 2 || c == 4;
    if k == 1 {
        return if c == 7 { 5.0 } else if minor { 3.0 } else { 4.0 };
    }
    if k == 2 {
        return 7.0;
    }
    if c == 0 || c == 6 {
        11.0
    } else if minor {
        10.0
    } else {
        12.0
    }
}
#[inline]
fn chord_at(t: f32) -> i32 {
    imod(ftoi((t / bar).floor()), 8)
}
// The bass under chord c (E over D: D).
#[inline]
fn bass_note(c: i32) -> f32 {
    if c == 1 { 50.0 } else { root(c) }
}

// ---------------------------------------------------------------------
// Struck and plucked voices

// A celesta's note, struck at u = 0: the fundamental ringing some 2 s, the
// double octave and two inharmonic overtones (a bar's, at 2.76 and 5.4
// times) dying fast.
#[inline]
fn celesta_note(f: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let p = tau * f * u;
    let attack = 1.0 - (-u * 1200.0).exp();
    let glint = if 5.4 * f > 15000.0 { 0.0 } else { 0.12 * (-u * 14.0).exp() * (5.4 * p).sin() };
    attack * ((-u * 1.8).exp() * p.sin() + 0.25 * (-u * 5.0).exp() * (4.0 * p).sin() + 0.15 * (-u * 8.0).exp() * (2.76 * p).sin() + glint)
}

// A glockenspiel's: brighter and longer, its overtones (2.76, 5.4) strong.
#[inline]
fn glock_note(f: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let p = tau * f * u;
    let attack = 1.0 - (-u * 3000.0).exp();
    let high = if 5.4 * f > 15000.0 { 0.0 } else { 0.2 * (-u * 9.0).exp() * (5.4 * p).sin() };
    attack * ((-u * 1.2).exp() * p.sin() + 0.4 * (-u * 4.0).exp() * (2.76 * p).sin() + high)
}

// A harp string, plucked near a third of its length: eight partials, the
// higher dying sooner, ringing some seconds.
#[inline]
fn harp_note(f: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let p = tau * f * u;
    let mut s = 0.0f32;
    let mut k = 0;
    while k < 8 {
        let kf = (k + 1) as f32;
        if !(kf * f > 12000.0) {
            s = s + (PI * kf * 0.3).sin() / (kf * kf.sqrt()) * (-u * (0.7 + 0.5 * kf)).exp() * (kf * p).sin();
        }
        k += 1;
    }
    (1.0 - (-u * 800.0).exp()) * s
}

// The celesta's arpeggio: eighths through the bar's chord an octave and a
// half up, rising then falling (tones 0 1 2 3 2 1), each ringing under the
// next three.
#[inline]
pub fn celesta(t: f32) -> f32 {
    let n = ftoi((t / eighth).floor());
    let u = t - n as f32 * eighth;
    let mut s = 0.0f32;
    let mut k = 0;
    while k < 4 {
        let m = n - k;
        if m >= 0 {
            let c = chord_at(m as f32 * eighth);
            let step = imod(m, 6);
            let j = if step < 4 { step } else { 6 - step };
            s = s + celesta_note(freq(root(c) + 24.0 + tone(c, j)), u + k as f32 * eighth);
        }
        k += 1;
    }
    s
}

// The harp's waltz: the bass on one (an octave down), ringing through the
// bar; the chord's three upper tones on two and three, rolled (each a
// little after the one below).
#[inline]
pub fn harp(t: f32) -> f32 {
    let b = (t / bar).floor();
    let ub = t - b * bar;
    let c = chord_at(t);
    let mut s = 0.8 * harp_note(freq(bass_note(c) - 12.0), ub);
    // (the last bar's chord, ringing over the barline)
    if b > 0.0 {
        let pc = imod(c + 7, 8);
        let mut j = 1;
        while j < 4 {
            s = s + 0.5 * harp_note(freq(root(pc) + tone(pc, j)), ub + beat - 0.02 * j as f32);
            j += 1;
        }
    }
    let beats = ftoi((ub / beat).floor());
    if beats >= 1 {
        let u = ub - beats as f32 * beat;
        let mut j = 1;
        while j < 4 {
            let tone_u = u - 0.02 * j as f32;
            // (struck on this beat, and on two still ringing on three)
            s = s + 0.5 * harp_note(freq(root(c) + tone(c, j)), tone_u);
            if beats == 2 {
                s = s + 0.5 * harp_note(freq(root(c) + tone(c, j)), tone_u + beat);
            }
            j += 1;
        }
    }
    s
}

// ---------------------------------------------------------------------
// Sustained voices

// One chord of the strings: its four tones an octave up, each two soft
// sawtooths a little apart in tune, with a slow vibrato; (left, right).
#[inline]
fn strings_chord(c: i32, tt: f32, amp: f32) -> (f32, f32) {
    let vib = 0.003 * smoothstep(0.5, 1.5, tt) / (tau * 4.5) * (1.0 - (tau * 4.5 * tt).cos());
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut k = 0;
    while k < 4 {
        let f = freq(root(c) + 12.0 + tone(c, k));
        let saw = |g: f32| {
            let mut s = 0.0f32;
            let mut h = 0;
            while h < 8 {
                let hf = (h + 1) as f32;
                let soft = 1.0 / (1.0 + (hf * g / 1500.0) * (hf * g / 1500.0));
                s = s + soft * (tau * g * hf * (tt + vib)).sin() / hf;
                h += 1;
            }
            s
        };
        l = l + amp * saw(f * 0.9965);
        r = r + amp * saw(f * 1.0035);
        k += 1;
    }
    (l, r)
}

// The strings: each bar's chord swelling in over a second, the last bar's
// fading under it.
#[inline]
pub fn strings(t: f32) -> (f32, f32) {
    let b = ftoi((t / bar).floor());
    let lt = t - b as f32 * bar;
    let c = imod(b, 8);
    let (l1, r1) = strings_chord(c, lt, smoothstep(0.0, 1.0, lt));
    let fade = 1.0 - smoothstep(0.0, 1.0, lt);
    let (l0, r0) = if b > 0 && fade > 0.0 { strings_chord(imod(c + 7, 8), lt + bar, fade) } else { (0.0, 0.0) };
    (l1 + l0, r1 + r0)
}

// The choir's "aah": harmonic k of a voice weighted by the vowel's three
// formants (some 700, 1150 and 2600 Hz).
#[inline]
fn aah(f: f32, tt: f32) -> f32 {
    let vib = 0.004 * smoothstep(0.3, 1.0, tt) / (tau * 5.2) * (1.0 - (tau * 5.2 * tt).cos());
    let mut s = 0.0f32;
    let mut k = 0;
    while k < 16 {
        let kf = (k + 1) as f32;
        let fk = kf * f;
        if !(fk > 4000.0) {
            let a = (fk - 700.0) / 260.0;
            let b = (fk - 1150.0) / 300.0;
            let c = (fk - 2600.0) / 400.0;
            let w = (-a * a).exp() + 0.6 * (-b * b).exp() + 0.25 * (-c * c).exp() + 0.08 / kf;
            s = s + w * (tau * fk * (tt + vib)).sin();
        }
        k += 1;
    }
    s
}

// The choir: the bar's chord's three upper tones in the octave of the
// strings, each voice two singers a little apart, swelling in.
#[inline]
pub fn choir(t: f32) -> (f32, f32) {
    let b = ftoi((t / bar).floor());
    let lt = t - b as f32 * bar;
    let c = imod(b, 8);
    let amp = smoothstep(0.0, 0.8, lt) * smoothstep(bar + 0.1, bar - 0.25, lt);
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut k = 1;
    while k < 4 {
        let f = freq(root(c) + 12.0 + tone(c, k));
        l = l + aah(f * 0.997, lt);
        r = r + aah(f * 1.003, lt);
        k += 1;
    }
    (amp * l, amp * r)
}

// ---------------------------------------------------------------------
// The tune

// Eight bars of six eighths, each a degree of D major (1 = D5 .. 10 =
// F#6; 13 = G#5, the Lydian's sharp fourth, over E), - holding the note
// before, . a rest; and the line under it in section 3 (played an octave
// down, long notes).
wizard_shader_macros::melody!(
    tune_code,
    6,
    "5 - - 3 - 2 | 3 - - 13 - 6 | 5 - - - 3 2 | 1 - - - - . |
     2 - 3 - 4 - | 5 - - 6 - 7 | 8 - - 6 - 5 | 5 - - - - ."
);
wizard_shader_macros::melody!(
    counter_code,
    6,
    "3 - - - - - | 2 - - - - - | 1 - - - - - | 2 - - - - - |
     4 - - 3 - - | 2 - - - - - | 3 - - 2 - - | 1 - - - - -"
);
#[inline]
fn melody_code(i: i32, counter: bool) -> i32 {
    if counter { counter_code(i) } else { tune_code(i) }
}

// Degree d of D major (1 = D5), or 13: G#5 (the Lydian's sharp fourth).
#[inline]
fn degree_midi(d: i32) -> f32 {
    if d == 13 { 80.0 } else { crate::synth::scale_degree(74.0, d, true) }
}

// The note at eighth i (of 48): (degree, its first eighth, its last); 0 if
// none.
#[inline]
fn note_at(i: i32, counter: bool) -> (i32, i32, i32) {
    crate::synth::tune_note(move |k| melody_code(k, counter), i, 48)
}

// The flute: near a sine (its octave 12 dB down, its twelfth 22), breath
// noise riding on it, a chiff of noise as each note speaks, vibrato after
// a quarter second, sliding from the note before (pf) over 40 ms.
#[inline]
fn flute_tone(rate: f32, t: f32, f: f32, pf: f32, tau_: f32, dur: f32, joined: bool) -> f32 {
    let attack = if joined { 1.0 } else { smoothstep(0.0, 0.05, tau_) };
    let env = attack * (if tau_ < dur { 1.0 } else { (-(tau_ - dur) * 18.0).exp() });
    let p = tau * f * crate::synth::sung_time(f, pf, tau_, 0.04, 0.006, 5.0, 0.25, 0.6);
    let tone_ = p.sin() + 0.25 * (2.0 * p).sin() + 0.08 * (3.0 * p).sin();
    let breath = 0.06 * bright_noise(rate, t) * (0.6 + 0.4 * p.sin());
    let chiff = if joined { 0.0 } else { 0.35 * (-tau_ * 40.0).exp() * bright_noise(rate, t) };
    env * (tone_ + breath) + attack * chiff * (if tau_ < dur { 1.0 } else { 0.0 })
}

// The line (tune or counter) at time t, shifted `shift` semitones: the
// note sounding and where it is in it, as flute_tone wants it.
#[inline]
fn line(rate: f32, t: f32, counter: bool, shift: f32) -> f32 {
    let i = imod(ftoi((t / eighth).floor()), 48);
    let u = t - eighth * (t / eighth).floor();
    let (d, j, e) = note_at(i, counter);
    if d == 0 {
        return 0.0;
    }
    let tau_ = (i - j) as f32 * eighth + u;
    let dur = (e - j + 1) as f32 * eighth - 0.03;
    let (pd, _, pe) = if j > 0 { note_at(j - 1, counter) } else { (0, 0, 0) };
    let joined = pd != 0 && pe == j - 1;
    let f = freq(degree_midi(d) + shift);
    let pf = if joined { freq(degree_midi(pd) + shift) } else { f };
    flute_tone(rate, t, f, pf, tau_, dur, joined)
}

// The tune struck on bars (the glockenspiel, or the celesta), a note a
// time, each ringing on to the next's start and a little after; `up`
// scale degrees above it (2: a third; G#'s third is B), `shift` semitones.
#[inline]
fn struck_line(t: f32, glock: bool, up: i32, shift: f32) -> f32 {
    let midi = |d: i32| degree_midi(if up == 0 { d } else if d == 13 { 6 } else { d + up }) + shift;
    let i = imod(ftoi((t / eighth).floor()), 48);
    let u = t - eighth * (t / eighth).floor();
    let mut s = 0.0f32;
    // (this note, and the one before it, ringing on)
    let (d, j, _) = note_at(i, false);
    let first = if d != 0 { j } else { i };
    if d != 0 {
        let tau_ = (i - j) as f32 * eighth + u;
        let f = freq(midi(d));
        s = s + if glock { glock_note(f, tau_) } else { celesta_note(f, tau_) };
    }
    if first > 0 {
        let (pd, pj, _) = note_at(first - 1, false);
        if pd != 0 {
            let tau_ = (i - pj) as f32 * eighth + u;
            let f = freq(midi(pd));
            s = s + if glock { glock_note(f, tau_) } else { celesta_note(f, tau_) };
        }
    }
    s
}

// ---------------------------------------------------------------------
// Percussion

// A timpani's stroke on D: a struck membrane's modes (1, 1.5, 1.98, 2.44
// times), the pitch settling as the head relaxes, a thud of noise.
#[inline]
fn timpani_at(rate: f32, t: f32, u: f32) -> f32 {
    if u < 0.0 {
        return 0.0;
    }
    let f = 73.4;
    let p = tau * (f * u + 0.03 * f / 8.0 * (1.0 - (-8.0 * u).exp()));
    let body = (-u * 2.2).exp() * p.sin() + 0.5 * (-u * 3.0).exp() * (1.5 * p).sin() + 0.3 * (-u * 4.0).exp() * (1.98 * p).sin() + 0.15 * (-u * 6.0).exp() * (2.44 * p).sin();
    (1.0 - (-u * 600.0).exp()) * body + 0.25 * (-u * 60.0).exp() * noise(rate, t)
}

// The timpani: a soft stroke on the first beat of every fourth bar from
// section 2 on; and the last bar of section 3, a roll (strokes on
// sixteenths, swelling).
#[inline]
pub fn timpani(rate: f32, t: f32) -> f32 {
    let k = section(t);
    let b = ftoi((t / bar).floor());
    let ub = t - b as f32 * bar;
    if k == 2 && imod(b, 8) == 7 {
        let s = beat / 4.0;
        let n = ftoi((ub / s).floor());
        let u = ub - n as f32 * s;
        let swell = 0.25 + 0.75 * (n as f32 / 11.0);
        return swell * 0.6 * (timpani_at(rate, t, u) + 0.7 * timpani_at(rate, t, u + s));
    }
    // (struck at bar 4k, or ringing on into the bar after; and the roll's
    // last stroke ringing on into section 4)
    let mut s = 0.0f32;
    if k >= 1 && imod(b, 4) == 0 {
        s = s + timpani_at(rate, t, ub);
    }
    if k >= 1 && imod(b, 4) == 1 {
        s = s + timpani_at(rate, t, ub + bar);
    }
    if k == 3 && b == 24 {
        s = s + 0.8 * timpani_at(rate, t, ub + beat / 4.0);
    }
    s
}

// The tambourine: jingles (bright noise, quick) on the off-beat eighths of
// two and three.
#[inline]
pub fn tambourine(rate: f32, t: f32) -> f32 {
    let n = ftoi((t / eighth).floor());
    let u = t - n as f32 * eighth;
    let step = imod(n, 6);
    if step == 3 || step == 5 {
        (-u * 30.0).exp() * smoothstep(0.0, 0.003, u) * bright_noise(rate, t)
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------

// The mix (left, right) before the limit, and what is sent to the room.
#[inline]
pub fn parts(rate: f32, t: f32) -> (f32, f32, f32) {
    let k = section(t);
    // the celesta's arpeggio throughout, softer under the struck tune
    let cel = (if k == 2 { 0.09 } else { 0.13 }) * celesta(t);
    let (sl, sr) = strings(t);
    let st = 0.08 * (in_sections(0, 1, 2.0, t) + in_sections(3, 3, 1.0, t)) + 0.05 * in_sections(2, 2, 0.5, t);
    let (sl, sr) = (st * sl, st * sr);
    let hp = 0.2 * (in_sections(1, 3, 0.05, t)) * harp(t);
    // the flute: the tune in sections 2 and 4, the counter-line in 3
    let fl = 0.18 * (in_sections(1, 1, 0.05, t) + in_sections(3, 3, 0.05, t)) * line(rate, t, false, 0.0)
        + 0.14 * in_sections(2, 2, 0.05, t) * line(rate, t, true, -12.0);
    // the struck tune: glockenspiel and celesta in section 3; in 4 the
    // glockenspiel a third above
    let bell = in_sections(2, 2, 0.05, t) * (0.2 * struck_line(t, true, 0, 0.0) + 0.14 * struck_line(t, false, 0, -12.0))
        + 0.1 * in_sections(3, 3, 0.05, t) * struck_line(t, true, 2, 0.0);
    let (cl, cr) = choir(t);
    let ch = 0.06 * (in_sections(2, 2, 1.5, t) + in_sections(3, 3, 1.5, t));
    let (cl, cr) = (ch * cl, ch * cr);
    let tim = 0.25 * timpani(rate, t);
    let tam = 0.06 * (in_sections(2, 3, 0.05, t)) * tambourine(rate, t);
    // (the celesta a little right, the harp left, the flute centre)
    let l = 0.4 * cel + sl + 0.6 * hp + fl + 0.55 * bell + cl + tim + 0.4 * tam;
    let r = 0.6 * cel + sr + 0.4 * hp + fl + 0.45 * bell + cr + tim + 0.6 * tam;
    let send = 0.8 * cel + 0.5 * (sl + sr) + 0.5 * hp + 0.9 * fl + 0.8 * bell + 0.6 * (cl + cr) + 0.4 * tim;
    (l, r, send)
}
