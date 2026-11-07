// fut/spells.fut: spells in flight, drawn as glows added along each camera
// ray: closed-form falloff around points and segments, so they cost no
// extra marching. Their state lives in the app, which passes it each frame
// as a flat array, `spell_stride` floats per slot:
//
//   kind, x, y, z, a, b, c, age, life (seconds), seed (an integer)
//
//   spell_fire    a bolt at (x, y, z) moving at velocity (a, b, c); its
//                 charge (0..1, bigger) in hundredths, as its seed's
//                 hundred-thousands
//   spell_strike  lightning striking the ground at (x, y, z) from height b
//   spell_wind    a blade at (x, y, z) moving at (a, b, c)
//   spell_burst   a fire bolt's explosion at (x, y, z), a the bolt's
//                 charge (0..1: up to twice the size)
//   spell_gust    a wind blade's burst into small blades at (x, y, z); it
//                 was moving at (a, b, c)
//   spell_mark    where an enemy's strike will land, (x, y, z), when its
//                 life is up (it then becomes the strike, from height b)
//   spell_charge  a fire bolt being charged at his orb (where he holds it
//                 this frame), or with a = 1 at (x, y, z) (an enemy's),
//                 its charge so far age / life
//   spell_windup  wind blades warming up, likewise: gusts swirling in
//                 round the orb, faster and brighter as age / life nears 1
//   spell_wisp    a defeated enemy's soul, its head at (x, y, z), the first
//                 of its tail's points at (a, b, c); age (of a life of 1)
//                 how far it has been taken into him, shrinking
//   spell_wisp_tail  a piece of a soul's tail, from (x, y, z) to (a, b, c),
//                 as wide as age there, and life at its end
//   spell_frost   a frost lance at (x, y, z) flying at (a, b, c)
//   spell_missile an arcane missile at (x, y, z) flying at (a, b, c)
//   spell_falling a meteor at (x, y, z) falling at (a, b, c)
//   spell_well    a gravity well at (x, y, z)
//   spell_spirit  his spirit familiar at (x, y, z), flying at (a, b, c)
//   spell_chain   lightning jumping from (x, y, z) to (a, b, c)
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
pub const spell_heal: i32 = 6;
pub const spell_lock: i32 = 7; // his strike locking on: a mark, green
pub const spell_charge: i32 = 8;
pub const spell_windup: i32 = 9;
pub const spell_wisp: i32 = 10;
pub const spell_wisp_tail: i32 = 11;
pub const spell_frost: i32 = 12;
pub const spell_missile: i32 = 13;
pub const spell_falling: i32 = 14;
pub const spell_well: i32 = 15;
pub const spell_spirit: i32 = 16;
pub const spell_chain: i32 = 17;
// (a soul's head's width, and its tail's at its end)
pub const wisp_head: f32 = 0.05;
pub const wisp_tip: f32 = 0.003;

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

// A piece of a soul's tail from a to b, as wide as w0 at a and w1 at b,
// tapering, and waving across the ray (the more, the further down the tail:
// where along it is told by its width), the wave running down it: its
// glow along the ray o + t d.
#[inline]
pub fn wisp_tail(o: Vec3, d: Vec3, tmax: f32, secs: f32, seed: i32, a: Vec3, b: Vec3, w0: f32, w1: f32) -> f32 {
    let along = b - a;
    let across = normalise(cross(along, d) + vec(0.0, 1e-4, 0.0));
    let down = move |w: f32| saturate((wisp_head - w) / (wisp_head - wisp_tip));
    let (s0, s1) = (down(w0), down(w1));
    let point = move |k: i32| {
        let f = k as f32 / 4.0;
        let s = s0 + (s1 - s0) * f;
        let wave = 0.16 * s * (11.0 * s - 13.0 * secs + seed as f32 * 0.7).sin();
        a + scale(f, along) + scale(wave, across)
    };
    let mut glow = 0.0f32;
    let mut k = 0;
    while k < 4 {
        let (dd, _, _) = ray_segment(o, d, tmax, point(k), point(k + 1));
        let w = w0 + (w1 - w0) * (k as f32 + 0.5) / 4.0;
        let (core, halo) = glow_line(w, 6.0 * w, dd);
        glow = glow.maximum(1.8 * core + 0.25 * halo);
        k += 1;
    }
    glow
}

