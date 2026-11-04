// fut/lightning.fut: crackling lightning on the orb, as emissive media
// integrated by the march.
//
// Two kinds of bolt: arcs that leap from the orb's surface into the air, and
// tendrils inside the orb from its core to its glass (plasma-globe style).
// Each bolt is a jagged polyline. It is re-seeded `lives` times per loop
// (phase-shifted per bolt), and within a life its brightness flickers and
// part of its jaggedness is re-randomised, which makes it crackle. All timing
// is in whole slots of the loop, so the animation still loops seamlessly.

use crate::prelude::*;

// Compile-time switches (orb_arcs, orb_tendrils, bolts_per_kind) are in
// generated.rs, set from params.json and the drawer.

pub const kind_arc: i32 = 0;
pub const kind_tendril: i32 = 1;
#[inline]
pub fn kind_enabled(kind: i32) -> bool {
    (kind == kind_arc && orb_arcs) || (kind == kind_tendril && orb_tendrils)
}

pub const segments: i32 = 7;
pub const lives: f32 = 30.0; // per loop: 2.5 per second
pub const flickers: f32 = 5.0; // per life

pub const arc_reach: f32 = 0.26; // arcs end at most this far from the orb's surface

#[inline]
pub fn rand_dir(a: f32, b: f32) -> Vec3 {
    let z = 2.0 * a - 1.0;
    let r = (0.0f32).maximum(1.0 - z * z).sqrt();
    let phi = 2.0 * PI * b;
    vec(r * phi.cos(), z, r * phi.sin())
}

// A bolt's current life: (seed slot, flicker slot, position in [0,1) within
// its life). Each bolt is phase-shifted so they don't all change at once.
#[derive(Clone, Copy)]
pub struct Life {
    pub slot: i32,
    pub flick: i32,
    pub u: f32,
}

#[inline]
pub fn bolt_life(tm: f32, id: i32) -> Life {
    let x = tm / (2.0 * PI) * lives + hash2(id, 101) * lives;
    let s = x.floor();
    let u = x - s;
    let slot = imod(ftoi(s), ftoi(lives));
    Life { slot, flick: slot * 64 + ftoi(u * flickers), u }
}

// Brightness over a life: a sharp strike that decays, flickering.
#[inline]
pub fn envelope(id: i32, l: Life) -> f32 {
    let strike = if l.u < 0.06 { l.u / 0.06 } else { (-(l.u - 0.06) * 3.0).exp() };
    strike * (0.45 + 0.55 * hash3(id, l.flick, 7))
}

// A bolt's two ends and length, fixed for its life.
#[derive(Clone, Copy)]
pub struct Ends {
    pub a: Vec3,
    pub b: Vec3,
    pub len: f32,
}

pub fn bolt_ends(kind: i32, id: i32, l: Life) -> Ends {
    let h = |j: i32| hash3(id, l.slot, j);
    if kind == kind_arc {
        let d0 = normalise(rand_dir(h(1), h(2)) + vec(0.0, 0.6, 0.0));
        let d1 = normalise(d0 + scale(0.9, rand_dir(h(3), h(4))));
        let len = 0.1 + (arc_reach - 0.12) * h(5);
        Ends { a: orb_pos() + scale(orb_radius, d0), b: orb_pos() + scale(orb_radius + len, d1), len }
    } else {
        let d = rand_dir(h(1), h(2));
        Ends { a: orb_pos() + scale(0.12 * orb_radius, d), b: orb_pos() + scale(0.96 * orb_radius, d), len: orb_radius }
    }
}

// Jagged offsets are at most this fraction of the bolt's length.
pub const jag: f32 = 0.3 * 1.4;

// Control point i (0..segments) of a bolt. The jagged offsets are mostly
// fixed for the life, with a part re-drawn each flicker.
pub fn bolt_point(kind: i32, id: i32, l: Life, e: Ends, i: i32) -> Vec3 {
    let h = |j: i32| hash3(id, l.slot, j);
    let Ends { a, b, len } = e;
    let f = i as f32 / segments as f32;
    let fixed = rand_dir(h(10 + i), h(30 + i));
    let crackle = rand_dir(hash3(id, l.flick, 50 + i), hash3(id, l.flick, 70 + i));
    let offset = scale(0.3 * len * (PI * f).sin(), fixed + scale(0.4, crackle));
    let v = (mix(a, b, f) + offset) - orb_pos();
    let r = length(v);
    // arcs stay outside the orb, tendrils inside its glass
    let r_p = if kind == kind_arc { r.maximum(orb_radius) } else { r.minimum(0.95 * orb_radius) };
    orb_pos() + scale(r_p / r.maximum(1e-6), v)
}

