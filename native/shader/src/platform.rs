// fut/platform.fut: what a wizard flies on, and the magic circle on it: a
// floating rock, a shard of ice or a magic carpet (platform_rock,
// platform_ice, platform_carpet; the "platform" parameter picks his, and
// each enemy has one of its own). Each is modelled with its flat top at
// y = 0, where the wizard stands, facing +z, inside the sphere
// platform_centre, radius platform_radius. Their tops are m_ground, the
// circle's material, whatever the platform (see platform_top in
// shading.fut); their sides m_rock, m_ice or m_carpet.

use crate::prelude::*;

pub const platform_rock: i32 = 0;
pub const platform_ice: i32 = 1;
pub const platform_carpet: i32 = 2;

pub const platform_centre: Vec3 = Vec3::new(0.0, -1.2, 0.0);
pub const platform_radius: f32 = 2.3;

// ---------------------------------------------------------------------
// Rune circle: 14 glyphs on a circle of radius 1.125, turning once per loop.

pub const rune_radius: f32 = 1.125;

// 2D distance in the ground plane to the nearest rune stroke.
#[inline(never)]
pub fn glyph_sdf(tm: f32, x: f32, z: f32) -> f32 {
    let c = rune_radius;
    let r = length2(vec2(x, z));
    // Far from the band, a cheap lower bound. It is only used where the glow
    // density is negligible, so the bound's looseness never shows.
    if (r - c).abs() > 0.45 {
        (r - c).abs() - 0.12
    } else {
        let n = 14f32;
        let sector = 2.0 * PI / n;
        let q = rot2(tm, vec2(x, z));
        let a = q.y.atan2(q.x) + PI;
        let idx = ftoi((a / sector).floor());
        let lp = rep_angle(n, vec(q.x, 0.0, q.y));
        let h1 = hash2(idx, 3);
        let h2 = hash2(idx, 7);
        let seg = move |ax: f32, az: f32, bx: f32, bz: f32| sd_capsule(lp, vec(c + ax, 0.0, az), vec(c + bx, 0.0, bz), 0.008);
        let g1 = seg(-0.08, -0.05 + 0.1 * h1, 0.08, 0.05 - 0.1 * h1);
        let g2 = seg(-0.08 + 0.16 * h2, -0.06, 0.08 - 0.16 * h1, 0.06);
        let g3 = if h2 > 0.5 { seg(0.0, -0.07, 0.0, 0.07) } else { sd_torus(lp - vec(c, 0.0, 0.0), 0.05, 0.004) };
        g1.minimum(g2.minimum(g3))
    }
}

// Approximate 3D distance to the nearest letter: the in-plane distance and
// the height above the ground, combined by Pythagoras.
#[inline]
pub fn glyph_dist(tm: f32, p: Vec3) -> f32 {
    if p.y > 0.35 {
        p.y // high above the ground: skip the strokes
    } else {
        let d = 0f32.maximum(glyph_sdf(tm, p.x, p.z));
        (d * d + p.y * p.y).sqrt()
    }
}

// Emission painted on the ground (before brightness): two rings and the glyphs.
#[inline(never)]
pub fn rune_paint(tm: f32, p: Vec3) -> f32 {
    let r = length2(vec2(p.x, p.z));
    let ring = move |w: f32, c: f32| 1.0 - smoothstep(w * 0.5, w, (r - c).abs());
    let rings = ring(0.018, 0.95).maximum(ring(0.012, 1.3));
    let glyph = 1.0 - smoothstep(0.0, 0.008, glyph_sdf(tm, p.x, p.z));
    rings.maximum(glyph)
}

// ---------------------------------------------------------------------
// The floating rock: an inverted cone with a flat top (the magic circle's
// outer ring, at radius 1.3, fits inside), roughened below by noise that
// fades out towards the rim.

pub const rock_radius: f32 = 1.6;
pub const rock_depth: f32 = 2.8;

pub fn rock_exact(p: Vec3) -> Hit {
    let cone = sd_capped_cone(p - vec(0.0, -rock_depth, 0.0), rock_depth, 0.3, rock_radius);
    // (the weight is 0 at and above the top face, where the wizard is, so the
    // noise is only evaluated below it)
    let rough = if p.y >= 0.0 {
        0.0
    } else {
        saturate(-p.y * 2.5) * (0.22 * vnoise3(scale(2.2, p)) + 0.1 * vnoise3(scale(5.3, p)) - 0.16)
    };
    // the noise steepens the field; scale it back to keep tracing safe
    (0.6 * (cone + rough), if p.y > -0.03 { m_ground } else { m_rock })
}

