// fut/music.fut: the music, as Shadertoy does sound: each stereo sample is
// a pure function of time, and the app asks for a chunk of samples at a
// time (the `music` entry point in lib.rs, as sound_chunk).
//
// Wizard disco: 32 bars at 120 beats a minute in D minor, on the chords
// Dm, Gm, Bb, A (i iv VI V, two bars each). Throughout: a four-on-the-floor
// kick, open hi-hats on the offbeats and closed ones on the sixteenths,
// claps on two and four, a crunchy octave-jumping bass and a harpsichord
// playing sixteenth arpeggios. In four sections of eight bars, each after a
// one-beat stop:
//   1  the groove;
//   2  with a string pad;
//   3  a synth lead plays the tune;
//   4  the tune again, with a harmony a third below, and the strings.
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

// The bass: the chord's root in eighths, low then an octave up; a
// band-limited sawtooth through a closing filter and a soft clipper, over a
// sine an octave below.
#[inline]
pub fn bass(t: f32) -> f32 {
    let e = ftoi((t / eighth).floor());
    let u = t - e as f32 * eighth;
    let root = chord_tone(chord_at(e as f32 * eighth), 0);
    let f = freq(root - 12.0 + (if imod(e, 2) == 1 { 12.0 } else { 0.0 }));
    let env = (1.0 - (-u * 400.0).exp()) * (-u * 4.0).exp() * smoothstep(0.92 * eighth, 0.8 * eighth, u);
    // (the filter: harmonic k damped by how far it is above the cutoff,
    // which falls from 30 to 6 times the note's frequency)
    let cutoff = 6.0 + 24.0 * (-u * 18.0).exp();
    let mut saw = 0.0f32;
    let mut k = 0;
    while k < 24 {
        let kf = (k + 1) as f32;
        let keep = if kf * f > 8000.0 { 0.0 } else { 1.0 / (1.0 + (kf / cutoff).powf(4.0)) };
        saw = saw + keep * (tau * f * kf * u).sin() / kf;
        k += 1;
    }
    let drive = 3.5f32;
    let crunch = (drive * saw).tanh() / drive.tanh();
    let sub = (tau * f * 0.5 * u).sin();
    env * (0.13 * crunch + 0.12 * sub)
}

// The kick on every beat: a sine falling from 150 to 50 Hz, with a click.
#[inline]
pub fn kick(t: f32) -> f32 {
    let u = t - beat * (t / beat).floor();
    let p = tau * (50.0 * u + 100.0 / 25.0 * (1.0 - (-25.0 * u).exp()));
    0.42 * (-u * 7.0).exp() * p.sin() + 0.05 * (-u * 300.0).exp()
}

// Claps on beats two and four: noise in three quick bursts, then a tail.
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
        0.55 * (-(u - 0.025) * 14.0).exp()
    };
    0.11 * flam * noise(rate, t)
}

// Hi-hats: open on the offbeat eighths, closed on the other sixteenths.
#[inline]
pub fn hats(rate: f32, t: f32) -> f32 {
    let n = ftoi((t / sixteenth).floor());
    let u = t - n as f32 * sixteenth;
    let env = if imod(n, 4) == 2 {
        0.09 * (-u * 9.0).exp()
    } else if imod(n, 2) == 1 {
        0.035 * (-u * 70.0).exp()
    } else {
        0.0
    };
    env * bright_noise(rate, t)
}

// ---------------------------------------------------------------------
// Strings

#[inline]
fn voice(ch: i32, tt: f32, amp: f32) -> (f32, f32) {
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut k = 0;
    while k < 3 {
        let f = freq(chord_tone(imod(ch, chords), k) + 24.0);
        let tone = |g: f32| (tau * g * tt).sin() + 0.45 * (2.0 * tau * g * tt).sin() + 0.2 * (3.0 * tau * g * tt).sin();
        let nl = l + amp * tone(f * 1.004);
        let nr = r + amp * tone(f * 0.996);
        l = nl;
        r = nr;
        k += 1;
    }
    (l, r)
}

// A string pad on the chord, two octaves up: each tone two detuned voices
// (one per side), a few harmonics, swelling in; the previous chord fades
// under the next.
#[inline]
pub fn strings(t: f32) -> (f32, f32) {
    let c = ftoi((t / chord_len).floor());
    let lt = t - c as f32 * chord_len;
    let (l1, r1) = voice(c, lt, 0.035 * smoothstep(0.0, 0.5, lt));
    let fade = (0.0f32).maximum(1.0 - lt / 0.4);
    let (l0, r0) = if fade > 0.0 && c > 0 { voice(c - 1, lt + chord_len, 0.035 * fade) } else { (0.0, 0.0) };
    // (in sections 2 and 4)
    let g = in_sections(1, 1, 0.6, t) + in_sections(3, 3, 0.6, t);
    (g * (l1 + l0), g * (r1 + r0))
}

// ---------------------------------------------------------------------
// The tune

