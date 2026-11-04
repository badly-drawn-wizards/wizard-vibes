// The renderer: one compute pass running the scene's shader a thread a
// pixel into a buffer of BGRA8 pixels (rows padded to 64 pixels, 256
// bytes), and, for the window, a copy of that buffer into the swapchain
// texture: the same path as the browser's (web/main.ts), without a blit.
// Each pass's GPU time is measured with timestamp queries where the
// adapter has them.

use crate::scene::{Camera, Scene};

pub struct Renderer {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    uniforms: wgpu::Buffer,
    workgroup: u32,
    target: Option<Target>,
    timer: Option<Timer>,
}

// The pixel buffer for one image size.
struct Target {
    w: u32,
    vw: u32,
    h: u32,
    pixels: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

struct Timer {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period: f32, // ns per tick
}

pub fn padded(vw: u32) -> u32 {
    vw.div_ceil(64) * 64
}

impl Renderer {
    pub async fn new(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>, scene: &Scene) -> Result<Renderer, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface,
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no adapter: {e}"))?;
        let info = adapter.get_info();
        eprintln!("adapter: {} ({:?}, {})", info.name, info.backend, info.driver_info);
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("wizard"),
                required_features: features,
                // (the shader's uniform block and its nesting of calls)
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no device: {e}"))?;

        let t = std::time::Instant::now();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wizard.native.wgsl"),
            source: wgpu::ShaderSource::Wgsl(scene.wgsl.as_str().into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("render"),
            layout: None,
            module: &module,
            entry_point: Some("native_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        eprintln!("pipeline: {:.1} s", t.elapsed().as_secs_f32());
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: scene.uniform_size(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let timer = features.contains(wgpu::Features::TIMESTAMP_QUERY).then(|| Timer {
            set: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("timer"), ty: wgpu::QueryType::Timestamp, count: 2 }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timer resolve"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timer read"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            period: queue.get_timestamp_period(),
        });
        Ok(Renderer { adapter, device, queue, pipeline, uniforms, workgroup: scene.workgroup, target: None, timer })
    }

    pub fn timed(&self) -> bool {
        self.timer.is_some()
    }

    fn ensure_target(&mut self, vw: u32, h: u32) {
        if !matches!(&self.target, Some(t) if t.vw == vw && t.h == h) {
            let w = padded(vw);
            let pixels = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pixels"),
                size: w as u64 * h as u64 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render"),
                layout: &self.pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: pixels.as_entire_binding() },
                ],
            });
            self.target = Some(Target { w, vw, h, pixels, bind });
        }
    }

    // Records a frame: renders the image and, if given, copies it into
    // `texture`. With `time`, the pass's GPU time is resolved too (read it
    // with gpu_ms once the work is done).
    pub fn frame(&mut self, scene: &Scene, vw: u32, h: u32, tm: f32, cam: Camera, texture: Option<&wgpu::Texture>, time: bool) {
        let data = scene.uniforms(padded(vw), vw, h, tm, cam);
        self.queue.write_buffer(&self.uniforms, 0, &data);
        let groups = (padded(vw) * h).div_ceil(self.workgroup);
        self.ensure_target(vw, h);
        let target = self.target.as_ref().unwrap();
        let timer = if time { self.timer.as_ref() } else { None };
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("render"),
                timestamp_writes: timer.map(|t| wgpu::ComputePassTimestampWrites {
                    query_set: &t.set,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: Some(1),
                }),
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &target.bind, &[]);
            // (up to 65535 groups: 16.7M pixels, over 4K)
            assert!(groups <= 65535, "image too large");
            pass.dispatch_workgroups(groups, 1, 1);
        }
        if let Some(tex) = texture {
            enc.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &target.pixels,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(target.w * 4), rows_per_image: Some(target.h) },
                },
                tex.as_image_copy(),
                wgpu::Extent3d { width: target.vw, height: target.h, depth_or_array_layers: 1 },
            );
        }
        if let Some(t) = timer {
            enc.resolve_query_set(&t.set, 0..2, &t.resolve, 0);
            enc.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, 16);
        }
        self.queue.submit([enc.finish()]);
    }

    // The last timed frame's GPU time in ms (waits for it).
    pub fn gpu_ms(&self) -> Option<f32> {
        let t = self.timer.as_ref()?;
        let slice = t.read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let ticks: [u64; 2] = bytemuck::pod_read_unaligned(&slice.get_mapped_range().unwrap()[..16]);
        t.read.unmap();
        Some(ticks[1].wrapping_sub(ticks[0]) as f32 * t.period / 1e6)
    }

    pub fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }

    // The image of the last frame, as RGBA rows (for --shot).
    pub fn read_image(&self) -> (u32, u32, Vec<u8>) {
        let t = self.target.as_ref().unwrap();
        let size = t.w as u64 * t.h as u64 * 4;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(&t.pixels, 0, &read, 0, size);
        self.queue.submit([enc.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.wait();
        let data = read.slice(..).get_mapped_range().unwrap();
        let mut rgba = Vec::with_capacity((t.vw * t.h * 4) as usize);
        for y in 0..t.h {
            for x in 0..t.vw {
                let i = ((y * t.w + x) * 4) as usize;
                rgba.extend_from_slice(&[data[i + 2], data[i + 1], data[i], 255]);
            }
        }
        (t.vw, t.h, rgba)
    }
}
