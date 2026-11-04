// fut/wizard.fut (the per-pixel part): camera, tone mapping, and the two
// functions the entry point calls, shade_pixel and finish_pixel.

use crate::prelude::*;

#[inline]
pub fn aces(x: f32) -> f32 {
    saturate((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14))
}

pub const default_target: Vec3 = Vec3::new(0.0, 1.15, 0.0);

// Camera orbiting a target: the ray through image point (x, y), in pixels
// from its top left.
pub fn camera_ray(target: Vec3, vw: i32, h: i32, yaw: f32, pitch: f32, dst: f32, x: f32, y: f32) -> (Vec3, Vec3) {
    let ro = target + scale(dst, vec(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()));
    let ro = vec(ro.x, ro.y.maximum(0.1), ro.z);
    let fw = normalise(target - ro);
    let rt = normalise(cross(fw, vec(0.0, 1.0, 0.0)));
    let up = cross(rt, fw);
    let fh = h as f32;
    let u = (x - 0.5 * vw as f32) / fh * 2.0;
    let v = (0.5 * fh - y) / fh * 2.0;
    let rd = normalise(scale(2.414, fw) + scale(u, rt) + scale(v, up));
    (ro, rd)
}

// The colour (before tone mapping) seen through image point (x, y), and
// how far the ray sees (infinity: the sky).
#[inline(never)]
pub fn shade_pixel(prm: Params, dr: Drive, target: Vec3, vw: i32, h: i32, tm: f32, yaw: f32, pitch: f32, dst: f32, x: f32, y: f32) -> (Vec3, f32) {
    let (ro, rd) = camera_ray(target, vw, h, yaw, pitch, dst, x, y);
    shade(prm, dr, tm, 2.0 / (h as f32 * 2.414), ro, rd)
}

// Tone mapping, gamma and vignette, for image point (x, y).
#[inline(never)]
pub fn finish_pixel(prm: Params, vw: i32, h: i32, x: f32, y: f32, c: Vec3) -> Vec3 {
    let c = if prm.view != 0 { c } else { vmap(aces, scale(prm.exposure, c)) };
    let c = vmap(|v| v.powf(1.0 / 2.2), c);
    let ux = x / vw as f32 - 0.5;
    let uy = y / h as f32 - 0.5;
    let vig = if prm.view != 0 { 1.0 } else { 1.0 - prm.vignette * (ux * ux + uy * uy) };
    scale(vig, c)
}

// The final colour seen through image point (x, y), with the first
// n_spells spells' glow, and how far the ray sees.
#[inline]
pub fn sample(prm: Params, dr: Drive, spells: &[f32], n_spells: i32, target: Vec3, vw: i32, h: i32, tm: f32, yaw: f32, pitch: f32, dst: f32, x: f32, y: f32) -> (Vec3, f32) {
    let (c, seen) = shade_pixel(prm, dr, target, vw, h, tm, yaw, pitch, dst, x, y);
    // the spells shine in front of whatever the ray sees
    let (ro, rd) = camera_ray(target, vw, h, yaw, pitch, dst, x, y);
    let fl = flight_of(prm, dr, tm);
    let c = c + spell_glow(prm, spells, n_spells, tm, to_world(fl, ro), to_world_dir(fl, rd), seen);
    (finish_pixel(prm, vw, h, x, y, c), seen)
}

#[inline]
pub fn luma(c: Vec3) -> f32 {
    0.299 * c.x + 0.587 * c.y + 0.114 * c.z
}

// ---------------------------------------------------------------------
// Anti-aliasing, as passes over the finished image: h rows of w pixels,
// pixel (x, y) at img[y * w + x], its finished colour and (in w) the
// distance its ray sees.

// Pixel (x, y), clamped to the image.
#[inline]
pub fn img_at(img: &[Vec4], h: i32, w: i32, y: i32, x: i32) -> Vec4 {
    img[(0i32.max((h - 1).min(y)) * w + 0i32.max((w - 1).min(x))) as usize]
}

