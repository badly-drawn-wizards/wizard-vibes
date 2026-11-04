// fut/spells.fut: spells in flight, drawn as glows added along each camera
// ray: closed-form falloff around points and segments, so they cost no
// extra marching. Their state lives in the app, which passes it each frame
// as a flat array, `spell_stride` floats per slot:
//
//   kind, x, y, z, a, b, c, age, life (seconds), seed (an integer)
//
//   spell_fire    a bolt at (x, y, z) moving at velocity (a, b, c)
//   spell_strike  lightning striking the ground at (x, y, z) from height b
//   spell_wind    a blade at (x, y, z) moving at (a, b, c)
//   spell_burst   a fire bolt's explosion at (x, y, z)
//   spell_gust    a wind blade's burst into small blades at (x, y, z); it
//                 was moving at (a, b, c)
//   spell_mark    where an enemy's strike will land, (x, y, z), when its
//                 life is up (it then becomes the strike, from height b)
//
// Every spell fades out over the last part of its life; the seed makes each
// one a little different. Any other kind is an empty slot. Positions are in
// the world.

use crate::prelude::*;

pub const spell_stride: i32 = 10;
pub const spell_fire: i32 = 0;
pub const spell_strike: i32 = 1;
pub const spell_wind: i32 = 2;
pub const spell_burst: i32 = 3;
pub const spell_gust: i32 = 4;
pub const spell_mark: i32 = 5;

// Where the ray o + t d (t in [0, tmax]) comes closest to the segment a-b:
// (distance, t, and how far beyond tmax the nearest point of the segment
// lies along the ray: how deep behind what the ray hits).
#[inline]
pub fn ray_segment(o: Vec3, d: Vec3, tmax: f32, a: Vec3, b: Vec3) -> (f32, f32, f32) {
    let v = b - a;
    let vv = (1e-8f32).maximum(dot(v, v));
    let w0 = o - a;
    let dv = dot(d, v);
    let denom = vv - dv * dv;
    let s = if denom > 1e-6 { saturate((dot(v, w0) * 1.0 - dv * dot(d, w0)) / denom) } else { 0.0 };
    let t = clamp(0.0, tmax, dot(a + scale(s, v) - o, d));
    let s = saturate(dot(o + scale(t, d) - a, v) / vv);
    let q = a + scale(s, v);
    (length(o + scale(t, d) - q), t, (0.0f32).maximum(dot(q - o, d) - tmax))
}

// ray_segment for the polyline through point 0, 1, .., n: the nearest of
// its segments.
#[inline]
pub fn ray_polyline(o: Vec3, d: Vec3, tmax: f32, n: i32, point: impl Fn(i32) -> Vec3) -> (f32, f32, f32) {
    let (mut best, mut bt, mut bb) = (INF, 0.0f32, 0.0f32);
    let mut k = 0;
    while k < n {
        let (dd, tt, b) = ray_segment(o, d, tmax, point(k), point(k + 1));
        if dd < best {
            best = dd;
            bt = tt;
            bb = b;
        }
        k += 1;
    }
    (best, bt, bb)
}

// A glowing crescent centred at c, facing f (its tips curve back, away
// from f) and spanning `width` either side along s: six segments, thickest
// in the middle and tapering to the tips. (core, halo, t, behind) of its
// brightest part along the ray (see ray_segment, glow_line).
#[inline]
pub fn crescent(o: Vec3, d: Vec3, tmax: f32, c: Vec3, f: Vec3, s: Vec3, width: f32, bend: f32, thick: f32) -> (f32, f32, f32, f32) {
    let point = move |k: i32| {
        let th = (k as f32 / 6.0 - 0.5) * 2.4;
        c + scale(width * th.sin(), s) - scale(bend * (1.0 - th.cos()), f)
    };
    let (mut core, mut halo, mut bt, mut bb) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut k = 0;
    while k < 6 {
        let (dd, tt, b) = ray_segment(o, d, tmax, point(k), point(k + 1));
        let taper = (PI * (k as f32 + 0.5) / 6.0).sin();
        let r = thick * (0.2 + 0.8 * taper);
        let cr = (-(dd * dd) / (r * r)).exp();
        let hl = (5.0 * r) * (5.0 * r) / (dd * dd + (5.0 * r) * (5.0 * r));
        if cr > core {
            core = cr;
            halo = halo.maximum(hl);
            bt = tt;
            bb = b;
        } else {
            halo = halo.maximum(hl);
        }
        k += 1;
    }
    (core, halo, bt, bb)
}