// The light one spell (in slot i) adds along the ray, taking the slot's
// values as scalars: spell_glow sums it over the slots.
#[inline(never)]
pub fn spell_light(fog: f32, view_distance: f32, secs: f32, o: Vec3, d: Vec3, tmax: f32, orb: Vec3, i: i32, kind: i32, p: Vec3, q: Vec3, age: f32, life: f32, seed: i32) -> Vec3 {
    let fade = smoothstep(life, 0.6 * life, age);
    let rnd = move |n: i32| hash2(seed.wrapping_add(n.wrapping_mul(7919)), 23);
    // (glows fade with the fog, and with depth behind what the ray hits,
    // over about their size: a burst on the ground still shows, but
    // nothing shines through the rock)
    let seen = move |t: f32, behind: f32, size: f32| (1.0 - fog_amount(fog, view_distance, t)) * (-behind / size).exp();
    if kind == spell_fire {
        // a ball with a short trail behind it, flickering (bigger, charged)
        let k = 1.0 + 1.9 * saturate((seed / 100000) as f32 / 100.0);
        let tail = p - scale(0.06, q);
        let (dist, t, behind) = ray_segment(o, d, tmax, tail, p);
        let (core, halo) = glow_line(0.12 * k, 0.8 * k, dist);
        let flick = 0.85 + 0.15 * (secs * 43.0 + i as f32 * 1.7).sin();
        let hot = (-(dist * dist) / (0.0016 * k * k)).exp();
        scale(
            fade * flick * seen(t, behind, 0.3 * k),
            scale(5.0 * core, mix(vec(1.0, 0.42, 0.08), vec(1.0, 0.9, 0.6), hot)) + scale(0.5 * halo, vec(1.0, 0.35, 0.05)),
        )
    } else if kind == spell_charge {
        // a fireball swelling at the orb as it charges, flickering faster
        // the fuller it is, embers circling in to it
        let f = saturate(age / life);
        let rad = 0.05 + 0.2 * f;
        let p = if q.x > 0.5 { p } else { orb };
        let (dist, t, behind) = ray_segment(o, d, tmax, p, p);
        let (core, halo) = glow_line(rad, 2.0 * rad, dist);
        let flick = 0.8 + 0.2 * (secs * (30.0 + 40.0 * f)).sin();
        let hot = (-(dist * dist) / (0.15 * rad * rad)).exp();
        let mut embers = 0.0f32;
        let mut j = 0;
        while j < 5 {
            let a = secs * (5.0 + j as f32) + 1.257 * j as f32;
            let rr = rad * (1.6 + 1.2 * (1.0 - (secs * 2.0 + j as f32 * 0.37).fract()));
            let e = p + vec(rr * a.cos(), 0.6 * rr * (1.3 * a).sin(), rr * a.sin());
            let (ed, _, _) = ray_segment(o, d, tmax, e, e);
            embers = embers + (-(ed * ed) / 0.0004).exp();
            j += 1;
        }
        // (b: a frost lance's charge, icy, its embers flakes of frost)
        let (deep, white, glow, spark) = if q.y > 0.5 {
            (vec(0.35, 0.7, 1.0), vec(0.85, 0.97, 1.0), vec(0.2, 0.55, 1.0), vec(0.7, 0.9, 1.0))
        } else {
            (vec(1.0, 0.42, 0.08), vec(1.0, 0.92, 0.65), vec(1.0, 0.35, 0.05), vec(1.0, 0.6, 0.2))
        };
        scale(flick * seen(t, behind, 0.3), scale(4.0 * core, mix(deep, white, hot)) + scale(0.2 * halo, glow) + scale(2.5 * f * embers, spark))
    } else if kind == spell_windup {
        // gusts swirling in round the orb: arcs circling it, tighter, faster
        // and brighter as it warms up
        let f = saturate(age / life);
        let c = if q.x > 0.5 { p } else { orb };
        let mut glow = 0.0f32;
        let mut tt = 0.0f32;
        let mut j = 0;
        while j < 3 {
            let r = 0.42 - 0.22 * f + 0.05 * j as f32;
            let a0 = secs * (6.0 + 10.0 * f) * (if j == 1 { -1.0 } else { 1.0 }) + 2.1 * j as f32;
            let tilt = 0.5 * j as f32 - 0.5;
            let point = move |k: i32| {
                let a = a0 + 0.35 * k as f32;
                c + vec(r * a.cos(), tilt * r * a.sin() * 0.6, r * a.sin())
            };
            let (dd, t, _) = ray_polyline(o, d, tmax, 4, point);
            let (core, halo) = glow_line(0.012 + 0.012 * f, 0.08, dd);
            glow = glow + core + 0.15 * halo;
            tt = t;
            j += 1;
        }
        scale((0.4 + 1.6 * f) * glow * seen(tt, 0.0, 0.3), vec(0.55, 0.92, 1.0))
    } else if kind == spell_wisp {
        // a soul, like a sperm: a bright oval head, and the start of its
        // tail behind it (the rest: spell_wisp_tail)
        // (taken in, as age nears 1: shrinking away)
        let k = 1.0 - saturate(age);
        let back = normalise(q - p + vec(0.0, 1e-4, 0.0));
        let (hd, t, behind) = ray_segment(o, d, tmax, p - scale(0.04 * k, back), p + scale(0.05 * k, back));
        let (core, halo) = glow_line(wisp_head * k + 1e-4, 0.45 * k + 1e-3, hd);
        let flick = 0.8 + 0.2 * (secs * 17.0 + seed as f32 * 0.37).sin();
        let tail = wisp_tail(o, d, tmax, secs, seed, p, q, wisp_head * k + 1e-4, 0.03 * k + 1e-4);
        scale(
            fade * flick * seen(t, behind, 0.3),
            scale(2.6 * core + tail, vec(0.82, 0.95, 1.0)) + scale(0.3 * halo, vec(0.45, 0.75, 1.0)),
        )
    } else if kind == spell_wisp_tail {
        let flick = 0.8 + 0.2 * (secs * 17.0 + seed as f32 * 0.37).sin();
        let (_, t, behind) = ray_segment(o, d, tmax, p, q);
        scale(flick * seen(t, behind, 0.3) * wisp_tail(o, d, tmax, secs, seed, p, q, age, life), vec(0.82, 0.95, 1.0))
    } else if kind == spell_frost {
        // a shard of ice (solid: see frost_hit), a cold haze about it and a
        // frosty trail behind it
        let dir = normalise(q + vec(0.0, 1e-4, 0.0));
        let (dist, t, behind) = ray_segment(o, d, tmax, p - scale(0.55, dir), p + scale(0.12, dir));
        let (_, halo) = glow_line(0.03, 0.3, dist);
        let (td, _, _) = ray_segment(o, d, tmax, p - scale(1.6, dir), p);
        let trail = (-(td * td) / 0.01).exp();
        scale(fade * seen(t, behind, 0.3), scale(0.35 * halo + 0.35 * trail, vec(0.45, 0.8, 1.0)))
    } else if kind == spell_missile {
        // a small violet bolt, a short trail behind it
        let tail = p - scale(0.05, q);
        let (dist, t, behind) = ray_segment(o, d, tmax, tail, p);
        let (core, halo) = glow_line(0.05, 0.4, dist);
        scale(fade * seen(t, behind, 0.2), scale(4.0 * core, vec(0.95, 0.75, 1.0)) + scale(0.5 * halo, vec(0.6, 0.3, 1.0)))
    } else if kind == spell_falling {
        // a meteor: a great ball of fire, its long tail streaming up behind
        let tail = p - scale(0.2, q);
        let (dist, t, behind) = ray_segment(o, d, tmax, tail, p);
        let (hd, _, _) = ray_segment(o, d, tmax, p, p);
        let (core, _) = glow_line(0.45, 1.0, hd);
        let (trail, halo) = glow_line(0.25, 2.5, dist);
        let flick = 0.85 + 0.15 * (secs * 31.0).sin();
        let hot = (-(hd * hd) / 0.03).exp();
        scale(
            flick * seen(t, behind, 1.0),
            scale(6.0 * core, mix(vec(1.0, 0.4, 0.08), vec(1.0, 0.9, 0.6), hot)) + scale(2.0 * trail, vec(1.0, 0.35, 0.05)) + scale(0.6 * halo, vec(1.0, 0.3, 0.05)),
        )
    } else if kind == spell_well {
        // a gravity well (q: its phase, its reach, seconds since it
        // stopped; see the client's Well): drifting, a small dark heart in
        // tight rings; open, rings of light turning round it, wider as it
        // reaches further, motes falling in; imploding, the rings drawn in,
        // faster and brighter; burst, a shell of light rushing out, fading
        let phase = ftoi(q.x);
        let reach = q.y;
        let since = q.z;
        let (hd, t, behind) = ray_segment(o, d, tmax, p, p);
        if phase == 3 {
            let f = saturate((since - 2.65) / 0.5);
            let shell = (-((hd - reach) * (hd - reach)) / (0.01 + 0.06 * f)).exp();
            let (core, _) = glow_line(0.4 * (1.0 - f), 1.2, hd);
            let fade = 1.0 - smoothstep(0.0, 1.0, reach / 6.0 * 0.5 + 0.5 * f);
            return scale(seen(t, behind, 1.0) * fade, scale(1.8 * shell, vec(0.75, 0.55, 1.0)) + scale(4.0 * core, vec(0.95, 0.85, 1.0)));
        }
        let (size, spin, bright) = if phase == 0 {
            (0.3, 4.0, 0.8)
        } else if phase == 1 {
            (0.35 + 0.09 * reach, 2.0, 1.0)
        } else {
            (0.15 + 0.09 * reach, 7.0, 2.2)
        };
        let rim = (-((hd - 0.3 * size) * (hd - 0.3 * size)) / 0.002).exp();
        let mut rings = 0.0f32;
        let mut j = 0;
        while j < 3 {
            let rr = (0.5 + 0.25 * j as f32) * size;
            let tilt = 0.5 * j as f32 - 0.4;
            let a0 = secs * spin * (1.0 + 0.5 * j as f32) * (if j == 1 { -1.0 } else { 1.0 });
            let point = move |k: i32| {
                let a = a0 + 0.9 * k as f32;
                p + vec(rr * a.cos(), tilt * rr * a.sin(), rr * a.sin())
            };
            let (dd, _, _) = ray_polyline(o, d, tmax, 6, point);
            let (c, h) = glow_line(0.015, 0.12, dd);
            rings = rings + c + 0.2 * h;
            j += 1;
        }
        let mut motes = 0.0f32;
        let mut k = 0;
        while k < 8 {
            let h1 = hash2(seed + k, 31);
            let f = fract(secs * (0.7 + 0.8 * (phase == 2) as i32 as f32) + h1);
            let r = 1.3 * size * (1.0 - f);
            let a = 6.28 * h1 + 3.0 * f;
            let m = p + vec(r * a.cos(), (hash2(seed + k, 32) - 0.5) * r, r * a.sin());
            let (md, _, _) = ray_segment(o, d, tmax, m, m);
            motes += (-(md * md) / 0.0006).exp();
            k += 1;
        }
        scale(bright * seen(t, behind, 0.5), scale(1.4 * rim + 1.2 * rings + 1.5 * motes, vec(0.7, 0.5, 1.0)))
    } else if kind == spell_spirit {
        // his familiar: a pale glowing orb, two wings beating either side
        let (hd, t, behind) = ray_segment(o, d, tmax, p, p);
        let (core, halo) = glow_line(0.08, 0.5, hd);
        let dir = normalise(vec(q.x, 0.0, q.z) + vec(0.0, 0.0, 1e-4));
        let side = normalise(cross(dir, vec(0.0, 1.0, 0.0)));
        let beat = 0.25 * (secs * 9.0).sin();
        let mut wings = 0.0f32;
        let mut s = -1.0f32;
        while s < 1.5 {
            let tip = p + scale(0.35 * s, side) + vec(0.0, 0.15 + beat, 0.0);
            let (wd, _, _) = ray_segment(o, d, tmax, p, tip);
            wings += (-(wd * wd) / 0.0012).exp();
            s += 2.0;
        }
        scale(fade * seen(t, behind, 0.3), scale(3.0 * core + 1.2 * wings, vec(0.9, 0.97, 1.0)) + scale(0.4 * halo, vec(0.55, 0.75, 1.0)))
    } else if kind == spell_chain {
        // lightning leaping on: a jagged arc, flickering, fading fast
        let flick = (secs * 30.0).floor() as i32;
        let point = move |k: i32| {
            let f = k as f32 / 8.0;
            let jit = if k == 0 || k == 8 { 0.0 } else { 0.35 };
            p + scale(f, q - p) + scale(jit, vec(hash3(seed, k, flick) - 0.5, hash3(seed, k, flick + 7) - 0.5, hash3(seed, k, flick + 13) - 0.5))
        };
        let (dd, t, behind) = ray_polyline(o, d, tmax, 8, point);
        let (core, halo) = glow_line(0.02, 0.2, dd);
        scale(fade * seen(t, behind, 0.3), scale(4.0 * core, vec(0.85, 0.92, 1.0)) + scale(0.5 * halo, vec(0.5, 0.65, 1.0)))
    } else if kind == spell_burst {
        // An explosion: a billowing fireball that swells fast and cools
        // from yellow-white through orange to a dull red, a shockwave ring
        // racing out from it, and sparks thrown out and falling.
        let size = 1.0 + saturate(q.x);
        let r = size * (3.2 * (1.0 - (-5.0 * age).exp()) + 0.2);
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
        let ring_r = 9.0 * size * age;
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
            let speed = (7.0 + 6.0 * h(4)) * size;
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
            c = c + scale(k_gain * fade * seen(t, behind, 0.2), scale(3.2 * core, vec(0.8, 0.97, 1.0)) + scale(0.4 * halo, vec(0.3, 0.8, 1.0)));
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
            blades = blades + scale(seen(t, behind, 0.2), scale(3.5 * core, vec(0.8, 0.97, 1.0)) + scale(0.45 * halo, vec(0.3, 0.8, 1.0)));
            j += 1;
        }
        let (gd, gt, gb) = ray_segment(o, d, tmax, p, p);
        let ring = {
            let u = (gd - 5.0 * age) / (0.2 + 0.3 * age);
            (-(u * u)).exp() * (-4.0 * age).exp()
        };
        scale(fade, blades + scale(0.8 * ring * seen(gt, gb, 1.0), vec(0.5, 0.88, 1.0)))
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
    } else if kind == spell_mark || kind == spell_lock {
        // a red ring closing in on where the strike will land, turning,
        // brighter and faster to pulse as it comes; and a thread of light
        // down to it from the sky (his own lock-on: the same, green)
        let green = kind == spell_lock;
        let (c_core, c_halo, c_thread, c_thread_halo) = if green {
            (vec(0.35, 1.0, 0.4), vec(0.15, 1.0, 0.2), vec(0.5, 1.0, 0.5), vec(0.1, 1.0, 0.2))
        } else {
            (vec(1.0, 0.25, 0.3), vec(1.0, 0.1, 0.15), vec(1.0, 0.4, 0.4), vec(1.0, 0.1, 0.1))
        };
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
            scale(seen(t, behind, 0.5), scale(3.0 * core, c_core) + scale(0.25 * halo, c_halo))
                + scale(u * seen(bt, bb, 0.5), scale(1.5 * bcore, c_thread) + scale(0.1 * bhalo, c_thread_halo)),
        )
    } else if kind == spell_heal {
        // healing, channelled: green motes spiralling up round him (p his
        // feet), each with a short trail, and a soft column of light; it
        // comes up as the channelling starts
        let on = smoothstep(0.0, 0.4, age);
        let mut c = splat(0.0);
        let mut j = 0;
        while j < 14 {
            let h = move |n: i32| hash2(j * 13 + n, 57);
            let mote = move |a: f32| {
                let rise = fract(0.45 * a + h(1));
                let ang = 2.0 * PI * h(2) + (2.0 + 1.5 * h(3)) * a;
                let r = 0.85 - 0.35 * rise;
                (p + vec(r * ang.cos(), -0.2 + 2.6 * rise, r * ang.sin()), rise)
            };
            let (now, rise) = mote(age);
            let (then, _) = mote((age - 0.03).maximum(0.0));
            let (dist, t, behind) = ray_segment(o, d, tmax, if (then - now).length() < 0.5 { then } else { now }, now);
            let (core, halo) = glow_line(0.02, 0.08, dist);
            let life_of = smoothstep(0.0, 0.1, rise) * smoothstep(1.0, 0.6, rise);
            c = c + scale(life_of * seen(t, behind, 0.3), scale(2.5 * core, vec(0.6, 1.0, 0.6)) + scale(0.3 * halo, vec(0.3, 1.0, 0.4)));
            j += 1;
        }
        let (cd, ct, cb) = ray_segment(o, d, tmax, p, p + vec(0.0, 2.2, 0.0));
        let column = 0.3 * 0.3 / (cd * cd + 0.3 * 0.3);
        scale(on, c + scale(0.05 * column * seen(ct, cb, 1.0), vec(0.3, 1.0, 0.45)))
    } else {
        splat(0.0)
    }
}

