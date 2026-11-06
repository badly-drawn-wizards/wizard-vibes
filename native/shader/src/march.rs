// fut/march.fut: scene-independent sphere tracing. Distance functions take
// a tolerance: `dist(tol, p)` must be exact when the true distance is below
// tol, and may otherwise return any lower bound that is at least tol.

use crate::prelude::*;

// Media at a point: a step cap, the radiance emitted per unit length, and
// the extinction per unit length.
#[derive(Clone, Copy)]
pub struct MediaSample {
    pub cap: f32,
    pub radiance: Vec3,
    pub extinction: f32,
}

// Marches up to distance tmax: (distance, material or -1, emission up to the
// hit, transmittance to the hit).
//
// `extended`: the ray starts on a surface (refracted or reflected there):
// its first stretch, while it gets away from that surface (and what it
// grazes, near it), has steps of its own (ESCAPE_STEPS, up to ESCAPE), each
// at least ESCAPE_MIN (unless the media want finer), so it spends none of
// the march's steps doing so.
pub const ESCAPE: f32 = 0.4;
pub const ESCAPE_STEPS: i32 = 64;
pub const ESCAPE_MIN: f32 = 0.004;

pub fn march(scene: impl Fn(f32, Vec3) -> Hit, media: impl Fn(Vec3) -> MediaSample, ro: Vec3, rd: Vec3, tmax: f32, extended: bool) -> (f32, i32, Vec3, f32) {
    let (mut t, mut i, mut m, mut acc, mut tr) = (0.02f32, 0i32, -1i32, splat(0.0), 1f32);
    let (mut e, escaping) = (0i32, extended);
    while i < march_steps && m < 0 && t < tmax && tr > 0.01 {
        // (getting away from the surface it starts on: steps of its own)
        let near = escaping && t < ESCAPE && e < ESCAPE_STEPS;
        let p = ro + scale(t, rd);
        let (d, mat) = scene(0.1, p);
        if d < 0.0004 * t {
            i += 1;
            m = mat;
        } else {
            let s = media(p);
            let dt = (d * 0.85).minimum(s.cap).minimum(tmax - t + 1e-4);
            // (not past what the media need: the lightning in the orb)
            let dt = if near { dt.maximum(ESCAPE_MIN.minimum(s.cap).minimum(tmax - t + 1e-4)) } else { dt };
            acc = acc + scale(dt * tr, s.radiance);
            tr = tr * (-s.extinction * dt).exp();
            t = t + dt;
            if near {
                e += 1;
            } else {
                i += 1;
            }
            m = -1;
        }
    }
    (t, m, acc, tr)
}

// Tetrahedral gradient, as a loop so the scene is inlined once.
pub fn normal(dist: impl Fn(f32, Vec3) -> f32, p: Vec3) -> Vec3 {
    let e = 0.0006;
    let mut n = splat(0.0);
    let mut i = 0;
    while i < 4 {
        let k = if i == 0 {
            vec(1.0, -1.0, -1.0)
        } else if i == 1 {
            vec(-1.0, -1.0, 1.0)
        } else if i == 2 {
            vec(-1.0, 1.0, -1.0)
        } else {
            vec(1.0, 1.0, 1.0)
        };
        n = n + scale(dist(0.1, p + scale(e, k)), k);
        i += 1;
    }
    normalise(n)
}

pub const shadow_k: f32 = 10.0;

// Penumbra-estimating shadow ray towards a light at distance tmax, from t0.
pub fn soft_shadow(dist: impl Fn(f32, Vec3) -> f32, ro: Vec3, rd: Vec3, t0: f32, tmax: f32) -> f32 {
    let k = shadow_k;
    let (mut res, mut t, mut ph, mut i) = (1f32, t0.maximum(0.02), 1e10f32, 0i32);
    while i < shadow_steps && t < tmax && res > 0.002 {
        let h = dist(t / k, ro + scale(t, rd));
        let y = h * h / (2.0 * ph);
        let d = (h * h - y * y).maximum(0.0).sqrt();
        if h < t / k {
            res = res.minimum(k * d / (t - y).maximum(1e-4));
        }
        t = t + clamp(0.01, 0.25, h);
        ph = h;
        i += 1;
    }
    smoothstep(0.0, 1.0, saturate(res))
}

// True if a shadow ray cannot be darkened by occluders inside (c, r).
#[inline]
pub fn shadow_misses(c: Vec3, r: f32, ro: Vec3, rd: Vec3, tmax: f32) -> bool {
    let oc = c - ro;
    let tc = clamp(0.0, tmax, dot(oc, rd));
    length(oc - scale(tc, rd)) - r > tmax / shadow_k
}

// Where a ray leaves the sphere (c, r).
#[inline]
pub fn sphere_exit(c: Vec3, r: f32, ro: Vec3, rd: Vec3) -> f32 {
    let oc = c - ro;
    let b = dot(oc, rd);
    b + (b * b - (dot(oc, oc) - r * r)).maximum(0.0).sqrt()
}

// Where a shadow ray of length tmax may start, for occluders inside (c, r):
// where it enters that sphere grown by tmax/k (or 0 from inside).
#[inline]
pub fn shadow_start(c: Vec3, r: f32, ro: Vec3, rd: Vec3, tmax: f32) -> f32 {
    let R = r + tmax / shadow_k;
    let oc = c - ro;
    let b = dot(oc, rd);
    b - (b * b - (dot(oc, oc) - R * R)).maximum(0.0).sqrt()
}

// Probes along the normal.
pub fn ambient_occlusion(dist: impl Fn(f32, Vec3) -> f32, p: Vec3, n: Vec3) -> f32 {
    let (mut occ, mut sca) = (0f32, 1f32);
    let mut i = 0;
    while i < 5 {
        let h = 0.01 + 0.12 * i as f32 / 4.0;
        let d = dist(0.13, p + scale(h, n));
        occ = occ + (h - d).maximum(0.0) * sca;
        sca = sca * 0.95;
        i += 1;
    }
    saturate(1.0 - 3.0 * occ)
}
