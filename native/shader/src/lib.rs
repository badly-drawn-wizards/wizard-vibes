// The wizard scene's per-pixel code, ported from Futhark (fut/) to Rust
// for rust-gpu: one module per Futhark module, under the same names (see
// PORTING.md). The entry point native_main does what dev/native.ts's WGSL
// entry point does: a thread a pixel, shade_pixel and finish_pixel, packed
// BGRA8 into a buffer. The others (see ENTRIES.md) render a frame as
// fut/wizard.fut's render_frame does (with the spells and anti-aliasing),
// and compute the music (fut/music.fut). The crate also builds for the
// host, where the app calls the collision probe (collide.rs) and the sound
// effects (sfx.rs, host only).

#![cfg_attr(target_arch = "spirv", no_std, feature(asm_experimental_arch))]
#![feature(float_minimum_maximum)]
#![allow(non_upper_case_globals, non_snake_case, clippy::too_many_arguments, clippy::excessive_precision)]

pub mod anim;
pub mod chip;
pub mod collide;
pub mod enemies;
pub mod figure;
pub mod foes;
// (from params.json: see wizard-params-macro)
pub mod generated {
    wizard_params_macro::params!();
}
pub mod glass;
pub mod hash;
pub mod lightning;
pub mod march;
pub mod materials;
pub mod media;
pub mod music;
pub mod orbfx;
pub mod platform;
pub mod prelude;
pub mod rig;
pub mod room;
pub mod scene;
pub mod sdf;
#[cfg(not(target_arch = "spirv"))]
pub mod sfx;
pub mod shading;
pub mod soundboard;
pub mod spells;
pub mod textures;
pub mod vec;
pub mod wizard;
pub mod world;

use prelude::*;
use spirv_std::glam::UVec3;
use spirv_std::memory::{Scope, Semantics};
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

// ---------------------------------------------------------------------
// A frame, as render_frame (fut/wizard.fut): shade, then (prm.aa) FXAA or
// edge supersampling, packed into `out` as native_main packs it. Buffers
// (see ENTRIES.md): colour holds each pixel's finished colour and the
// distance its ray sees, vw wide (pixel (x, y) at y * vw + x); edges holds
// the number of edge pixels found, then their indices into colour; rays
// one colour per extra ray (4 an edge pixel); args the indirect dispatch
// sizes of edge_rays and edge_resolve.

#[inline]
fn pack(c: Vec3) -> u32 {
    byte(c.z) | (byte(c.y) << 8) | (byte(c.x) << 16) | 0xff000000
}

// The colour at image point (x, y), with the spells (sample).
#[inline]
fn sample_at(u: &Uniforms, spells: &[f32], x: f32, y: f32) -> (Vec3, f32) {
    let (prm, dr) = unpack(u);
    let n = u.a_n_spells.min(sdiv(spells.len() as i32, spell_stride));
    sample(prm, dr, spells, n, default_target, u.a_vw, u.a_h, u.a_tm, u.a_yaw, u.a_pitch, u.a_dst, x, y)
}

// How many edge pixels the later passes handle: as many as were found, up
// to what the edges and rays buffers hold.
#[inline]
fn edge_count(edges: &[u32], rays: &[Vec4]) -> u32 {
    edges[0].min(edges.len() as u32 - 1).min(rays.len() as u32 / 4)
}

// Pass 1: a thread per pixel of out (w * h): shades the pixel into colour,
// and with no anti-aliasing (aa not 1 or 2) packs it into out too.
#[spirv(compute(threads(256)))]
pub fn shade(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] out: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] colour: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] spells: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] edges: &mut [u32],
) {
    let i = gid.x as i32;
    if i >= u.a_w * u.a_h {
        return;
    }
    let plain = u.p_aa != 1 && u.p_aa != 2;
    if i == 0 && u.p_aa == 2 {
        edges[0] = 0;
    }
    let px = srem(i, u.a_w);
    let py = sdiv(i, u.a_w);
    if px >= u.a_vw {
        if plain {
            out[i as usize] = 0xff000000;
        }
        return;
    }
    let (c, seen) = sample_at(u, spells, px as f32 + 0.5, py as f32 + 0.5);
    colour[(py * u.a_vw + px) as usize] = vec4(c.x, c.y, c.z, seen);
    if plain {
        out[i as usize] = pack(c);
    }
}

