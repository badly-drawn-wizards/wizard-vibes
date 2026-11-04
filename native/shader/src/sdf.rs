// fut/sdf.fut: signed distance primitives and operators (after Inigo
// Quilez).

use crate::prelude::*;

#[inline]
pub fn sd_sphere(p: Vec3, r: f32) -> f32 {
    length(p) - r
}

#[inline]
pub fn sd_box(p: Vec3, b: Vec3) -> f32 {
    let q = vabs(p) - b;
    length(vmaxs(q, 0.0)) + maxc(q).minimum(0.0)
}

#[inline]
pub fn sd_capsule(p: Vec3, a: Vec3, b: Vec3, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = saturate(dot(pa, ba) / dot(ba, ba));
    length(pa - scale(h, ba)) - r
}

// Cone frustum: radius r1 at y=0, r2 at y=h.
pub fn sd_capped_cone(p: Vec3, h: f32, r1: f32, r2: f32) -> f32 {
    let hh = h * 0.5;
    let qx = length2(vec2(p.x, p.z));
    let qy = p.y - hh;
    let k1x = r2;
    let k1y = hh;
    let k2x = r2 - r1;
    let k2y = 2.0 * hh;
    let cax = qx - qx.minimum(if qy < 0.0 { r1 } else { r2 });
    let cay = qy.abs() - hh;
    let t = saturate(((k1x - qx) * k2x + (k1y - qy) * k2y) / (k2x * k2x + k2y * k2y));
    let cbx = qx - k1x + k2x * t;
    let cby = qy - k1y + k2y * t;
    let s = if cbx < 0.0 && cay < 0.0 { -1.0 } else { 1.0 };
    s * (cax * cax + cay * cay).minimum(cbx * cbx + cby * cby).sqrt()
}

// Torus in the xz plane.
#[inline]
pub fn sd_torus(p: Vec3, big: f32, small: f32) -> f32 {
    length2(vec2(length2(vec2(p.x, p.z)) - big, p.y)) - small
}

// Vertical cylinder of radius r and half-height h.
#[inline]
pub fn sd_cylinder(p: Vec3, r: f32, h: f32) -> f32 {
    let dx = length2(vec2(p.x, p.z)) - r;
    let dy = p.y.abs() - h;
    dx.maximum(dy).minimum(0.0) + length2(vec2(dx.maximum(0.0), dy.maximum(0.0)))
}

// Approximate (not exact) ellipsoid distance.
#[inline]
pub fn sd_ellipsoid(p: Vec3, r: Vec3) -> f32 {
    let k0 = length(vec(p.x / r.x, p.y / r.y, p.z / r.z));
    let k1 = length(vec(p.x / (r.x * r.x), p.y / (r.y * r.y), p.z / (r.z * r.z)));
    k0 * (k0 - 1.0) / k1
}

#[inline]
pub fn smin(k: f32, a: f32, b: f32) -> f32 {
    let h = (k - (a - b).abs()).maximum(0.0) / k;
    a.minimum(b) - h * h * k * 0.25
}

#[inline]
pub fn smax(k: f32, a: f32, b: f32) -> f32 {
    -smin(k, -a, -b)
}

// Bend space around the z axis as y increases (the droopy hat tip).
#[inline]
pub fn bend(k: f32, p: Vec3) -> Vec3 {
    let a = k * p.y * p.y;
    let r = rot2(a, vec2(p.x, p.y));
    vec(r.x, r.y, p.z)
}

// Fold the xz plane into one of n angular sectors; returns local point.
#[inline]
pub fn rep_angle(n: f32, p: Vec3) -> Vec3 {
    let sector = 2.0 * PI / n;
    let a = p.z.atan2(p.x);
    let a2 = (fract(a / sector + 0.5) - 0.5) * sector;
    let r = length2(vec2(p.x, p.z));
    vec(r * a2.cos(), p.y, r * a2.sin())
}

// Distance paired with a material id; umin keeps the closer one.
pub type Hit = (f32, i32);

#[inline]
pub fn umin(a: Hit, b: Hit) -> Hit {
    if a.0 <= b.0 { a } else { b }
}

// Cone with rounded ends between points a (radius ra) and b (radius rb).
pub fn sd_round_cone_ab(p: Vec3, a: Vec3, b: Vec3, ra: f32, rb: f32) -> f32 {
    let ba = b - a;
    let l2 = dot(ba, ba);
    let rr = ra - rb;
    let a2 = l2 - rr * rr;
    let il2 = 1.0 / l2;
    let pa = p - a;
    let y = dot(pa, ba);
    let z = y - l2;
    let xv = scale(l2, pa) - scale(y, ba);
    let x2 = dot(xv, xv);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = sgn(rr) * rr * rr * x2;
    if sgn(z) * a2 * z2 > k {
        (x2 + z2).sqrt() * il2 - rb
    } else if sgn(y) * a2 * y2 < k {
        (x2 + y2).sqrt() * il2 - ra
    } else {
        ((x2 * a2 * il2).sqrt() + y * rr) * il2 - ra
    }
}

// Where the ray ro + t rd meets the sphere (c, r): (entry, exit), with
// entry > exit if it misses.
#[inline]
pub fn ray_sphere(c: Vec3, r: f32, ro: Vec3, rd: Vec3) -> (f32, f32) {
    let oc = c - ro;
    let b = dot(oc, rd);
    let disc = b * b - (dot(oc, oc) - r * r);
    if disc <= 0.0 {
        (INF, -INF)
    } else {
        let s = disc.sqrt();
        (b - s, b + s)
    }
}

// Bounding spheres: farther than `margin` outside (c, r), the distance to
// the sphere is a safe lower bound.
#[inline]
pub fn sphere_bounded(c: Vec3, r: f32, margin: f32, exact: impl Fn(Vec3) -> f32, p: Vec3) -> f32 {
    let b = length(p - c) - r;
    if b > margin { b } else { exact(p) }
}

#[inline]
pub fn sphere_bounded_hit(c: Vec3, r: f32, margin: f32, m: i32, exact: impl Fn(Vec3) -> Hit, p: Vec3) -> Hit {
    let b = length(p - c) - r;
    if b > margin { (b, m) } else { exact(p) }
}