// A glowing line: a hot core of radius r and a soft halo of radius h.
#[inline]
pub fn glow_line(r: f32, h: f32, dist: f32) -> (f32, f32) {
    ((-(dist * dist) / (r * r)).exp(), h * h / (dist * dist + h * h))
}

// The light one spell (in slot i) adds along the ray, taking the slot's
// values as scalars: spell_glow sums it over the slots.
#[inline(never)]
pub fn spell_light(fog: f32, view_distance: f32, secs: f32, o: Vec3, d: Vec3, tmax: f32, i: i32, kind: i32, p: Vec3, q: Vec3, age: f32, life: f32, seed: i32) -> Vec3 {
    let fade = smoothstep(life, 0.6 * life, age);
    let rnd = move |n: i32| hash2(seed.wrapping_add(n.wrapping_mul(7919)), 23);
    // (glows fade with the fog, and with depth behind what the ray hits,
    // over about their size: a burst on the ground still shows, but
    // nothing shines through the rock)
    let seen = move |t: f32, behind: f32, size: f32| (1.0 - fog_amount(fog, view_distance, t)) * (-behind / size).exp();
    if kind == spell_fire {
        // a ball with a short trail behind it, flickering
        let tail = p - scale(0.06, q);
        let (dist, t, behind) = ray_segment(o, d, tmax, tail, p);
        let (core, halo) = glow_line(0.12, 0.8, dist);
        let flick = 0.85 + 0.15 * (secs * 43.0 + i as f32 * 1.7).sin();
        let hot = (-(dist * dist) / 0.0016).exp();
        scale(
            fade * flick * seen(t, behind, 0.3),
            scale(5.0 * core, mix(vec(1.0, 0.42, 0.08), vec(1.0, 0.9, 0.6), hot)) + scale(0.5 * halo, vec(1.0, 0.35, 0.05)),
        )
    } else if kind == spell_burst {
        // An explosion: a billowing fireball that swells fast and cools
        // from yellow-white through orange to a dull red, a shockwave ring
        // racing out from it, and sparks thrown out and falling.
        let r = 3.2 * (1.0 - (-5.0 * age).exp()) + 0.2;
        let (dist, t, behind) = ray_segment(o, d, tmax, p, p);
        let x = o + scale(t, d); // where the ray passes closest
        let turb = vnoise2((x.x - p.x) * 1.4 + (x.z - p.z) * 0.7 + 7.0 * i as f32, (x.y - p.y) * 1.4 - 3.0 * age);
        let heat = (-2.2 * age).exp();
        let body = smoothstep(r, 0.25 * r, dist * (0.8 + 0.45 * turb));
        let colour = mix(
            vec(0.9, 0.12, 0.02),
            mix(vec(1.0, 0.45, 0.08), vec(1.0, 0.9, 0.6), saturate(2.0 * heat - 0.6)),
            saturate(1.5 * heat * (1.0 - dist / r)),
        );
        let halo = (1.6 * r) * (1.6 * r) / (dist * dist + (1.6 * r) * (1.6 * r));
        let ring_r = 9.0 * age;
        // (squared by hand: WGSL's pow is undefined for negative bases)
        let ring = {
            let u = (dist - ring_r) / (0.25 + 0.4 * age);
            (-(u * u)).exp() * (-4.0 * age).exp()
        };
        let fire = scale(7.0 * heat * body, colour) + scale(0.55 * heat * halo, vec(1.0, 0.35, 0.06)) + scale(1.6 * ring, vec(1.0, 0.75, 0.45));
        // sparks: hashed directions, mostly upwards, falling under gravity
        let mut sparks = 0.0f32;
        let mut j = 0;
        while j < 8 {
            let h = move |n: i32| hash2(i.wrapping_mul(31).wrapping_add(j * 7).wrapping_add(n), 91);
            let dir = normalise(vec(h(1) - 0.5, 0.4 + h(2), h(3) - 0.5));
            let speed = 7.0 + 6.0 * h(4);
            let at_t = move |a: f32| p + scale(speed * a, dir) - vec(0.0, 4.9 * a * a, 0.0);
            let (sd, _, _) = ray_segment(o, d, tmax, at_t((0.0f32).maximum(age - 0.05)), at_t(age));
            sparks = sparks + (-(sd * sd) / 0.0025).exp();
            j += 1;
        }
        let ember = scale(5.0 * sparks * (-2.5 * age).exp(), vec(1.0, 0.6, 0.2));
        scale(fade * seen(t, behind, r), fire + ember)
    } else if kind == spell_wind {
        // a pale crescent across its path, its tips curving back, sharp at
        // the ends; each its own width, curve and tilt, rolling slowly and
        // wobbling a little; three fading echoes trail it
        let f = normalise(q);
        let side = normalise(cross(f, vec(0.0, 1.0, 0.0)) + vec(0.0, 0.001, 0.0));
        let up = cross(side, f);
        let spin = if rnd(1) < 0.5 { -1.0 } else { 1.0 };
        let width = 0.7 + 0.45 * rnd(6);
        let bend = 0.35 + 0.35 * rnd(7);
        let fade = fade * saturate(8.0 * age);
        let mut c = splat(0.0);
        let mut k = 0;
        while k < 4 {
            let a = age - 0.035 * k as f32;
            let roll = (rnd(2) - 0.5) * 1.2 + spin * (0.25 + 0.35 * rnd(3)) * a + 0.12 * (a * (5.0 + 4.0 * rnd(4)) + 6.283 * rnd(5)).sin();
            let s = scale(roll.cos(), side) + scale(roll.sin(), up);
            let centre = p - scale(0.035 * k as f32, q);
            let (core, halo, t, behind) = crescent(o, d, tmax, centre, f, s, width, bend, 0.07 - 0.012 * k as f32);
            let k_gain = (0.45f32).powf(k as f32);
            c = c + scale(k_gain * fade * seen(t, behind, 0.2), scale(3.2 * core, vec(0.85, 1.0, 0.95)) + scale(0.4 * halo, vec(0.4, 0.85, 0.75)));
            k += 1;
        }
        c
    } else if kind == spell_gust {
        // the blade's burst: eight small crescents flying out (mostly the
        // way it was going), slowing, spinning and shrinking, and a faint
        // ring of air spreading from where it broke (only the travel's
        // horizontal part aims them)
        let travel = vec(q.x, 0.0, q.z);
        let mut blades = splat(0.0);
        let mut j = 0;
        while j < 8 {
            let h = move |n: i32| hash2(seed.wrapping_mul(7).wrapping_add(j * 31).wrapping_add(n), 41);
            let dir = normalise(vec(h(1) - 0.5, 0.15 + 0.55 * h(2), h(3) - 0.5) + scale(0.018, travel));
            let speed = 9.0 * (0.7 + 0.6 * h(4));
            let centre = p + scale(speed * (1.0 - (-3.0 * age).exp()) / 3.0, dir);
            let side = normalise(cross(dir, vec(0.0, 1.0, 0.0)) + vec(0.001, 0.0, 0.0));
            let up = cross(side, dir);
            let roll = 6.283 * h(5) + (if h(6) < 0.5 { -9.0 } else { 9.0 }) * age;
            let s = scale(roll.cos(), side) + scale(roll.sin(), up);
            let size = 0.5 * (1.0 - 0.5 * age / life);
            let (core, halo, t, behind) = crescent(o, d, tmax, centre, dir, s, size, 0.45 * size, 0.05);
            blades = blades + scale(seen(t, behind, 0.2), scale(3.5 * core, vec(0.85, 1.0, 0.95)) + scale(0.45 * halo, vec(0.4, 0.85, 0.75)));
            j += 1;
        }
        let (gd, gt, gb) = ray_segment(o, d, tmax, p, p);
        let ring = {
            let u = (gd - 5.0 * age) / (0.2 + 0.3 * age);
            (-(u * u)).exp() * (-4.0 * age).exp()
        };
        scale(fade, blades + scale(0.8 * ring * seen(gt, gb, 1.0), vec(0.6, 0.95, 0.85)))
    } else if kind == spell_strike {
        // a jagged bolt from the sky, re-forking several times a second,
        // flashing as it fades, with a flash on the ground
        let top = vec(p.x, q.y, p.z);
        let flash = ftoi(age * 14.0);
        let env = fade * (-4.0 * age).exp() * (0.55 + 0.45 * hash2(seed.wrapping_add(flash), 17));
        let point = move |k: i32| {
            let u = k as f32 / 8.0;
            let jx = if k == 0 || k == 8 { 0.0 } else { hash2(seed.wrapping_mul(13).wrapping_add(k), flash) - 0.5 };
            let jz = if k == 0 || k == 8 { 0.0 } else { hash2(seed.wrapping_mul(7).wrapping_sub(k), flash.wrapping_add(3)) - 0.5 };
            mix(top, p, u) + vec(2.4 * jx, 0.0, 2.4 * jz)
        };
        let (dist, t, behind) = ray_polyline(o, d, tmax, 8, point);
        let (core, halo) = glow_line(0.06, 0.9, dist);
        let (gd, gt, gb) = ray_segment(o, d, tmax, p, p);
        let ground = 3.5 * 3.5 / (gd * gd + 3.5 * 3.5);
        // and a ring of light racing out over the ground where it hits
        let ring = {
            let u = (gd - 7.0 * age) / (0.3 + 0.5 * age);
            fade * (-(u * u)).exp() * (-3.0 * age).exp()
        };
        scale(
            env,
            scale(seen(t, behind, 0.5), scale(9.0 * core, vec(0.85, 0.9, 1.0)) + scale(0.8 * halo, vec(0.55, 0.6, 1.0)))
                + scale(1.5 * ground * seen(gt, gb, 2.0), vec(0.6, 0.65, 1.0)),
        ) + scale(2.0 * ring * seen(gt, gb, 3.0), vec(0.7, 0.8, 1.0))
    } else if kind == spell_mark {
        // a red ring closing in on where the strike will land, turning,
        // brighter and faster to pulse as it comes; and a thread of light
        // down to it from the sky
        let u = saturate(age / life);
        let r = 2.2 - 1.3 * u;
        let point = move |k: i32| {
            let a = k as f32 * (2.0 * PI / 20.0) + 2.0 * age;
            p + vec(r * a.cos(), 0.0, r * a.sin())
        };
        let (dist, t, behind) = ray_polyline(o, d, tmax, 20, point);
        let (core, halo) = glow_line(0.03, 0.15, dist);
        let (bd, bt, bb) = ray_segment(o, d, tmax, vec(p.x, q.y, p.z), p);
        let (bcore, bhalo) = glow_line(0.012, 0.08, bd);
        let env = (0.4 + u) * (0.75 + 0.25 * (age * (12.0 + 25.0 * u)).sin());
        scale(
            env,
            scale(seen(t, behind, 0.5), scale(3.0 * core, vec(1.0, 0.25, 0.3)) + scale(0.25 * halo, vec(1.0, 0.1, 0.15)))
                + scale(u * seen(bt, bb, 0.5), scale(1.5 * bcore, vec(1.0, 0.4, 0.4)) + scale(0.1 * bhalo, vec(1.0, 0.1, 0.1))),
        )
    } else {
        splat(0.0)
    }
}

#[inline]
fn at(spells: &[f32], i: i32, j: i32) -> f32 {
    unsafe { *spells.get_unchecked((i * spell_stride + j) as usize) }
}

// The light the spells add along the world-space ray o + t d, t in
// [0, tmax] (nearer than whatever the ray hits), dimmed by the fog: the
// first n slots of spells (n at most spells.len() / spell_stride).
#[inline]
pub fn spell_glow(prm: Params, spells: &[f32], n: i32, secs: f32, o: Vec3, d: Vec3, tmax: f32) -> Vec3 {
    let tmax = tmax.minimum(prm.view_distance);
    let mut c = splat(0.0);
    let mut i = 0;
    while i < n {
        c = c + spell_light(
            prm.fog,
            prm.view_distance,
            secs,
            o,
            d,
            tmax,
            i,
            ftoi(at(spells, i, 0)),
            vec(at(spells, i, 1), at(spells, i, 2), at(spells, i, 3)),
            vec(at(spells, i, 4), at(spells, i, 5), at(spells, i, 6)),
            at(spells, i, 7),
            at(spells, i, 8),
            ftoi(at(spells, i, 9)),
        );
        i += 1;
    }
    c
}