// ---------------------------------------------------------------------
// The shard of ice: a seven-sided pyramid, point down, with three smaller
// ones hanging beside its point and a crown of crystals round its rim.

// A pyramid of n faces, apothem r at its base (y = 0) and its point h
// below (h < 0: above), as the most a point lies outside any face (a lower
// bound on the distance, exact off the edges). (Private: world.rs has a
// different sd_pyramid.)
fn sd_pyramid(n: f32, r: f32, h: f32, p: Vec3) -> f32 {
    let q = rep_angle(n, p);
    let l = (h * h + r * r).sqrt();
    let face = (h.abs() * q.x - r * (if h > 0.0 { q.y } else { -q.y }) - h.abs() * r) / l;
    face.maximum(if h > 0.0 { q.y } else { -q.y })
}

pub fn ice_exact(p: Vec3) -> Hit {
    let body = sd_pyramid(7.0, 1.55, 2.7, p);
    // the hanging shards, round the point
    let hang = move |k: i32| {
        let a = k as f32 * 2.1 + 0.4;
        sd_pyramid(5.0, 0.42 - 0.05 * k as f32, 1.3 + 0.25 * k as f32, p - vec(0.7 * a.cos(), -0.8 - 0.15 * k as f32, 0.7 * a.sin()))
    };
    let below = hang(0).minimum(hang(1).minimum(hang(2)));
    // the crown: five crystals on the rim, of different heights
    let pr = vec(p.x * 0.96 - p.z * 0.28, p.y, p.z * 0.96 + p.x * 0.28);
    let c = rep_angle(5.0, pr);
    let sector = ftoi((pr.z.atan2(pr.x) / (2.0 * PI) * 5.0 + 0.5).floor());
    let tall = 0.25 + 0.3 * hash2(sector, 41);
    let crown = sd_pyramid(4.0, 0.13, -tall, c - vec(1.4, -0.05, 0.0));
    let d = body.minimum(below.minimum(crown));
    (0.9 * d, if p.y > -0.03 && crown > 0.01 { m_ground } else { m_ice })
}

// ---------------------------------------------------------------------
// The magic carpet: a rug 2.3 by 3.2, rippling more towards its ends (it
// is flat under his feet), with tassels along both ends.

pub const carpet_half: Vec3 = Vec3::new(1.15, 0.025, 1.6);

// How far the carpet's surface is lifted at (x, z).
#[inline]
pub fn carpet_lift(secs: f32, x: f32, z: f32) -> f32 {
    0.07 * (3.0 * z - 5.0 * secs + 0.8 * x).sin() * smoothstep(0.5, 1.6, z.abs())
        + 0.03 * (4.0 * x + 3.0 * secs).sin() * smoothstep(0.5, 1.15, x.abs())
}

pub fn carpet_exact(secs: f32, p: Vec3) -> Hit {
    let lift = carpet_lift(secs, p.x, p.z);
    let rug = sd_box(p - vec(0.0, lift - carpet_half.y - 0.008, 0.0), carpet_half) - 0.008;
    // the tassels: a short cord every 0.115 along each end
    let cell = 0.115;
    let cx = clamp(-1.1, 1.1, cell * round_even(p.x / cell));
    let end_lift = carpet_lift(secs, cx, if p.z < 0.0 { -carpet_half.z } else { carpet_half.z });
    let az = p.z.abs();
    let tassel = sd_capsule(
        vec(p.x, p.y, az),
        vec(cx, end_lift - 0.03, carpet_half.z - 0.02),
        vec(cx, end_lift - 0.06, carpet_half.z + 0.13),
        0.013,
    );
    // (the ripple tilts the field by up to about 0.35, so it is scaled back)
    let d = (0.75 * rug).minimum(tassel);
    (d, if az < carpet_half.z && p.y > lift - 0.012 { m_ground } else { m_carpet })
}

// ---------------------------------------------------------------------

// A platform of the given kind, at time secs.
pub fn platform_exact(kind: i32, secs: f32, p: Vec3) -> Hit {
    if kind == platform_ice {
        ice_exact(p)
    } else if kind == platform_carpet {
        carpet_exact(secs, p)
    } else {
        rock_exact(p)
    }
}

#[inline(never)]
pub fn platform(kind: i32, secs: f32, p: Vec3) -> Hit {
    sphere_bounded_hit(platform_centre, platform_radius, 0.3, m_rock, move |q| platform_exact(kind, secs, q), p)
}
