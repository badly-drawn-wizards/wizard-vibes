// The room: a reverb that the clap, the strings, the lead and a little of
// the harpsichord are sent to. Freeverb's (Jezar at Dreampoint, public
// domain): per side, eight damped feedback combs in parallel, then four
// allpasses in series, the right side's delays 23 samples longer, all
// scaled from 44.1 kHz to the rate.
//
// Its state (each delay line, its position, each comb's damping filter)
// carries from one chunk of music to the next, in two slots as the
// soundboard's does: chunk c reads slot (c - 1) % 2 and writes slot c % 2
// (the first chunk starts from silence), so a chunk asked for again starts
// from the same state. It runs on the host (music::finish_chunk), after
// the GPU's part of the chunk: sequential through its delay lines, it is
// cheap on a CPU and slow on a GPU, where a thread a comb took 30 ms a
// half-second chunk, holding up the frame behind it.

use crate::prelude::*;

pub const combs: usize = 16; // 8 a side
pub const allpasses: usize = 8; // 4 a side
// (room for the delays at up to 96 kHz)
const comb_ring: usize = 4096;
const allpass_ring: usize = 2048;
// a comb's state: its ring, its filter's memory, its position
const comb_size: usize = comb_ring + 2;
const allpass_size: usize = allpass_ring + 1;
pub const slot: usize = combs * comb_size + allpasses * allpass_size;

const feedback: f32 = 0.84; // (Freeverb's room size 0.5)
const damp: f32 = 0.2;
const input_gain: f32 = 0.015;
const allpass_feedback: f32 = 0.5;

#[inline]
fn scaled(samples_44k: i32, rate: f32) -> usize {
    ftoi(samples_44k as f32 * rate / 44100.0) as usize
}

// Comb k's delay (k = side * 8 + c).
#[inline]
pub fn comb_len(k: usize, rate: f32) -> usize {
    let c = if k >= 8 { k - 8 } else { k };
    let base = if c == 0 {
        1116
    } else if c == 1 {
        1188
    } else if c == 2 {
        1277
    } else if c == 3 {
        1356
    } else if c == 4 {
        1422
    } else if c == 5 {
        1491
    } else if c == 6 {
        1557
    } else {
        1617
    };
    scaled(base + if k >= 8 { 23 } else { 0 }, rate)
}

// Allpass j of a side's delay (j = 0..3).
#[inline]
pub fn allpass_len(side: usize, j: usize, rate: f32) -> usize {
    let base = if j == 0 {
        556
    } else if j == 1 {
        441
    } else if j == 2 {
        341
    } else {
        225
    };
    scaled(base + if side == 1 { 23 } else { 0 }, rate)
}

// Where chunk `chunk`'s state comes from and goes (offsets of the slots).
#[inline]
fn slots(chunk: i32) -> (usize, usize) {
    if srem(chunk, 2) == 0 { (slot, 0) } else { (0, slot) }
}

// Comb k through a chunk of n frames: its input, the send (every `stride`th
// value of `dry` from `offset`), into its row of `out` (k * n ..).
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn comb_run(k: usize, rate: f32, n: usize, chunk: i32, dry: &[f32], stride: usize, offset: usize, state: &mut [f32], out: &mut [f32]) {
    let (inp, outp) = slots(chunk);
    let base = k * comb_size;
    let len = comb_len(k, rate);
    // (the state carried on: copied into this chunk's slot, or cleared)
    let mut i = 0;
    while i < comb_size {
        state[outp + base + i] = if chunk == 0 { 0.0 } else { state[inp + base + i] };
        i += 1;
    }
    let ring = outp + base;
    let mut store = state[ring + comb_ring];
    let mut pos = ftoi(state[ring + comb_ring + 1]) as usize;
    let mut i = 0;
    while i < n {
        let x = input_gain * dry[i * stride + offset];
        let y = state[ring + pos];
        store = y * (1.0 - damp) + store * damp;
        state[ring + pos] = x + store * feedback;
        pos += 1;
        if pos >= len {
            pos = 0;
        }
        out[k * n + i] = y;
        i += 1;
    }
    state[ring + comb_ring] = store;
    state[ring + comb_ring + 1] = pos as f32;
}

// One side's allpasses through a chunk: the sum of its combs (rows of
// `rows`) in, the side's wet signal out (every other value of `out`).
#[inline]
pub fn allpass_run(side: usize, rate: f32, n: usize, chunk: i32, rows: &[f32], state: &mut [f32], out: &mut [f32]) {
    let (inp, outp) = slots(chunk);
    let a0 = combs * comb_size + side * 4 * allpass_size;
    let mut i = 0;
    while i < 4 * allpass_size {
        state[outp + a0 + i] = if chunk == 0 { 0.0 } else { state[inp + a0 + i] };
        i += 1;
    }
    let (l0, l1, l2, l3) = (allpass_len(side, 0, rate), allpass_len(side, 1, rate), allpass_len(side, 2, rate), allpass_len(side, 3, rate));
    let r0 = outp + a0;
    let (r1, r2, r3) = (r0 + allpass_size, r0 + 2 * allpass_size, r0 + 3 * allpass_size);
    let mut p0 = ftoi(state[r0 + allpass_ring]) as usize;
    let mut p1 = ftoi(state[r1 + allpass_ring]) as usize;
    let mut p2 = ftoi(state[r2 + allpass_ring]) as usize;
    let mut p3 = ftoi(state[r3 + allpass_ring]) as usize;
    let mut i = 0;
    while i < n {
        let mut x = 0.0f32;
        let mut c = 0;
        while c < 8 {
            x += rows[(side * 8 + c) * n + i];
            c += 1;
        }
        // (Freeverb's allpass: out = buffered - in; buffer = in + buffered * g)
        let b = state[r0 + p0];
        state[r0 + p0] = x + b * allpass_feedback;
        x = b - x;
        p0 += 1;
        if p0 >= l0 {
            p0 = 0;
        }
        let b = state[r1 + p1];
        state[r1 + p1] = x + b * allpass_feedback;
        x = b - x;
        p1 += 1;
        if p1 >= l1 {
            p1 = 0;
        }
        let b = state[r2 + p2];
        state[r2 + p2] = x + b * allpass_feedback;
        x = b - x;
        p2 += 1;
        if p2 >= l2 {
            p2 = 0;
        }
        let b = state[r3 + p3];
        state[r3 + p3] = x + b * allpass_feedback;
        x = b - x;
        p3 += 1;
        if p3 >= l3 {
            p3 = 0;
        }
        out[2 * i + side] = x;
        i += 1;
    }
    state[r0 + allpass_ring] = p0 as f32;
    state[r1 + allpass_ring] = p1 as f32;
    state[r2 + allpass_ring] = p2 as f32;
    state[r3 + allpass_ring] = p3 as f32;
}
