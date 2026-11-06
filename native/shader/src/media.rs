// fut/media.fut: everything the march integrates: the orb's glow cloud, the
// glowing rune letters, the lightning, and a faint haze inside the orb's
// glass.

use crate::prelude::*;

// A cloud around the orb whose density falls off exponentially with
// distance from the orb's surface, flickering with the lightning. It is
// outside the glass only: inside there is just the haze, so the orb's
// refraction stays visible.
#[inline]
pub fn orb_glow_density(prm: Params, g: Glow, od: f32) -> f32 {
    if od < 0.0 { 0.0 } else { prm.orb_glow * g.level * (-od / prm.orb_glow_extent).exp() }
}

// The rune letters glow in a thin layer hugging the strokes, pulsing gently.
#[inline]
pub fn rune_glow_density(prm: Params, tm: f32, gd: f32) -> f32 {
    prm.rune_glow * rune_pulse(prm.rune_pulse, tm) * (-gd / 0.05).exp()
}

// Inside a glass orb, a faint glowing haze.
pub const globe_haze: f32 = 0.3;

// Clouds are lit by the moon, more brightly towards their tops, over a dim
// ambient; they scatter what they absorb.
#[inline]
pub fn cloud_colour(colour: Vec3, h: f32) -> Vec3 {
    vec(0.04, 0.045, 0.09) + scale(0.12 + 0.3 * h, colour)
}

// (compiled once: see scene in scene.rs)
// The colour a glowing material gives off, used both for its surface and
// for the glow around it, so the two always match. 0 for materials that do
// not glow.
#[inline(never)]
pub fn glow_colour(_prm: Params, m: i32) -> Vec3 {
    if m == m_potion {
        potion_colour_1
    } else if m == m_potion + 1 {
        potion_colour_2
    } else if m == m_potion + 2 {
        potion_colour_3
    } else {
        splat(0.0)
    }
}

// The potions' liquids glow in a thin halo, coloured by whichever liquid is
// nearest.
#[inline]
pub fn potion_glow_density(prm: Params, ld: f32) -> f32 {
    prm.potion_glow * (-(ld.maximum(0.0)) / 0.012).exp()
}

// (few call sites: the whole pose is passed)
#[inline(never)]
pub fn media(pose: Pose, g: Glow, p: Vec3) -> MediaSample {
    let tm = pose.tm;
    let vis = pose.prm.layers;
    let glow = has(vis, layers_glow); // the glow clouds round the orb, runes and potions
    let orb = has(vis, layers_orb); // the haze and lightning inside the orb
    // (the orb and its arcs go where the staff holds them)
    let q = staff_rest(pose, p);
    let od = sd_orb(q);
    let gd = if glow { glyph_dist(tm, p) } else { 1e9 };
    // in the orb: lightning arcs, flames or a whirlwind, by the spell in hand
    let bolts = if !orb {
        no_media
    } else if pose.spell == 0 {
        fire_media(pose.secs, q)
    } else if pose.spell == 2 || pose.spell == 3 {
        wind_media(pose.secs, q)
    } else {
        let l = lightning_media(tm, q);
        MediaSample { radiance: scale(pose.prm.lightning, l.radiance), ..l }
    };
    let haze = if orb && od < 0.0 { globe_haze } else { 0.0 };
    let od = if glow || orb { od } else { 1e9 }; // (no step cap near a hidden orb)
    let (ld, lm) = if glow { nearest_liquid(pose, p) } else { (1e9, m_potion) };
    MediaSample {
        cap: (0.3 * ld.abs() + 0.004).minimum(bolts.cap.minimum((0.3 * od.abs() + 0.015).minimum(0.3 * gd + 0.01))),
        radiance: scale(if glow { pose.orb_boost * orb_glow_density(pose.prm, g, od) } else { 0.0 }, pose.orb_tint)
            + scale(pose.runes * rune_glow_density(pose.prm, tm, gd), pose.prm.rune_colour)
            + scale(haze, pose.orb_tint)
            + bolts.radiance
            + scale(potion_glow_density(pose.prm, ld), glow_colour(pose.prm, lm)),
        extinction: 0.0,
    }
}

// The clouds are integrated separately from the march (see `clouds`): their
// slab lies between two world heights, so each ray's way through it is known
// in advance, and the march need not take small steps (each evaluating the
// whole scene) to cross it.

// Towards the view distance, where rays stop, everything fades into the sky
// (1 near .. 0 at the view distance), so there is no edge.
#[inline]
pub fn view_fade_at(view_distance: f32, t: f32) -> f32 {
    smoothstep(view_distance, 0.7 * view_distance, t)
}
#[inline]
pub fn view_fade(prm: Params, t: f32) -> f32 {
    view_fade_at(prm.view_distance, t)
}

// What the clouds depend on, projected out of the pose (so clouds_through,
// a real GPU function, takes a few scalars rather than the whole pose's).
#[derive(Clone, Copy)]
pub struct CloudIn {
    pub fl: Flight,
    pub secs: f32,
    pub cover: f32,
    pub density: f32,
    pub colour: Vec3,
    pub view_distance: f32,
}

// Radiance and transmittance of the clouds along ro + t rd for t in
// [0, tend]. About 24 samples, spread over the part of the ray inside the
// slab; clear sky is skipped using the bound from cloud_sample.
#[inline(never)]
pub fn clouds_through(ci: CloudIn, ro: Vec3, rd: Vec3, tend: f32) -> (Vec3, f32) {
    let wo = to_world(ci.fl, ro);
    let wd = to_world_dir(ci.fl, rd);
    // where the ray's world height crosses the slab's bottom and top
    let (t0, t1) = if wd.y.abs() < 1e-5 {
        if cloud_gap(wo.y) > 0.0 { (1.0, 0.0) } else { (0.0, tend) }
    } else {
        let ta = (cloud_bottom - wo.y) / wd.y;
        let tb = (cloud_top - wo.y) / wd.y;
        ((0.0f32).maximum(ta.minimum(tb)), tend.minimum(ta.maximum(tb)))
    };
    if t1 <= t0 {
        return (splat(0.0), 1.0);
    }
    let dt = (0.3f32).maximum((t1 - t0) / cloud_samples as f32);
    let (mut t, mut i, mut rad, mut tr) = (t0 + 0.5 * dt, 0i32, splat(0.0), 1f32);
    while t < t1 && i < 64 && tr > 0.02 {
        let w = wo + scale(t, wd);
        let (dens, clear) = cloud_sample(ci.cover, ci.density, ci.secs, w);
        let dens = dens * view_fade_at(ci.view_distance, t);
        if dens <= 0.0 {
            t = t + dt.maximum(clear);
            i += 1;
        } else {
            let ch = (w.y - cloud_bottom) / (cloud_top - cloud_bottom);
            let rad_n = rad + scale(dens * dt * tr, cloud_colour(ci.colour, ch));
            let tr_n = tr * (-dens * dt).exp();
            t = t + dt;
            i += 1;
            rad = rad_n;
            tr = tr_n;
        }
    }
    (rad, tr)
}

pub fn clouds(pose: Pose, ro: Vec3, rd: Vec3, tend: f32) -> (Vec3, f32) {
    let prm = pose.prm;
    clouds_through(
        CloudIn {
            fl: pose.fl,
            secs: pose.secs,
            cover: prm.cloud_cover,
            density: prm.cloud_density,
            colour: prm.cloud_colour,
            view_distance: prm.view_distance,
        },
        ro,
        rd,
        tend,
    )
}
