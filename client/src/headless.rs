// Without a window: --shot renders one frame at t0 and writes it as a PPM;
// --bench renders FRAMES frames at t0 + f / 30 (after one to warm up) and
// reports the mean time per frame, and the GPU time per frame where the
// device has timestamp queries. Both with the default parameters (as
// --set changes them), the page's camera and on the flight path (unless
// --drive sets the drive record's fields: e.g. free=1 thumb=1).

use crate::params::Schema;
use crate::render::frame::Frame;
use crate::render::{Args, Shader, padded};
use crate::source::{ShaderFiles, build_dir};
use crate::types::{BENCH_CAMERA, ON_PATH, PAGE_CAMERA, ParamValues, Value};
use std::path::Path;
use std::time::Instant;

pub struct Options {
    pub shot: Option<std::path::PathBuf>,
    pub bench: Option<u32>,
    pub size: (u32, u32),
    pub t0: f32,
    pub sets: Vec<(String, f64)>,
    pub drive: Vec<(String, f32)>,
    // --camera YAW,PITCH,DIST: the shot's camera, for the page's
    pub camera: Option<crate::types::Camera>,
    // --spell KIND,X,Y,Z,AGE[,A[,LIFE[,B,C]]]: spells in the shot (world
    // space; see spells.rs)
    pub spells: Vec<f32>,
    pub wgsl: bool,
    // --music N: N frames of music from t0 on the GPU, checked against the
    // shader crate's own on the CPU
    pub music: Option<u32>,
    // --song K: which song (see wizard_shader::music::songs)
    pub song: u32,
}

fn load(root: &Path, spv: bool) -> Result<ShaderFiles, String> {
    let d = root.join(build_dir(None));
    let read = |f: &str| std::fs::read_to_string(d.join(f)).map_err(|e| format!("{}: {e} (build the shader: see README.md)", d.join(f).display()));
    let meta = read("shader.json")?;
    let meta: crate::source::Meta = serde_json::from_str(&meta).map_err(|e| format!("shader.json: {e}"))?;
    // (as WGSL, each entry point's own module, as the browser has them)
    let mut entry_wgsl = std::collections::HashMap::new();
    if !spv {
        for e in &meta.entries {
            if let Some(f) = &e.wgsl {
                entry_wgsl.insert(e.name.clone(), read(f)?);
            }
        }
    }
    Ok(ShaderFiles {
        version: String::new(),
        meta,
        wgsl: read("shader.wgsl")?,
        spv: if spv { std::fs::read(d.join("shader.spv")).ok() } else { None },
        entry_wgsl,
    })
}

