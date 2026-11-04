// Native client for the Futhark ray-marched wizard: the scene's per-pixel
// code, as Futhark compiles it to WGSL for the browser, run by wgpu on
// Vulkan (Metal, DX12) in a window, on the flight path, with the default
// parameters; the frame rate and GPU time per frame in the title (and on
// stdout every second). To compare with the browser's.
//
// usage: wizard-native [--build DIR [--unchecked] | --rust-gpu | --spirv FILE] [--vsync] [--scale S]
//        [--set PARAM=VALUE ...]   (a scalar parameter, e.g. --set view=2)
//        wizard-native --bench [FRAMES] [--size WxH] [--t0 T]
//        wizard-native --shot FILE.ppm [--size WxH] [--t0 T]
// (run from the repository, or set WIZARD_ROOT; see native/README.md)

mod gpu;
mod scene;

use gpu::{Renderer, Shader};
use scene::{Camera, Scene, BENCH_CAMERA, PAGE_CAMERA};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

struct Args {
    build: Option<PathBuf>,
    bench: Option<u32>,
    shot: Option<PathBuf>,
    size: (u32, u32),
    t0: f32,
    vsync: bool,
    scale: f32,
    spirv: Option<PathBuf>,
    unchecked: bool,
    sets: Vec<(String, f64)>,
}

fn args() -> Args {
    let mut a = Args { build: None, bench: None, shot: None, size: (1920, 1080), t0: 0.0, vsync: false, scale: 1.0, spirv: None, unchecked: false, sets: vec![] };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| die(&format!("{arg} needs a value")));
        match arg.as_str() {
            "--build" => a.build = Some(value().into()),
            "--shot" => a.shot = Some(value().into()),
            "--t0" => a.t0 = value().parse().unwrap_or_else(|_| die("--t0: a number")),
            "--scale" => a.scale = value().parse().unwrap_or_else(|_| die("--scale: a number")),
            "--size" => {
                let v = value();
                let (w, h) = v.split_once('x').unwrap_or_else(|| die("--size: WxH"));
                a.size = (w.parse().unwrap_or_else(|_| die("--size: WxH")), h.parse().unwrap_or_else(|_| die("--size: WxH")));
            }
            "--vsync" => a.vsync = true,
            "--unchecked" => a.unchecked = true,
            "--set" => {
                let v = value();
                let (k, x) = v.split_once('=').unwrap_or_else(|| die("--set: key=value"));
                a.sets.push((k.to_string(), x.parse().unwrap_or_else(|_| die("--set: a number"))));
            }
            "--spirv" => a.spirv = Some(value().into()),
            "--rust-gpu" => a.spirv = Some("native/shader.spv".into()),
            "--bench" => {
                a.bench = Some(match it.peek() {
                    Some(n) if n.parse::<u32>().is_ok() => it.next().unwrap().parse().unwrap(),
                    _ => 60,
                })
            }
            _ => die(&format!("unknown argument {arg}")),
        }
    }
    a
}

fn die(msg: &str) -> ! {
    eprintln!("wizard-native: {msg}");
    std::process::exit(2)
}

fn root() -> PathBuf {
    if let Ok(r) = std::env::var("WIZARD_ROOT") {
        return r.into();
    }
    // the repository: the working directory or one of its parents
    let mut d = std::env::current_dir().unwrap();
    loop {
        if d.join("params.json").exists() {
            return d;
        }
        if !d.pop() {
            die("not in the repository (no params.json above here); set WIZARD_ROOT");
        }
    }
}

fn main() {
    let a = args();
    let root = root();
    let scene = Scene::load(&root, a.build.clone(), &a.sets).unwrap_or_else(|e| die(&e));
    let shader = match &a.spirv {
        None => {
            eprintln!("build: {}", scene.build.display());
            Shader::Wgsl { unchecked: a.unchecked }
        }
        Some(p) => {
            let p = if p.is_relative() { root.join(p) } else { p.clone() };
            let bytes = std::fs::read(&p).unwrap_or_else(|e| die(&format!("{}: {e} (build it with native/shader-builder)", p.display())));
            eprintln!("shader: {}", p.display());
            Shader::Spirv(bytemuck::pod_collect_to_vec(&bytes))
        }
    };
    if a.bench.is_some() || a.shot.is_some() {
        headless(&a, &scene, &shader);
    } else {
        let event_loop = EventLoop::new().unwrap_or_else(|e| die(&format!("no event loop: {e}")));
        let mut app = App { scene, shader, vsync: a.vsync, scale: a.scale, state: None };
        event_loop.run_app(&mut app).unwrap();
    }
}