// Distance to the nearest point of a bolt's polyline.
pub fn bolt_dist(kind: i32, id: i32, l: Life, e: Ends, p: Vec3) -> f32 {
    let (mut d, mut prev) = (1e9f32, bolt_point(kind, id, l, e, 0));
    let mut i = 0i32;
    while i < segments {
        let next = bolt_point(kind, id, l, e, i + 1);
        let d_n = d.minimum(sd_capsule(p, prev, next, 0.0));
        d = d_n;
        prev = next;
        i += 1;
    }
    d
}

// Beyond this distance from a bolt its halo is negligible, so it is skipped.
pub const bolt_margin: f32 = 0.12;

pub const bolt_core_colour: Vec3 = Vec3::new(1.0, 1.0, 1.0);
pub const bolt_halo_colour: Vec3 = Vec3::new(0.3, 0.75, 1.0);

// Lightning media at p: a thin white-hot core and a cyan halo around each
// bolt. Only evaluated near the orb.
#[inline(never)]
pub fn lightning_media(tm: f32, p: Vec3) -> MediaSample {
    let reach = orb_radius + arc_reach + 0.15;
    if length(p - orb_pos()) > reach {
        MediaSample { cap: length(p - orb_pos()) - reach + 0.02, radiance: splat(0.0), extinction: 0.0 }
    } else {
        let mut acc = MediaSample { cap: 1e9f32, radiance: splat(0.0), extinction: 0f32 };
        let mut kind = 0i32;
        while kind < 2 {
            if kind_enabled(kind) {
                let mut b = 0i32;
                while b < bolts_per_kind {
                    let id = kind * 100 + b;
                    let l = bolt_life(tm, id);
                    let e = bolt_ends(kind, id, l);
                    // the whole bolt lies within this sphere
                    let c = mix(e.a, e.b, 0.5);
                    let r = 0.5 * length(e.b - e.a) + jag * e.len;
                    let dc = length(p - c) - r;
                    acc = if dc > bolt_margin {
                        MediaSample { cap: acc.cap.minimum(0.3 * (dc + bolt_margin) + 0.002), ..acc }
                    } else {
                        let d = bolt_dist(kind, id, l, e, p);
                        let k = envelope(id, l);
                        let core = 220.0 * (-d / 0.003).exp();
                        let halo = 1.4 * (-d / 0.025).exp();
                        MediaSample {
                            cap: acc.cap.minimum(0.3 * d + 0.002),
                            radiance: acc.radiance + scale(k, scale(core, bolt_core_colour) + scale(halo, bolt_halo_colour)),
                            ..acc
                        }
                    };
                    b += 1;
                }
            }
            kind += 1;
        }
        acc
    }
}

// Overall brightness of the bolts right now, in [0, 1]: drives the flicker
// of the orb's light on the scene.
pub fn flash(tm: f32) -> f32 {
    let n = ((orb_arcs as i32 + orb_tendrils as i32) * bolts_per_kind) as f32;
    let mut t = 0f32;
    let mut kind = 0i32;
    while kind < 2 {
        if kind_enabled(kind) {
            let mut b = 0i32;
            while b < bolts_per_kind {
                let id = kind * 100 + b;
                t = t + envelope(id, bolt_life(tm, id));
                b += 1;
            }
        }
        kind += 1;
    }
    let total = t;
    if n > 0.0 { total / n } else { 0.0 }
}

// How bright the orb's magic is right now. It follows the lightning, so the
// glow cloud, the orb's light and the runes flicker with the bolts. The
// flash averages 0.245 over a loop (range 0.05-0.68), so the level averages
// 0.94 (range 0.71-1.46).
#[derive(Clone, Copy)]
pub struct Glow {
    pub flash: f32,
    pub level: f32,
}

#[inline]
pub fn glow_now(tm: f32) -> Glow {
    let f = flash(tm);
    Glow { flash: f, level: 0.65 + 1.2 * f }
}
