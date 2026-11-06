// fut/glass.fut: glass of any shape: Fresnel reflection and refraction at
// surfaces given as signed distance functions (negative inside). The orb (a
// sphere, met analytically), the potions' flasks and his shield all go
// through here.

use crate::prelude::*;

// Refraction of i at a surface with normal n facing against i, for the ratio
// eta of refractive indices (from / to). Reflects on total internal reflection.
#[inline]
pub fn refract(i: Vec3, n: Vec3, eta: f32) -> Vec3 {
    let cosi = -(dot(n, i));
    let k = 1.0 - eta * eta * (1.0 - cosi * cosi);
    if k < 0.0 { reflect(i, n) } else { scale(eta, i) + scale(eta * cosi - k.sqrt(), n) }
}

// Schlick's approximation of the reflected fraction.
#[inline]
pub fn fresnel(ior: f32, cosi: f32) -> f32 {
    let r0 = ((1.0 - ior) / (1.0 + ior)).powf(2.0);
    r0 + (1.0 - r0) * pow(1.0 - saturate(cosi), 5.0)
}

// A ray along rd meeting a surface of normal n (either way round) into a
// medium of index ior (from air; pass 1 / ior for the way out): the normal
// turned to face the ray, the reflected fraction, and the reflected and
// refracted directions.
#[derive(Clone, Copy)]
pub struct Interface {
    pub n: Vec3,
    pub f: f32,
    pub reflected: Vec3,
    pub refracted: Vec3,
}

pub fn fresnel_at(ior: f32, rd: Vec3, n: Vec3) -> Interface {
    let n = if dot(n, rd) > 0.0 { scale(-1.0, n) } else { n };
    let cosi = saturate(-(dot(n, rd)));
    // (leaving a denser medium, Schlick needs the angle on the denser side)
    let f = if ior >= 1.0 {
        fresnel(ior, cosi)
    } else {
        let s2 = (1.0 - cosi * cosi) / (ior * ior);
        if s2 >= 1.0 { 1.0 } else { fresnel(1.0 / ior, (1.0 - s2).sqrt()) }
    };
    Interface { n, f, reflected: reflect(rd, n), refracted: refract(rd, n, 1.0 / ior) }
}

// Where the ray ro + t rd first meets glass `sdf` for t in [t0, t1], from
// the side it starts on (outside if sdf > 0 there): (whether it does, t,
// the surface normal there). Sphere-traced, in at most `steps` steps;
// callers bound the stretch [t0, t1] tightly (e.g. by a bounding sphere).
//
// `escape`: the ray starts on the glass (just through it, or just out of
// it), which sphere tracing would crawl away from in ever so small steps
// (on a grazing ray, for all its steps): it first gets away from it with
// steps of its own (GLASS_ESCAPE_STEPS), each at least GLASS_ESCAPE_MIN,
// not meeting the glass again until GLASS_ESCAPE from where it started.
// (For convex glass, as the orbs are: it cannot meet the same surface
// again so soon.)
pub const GLASS_ESCAPE: f32 = 0.01;
pub const GLASS_ESCAPE_STEPS: i32 = 32;
pub const GLASS_ESCAPE_MIN: f32 = 0.004;

pub fn glass_hit(sdf: impl Fn(Vec3) -> f32, steps: i32, ro: Vec3, rd: Vec3, t0: f32, t1: f32, escape: bool) -> (bool, f32, Vec3) {
    let side = if sdf(ro + scale(t0, rd)) < 0.0 { -1f32 } else { 1.0 };
    let (mut t, mut i, mut e, mut hit) = (t0, 0i32, 0i32, false);
    while !hit && i < steps && t < t1 {
        let d = side * sdf(ro + scale(t, rd));
        let leaving = escape && e < GLASS_ESCAPE_STEPS && (t - t0 < GLASS_ESCAPE || d < GLASS_ESCAPE_MIN);
        if leaving && t - t0 < GLASS_ESCAPE {
            // (still by the surface it started on: on, regardless)
            t = t + (0.9 * d).maximum(GLASS_ESCAPE_MIN);
            e += 1;
        } else if d < 0.0004 * t + 1e-4 {
            i += 1;
            hit = true;
        } else {
            t = t + 0.9 * d;
            if leaving {
                e += 1;
            } else {
                i += 1;
            }
            hit = false;
        }
    }
    if !hit {
        (false, t, vec(0.0, 1.0, 0.0))
    } else {
        // (onto the surface itself: within the tolerance short of it, a ray
        // carrying on through from here would start on the wrong side)
        let t = t + side * sdf(ro + scale(t, rd));
        let p = ro + scale(t, rd);
        (true, t, normal(|_, q| sdf(q), p))
    }
}