// Pass 2 with aa = 1: a thread per pixel of out (w * h): FXAA over colour,
// packed into out.
#[spirv(compute(threads(256)))]
pub fn fxaa(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] out: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] colour: &mut [Vec4],
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
    let py = sdiv(i, u.a_w);
    let c = fxaa_at(u.p_aa_threshold, u.a_h, u.a_vw, colour, py, px);
    out[i as usize] = pack(c);
}

// Edge supersampling (aa = 2), pass 2: a thread per pixel of the image
// (vw * h): appends the edge pixels' indices to edges.
#[spirv(compute(threads(256)))]
pub fn edge_mark(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] colour: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] edges: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] rays: &mut [Vec4],
) {
    let i = gid.x as i32;
    let vw = u.a_vw;
    if i >= vw * u.a_h {
        return;
    }
    if edge_at(3.0 * u.p_aa_threshold, u.a_h, vw, colour, sdiv(i, vw), srem(i, vw)) {
        let cap = (edges.len() as u32 - 1).min(rays.len() as u32 / 4);
        let k = unsafe { spirv_std::arch::atomic_i_add::<u32, { Scope::QueueFamily as u32 }, { Semantics::NONE.bits() }>(&mut edges[0], 1) };
        if k < cap {
            edges[(k + 1) as usize] = i as u32;
        }
    }
}

// Pass 3: one thread (dispatch 1 workgroup): the indirect dispatch sizes
// (x, y, z) for edge_rays (args[0..3]: a thread per extra ray) and
// edge_resolve (args[3..6]: a thread per edge pixel).
#[spirv(compute(threads(256)))]
pub fn edge_args(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] edges: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] rays: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 6)] args: &mut [u32],
) {
    if gid.x != 0 {
        return;
    }
    let n = edge_count(edges, rays);
    args[0] = (4 * n + 255) >> 8;
    args[1] = 1;
    args[2] = 1;
    args[3] = (n + 255) >> 8;
    args[4] = 1;
    args[5] = 1;
}

// Pass 4: a thread per extra ray (4 an edge pixel; dispatch args[0..3], or
// enough for the most edges the buffers hold): shades it into rays.
#[spirv(compute(threads(256)))]
pub fn edge_rays(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] spells: &[f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] edges: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] rays: &mut [Vec4],
) {
    let k = gid.x;
    if k >= 4 * edge_count(edges, rays) {
        return;
    }
    let i = edges[(1 + (k >> 2)) as usize] as i32;
    let (dx, dy) = edge_offset((k & 3) as i32);
    let vw = u.a_vw;
    let (c, _) = sample_at(u, spells, srem(i, vw) as f32 + 0.5 + dx, sdiv(i, vw) as f32 + 0.5 + dy);
    rays[k as usize] = vec4(c.x, c.y, c.z, 0.0);
}

// Pass 5: a thread per edge pixel (dispatch args[3..6], or enough for the
// most edges the buffers hold): averages its four extra rays with its first
// sample, in colour.
#[spirv(compute(threads(256)))]
pub fn edge_resolve(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] colour: &mut [Vec4],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] edges: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] rays: &mut [Vec4],
) {
    let j = gid.x;
    if j >= edge_count(edges, rays) {
        return;
    }
    let i = edges[(1 + j) as usize] as usize;
    let k = (4 * j) as usize;
    let c = colour[i];
    let m = scale(0.2, rays[k].truncate() + rays[k + 1].truncate() + rays[k + 2].truncate() + rays[k + 3].truncate() + c.truncate());
    colour[i] = vec4(m.x, m.y, m.z, c.w);
}