// --bench: like the browser's (window.wizard.bench, dev/bench-daemon.ts):
// a warm-up frame, then FRAMES frames at t0 + f / 30 with nothing
// presented, reporting the mean time per frame and GPU time per frame.
// --shot: one frame at t0, written as a PPM.
fn headless(a: &Args, scene: &Scene, shader: &Shader) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut r = pollster::block_on(Renderer::new(&instance, None, scene, shader)).unwrap_or_else(|e| die(&e));
    let (vw, h) = a.size;
    if let Some(path) = &a.shot {
        r.frame(scene, vw, h, a.t0, PAGE_CAMERA, None, false);
        let (w, h, rgba) = r.read_image();
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for px in rgba.chunks(4) {
            ppm.extend_from_slice(&px[..3]);
        }
        std::fs::write(path, ppm).unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
        eprintln!("wrote {}", path.display());
        return;
    }
    let frames = a.bench.unwrap();
    r.frame(scene, vw, h, a.t0, BENCH_CAMERA, None, false); // warm-up
    r.wait();
    let start = Instant::now();
    let mut gpu = 0.0;
    for f in 0..frames {
        r.frame(scene, vw, h, a.t0 + f as f32 / 30.0, BENCH_CAMERA, None, r.timed());
        // (reading the GPU time waits for the frame, as the browser's
        // benchmark awaits each one)
        gpu += r.gpu_ms().unwrap_or(0.0);
    }
    r.wait();
    let ms = start.elapsed().as_secs_f32() * 1000.0 / frames as f32;
    if r.timed() {
        println!("{vw}x{h}, {frames} frames: {ms:.2} ms a frame, GPU {:.2} ms", gpu / frames as f32);
    } else {
        println!("{vw}x{h}, {frames} frames: {ms:.2} ms a frame (no timestamp queries)");
    }
}

struct App {
    scene: Scene,
    shader: Shader,
    vsync: bool,
    scale: f32,
    state: Option<State>,
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    start: Instant,
    cam: Camera,
    // the counter: frames and summed GPU time since `since`
    since: Instant,
    frames: u32,
    gpu_ms: f32,
    timed: u32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(Window::default_attributes().with_title("wizard").with_inner_size(winit::dpi::PhysicalSize::new(1920, 1080)))
                .unwrap_or_else(|e| die(&format!("no window: {e}"))),
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(window.clone())));
        let surface = instance.create_surface(window.clone()).unwrap_or_else(|e| die(&format!("no surface: {e}")));
        let renderer = pollster::block_on(Renderer::new(&instance, Some(&surface), &self.scene, &self.shader)).unwrap_or_else(|e| die(&e));
        let caps = surface.get_capabilities(&renderer.adapter);
        if !caps.formats.contains(&wgpu::TextureFormat::Bgra8Unorm) || !caps.usages.contains(wgpu::TextureUsages::COPY_DST) {
            die(&format!("the surface takes no copies into bgra8unorm (formats {:?}, usages {:?})", caps.formats, caps.usages));
        }
        // without vsync, to see the frame rate the GPU can reach
        let present_mode = if self.vsync {
            wgpu::PresentMode::Fifo
        } else {
            [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox].into_iter().find(|m| caps.present_modes.contains(m)).unwrap_or(wgpu::PresentMode::Fifo)
        };
        eprintln!("present mode: {present_mode:?}");
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: Default::default(),
        };
        surface.configure(&renderer.device, &config);
        let now = Instant::now();
        self.state = Some(State { window, surface, config, renderer, start: now, cam: PAGE_CAMERA, since: now, frames: 0, gpu_ms: 0.0, timed: 0 });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput { event: KeyEvent { logical_key: Key::Named(NamedKey::Escape), state: ElementState::Pressed, .. }, .. } => el.exit(),
            WindowEvent::Resized(size) => {
                s.config.width = size.width.max(1);
                s.config.height = size.height.max(1);
                s.surface.configure(&s.renderer.device, &s.config);
            }
            WindowEvent::RedrawRequested => {
                let frame = match s.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                        s.window.request_redraw();
                        return;
                    }
                    _ => {
                        s.surface.configure(&s.renderer.device, &s.config);
                        s.window.request_redraw();
                        return;
                    }
                };
                // the image is rendered at `scale` and copied into the
                // swapchain's corner (as the page's render scale, without
                // the stretch)
                let vw = ((s.config.width as f32 * self.scale) as u32).clamp(1, s.config.width);
                let h = ((s.config.height as f32 * self.scale) as u32).clamp(1, s.config.height);
                let tm = s.start.elapsed().as_secs_f32();
                // (the GPU time of every 15th frame: reading it waits for that frame)
                let time = s.frames % 15 == 0 && s.renderer.timed();
                s.renderer.frame(&self.scene, vw, h, tm, s.cam, Some(&frame.texture), time);
                s.window.pre_present_notify();
                s.renderer.queue.present(frame);
                if time {
                    if let Some(ms) = s.renderer.gpu_ms() {
                        s.gpu_ms += ms;
                        s.timed += 1;
                    }
                }
                s.frames += 1;
                let dt = s.since.elapsed().as_secs_f32();
                if dt >= 1.0 {
                    let fps = s.frames as f32 / dt;
                    let gpu = if s.timed > 0 { format!(", GPU {:.2} ms", s.gpu_ms / s.timed as f32) } else { String::new() };
                    let line = format!("{fps:.1} fps ({:.2} ms){gpu} at {vw}x{h}", 1000.0 / fps);
                    println!("{line}");
                    s.window.set_title(&format!("wizard: {line}"));
                    (s.since, s.frames, s.gpu_ms, s.timed) = (Instant::now(), 0, 0.0, 0);
                }
                s.window.request_redraw();
            }
            _ => {}
        }
    }
}
