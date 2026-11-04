// The wizard scene's per-pixel code, ported from Futhark (fut/) to Rust
// for rust-gpu: one module per Futhark module, under the same names (see
// PORTING.md). The entry point, native_main, does what dev/native.ts's
// WGSL entry point does: a thread a pixel, shade_pixel and finish_pixel,
// packed BGRA8 into a buffer.

#![cfg_attr(target_arch = "spirv", no_std, feature(asm_experimental_arch))]
#![feature(float_minimum_maximum)]
#![allow(non_upper_case_globals, non_snake_case, clippy::too_many_arguments, clippy::excessive_precision)]

pub mod anim;
pub mod enemies;
pub mod figure;
pub mod foes;
pub mod generated;
pub mod glass;
pub mod hash;
pub mod lightning;
pub mod march;
pub mod materials;
pub mod media;
pub mod orbfx;
pub mod platform;
pub mod prelude;
pub mod rig;
pub mod scene;
pub mod sdf;
pub mod shading;
pub mod textures;
pub mod vec;
pub mod wizard;
pub mod world;

use prelude::*;
use spirv_std::glam::UVec3;
use spirv_std::spirv;

#[inline]
fn byte(v: f32) -> u32 {
    (saturate(v) * 255.0 + 0.5) as u32
}

#[spirv(compute(threads(256)))]
pub fn native_main(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] out: &mut [u32],
) {
    let i = gid.x as i32;
    if i >= u.a_w * u.a_h {
        return;
    }
    let px = srem(i, u.a_w);
    if px >= u.a_vw {
        out[i as usize] = 0xff000000;
        return;
    }
    let x = px as f32 + 0.5;
    let y = sdiv(i, u.a_w) as f32 + 0.5;
    let (prm, dr) = unpack(u);
    let (c, _seen) = shade_pixel(prm, dr, default_target, u.a_vw, u.a_h, u.a_tm, u.a_yaw, u.a_pitch, u.a_dst, x, y);
    let f = finish_pixel(prm, u.a_vw, u.a_h, x, y, c);
    out[i as usize] = byte(f.z) | (byte(f.y) << 8) | (byte(f.x) << 16) | 0xff000000;
}
