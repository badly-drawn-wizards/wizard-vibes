// The renderer: the shader crate's compute entry points (native/shader),
// built by native/shader-builder, run with wgpu. The scene is rendered a
// thread a pixel into a buffer of BGRA8 pixels (rows padded to 64 pixels,
// 256 bytes), copied into a texture that egui draws under the UI.
//
// Natively the SPIR-V runs as it is where the device takes it (wgpu's
// passthrough); otherwise, and in the browser, naga's WGSL of it.
//
// Every entry point's bindings come from shader.json (see
// native/shader-builder), so the layouts match whatever the shader declares;
// buffers are shared between entry points by binding number.

pub mod frame;

use crate::source::{From, Meta, ShaderFiles};
use crate::types::{Camera, Drive, ParamValues};
use std::borrow::Cow;
use std::collections::HashMap;

pub fn padded(vw: u32) -> u32 {
    vw.div_ceil(64) * 64
}

// The frame's arguments to the shader.
#[derive(Clone, Copy, Debug)]
pub struct Args {
    pub vw: u32,
    pub h: u32,
    pub tm: f32,
    pub camera: Camera,
    pub n_spells: u32,
    pub m_t0: f32,
    pub m_rate: f32,
    pub m_n: u32,
    pub m_chunk: u32,
}

struct Kernel {
    pipeline: wgpu::ComputePipeline,
    workgroup: u32,
}

pub struct Shader {
    pub version: String,
    pub meta: Meta,
    kernels: HashMap<String, Kernel>,
    // one layout for every entry point: all the bindings any of them uses
    // (the SPIR-V run as it is is not checked against its layout, and its
    // entry points may refer to more than they use: a layout per entry point
    // with only what naga sees each use broke the driver's pipelines)
    layout: wgpu::BindGroupLayout,
    bindings: Vec<u32>,
    // bound where a pass has no buffer of its own (and in place of the
    // indirect dispatch's arguments, which may not be bound as storage
    // then): one per binding, as one buffer may not be bound both read-only
    // and writable in a dispatch
    dummies: HashMap<u32, wgpu::Buffer>,
    // whether it runs as SPIR-V (else WGSL)
    pub spirv: bool,
}

// What the device can do with shaders here.
pub fn passthrough(device: &wgpu::Device) -> bool {
    device.features().contains(wgpu::Features::PASSTHROUGH_SHADERS)
}

impl Shader {
    pub fn new(device: &wgpu::Device, files: &ShaderFiles) -> Result<Shader, String> {
        let spirv = files.spv.is_some() && passthrough(device);
        // (WGSL from naga: the checks wgpu would add are off, as for the
        // SPIR-V, which has none)
        let module = if spirv {
            let words: Vec<u32> = bytemuck::pod_collect_to_vec(files.spv.as_ref().unwrap());
            let eps = files
                .meta
                .entries
                .iter()
                .map(|e| wgpu::PassthroughShaderEntryPoint { name: e.name.clone().into(), workgroup_size: (e.workgroup[0], e.workgroup[1], e.workgroup[2]) })
                .collect();
            unsafe {
                device.create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough {
                    label: Some("shader.spv"),
                    entry_points: Cow::Owned(eps),
                    spirv: Some(Cow::Owned(words)),
                    ..Default::default()
                })
            }
        } else {
            // (errors reach the device's uncaptured-error handler: see app.rs)
            unsafe {
                device.create_shader_module_trusted(
                    wgpu::ShaderModuleDescriptor { label: Some("shader.wgsl"), source: wgpu::ShaderSource::Wgsl(files.wgsl.as_str().into()) },
                    wgpu::ShaderRuntimeChecks::unchecked(),
                )
            }
        };
        let mut kinds: std::collections::BTreeMap<u32, String> = Default::default();
        for b in files.meta.entries.iter().flat_map(|e| &e.bindings).filter(|b| b.group == 0) {
            match kinds.get(&b.binding) {
                Some(k) if *k != b.kind => return Err(format!("binding {} is {} in one entry point and {} in another", b.binding, k, b.kind)),
                _ => kinds.insert(b.binding, b.kind.clone()),
            };
        }
        let entries: Vec<wgpu::BindGroupLayoutEntry> = kinds
            .iter()
            .map(|(&binding, kind)| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: match kind.as_str() {
                        "uniform" => wgpu::BufferBindingType::Uniform,
                        "storage-read" => wgpu::BufferBindingType::Storage { read_only: true },
                        _ => wgpu::BufferBindingType::Storage { read_only: false },
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("shader"), entries: &entries });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shader"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let mut kernels = HashMap::new();
        for e in &files.meta.entries {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&e.name),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(&e.name),
                compilation_options: Default::default(),
                cache: None,
            });
            kernels.insert(e.name.clone(), Kernel { pipeline, workgroup: e.workgroup[0].max(1) });
        }
        let dummies = kinds
            .keys()
            .map(|&b| {
                let buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("dummy {b}")),
                    size: 256,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::UNIFORM,
                    mapped_at_creation: false,
                });
                (b, buf)
            })
            .collect();
        let bindings = kinds.keys().copied().collect();
        Ok(Shader { version: files.version.clone(), meta: files.meta.clone(), kernels, layout, bindings, dummies, spirv })
    }

    pub fn has(&self, entry: &str) -> bool {
        self.kernels.contains_key(entry)
    }

    // The uniform block's bytes for a frame.
    pub fn uniforms(&self, params: &ParamValues, drive: &Drive, a: &Args) -> Vec<u8> {
        let fields = &self.meta.fields;
        let mut out = vec![0u8; (fields.len() * 4).div_ceil(16) * 16];
        for (i, f) in fields.iter().enumerate() {
            let v: f64 = match &f.from {
                From::Param { id, index } => match (params.get(id), index) {
                    (Some(crate::types::Value::Colour(c)), Some(k)) => c[*k],
                    (Some(v), _) => v.f32() as f64,
                    (None, _) => 0.0,
                },
                From::Drive { id } => drive.field(id) as f64,
                From::Arg { id } => match id.as_str() {
                    "w" => padded(a.vw) as f64,
                    "vw" => a.vw as f64,
                    "h" => a.h as f64,
                    "tm" => a.tm as f64,
                    "yaw" => a.camera.yaw as f64,
                    "pitch" => a.camera.pitch as f64,
                    "dst" => a.camera.dist as f64,
                    "n_spells" => a.n_spells as f64,
                    "m_t0" => a.m_t0 as f64,
                    "m_rate" => a.m_rate as f64,
                    "m_n" => a.m_n as f64,
                    "m_chunk" => a.m_chunk as f64,
                    _ => 0.0,
                },
            };
            let bytes = if f.ty == "i32" { (v.round() as i32).to_le_bytes() } else { (v as f32).to_le_bytes() };
            out[4 * i..4 * i + 4].copy_from_slice(&bytes);
        }
        out
    }
}

