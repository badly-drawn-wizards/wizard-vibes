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