// A frost lance's shard: its centre, axis, half-length and half-width,
// from its slot (its head at p, flying along q, its charge in its seed: see
// the client's cast_frost).
#[inline]
fn shard_of(p: Vec3, q: Vec3, seed: i32) -> (Vec3, Vec3, f32, f32) {
    let power = saturate((seed / 100000) as f32 / 100.0);
    let dir = normalise(q + vec(0.0, 1e-4, 0.0));
    let half = 0.38 * (1.0 + 0.5 * power);
    (p - scale(half - 0.12, dir), dir, half, 0.075 * (1.0 + 0.4 * power))
}

// Where the world-space ray o + t d first meets a frost lance, solid ice,
// nearer than tmax: (t, its normal, its charge), t INFINITY if nowhere.
// Each is a long four-sided crystal (two pyramids, base to base, along its
// flight): the ray tried against its bounding sphere, then sphere-traced
// within it (each lance costs a ray once, not every step of the march).
pub fn frost_hit(spells: &[f32], n: i32, o: Vec3, d: Vec3, tmax: f32) -> (f32, Vec3, f32) {
    let mut best = (f32::INFINITY, vec(0.0, 1.0, 0.0), 0.0);
    let mut i = 0;
    while i < n {
        if ftoi(at(spells, i, 0)) == spell_frost {
            let p = vec(at(spells, i, 1), at(spells, i, 2), at(spells, i, 3));
            let q = vec(at(spells, i, 4), at(spells, i, 5), at(spells, i, 6));
            let seed = ftoi(at(spells, i, 9));
            let (c, ax, half, w) = shard_of(p, q, seed);
            let r = half + w;
            let oc = o - c;
            let b = dot(oc, d);
            let disc = b * b - (dot(oc, oc) - r * r);
            if disc > 0.0 {
                let s = disc.sqrt();
                let (t0, t1) = ((-b - s).maximum(0.0), (-b + s).minimum(tmax.minimum(best.0)));
                // (its frame: across it, u and v)
                let u = normalise(cross(ax, if ax.y.abs() < 0.9 { vec(0.0, 1.0, 0.0) } else { vec(1.0, 0.0, 0.0) }));
                let v = cross(ax, u);
                // (|x| / w + |y| / w + |z| / half - 1, scaled to a lower
                // bound on the distance)
                let k = w / (2.0 + (w / half) * (w / half)).sqrt();
                let mut t = t0;
                let mut j = 0;
                while j < 32 && t < t1 {
                    let x = o + scale(t, d) - c;
                    let (a, bb, z) = (dot(x, u), dot(x, v), dot(x, ax));
                    let f = (a.abs() / w + bb.abs() / w + z.abs() / half - 1.0) * k;
                    if f < 1e-3 {
                        let nrm = normalise(scale(sgn(a) / w, u) + scale(sgn(bb) / w, v) + scale(sgn(z) / half, ax));
                        best = (t, nrm, saturate((seed / 100000) as f32 / 100.0));
                        j = 32;
                    } else {
                        t += f;
                    }
                    j += 1;
                }
            }
        }
        i += 1;
    }
    best
}

