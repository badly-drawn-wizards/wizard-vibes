// A frame of the scene: the uniforms and the spells written, and the
// shader's passes recorded (see native/shader/ENTRIES.md), leaving the
// image as BGRA8 pixels (rows padded to 64) in the pixel buffer. Shared by
// the window and the headless modes (--shot, --bench). Also the music's
// chunks, computed by the shader and read back.

use super::{Args, Buffers, Shader, dispatch, dispatch_indirect, padded};
use crate::types::{Drive, MusicRequest, ParamValues};
use std::sync::{Arc, Mutex};

// The shader's bindings (ENTRIES.md).
pub const UNIFORMS: u32 = 0;
pub const PIXELS: u32 = 1;
const COLOUR: u32 = 2;
const SPELLS: u32 = 3;
const EDGES: u32 = 4;
const RAYS: u32 = 5;
const ARGS: u32 = 6;
const MUSIC: u32 = 7;
const DRY: u32 = 8;
const MODES: u32 = 9;
const BOARD: u32 = 10;

// The share of pixels that edge supersampling can take (typical frames
// have 1-5% edge pixels; past this, edges keep their one sample).
const EDGE_SHARE: f64 = 0.15;

#[derive(Default)]
pub struct Frame {
    pub bufs: Buffers,
    // the music's own uniforms and output (apart from the frame's, so a
    // chunk can be computed whenever it is asked for)
    music: Buffers,
    // a chunk being read back, and the one read
    music_read: Option<(MusicRequest, wgpu::Buffer)>,
    // (the request read back: Ok, or Err if the readback failed)
    music_done: Arc<Mutex<Option<Result<MusicRequest, ()>>>>,
    // the room's state (see wizard_shader::room), carried from chunk to chunk
    room: Vec<f32>,
}