// The buffers the entry points share, by binding number, grown as needed.
#[derive(Default)]
pub struct Buffers {
    bufs: HashMap<u32, wgpu::Buffer>,
}

impl Buffers {
    // The buffer at `binding`, at least `size` bytes.
    pub fn get(&mut self, device: &wgpu::Device, binding: u32, size: u64, usage: wgpu::BufferUsages) -> &wgpu::Buffer {
        let size = size.max(16).div_ceil(16) * 16;
        let fits = self.bufs.get(&binding).is_some_and(|b| b.size() >= size && b.usage().contains(usage));
        if !fits {
            let b = device.create_buffer(&wgpu::BufferDescriptor { label: Some(&format!("binding {binding}")), size, usage, mapped_at_creation: false });
            self.bufs.insert(binding, b);
        }
        &self.bufs[&binding]
    }

    pub fn peek(&self, binding: u32) -> Option<&wgpu::Buffer> {
        self.bufs.get(&binding)
    }
}

// The bind group for a pass: the shared buffers at their bindings (the
// dummy for any missing, and for `except`).
fn bind_group(device: &wgpu::Device, shader: &Shader, bufs: &Buffers, label: &str, except: Option<u32>) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry> = shader
        .bindings
        .iter()
        .map(|&b| {
            let buf = if Some(b) == except { None } else { bufs.peek(b) };
            wgpu::BindGroupEntry { binding: b, resource: buf.unwrap_or(&shader.dummies[&b]).as_entire_binding() }
        })
        .collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(label), layout: &shader.layout, entries: &entries })
}

// Records a dispatch of `entry` over `threads` threads (in x), with the
// shared buffers bound.
pub fn dispatch(device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, shader: &Shader, bufs: &Buffers, entry: &str, threads: u32) {
    let Some(k) = shader.kernels.get(entry) else { return };
    let bind = bind_group(device, shader, bufs, entry, None);
    let groups = threads.div_ceil(k.workgroup);
    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(entry), timestamp_writes: None });
    pass.set_pipeline(&k.pipeline);
    pass.set_bind_group(0, &bind, &[]);
    // (the shaders index threads by x alone: up to 16.7M, over 4K)
    assert!(groups <= 65535, "{entry}: {threads} threads is too many");
    pass.dispatch_workgroups(groups, 1, 1);
}

// Records an indirect dispatch of `entry`, its group counts at `offset`
// bytes into the buffer at `binding` (bound as the dummy meanwhile: a
// buffer may not be both).
pub fn dispatch_indirect(device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, shader: &Shader, bufs: &Buffers, entry: &str, binding: u32, offset: u64) {
    let Some(k) = shader.kernels.get(entry) else { return };
    let bind = bind_group(device, shader, bufs, entry, Some(binding));
    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(entry), timestamp_writes: None });
    pass.set_pipeline(&k.pipeline);
    pass.set_bind_group(0, &bind, &[]);
    pass.dispatch_workgroups_indirect(bufs.peek(binding).expect("indirect buffer"), offset);
}