pub fn run(root: &Path, o: &Options) -> Result<(), String> {
    let schema: Schema = serde_json::from_str(&std::fs::read_to_string(root.join("params.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut values: ParamValues = ParamValues(schema.params().iter().filter(|p| !p.compile).map(|p| (p.id.clone(), p.default.clone())).collect());
    // (outside fight mode there are no enemies)
    values.0.insert("enemies".into(), Value::Num(0.0));
    let mut drive = ON_PATH;
    for (k, v) in &o.drive {
        if !drive.set(k, *v) {
            return Err(format!("--drive: no field {k}"));
        }
    }
    let drive = &drive;
    for (k, v) in &o.sets {
        if !values.0.contains_key(k) {
            return Err(format!("--set: no parameter {k}"));
        }
        values.0.insert(k.clone(), Value::Num(*v));
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))
        .map_err(|e| format!("no adapter: {e}"))?;
    let info = adapter.get_info();
    eprintln!("adapter: {} ({:?}, {})", info.name, info.backend, info.driver_info);
    // (timestamps round the frame's passes are written in the encoder)
    let mut features = adapter.features() & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS | wgpu::Features::PASSTHROUGH_SHADERS);
    if !features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS) {
        features.remove(wgpu::Features::TIMESTAMP_QUERY);
    }
    if o.wgsl {
        features.remove(wgpu::Features::PASSTHROUGH_SHADERS);
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("wizard"), required_features: features, required_limits: adapter.limits(), ..Default::default() }))
        .map_err(|e| format!("no device: {e}"))?;
    let t = Instant::now();
    let shader = Shader::new(&device, &load(root, !o.wgsl)?)?;
    eprintln!("shader: {} in {:.1} s", if shader.spirv { "SPIR-V" } else { "WGSL" }, t.elapsed().as_secs_f32());
    let (vw, h) = o.size;
    let mut frame = Frame::default();
    let n_spells = (o.spells.len() / 10) as u32;
    let args = |tm: f32, cam| Args { vw, h, tm, camera: cam, n_spells, m_t0: 0.0, m_rate: 0.0, m_n: 0, m_chunk: 0, m_song: 0, o_t: 0.0, o_kind: 0 };
    let timer = features.contains(wgpu::Features::TIMESTAMP_QUERY).then(|| {
        (
            device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 2 }),
            device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false }),
            device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
        )
    });
    let wait = || device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string());

    if let Some(n) = o.music {
        // (consecutive chunks of n frames from t0, through the soundboard
        // carried from chunk to chunk, against the shader crate's own on
        // the CPU; the drums' noise, hashed from the sample index, may take
        // a neighbouring sample's value where t * rate rounds differently,
        // so a few samples differ by as much as the noise is loud (the
        // kick's snap: up to ~0.8); what matters is that they differ in
        // few samples: the difference's rms)
        let (rate, chunks) = (48000.0, 8);
        let mut state = wizard_shader::music::State::new();
        let (mut worst, mut sum, mut diff, mut count, mut gpu_ms) = (0f32, 0f64, 0f64, 0usize, 0f64);
        for c in 0..chunks {
            let t0 = o.t0 + (c * n) as f32 / rate;
            let req = crate::types::MusicRequest { t0, rate, frames: n, chunk: c, song: o.song };
            if !frame.music(&device, &queue, &shader, &values, &ON_PATH, req) {
                return Err("the shader has no music entry points".into());
            }
            let t = Instant::now();
            let data = loop {
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
                if let Some((_, d)) = frame.music_ready() {
                    break d;
                }
            };
            gpu_ms += t.elapsed().as_secs_f64() * 1000.0;
            let cpu = wizard_shader::music::render_chunk(o.song as i32, t0, rate, n as usize, c, &mut state);
            worst = data.iter().zip(&cpu).map(|(a, b)| (a - b).abs()).fold(worst, f32::max);
            sum += data.iter().map(|v| (v * v) as f64).sum::<f64>();
            diff += data.iter().zip(&cpu).map(|(a, b)| ((a - b) * (a - b)) as f64).sum::<f64>();
            count += data.len();
        }
        println!("music: {chunks} chunks of {n} frames, {:.1} ms each (GPU and readback), rms {:.4}, GPU - CPU: rms {:.2e}, max {worst:.2e}", gpu_ms / chunks as f64, (sum / count as f64).sqrt(), (diff / count as f64).sqrt());
        return Ok(());
    }
    if let Some(path) = &o.shot {
        let mut enc = device.create_command_encoder(&Default::default());
        frame.record(&device, &queue, &mut enc, &shader, &values, drive, &args(o.t0, o.camera.unwrap_or(PAGE_CAMERA)), &o.spells);
        let w = padded(vw);
        let size = w as u64 * h as u64 * 4;
        let read = device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        enc.copy_buffer_to_buffer(frame.pixels(), 0, &read, 0, size);
        queue.submit([enc.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        wait()?;
        let data = read.slice(..).get_mapped_range().unwrap();
        let mut ppm = format!("P6\n{vw} {h}\n255\n").into_bytes();
        for y in 0..h {
            for x in 0..vw {
                let i = ((y * w + x) * 4) as usize;
                ppm.extend_from_slice(&[data[i + 2], data[i + 1], data[i]]);
            }
        }
        std::fs::write(path, ppm).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("wrote {}", path.display());
        return Ok(());
    }

    let frames = o.bench.unwrap_or(60);
    let mut enc = device.create_command_encoder(&Default::default());
    frame.record(&device, &queue, &mut enc, &shader, &values, drive, &args(o.t0, o.camera.unwrap_or(BENCH_CAMERA)), &[]);
    queue.submit([enc.finish()]);
    wait()?;
    let start = Instant::now();
    let mut gpu = 0.0;
    for f in 0..frames {
        let mut enc = device.create_command_encoder(&Default::default());
        if let Some((set, _, _)) = &timer {
            enc.write_timestamp(set, 0);
        }
        frame.record(&device, &queue, &mut enc, &shader, &values, drive, &args(o.t0 + f as f32 / 30.0, o.camera.unwrap_or(BENCH_CAMERA)), &[]);
        if let Some((set, resolve, read)) = &timer {
            enc.write_timestamp(set, 1);
            enc.resolve_query_set(set, 0..2, resolve, 0);
            enc.copy_buffer_to_buffer(resolve, 0, read, 0, 16);
        }
        queue.submit([enc.finish()]);
        if let Some((_, _, read)) = &timer {
            read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            wait()?;
            let ticks: [u64; 2] = bytemuck::pod_read_unaligned(&read.slice(..).get_mapped_range().unwrap()[..16]);
            read.unmap();
            gpu += ticks[1].wrapping_sub(ticks[0]) as f32 * queue.get_timestamp_period() / 1e6;
        } else {
            wait()?;
        }
    }
    let ms = start.elapsed().as_secs_f32() * 1000.0 / frames as f32;
    if timer.is_some() {
        println!("{vw}x{h}, {frames} frames: {ms:.2} ms a frame, GPU {:.2} ms", gpu / frames as f32);
    } else {
        println!("{vw}x{h}, {frames} frames: {ms:.2} ms a frame");
    }
    Ok(())
}