impl Frame {
    // Records the scene for `args` into `enc`, with `spells` (spell_stride
    // floats a slot) in flight.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        enc: &mut wgpu::CommandEncoder,
        shader: &Shader,
        params: &ParamValues,
        drive: &Drive,
        args: &Args,
        spells: &[f32],
    ) {
        use wgpu::BufferUsages as U;
        let (vw, h) = (args.vw, args.h);
        let w = padded(vw);
        let data = shader.uniforms(params, drive, args);
        let b = &mut self.bufs;
        queue.write_buffer(b.get(device, UNIFORMS, data.len() as u64, U::UNIFORM | U::COPY_DST), 0, &data);
        let sp = b.get(device, SPELLS, (spells.len().max(4) * 4) as u64, U::STORAGE | U::COPY_DST);
        if !spells.is_empty() {
            queue.write_buffer(sp, 0, bytemuck::cast_slice(spells));
        }
        let px = (vw * h) as u64;
        b.get(device, PIXELS, w as u64 * h as u64 * 4, U::STORAGE | U::COPY_SRC);
        b.get(device, COLOUR, px * 16, U::STORAGE);
        let cap = ((px as f64 * EDGE_SHARE) as u64).max(1);
        b.get(device, EDGES, (1 + cap) * 4, U::STORAGE);
        b.get(device, RAYS, cap * 4 * 16, U::STORAGE);
        b.get(device, ARGS, 24, U::STORAGE | U::INDIRECT);
        if !shader.has("shade") {
            // (an older shader: the scene alone)
            dispatch(device, enc, shader, b, "native_main", w * h);
            return;
        }
        dispatch(device, enc, shader, b, "shade", w * h);
        match params.f32("aa").round() as i32 {
            1 => dispatch(device, enc, shader, b, "fxaa", w * h),
            2 => {
                dispatch(device, enc, shader, b, "edge_mark", vw * h);
                dispatch(device, enc, shader, b, "edge_args", 1);
                dispatch_indirect(device, enc, shader, b, "edge_rays", ARGS, 0);
                dispatch_indirect(device, enc, shader, b, "edge_resolve", ARGS, 12);
                dispatch(device, enc, shader, b, "pack_pixels", w * h);
            }
            _ => {}
        }
    }

    pub fn pixels(&self) -> &wgpu::Buffer {
        self.bufs.peek(PIXELS).expect("no frame recorded")
    }

    // Starts computing a chunk of music (unless one is being read back:
    // then false, and it can be asked again later).
    // (chunks must come in order, req.chunk counting them from 0: the
    // soundboard's state, in two slots of its buffer, carries on from the
    // last; see the soundboard entry point)
    pub fn music(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, shader: &Shader, params: &ParamValues, drive: &Drive, req: MusicRequest) -> bool {
        use wgpu::BufferUsages as U;
        if self.music_read.is_some() || !shader.has("music_mix") {
            return false;
        }
        // (the loop time, wrapped in double precision)
        let period = wizard_shader::music::song_period(req.song as i32);
        let t0 = (req.t0 as f64).rem_euclid(period as f64) as f32;
        let args = Args { vw: 1, h: 1, tm: 0.0, camera: crate::types::PAGE_CAMERA, n_spells: 0, m_t0: t0, m_rate: req.rate, m_n: req.frames, m_chunk: req.chunk, m_song: req.song, o_t: 0.0, o_kind: 0 };
        let data = shader.uniforms(params, drive, &args);
        let frames = req.frames as u64;
        // (3 floats a frame back: see music_mix and music::finish_chunk)
        let n = 3 * frames * 4;
        let m = &mut self.music;
        queue.write_buffer(m.get(device, UNIFORMS, data.len() as u64, U::UNIFORM | U::COPY_DST), 0, &data);
        m.get(device, MUSIC, n, U::STORAGE | U::COPY_SRC);
        m.get(device, DRY, 4 * frames * 4, U::STORAGE);
        m.get(device, MODES, wizard_shader::soundboard::modes as u64 * frames * 4, U::STORAGE);
        m.get(device, BOARD, 2 * 2 * wizard_shader::soundboard::modes as u64 * 4, U::STORAGE);

        let read = device.create_buffer(&wgpu::BufferDescriptor { label: Some("music read"), size: n, usage: U::MAP_READ | U::COPY_DST, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&Default::default());
        // (each song its own kernel; older builds, one for all)
        let kernel = wizard_shader::music::song_kernels.get(req.song as usize).copied().unwrap_or("music");
        dispatch(device, &mut enc, shader, m, if shader.has(kernel) { kernel } else { "music" }, req.frames);
        dispatch(device, &mut enc, shader, m, "soundboard", wizard_shader::soundboard::modes as u32);

        dispatch(device, &mut enc, shader, m, "music_mix", req.frames);
        enc.copy_buffer_to_buffer(m.peek(MUSIC).unwrap(), 0, &read, 0, n);
        queue.submit([enc.finish()]);
        let done = self.music_done.clone();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            *done.lock().unwrap() = Some(r.map(|_| req).map_err(|_| ()));
        });
        self.music_read = Some((req, read));
        true
    }

    // The chunk of music read back, once there is one.
    pub fn music_ready(&mut self) -> Option<(MusicRequest, Vec<f32>)> {
        let done = self.music_done.lock().unwrap().take()?;
        let (r, buf) = self.music_read.take()?;
        // (a failed readback is dropped: the audio asks for that chunk again)
        let Ok(req) = done else { return None };
        debug_assert_eq!(r, req);
        let mixed: Vec<f32> = bytemuck::pod_collect_to_vec(&buf.slice(..).get_mapped_range().unwrap());
        buf.unmap();
        // (the room, on the CPU, carried on from the last chunk: in order)
        if self.room.len() != 2 * wizard_shader::room::slot {
            self.room = vec![0.0; 2 * wizard_shader::room::slot];
        }
        Some((req, wizard_shader::music::finish_chunk(&mixed, req.rate, req.chunk, &mut self.room)))
    }
}

// The uniforms as the shader crate's own records (for its code on the CPU:
// the collision probe), if the block is laid out as the crate was built
// with (else None: the client needs rebuilding for the new parameters).
pub fn host_records(shader: &Shader, params: &ParamValues, drive: &Drive) -> Option<(wizard_shader::generated::Params, wizard_shader::generated::Drive)> {
    let size = std::mem::size_of::<wizard_shader::generated::Uniforms>();
    if shader.meta.fields.len() * 4 != size {
        return None;
    }
    let args = Args { vw: 1, h: 1, tm: 0.0, camera: crate::types::PAGE_CAMERA, n_spells: 0, m_t0: 0.0, m_rate: 0.0, m_n: 0, m_chunk: 0, m_song: 0, o_t: 0.0, o_kind: 0 };
    let bytes = shader.uniforms(params, drive, &args);
    // (Uniforms is repr(C), 4-byte scalars throughout, as the block)
    let u: wizard_shader::generated::Uniforms = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const _) };
    Some(wizard_shader::generated::unpack(&u))
}