// Pass 6 (after edge_resolve): a thread per pixel of out (w * h): packs
// colour into out.
#[spirv(compute(threads(256)))]
pub fn pack_pixels(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] out: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] colour: &mut [Vec4],
) {
    let i = gid.x as i32;
    if i >= u.a_w * u.a_h {
        return;
    }
    let px = srem(i, u.a_w);
    out[i as usize] = if px >= u.a_vw { 0xff000000 } else { pack(colour[(sdiv(i, u.a_w) * u.a_vw + px) as usize].truncate()) };
}

// ---------------------------------------------------------------------
// The music (fut/music.fut sound_chunk): a thread per value, 2 * a_m_n
// values: a_m_n stereo samples (interleaved left, right) from time a_m_t0
// within the loop, at a_m_rate samples a second.
#[spirv(compute(threads(256)))]
pub fn music(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 8)] dry: &mut [f32],
) {
    // (a thread a frame: the mix before the limit, the harpsichord, and
    // the room's send)
    let i = gid.x as i32;
    if i >= u.a_m_n {
        return;
    }
    let (l, r, h, s) = music::song_parts(u.a_m_song, u.a_m_rate, music::song_wrap(u.a_m_song, u.a_m_t0 + i as f32 / u.a_m_rate));
    let k = 4 * i as usize;
    dry[k] = l;
    dry[k + 1] = r;
    dry[k + 2] = h;
    dry[k + 3] = s;
}

// The soundboard over a chunk: a thread a mode, through the chunk's
// frames in turn, from the state the last chunk left in its slot (chunk
// a_m_chunk reads slot (a_m_chunk - 1) % 2, writes slot a_m_chunk % 2, and
// the first chunk starts from rest), so a chunk asked for again (its
// readback lost) starts from the same state.
#[spirv(compute(threads(256)))]
pub fn soundboard(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 8)] dry: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] modes_out: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 10)] state: &mut [f32],
) {
    let m = gid.x as usize;
    if m >= soundboard::modes {
        return;
    }
    let (a1, a2, b, _, _) = soundboard::mode_coeffs(m, u.a_m_rate);
    let slot = 2 * soundboard::modes;
    let (inp, outp) = if srem(u.a_m_chunk, 2) == 0 { (slot, 0) } else { (0, slot) };
    let (mut y1, mut y2) = if u.a_m_chunk == 0 { (0.0, 0.0) } else { (state[inp + 2 * m], state[inp + 2 * m + 1]) };
    let n = u.a_m_n as usize;
    let mut i = 0;
    while i < n {
        let y = a1 * y1 - a2 * y2 + b * dry[4 * i + 2];
        y2 = y1;
        y1 = y;
        modes_out[m * n + i] = y;
        i += 1;
    }
    state[outp + 2 * m] = y1;
    state[outp + 2 * m + 1] = y2;
}

// The music's chunk: the mix with the soundboard's modes panned into it
// (left, right, before the limit) and the room's send, 3 floats a frame, a
// thread a frame. (The room, sequential through its delay lines, runs on
// the host, which finishes the chunk: see music::finish_chunk.)
#[spirv(compute(threads(256)))]
pub fn music_mix(
    #[spirv(global_invocation_id)] gid: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] u: &Uniforms,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 7)] samples: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 8)] dry: &mut [f32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] modes_out: &mut [f32],
) {
    let i = gid.x as usize;
    let n = u.a_m_n as usize;
    if i >= n {
        return;
    }
    let (mut l, mut r) = (dry[4 * i], dry[4 * i + 1]);
    let mut m = 0;
    while m < soundboard::modes {
        let (_, _, _, pl, pr) = soundboard::mode_coeffs(m, u.a_m_rate);
        let y = modes_out[m * n + i];
        l += pl * y;
        r += pr * y;
        m += 1;
    }
    samples[3 * i] = l;
    samples[3 * i + 1] = r;
    samples[3 * i + 2] = dry[4 * i + 3];
}