// What a frost lance's ice shows where the ray (direction d) meets it, with
// normal nrm and charge `power`: what is behind it, tinted and dimmed through
// it; the sky-ish light its facets catch, more at a glance; a cold light
// within, brighter charged.
pub fn lance_ice(behind: Vec3, d: Vec3, nrm: Vec3, power: f32) -> Vec3 {
    let cos = dot(d, nrm).abs();
    let fres = 0.04 + 0.96 * (1.0 - cos).powf(5.0);
    let rr = reflect(d, nrm);
    let sky = mix(vec(0.05, 0.07, 0.15), vec(0.6, 0.75, 1.0), saturate(0.5 * rr.y + 0.5));
    let glint = 3.0 * saturate(dot(rr, normalise(vec(0.3, 0.8, 0.5)))).powf(40.0);
    let through = behind * vec(0.5, 0.78, 1.0) * 0.85;
    scale(1.0 - fres, through) + scale(fres, sky) + splat(glint) + scale(0.35 * (1.0 + power) * (1.0 - cos), vec(0.4, 0.75, 1.0))
}

#[inline]
fn at(spells: &[f32], i: i32, j: i32) -> f32 {
    unsafe { *spells.get_unchecked((i * spell_stride + j) as usize) }
}

// The light the spells add along the world-space ray o + t d, t in
// [0, tmax] (nearer than whatever the ray hits), dimmed by the fog: the
// first n slots of spells (n at most spells.len() / spell_stride); `orb`
// where his orb is (world), for a charge.
#[inline]
pub fn spell_glow(prm: Params, spells: &[f32], n: i32, secs: f32, o: Vec3, d: Vec3, tmax: f32, orb: Vec3) -> Vec3 {
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
            orb,
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