// The melody: eight bars of eighth notes, each a code: 0 a rest, 15 hold
// the note before, otherwise a degree of the scale (1 = D4 .. 12 = A5;
// 13 = C#5). Each bar's eight codes are packed as hex digits (the first
// eighth in the lowest).
#[inline]
pub fn melody_code(i: i32) -> i32 {
    let b = imod(idiv(i, 8), 8);
    let w: u32 = if b == 0 {
        0x789af8f5
    } else if b == 1 {
        0xa980fff5
    } else if b == 2 {
        0xf6f89afb
    } else if b == 3 {
        0x6540fff8
    } else if b == 4 {
        0xf89abafa
    } else if b == 5 {
        0x00ff87f8
    } else if b == 6 {
        0x9df5fdf9
    } else {
        0xd9abfffc
    };
    ((w >> ((4 * imod(i, 8)) as u32)) & 15) as i32
}

#[inline]
pub fn degree_midi(d: i32) -> f32 {
    (if d == 13 {
        73
    } else {
        let o = idiv(d - 1, 7);
        let k = imod(d - 1, 7);
        // D natural minor: D E F G A Bb C
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
        62 + 12 * o + step
    }) as f32
}

// The note sounding at eighth i (or the one it is releasing from, after
// it): (degree, its first eighth, its last eighth); degree 0 if none.
#[inline]
pub fn note_at(i: i32) -> (i32, i32, i32) {
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

// A synth lead: a square-ish wave (odd harmonics, 1/k), two voices a hair
// apart, with vibrato growing in on long notes. tau_: time since the note
// began; dur: how long it is held.
#[inline]
pub fn lead_tone(f: f32, tau_: f32, dur: f32) -> f32 {
    let env = (1.0 - (-tau_ * 120.0).exp())
        * (0.75 + 0.25 * (-tau_ * 6.0).exp())
        * (if tau_ < dur { 1.0 } else { (-(tau_ - dur) * 30.0).exp() });
    let depth = 0.005 * smoothstep(0.15, 0.45, tau_);
    let wob = depth / (2.0 * PI * 5.5) * (1.0 - (2.0 * PI * 5.5 * tau_).cos());
    let osc = |g: f32| {
        let mut s = 0.0f32;
        let mut k = 0;
        while k < 6 {
            let kf = (2 * k + 1) as f32;
            if !(kf * g > 9000.0) {
                s = s + (tau * g * kf * (tau_ + wob)).sin() / kf;
            }
            k += 1;
        }
        s
    };
    env * (osc(f) + 0.6 * osc(f * 1.003))
}

// The tune in sections 3 and 4, an octave up; in section 4 also a third
// below (two scale degrees down).
#[inline]
pub fn tune(t: f32) -> (f32, f32) {
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
    let main = 0.07 * lead_tone(freq(degree_midi(d) + 12.0), tau_, dur);
    let harmony_degree = if d == 13 { 5 } else { d - 2 };
    let second = if section(t) == 3 && harmony_degree >= 1 { 0.045 * lead_tone(freq(degree_midi(harmony_degree) + 12.0), tau_, dur) } else { 0.0 };
    let g = in_sections(2, 3, 0.05, t);
    (g * (main + 0.7 * second), g * (0.8 * main + second))
}

// ---------------------------------------------------------------------

// The mix at time t (within the loop), softly limited.
#[inline(never)]
pub fn music_at(rate: f32, t: f32) -> (f32, f32) {
    let (l, r, _) = music_parts(rate, t);
    (soft(l), soft(r))
}

// The soft limit on the mix.
#[inline]
pub fn soft(x: f32) -> f32 {
    (1.1 * x).tanh() / 1.1
}

// The mix at time t before the limit (left, right), and the harpsichord in
// it (mono), which drives the soundboard (see soundboard.rs).
#[inline]
pub fn music_parts(rate: f32, t: f32) -> (f32, f32, f32) {
    let groove = if stopped(t) { 0.0 } else { 1.0 };
    let drums = groove * (kick(t) + clap(rate, t));
    let hh = groove * hats(rate, t);
    let low = groove * bass(t);
    // (the harpsichord steps back while the tune plays)
    let (hl, hr) = harpsichord(rate, t);
    let harp = if section(t) >= 2 { 0.6 } else { 1.0 };
    let (sl, sr) = strings(t);
    let (ml, mr) = tune(t);
    (drums + low + 0.6 * hh + harp * hl + sl + ml, drums + low + hh + harp * hr + sr + mr, 0.5 * harp * (hl + hr))
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

// A chunk of the music as the GPU makes it (the music, soundboard and
// music_mix entry points): n stereo frames from loop time t0 (wrapped),
// with the soundboard carried on from `state` (modes * 2 floats; zeros
// for the first chunk), which is left as at the chunk's end.
#[cfg(not(target_arch = "spirv"))]
pub fn render_chunk(t0: f32, rate: f32, n: usize, state: &mut [f32]) -> Vec<f32> {
    let parts: Vec<(f32, f32, f32)> = (0..n).map(|i| music_parts(rate, wrap(t0 + i as f32 / rate))).collect();
    let harp: Vec<f32> = parts.iter().map(|p| p.2).collect();
    let mut wet = vec![0.0; 2 * n];
    crate::soundboard::run(rate, &harp, state, &mut wet);
    let mut out = Vec::with_capacity(2 * n);
    for (i, (l, r, _)) in parts.iter().enumerate() {
        out.push(soft(l + wet[2 * i]));
        out.push(soft(r + wet[2 * i + 1]));
    }
    out
}