#[inline]
fn at3(img: &[Vec4], h: i32, w: i32, y: i32, x: i32) -> Vec3 {
    img_at(img, h, w, y, x).truncate()
}
#[inline]
fn at2(img: &[Vec4], h: i32, w: i32, y: i32, x: i32) -> (Vec3, f32) {
    let v = img_at(img, h, w, y, x);
    (v.truncate(), v.w)
}

// FXAA, simplified (no search along the edge): where the local contrast
// exceeds the threshold, find whether the edge runs across or along, and
// blend towards the neighbour across it, by how much the pixel stands out
// from its neighbourhood. Pixel (x, y) of fxaa's output.
#[inline]
pub fn fxaa_at(threshold: f32, h: i32, w: i32, img: &[Vec4], y: i32, x: i32) -> Vec3 {
    let c = at3(img, h, w, y, x);
    let (n, s, e, wst) = (at3(img, h, w, y - 1, x), at3(img, h, w, y + 1, x), at3(img, h, w, y, x + 1), at3(img, h, w, y, x - 1));
    let (ln, ls, le, lw, lc) = (luma(n), luma(s), luma(e), luma(wst), luma(c));
    let hi = lc.maximum(ln.maximum(ls).maximum(le.maximum(lw)));
    let lo = lc.minimum(ln.minimum(ls).minimum(le.minimum(lw)));
    let range = hi - lo;
    if range < threshold.maximum(0.125 * hi) {
        return c;
    }
    let (lne, lnw, lse, lsw) = (luma(at3(img, h, w, y - 1, x + 1)), luma(at3(img, h, w, y - 1, x - 1)), luma(at3(img, h, w, y + 1, x + 1)), luma(at3(img, h, w, y + 1, x - 1)));
    // an edge running along x has its contrast across y
    let along_x = (ln + ls - 2.0 * lc).abs() * 2.0 + (lne + lse - 2.0 * le).abs() + (lnw + lsw - 2.0 * lw).abs();
    let along_y = (le + lw - 2.0 * lc).abs() * 2.0 + (lne + lnw - 2.0 * ln).abs() + (lse + lsw - 2.0 * ls).abs();
    let horizontal = along_x >= along_y;
    let (a, la, b, lb) = if horizontal { (n, ln, s, ls) } else { (wst, lw, e, le) };
    let other = if (la - lc).abs() >= (lb - lc).abs() { a } else { b };
    let avg = (2.0 * (ln + ls + le + lw) + lne + lnw + lse + lsw) / 12.0;
    let sub = smoothstep(0.0, 1.0, saturate((avg - lc).abs() / range));
    mix(c, other, 0.5 * sub * sub + 0.15)
}

// Supersampling at edges: pixels on a geometric edge (where the distance
// the ray sees jumps by over 4% from a neighbour's, or between something
// and the sky) or of high contrast in luma (stars, glows) are shaded again
// with four more rays on a rotated grid and averaged with their first
// sample. Whether pixel (x, y) is one (img: colour and distance seen).
#[inline]
pub fn edge_at(threshold: f32, h: i32, w: i32, img: &[Vec4], y: i32, x: i32) -> bool {
    let (c, t) = at2(img, h, w, y, x);
    let lc = luma(c);
    let differs = |(cn, tn): (Vec3, f32)| {
        (luma(cn) - lc).abs() > threshold
            || (if isinf(t) || isinf(tn) { isinf(t) != isinf(tn) } else { (t - tn).abs() > 0.04 * t.minimum(tn) })
    };
    differs(at2(img, h, w, y - 1, x)) || differs(at2(img, h, w, y + 1, x)) || differs(at2(img, h, w, y, x - 1)) || differs(at2(img, h, w, y, x + 1))
}

// Extra ray k (0..3) of an edge pixel: its offset from the pixel's centre.
#[inline]
pub fn edge_offset(k: i32) -> (f32, f32) {
    if k == 0 {
        (0.125, -0.375)
    } else if k == 1 {
        (0.375, 0.125)
    } else if k == 2 {
        (-0.125, 0.375)
    } else {
        (-0.375, -0.125)
    }
}
