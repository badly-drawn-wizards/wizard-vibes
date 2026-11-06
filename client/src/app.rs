// The app: the scene, drawn by the shader (render/), under egui's UI: the
// top bar (the status, folding to the frame rate, and the music and sound
// switches), the pause menu (its settings: mode, platform, song, quality,
// render scale, target frame rate; and the parameters, params.rs), the
// game's HUD (game/), and an overlay for shader errors. Natively and in the
// browser alike.
//
// Hot reload: when a new shader build appears (source.rs), it is loaded
// while the old one keeps drawing, then swapped in. The app's own code is
// hot-patched by Subsecond under `dx serve --hotpatch` (see README.md).

use crate::i18n::tr;
use crate::audio::Audio;
use crate::game::{Game, LEVELS, Level};
use crate::params::{Params, Schema};
use crate::render::{self, Args, Shader, frame::{Frame, host_records}};
use crate::source::{Event, Source};
use crate::types::{BuildInfo, FrameOut, ParamValues, Value};
use std::collections::HashMap;
use web_time::Instant;

// The sound effects' sample rate (resampled to the device's by the audio).
const SFX_RATE: f32 = 48000.0;

const SCALE: &str = "wizard-scale";
const BAR: &str = "wizard-bar-open";
const LANG: &str = "wizard-lang";
const TARGET_FPS: &str = "wizard-target-fps";
const DYNAMIC: &str = "wizard-dynamic-scale";

pub struct App {
    source: Source,
    params: Option<Params>,
    shader: Option<Shader>,
    error: Option<String>,
    building: bool,
    // natively, a shader being made off the UI thread (its module and
    // pipelines: the driver compiling each kernel takes seconds), and when
    // it was started; the current one draws meanwhile
    #[cfg(not(target_arch = "wasm32"))]
    compiling: Option<(std::sync::mpsc::Receiver<Result<Shader, String>>, Instant)>,
    frame: Frame,
    // the image the scene is copied into, and its egui id
    image: Option<(wgpu::Texture, egui::TextureId)>,
    start: Instant,
    last: Instant,
    fps: f32,
    // dancing: how much (eased), the beat, and its rate (beats a second)
    groove: f32,
    beat: f64,
    beat_rate: f64,
    // the render scale (by hand, or dynamic: see scale.rs)
    scale: crate::scale::RenderScale,
    // the top bar open (else just the frame rate), the pause menu's tab (0
    // settings, 1 controls, 2 parameters) and its tabs' width (last frame,
    // to centre them), whether it was open last frame (to put the cursor on
    // its tab as it opens), its focus navigation (see nav.rs), and the key
    // events the controller's A makes for it (fed to egui before the next
    // frame)
    bar_open: bool,
    menu_tab: usize,
    tabs_width: f32,
    menu_was_open: bool,
    nav: crate::nav::Nav,
    menu_events: Vec<egui::Event>,
    // the last error the device reported (validation of the shader, etc.)
    // the GPU's errors, the first first (the cause; what follows from it is
    // mostly "... is invalid"), and how many are shown
    device_error: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    device_errors_shown: usize,
    // why this GPU cannot run the app, if it cannot (in a browser without
    // WebGPU, wgpu falls back to WebGL2, which has no compute shaders)
    unsupported: Option<String>,
    game: Game,
    audio: Audio,
    // natively, while he plays (not paused): the screen kept from dimming
    // and the system from sleeping for idleness (see keep_awake)
    #[cfg(not(target_arch = "wasm32"))]
    awake: Option<keepawake::KeepAwake>,
    #[cfg(not(target_arch = "wasm32"))]
    awake_failed: bool,
    info: BuildInfo,
    // the sound effects made so far (one a frame, from the shader crate)
    sfx: Vec<Vec<f32>>,
    // the drawer's values saved last time (applied once the schema comes)
    saved_params: Option<HashMap<String, Value>>,
    // the quality panel: set up once there is a shader; the level whose
    // build is being loaded
    quality_ready: bool,
    quality_loading: Option<Level>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    shot_asked: bool,
    // load the next shader even if it looks the same (a restarted server)
    reload: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, source: Source) -> App {
        let storage = cc.storage;
        // (dynamic unless turned off; in the browser, ?scale=0.25 (to 1)
        // sets it by hand)
        let fixed = url_param("scale").and_then(|v| v.parse::<f32>().ok()).filter(|&v| v > 0.0 && v <= 1.0);
        let manual = fixed.or_else(|| storage.and_then(|s| eframe::get_value(s, SCALE))).unwrap_or(1.0);
        let dynamic = fixed.is_none() && storage.and_then(|s| eframe::get_value(s, DYNAMIC)).unwrap_or(true);
        let mut scale = crate::scale::RenderScale::new(dynamic, manual);
        if let Some(fps) = storage.and_then(|s| eframe::get_value::<f32>(s, TARGET_FPS)) {
            scale.set_target(fps);
        }
        let bar_open = storage.and_then(|s| eframe::get_value(s, BAR)).unwrap_or(true);
        if let Some(l) = storage.and_then(|s| s.get_string(LANG)).and_then(|k| crate::i18n::Lang::from_key(&k)) {
            crate::i18n::set_lang(l);
        }
        let now = Instant::now();
        let n = wizard_shader::sfx::sfx_count;
        let info = BuildInfo {
            spell_limit: wizard_shader::generated::max_spells as u32,
            sfx_rate: SFX_RATE,
            sfx_seconds: (0..n).map(wizard_shader::sfx::sfx_length).collect(),
        };
        let device_error = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            let slot = device_error.clone();
            // (what the GPU is, and if it is lost, why: after that its
            // errors go unreported and what is made on it is invalid)
            let info = rs.adapter.get_info();
            let l = rs.device.limits();
            log::info!(
                "GPU: {} ({:?}, {}); buffers up to {} MB, storage bindings up to {} MB, {} storage buffers a stage",
                info.name,
                info.backend,
                info.driver_info,
                l.max_buffer_size >> 20,
                l.max_storage_buffer_binding_size >> 20,
                l.max_storage_buffers_per_shader_stage
            );
            let lost = device_error.clone();
            rs.device.set_device_lost_callback(move |reason, msg| {
                let e = format!("The GPU device was lost ({reason:?}): {msg}");
                log::error!("{e}");
                lost.lock().unwrap().push(e);
            });
            rs.device.on_uncaptured_error(std::sync::Arc::new(move |e: wgpu::Error| {
                let e = e.to_string();
                let mut all = slot.lock().unwrap();
                if all.len() < 8 && !all.contains(&e) {
                    log::error!("{e}");
                    all.push(e);
                }
            }));
        }
        App {
            source,
            params: None,
            shader: None,
            error: None,
            building: false,
            #[cfg(not(target_arch = "wasm32"))]
            compiling: None,
            frame: Frame::default(),
            image: None,
            start: now,
            last: now,
            fps: 0.0,
            scale,
            bar_open,
            menu_tab: 0,
            menu_was_open: false,
            nav: Default::default(),
            tabs_width: 0.0,
            menu_events: Vec::new(),
            device_error,
            device_errors_shown: 0,
            groove: 0.0,
            beat: 0.0,
            beat_rate: 2.0,
            unsupported: cc.wgpu_render_state.as_ref().and_then(|rs| {
                let caps = rs.adapter.get_downlevel_capabilities();
                if caps.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS) {
                    return None;
                }
                let info = rs.adapter.get_info();
                Some(format!(
                    "This browser gives the page no WebGPU, only {:?} ({}), which has no compute shaders: \
                     the whole scene is drawn by them.\n\n\
                     Try Chrome or Edge, Safari 26, or Firefox with WebGPU (on by default on Windows; \
                     elsewhere set dom.webgpu.enabled in about:config, which on Android takes Firefox Nightly).",
                    info.backend, info.name
                ))
            }),
            game: {
                let mut g = Game::new(storage);
                g.set_build(&info);
                g
            },
            audio: Audio::new(storage),
            #[cfg(not(target_arch = "wasm32"))]
            awake: None,
            #[cfg(not(target_arch = "wasm32"))]
            awake_failed: false,
            info,
            sfx: vec![],
            saved_params: storage.and_then(|s| eframe::get_value(s, crate::params::STORE)),
            quality_ready: false,
            quality_loading: None,
            shot_asked: false,
            reload: false,
        }
    }

    // The quality level whose compile-time values `compile` are, if any.
    fn level_of(&self, compile: &HashMap<String, Value>) -> Option<Level> {
        let presets = &self.params.as_ref()?.schema.quality;
        LEVELS.into_iter().find(|l| presets.get(l.key()).is_some_and(|p| !p.is_empty() && p.iter().all(|(k, v)| compile.get(k) == Some(v))))
    }

    // The quality panel follows the builds: once the first shader runs it is
    // set up (the level running, if it is one); then each level asked for
    // loads its build (the drawer's own if that is the level), and is done
    // once that runs.
    fn quality(&mut self) {
        let Some(sh) = &self.shader else { return };
        let running = sh.meta.preset.as_deref().and_then(Level::from_key).or_else(|| self.level_of(&sh.meta.compile));
        let drawers = self.params.as_ref().and_then(|p| self.level_of(&p.compile));
        if !self.quality_ready && self.params.is_some() {
            self.quality_ready = true;
            // (with a dev server it builds the levels; a static site has them built)
            let enabled = self.params.as_ref().is_some_and(|p| !p.schema.quality.is_empty());
            self.game.quality_setup(running, running, enabled);
        }
        if let Some(l) = self.quality_loading {
            if running == Some(l) {
                self.quality_loading = None;
                self.game.quality_done(Some(l));
            }
        }
        if let Some(l) = self.game.quality_request() {
            self.quality_loading = Some(l);
            self.source.follow(if drawers == Some(l) { None } else { Some(l.key()) });
        }
    }

    // Makes the next sound effect, if any are still to make; with the last,
    // hands them all to the audio.
    fn make_sfx(&mut self) {
        let n = wizard_shader::sfx::sfx_count as usize;
        if self.sfx.len() >= n {
            return;
        }
        let k = self.sfx.len() as i32;
        let frames = (wizard_shader::sfx::sfx_length(k) * SFX_RATE).ceil() as usize;
        self.sfx.push(wizard_shader::sfx::sfx_samples(k, SFX_RATE, frames));
        if self.sfx.len() == n {
            self.audio.set_build(&self.info, self.sfx.clone());
        }
    }

    fn handle_events(&mut self, frame: &eframe::Frame) {
        let Some(rs) = frame.wgpu_render_state() else { return };
        // (no compute shaders: no shader to build, nothing to draw)
        if self.unsupported.is_some() {
            while self.source.poll().is_some() {}
            return;
        }
        while let Some(ev) = self.source.poll() {
            match ev {
                Event::Schema(s) => self.set_schema(s),
                Event::Building => self.building = true,
                Event::Error(e) => {
                    self.building = false;
                    self.error = Some(e);
                }
                Event::ServerChanged => self.reload = true,
                Event::Shader(files) => {
                    self.building = false;
                    if !self.reload && self.shader.as_ref().is_some_and(|s| s.version == files.version) {
                        continue;
                    }
                    self.reload = false;
                    // natively, off the UI thread (a newer build replaces one
                    // still compiling: its result goes nowhere); in the
                    // browser the page's GPU process compiles, so here
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let device = rs.device.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(Shader::new(&device, &files));
                        });
                        self.compiling = Some((rx, Instant::now()));
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let t = Instant::now();
                        let made = Shader::new(&rs.device, &files);
                        self.install(made, t);
                    }
                }
            }
        }
    }

    // A shader made (or not), started at `since`: in place of the last.
    fn install(&mut self, made: Result<Shader, String>, since: Instant) {
        match made {
            Ok(s) => {
                log::info!("shader {} ({}) loaded in {:.1} s", s.version, if s.spirv { "SPIR-V" } else { "WGSL" }, since.elapsed().as_secs_f32());
                if let Some(p) = self.params.as_mut() {
                    p.compile = s.meta.compile.clone();
                }
                self.shader = Some(s);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("loading the shader: {e}")),
        }
    }

    // Natively: a shader finished compiling off the UI thread, if one has.
    #[cfg(not(target_arch = "wasm32"))]
    fn take_compiled(&mut self) {
        let Some((rx, since)) = &self.compiling else { return };
        let since = *since;
        match rx.try_recv() {
            Ok(made) => {
                self.compiling = None;
                self.install(made, since);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.compiling = None;
                self.error = Some("loading the shader: its thread stopped".into());
            }
        }
    }

    // Whether a shader is being made (natively, off the UI thread).
    fn compiling(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.compiling.is_some();
        #[cfg(target_arch = "wasm32")]
        false
    }

    fn set_schema(&mut self, s: Schema) {
        // (a new schema keeps the values set so far, where they still apply)
        let mut p = Params::new(s, None);
        if let Some(saved) = self.saved_params.take() {
            for (k, v) in saved {
                p.set(&k, v);
            }
        }
        if let Some(old) = &self.params {
            for (k, v) in old.values().0 {
                p.set(&k, v);
            }
        }
        p.can_rebuild = self.source.has_server();
        if let Some(sh) = &self.shader {
            p.compile = sh.meta.compile.clone();
        }
        self.params = Some(p);
    }

    // The top bar, at the top left: the status, and the music and fx
    // switches, then a triangle at its end that folds it away (to just the
    // frame rate); its far corner rounded. (The settings and the
    // parameters are in the pause menu.)
    fn top_bar(&mut self, ctx: &egui::Context) {
        let fill = ctx.global_style().visuals.panel_fill;
        egui::Area::new(egui::Id::unique("hud")).fixed_pos(egui::pos2(0.0, 0.0)).show(ctx, |ui| {
            let corner = egui::CornerRadius { nw: 0, ne: 0, sw: 0, se: 8 };
            egui::Frame::NONE.fill(fill).corner_radius(corner).inner_margin(egui::Margin { left: 8, right: 6, top: 4, bottom: 4 }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.bar_open {
                        let status = match (&self.shader, self.building || self.compiling()) {
                            (_, true) => tr("compiling…").to_string(),
                            (None, _) => tr("loading…").to_string(),
                            (Some(s), _) => format!("{} · {}", &s.version[..8], if s.spirv { "SPIR-V" } else { "WGSL" }),
                        };
                        let size = self.image.as_ref().map(|(t, _)| format!(" · {}×{}", t.width(), t.height())).unwrap_or_default();
                        let scale = format!(" · {:.0}%{}", 100.0 * self.scale.get(), if self.scale.dynamic { tr(" dynamic") } else { "" });
                        let state = format!("{}{}", if self.game.paused() { tr(" · paused") } else { "" }, if self.game.pad_connected() { tr(" · controller") } else { "" });
                        ui.monospace(format!("{status}{size}{scale} · {:>5.1} fps{state}", self.fps));
                        ui.separator();
                        self.audio.toggles(ui);
                    } else {
                        // (room for three digits, so it stays put)
                        ui.monospace(format!("{:>3.0} fps", self.fps));
                    }
                    if crate::game::hud::triangle(ui, self.bar_open).on_hover_text(tr(if self.bar_open { "Fold the bar" } else { "Unfold the bar" })).clicked() {
                        self.bar_open = !self.bar_open;
                    }
                });
            });
        });
    }

    // The pause menu (while paused): tabs of settings, controls and
    // parameters, navigable by keyboard (arrows, Enter; Escape resumes) and
    // controller (the D-pad, A; LT / RT, or LB / RB, the tabs, whose glyphs
    // flank them while it is in use; B or Start resumes): rows of controls
    // (see nav.rs), the first the tabs, where the cursor starts.
    fn pause_menu(&mut self, ctx: &egui::Context, secs: f64) {
        let open = self.game.paused() && !self.game.overlay();
        let opening = open && !self.menu_was_open;
        self.menu_was_open = open;
        if !open {
            return;
        }
        if opening {
            self.nav.go(0, self.menu_tab);
            self.nav.active = true;
        }
        self.nav.begin();
        let nav = &mut self.nav;
        let names = ["Settings", "Controls", "Parameters"].map(tr);
        let pad = self.game.device() == crate::game::spellbar::Device::Pad;
        egui::Window::new(tr("Paused"))
            .id(egui::Id::unique("pause-menu"))
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_width(460.0)
            .show(ctx, |ui| {
                // (centred: by their width last frame)
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - self.tabs_width) / 2.0).max(0.0));
                    let tabs = ui.horizontal(|ui| {
                        if pad {
                            crate::game::hud::chip(ui, crate::game::hud::Chip::Shoulder("LT"));
                            ui.add_space(4.0);
                        }
                        nav.row();
                        for (k, name) in names.iter().enumerate() {
                            let r = crate::game::hud::fill_toggle(ui, name, self.menu_tab == k);
                            nav.item(ui, &r);
                            if r.clicked() {
                                self.menu_tab = k;
                            }
                        }
                        if pad {
                            ui.add_space(4.0);
                            crate::game::hud::chip(ui, crate::game::hud::Chip::Shoulder("RT"));
                        }
                    });
                    self.tabs_width = tabs.response.rect.width();
                });
                ui.separator();
                let tall = ctx.content_rect().height() * 0.7;
                match self.menu_tab {
                    0 => Self::settings(&mut self.game, &mut self.params, &mut self.audio, &mut self.scale, nav, ui),
                    1 => {
                        egui::ScrollArea::vertical().max_height(tall).auto_shrink([false, true]).show(ui, |ui| self.game.controls_ui(ui, nav));
                    }
                    _ => match self.params.as_mut() {
                        Some(p) => {
                            egui::ScrollArea::vertical().max_height(tall).show(ui, |ui| p.ui(ui, secs, nav));
                        }
                        None => {
                            ui.label(tr("(the parameters come with the shader)"));
                        }
                    },
                }
            });
        self.nav.end();
        self.game.menu_holds = self.nav.held();
    }

    // The settings tab: a row of the navigation for each setting.
    fn settings(game: &mut Game, params: &mut Option<Params>, audio: &mut Audio, scale: &mut crate::scale::RenderScale, nav: &mut crate::nav::Nav, ui: &mut egui::Ui) {
        egui::Grid::new("settings").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label(tr("Language"));
            nav.row();
            ui.horizontal(|ui| {
                for l in crate::i18n::LANGS {
                    let r = crate::game::hud::fill_toggle(ui, l.name(), crate::i18n::lang() == l);
                    nav.item(ui, &r);
                    if r.clicked() {
                        crate::i18n::set_lang(l);
                    }
                }
            });
            ui.end_row();

            ui.label(tr("Mode"));
            nav.row();
            ui.horizontal(|ui| game.mode_ui(ui, nav));
            ui.end_row();

            ui.label(tr("Enemies"));
            nav.row();
            ui.horizontal(|ui| game.sides_ui(ui, nav));
            ui.end_row();

            ui.label(tr("Camera"));
            nav.row();
            ui.horizontal(|ui| game.camera_ui(ui, nav));
            ui.end_row();

            ui.label(tr("Platform"));
            nav.row();
            ui.horizontal(|ui| {
                if let Some(p) = params.as_mut() {
                    let platform = p.get("platform").map(|v| v.f32() as usize).unwrap_or(0);
                    for (i, n) in ["Rock", "Ice", "Magic carpet"].map(tr).iter().enumerate() {
                        let r = crate::game::hud::fill_toggle(ui, n, platform == i);
                        nav.item(ui, &r);
                        if r.clicked() {
                            p.set("platform", Value::Num(i as f64));
                        }
                    }
                }
            });
            ui.end_row();

            ui.label(tr("Song"));
            nav.row();
            audio.songs(ui, nav);
            ui.end_row();

            audio.settings(ui, nav);

            ui.label(tr("Quality"));
            nav.row();
            ui.horizontal(|ui| game.quality_ui(ui, nav));
            ui.end_row();

            // the render scale: dynamic, or a slider by hand (which shows
            // the dynamic scale too, and moved, takes over)
            ui.label(tr("Render scale"));
            nav.row();
            ui.horizontal(|ui| {
                let r = crate::game::hud::fill_toggle(ui, tr("dynamic"), scale.dynamic).on_hover_text(tr("Scale the resolution to keep the target frame rate"));
                nav.item(ui, &r);
                if r.clicked() {
                    let on = !scale.dynamic;
                    scale.set_dynamic(on);
                }
                let mut s = scale.get();
                let r = ui.add_sized([140.0, ui.spacing().interact_size.y], egui::Slider::new(&mut s, crate::scale::MIN..=crate::scale::MAX).show_value(false));
                let steps = nav.slider(ui, &r);
                ui.monospace(format!("{:>3.0}%", 100.0 * scale.get()));
                if r.changed() || steps != 0 {
                    scale.set_manual((s + 0.05 * steps as f32).clamp(crate::scale::MIN, crate::scale::MAX));
                }
            });
            ui.end_row();

            ui.label(tr("Target fps"));
            nav.row();
            ui.horizontal(|ui| {
                let mut fps = scale.target_fps;
                let r = ui.add_sized([140.0, ui.spacing().interact_size.y], egui::Slider::new(&mut fps, crate::scale::TARGET_MIN..=crate::scale::TARGET_MAX).step_by(1.0).show_value(false));
                let steps = nav.slider(ui, &r);
                ui.monospace(format!("{fps:>3.0}"));
                if r.changed() || steps != 0 {
                    scale.set_target((fps + 5.0 * steps as f32).clamp(crate::scale::TARGET_MIN, crate::scale::TARGET_MAX));
                }
            });
            ui.end_row();
        });
    }

    // The controller's presses in the pause menu: the D-pad moves its
    // cursor (nav.rs), A presses what it is on (as Enter, sent before the
    // next frame; on a slider, takes hold of it or lets go), B lets go of
    // a slider, the shoulders or triggers change tabs.
    fn menu_from_pad(&mut self) {
        let Some(p) = self.game.menu_input() else { return };
        if p.back && self.nav.held() {
            self.nav.push(crate::nav::Move::Release);
        }
        if p.tab != 0 {
            self.menu_tab = (self.menu_tab as i32 + p.tab).rem_euclid(3) as usize;
            self.nav.go(0, self.menu_tab);
            self.nav.active = true;
        }
        for (on, m) in p.nav.iter().zip(MOVES) {
            if *on {
                self.nav.push(m);
            }
        }
        if p.confirm && self.nav.on_slider() {
            self.nav.push(crate::nav::Move::Press);
        } else if p.confirm {
            for pressed in [true, false] {
                self.menu_events.push(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
            }
        }
    }

    // Renders the scene for this frame into the image, at `vw` x `h`.
    fn render(&mut self, rs: &egui_wgpu::RenderState, vw: u32, h: u32, params: &ParamValues, out: &FrameOut) {
        let Some(shader) = &self.shader else { return };
        let (device, queue) = (&rs.device, &rs.queue);
        // the image, remade when its size changes
        let fits = self.image.as_ref().is_some_and(|(t, _)| t.width() == vw && t.height() == h);
        if !fits {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene"),
                size: wgpu::Extent3d { width: vw, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // (the shader's pixels are gamma-encoded, as egui takes a
                // texture's samples to be: so not an sRGB format, which would
                // decode them)
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            let mut r = rs.renderer.write();
            let id = match self.image.take() {
                Some((_, id)) => {
                    r.update_egui_texture_from_wgpu_texture(device, &view, wgpu::FilterMode::Linear, id);
                    id
                }
                None => r.register_native_texture(device, &view, wgpu::FilterMode::Linear),
            };
            self.image = Some((tex, id));
        }
        let n_spells = (out.spells.len() / wizard_shader::spells::spell_stride as usize) as u32;
        let args = Args { vw, h, tm: out.time, camera: out.camera, n_spells, m_t0: 0.0, m_rate: 0.0, m_n: 0, m_chunk: 0, m_song: 0 };
        let mut enc = device.create_command_encoder(&Default::default());
        self.frame.record(device, queue, &mut enc, shader, params, &out.drive, &args, &out.spells);
        let (tex, _) = self.image.as_ref().unwrap();
        enc.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: self.frame.pixels(),
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(render::padded(vw) * 4), rows_per_image: Some(h) },
            },
            tex.as_image_copy(),
            wgpu::Extent3d { width: vw, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
    }
}

// (the D-pad's and the arrow keys' order: up, down, left, right)
const MOVES: [crate::nav::Move; 4] = [crate::nav::Move::Up, crate::nav::Move::Down, crate::nav::Move::Left, crate::nav::Move::Right];

impl eframe::App for App {
    // (through Subsecond: under `dx serve --hotpatch` an edit to the app's
    // code patches the running app, which keeps its state; otherwise a
    // plain call)
    // (a binding being taken: the keys are its; in the pause menu: the
    // arrow keys move its cursor, not egui's focus, Enter on a slider takes
    // hold of it, Escape lets go, and a click lets the cursor go; the
    // controller's A, as Enter: see menu_from_pad)
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.game.capturing() {
            // (the modifiers as of the last frame)
            let mods = ctx.input(|i| i.modifiers);
            self.game.capture_keys(&mut raw_input.events, mods);
        } else if self.game.paused() && !self.game.overlay() {
            let keys = [egui::Key::ArrowUp, egui::Key::ArrowDown, egui::Key::ArrowLeft, egui::Key::ArrowRight];
            let nav = &mut self.nav;
            raw_input.events.retain(|e| match e {
                egui::Event::Key { key, pressed, .. } if keys.contains(key) => {
                    if *pressed {
                        nav.push(MOVES[keys.iter().position(|k| k == key).unwrap()]);
                    }
                    false
                }
                egui::Event::Key { key: egui::Key::Enter, pressed, .. } if nav.on_slider() => {
                    if *pressed {
                        nav.push(crate::nav::Move::Press);
                    }
                    false
                }
                egui::Event::Key { key: egui::Key::Escape, pressed, .. } if nav.held() => {
                    if *pressed {
                        nav.push(crate::nav::Move::Release);
                    }
                    false
                }
                egui::Event::PointerButton { pressed: true, .. } => {
                    nav.mouse();
                    true
                }
                _ => true,
            });
        }
        raw_input.events.append(&mut self.menu_events);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        subsecond::call(|| self.frame_ui(ui, frame));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_state(storage);
    }
}

impl App {
    // WIZARD_SCREENSHOT=FILE.ppm: after a few seconds, the window into FILE,
    // then quit (to check the native UI without a screen grabber).
    #[cfg(not(target_arch = "wasm32"))]
    fn screenshot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("WIZARD_SCREENSHOT") else { return };
        if (Instant::now() - self.start).as_secs_f32() > 8.0 && !self.shot_asked {
            self.shot_asked = true;
            let c = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let mut ppm = format!("P6\n{} {}\n255\n", img.width(), img.height()).into_bytes();
                for p in &img.pixels {
                    ppm.extend_from_slice(&[p.r(), p.g(), p.b()]);
                }
                let _ = std::fs::write(&path, ppm);
                c.send_viewport_cmd(egui::ViewportCommand::Close);
                c.request_repaint();
            })));
        }
    }

    fn frame_ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(not(target_arch = "wasm32"))]
        self.screenshot(&ctx);
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + 0.1 / dt;
        }
        // (the frame rate says something of the scale only while drawing a
        // shader that is in place, and not paused)
        let drawing = self.shader.is_some() && !self.building && !self.compiling() && self.unsupported.is_none() && !self.game.paused();
        self.scale.frame(dt, drawing);
        self.handle_events(frame);
        #[cfg(not(target_arch = "wasm32"))]
        self.take_compiled();
        let secs = (now - self.start).as_secs_f64();
        let server = self.source.has_server();
        if let Some(p) = self.params.as_mut() {
            p.can_rebuild = server;
            // (recompiled: the drawer's own build, rebuilt with the changes,
            // runs from now on, not a quality level's)
            if let Some(changes) = p.take_compile_changes() {
                self.source.send_config(changes);
                self.source.follow(None);
                self.game.quality_custom();
            }
        }

        {
            let all = self.device_error.lock().unwrap();
            if all.len() > self.device_errors_shown {
                self.device_errors_shown = all.len();
                self.error = Some(all.join("\n\n"));
            }
        }
        if !self.game.picture() {
            self.top_bar(&ctx);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.keep_awake();
        let rs = frame.wgpu_render_state().cloned();
        self.audio.frame(&ctx);
        self.make_sfx();
        self.quality();
        // (M: the music, unless a control has the keys)
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.audio.toggle_music();
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let rect = ui.max_rect();
            let view = ui.interact(rect, egui::Id::unique("wizard-view"), egui::Sense::click_and_drag());
            let ppp = ctx.pixels_per_point();
            let scale = self.scale.get();
            let vw = ((rect.width() * ppp * scale) as u32).max(1);
            let h = ((rect.height() * ppp * scale) as u32).max(1);
            let (Some(rs), Some(p), None) = (&rs, &self.params, &self.unsupported) else { return };
            let mut values = p.values();
            let mut out = self.game.step(&ctx, &view, dt, &values);
            // he dances to the music: on its beat while it plays, easing in
            // and out (the beat carrying on as he stops)
            match self.audio.music_position() {
                Some((song, secs)) => {
                    self.beat_rate = wizard_shader::music::song_bpm(song as i32) as f64 / 60.0;
                    self.beat = secs * self.beat_rate;
                    self.groove += (1.0 - self.groove) * (1.0 - (-dt / 0.6).exp());
                }
                None => {
                    self.beat += dt as f64 * self.beat_rate;
                    self.groove *= (-dt / 0.4).exp();
                }
            }
            out.drive.groove = self.groove;
            out.drive.beat = self.beat as f32;
            for (k, v) in &out.overrides {
                values.0.insert(k.clone(), v.clone());
            }
            // his orb as the renderer poses it (banked: see Game::set_orb),
            // for spells to leave from
            if let Some(sh) = &self.shader {
                if let Some((prm, dr)) = host_records(sh, &values, &out.drive) {
                    let fl = wizard_shader::rig::flight_of(prm, dr, out.time);
                    let o = wizard_shader::world::roll(-fl.bank, wizard_shader::rig::held_orb(prm, dr, out.time));
                    self.game.set_orb([o.x, o.y, o.z]);
                }
            }
            // the collision probe, on the CPU (the shader crate's code)
            if let (Some(req), Some(sh)) = (&out.probe, &self.shader) {
                if let Some((prm, dr)) = host_records(sh, &values, &out.drive) {
                    let mut data = vec![0.0; wizard_shader::collide::probe_len(req.pts.len() / 3)];
                    wizard_shader::collide::probe(&prm, &dr, out.time, &req.pts, &mut data);
                    self.game.apply_probe(req, &data);
                }
            }
            for ev in &out.sfx {
                self.audio.play(*ev);
            }
            self.render(rs, vw, h, &values, &out);
            // the music: a chunk read back is handed over; one asked for is started
            if let Some((req, data)) = self.frame.music_ready() {
                self.audio.music_chunk(req, &data);
            }
            if let (Some(sh), Some(req)) = (&self.shader, self.audio.music_request()) {
                self.frame.music(&rs.device, &rs.queue, sh, &values, &out.drive, req);
            }
            if let Some((_, id)) = &self.image {
                ui.painter().image(*id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
        });
        self.game.ui(&ctx);
        self.menu_from_pad();
        self.pause_menu(&ctx, secs);
        if let Some(e) = &self.unsupported {
            egui::Window::new("No WebGPU").collapsible(false).default_width(420.0).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).show(&ctx, |ui| {
                ui.label(e.as_str());
            });
        } else if let Some(e) = &self.error {
            egui::Window::new("Shader error").collapsible(true).default_width(700.0).show(&ctx, |ui| {
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| ui.monospace(e.as_str()));
            });
        }
        ctx.request_repaint();
    }

    // Natively: while he plays (a shader running, not paused), the screen
    // is kept on and the system awake (an idle inhibitor: on Linux through
    // logind or the screensaver's D-Bus service); paused, let go. (Tried
    // once: without the services, it is not asked again.)
    #[cfg(not(target_arch = "wasm32"))]
    fn keep_awake(&mut self) {
        let playing = self.shader.is_some() && !self.game.paused();
        if playing && self.awake.is_none() && !self.awake_failed {
            match keepawake::Builder::default().display(true).idle(true).reason("Playing").app_name("Wizard").app_reverse_domain("com.causalnexus.wizard").create() {
                Ok(a) => self.awake = Some(a),
                Err(e) => {
                    log::warn!("could not keep the screen on: {e}");
                    self.awake_failed = true;
                }
            }
        } else if !playing {
            self.awake = None;
        }
    }

    fn save_state(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SCALE, &self.scale.manual);
        eframe::set_value(storage, DYNAMIC, &self.scale.dynamic);
        eframe::set_value(storage, BAR, &self.bar_open);
        storage.set_string(LANG, crate::i18n::lang().key().to_string());
        eframe::set_value(storage, TARGET_FPS, &self.scale.target_fps);
        if let Some(p) = &self.params {
            p.save(storage);
        }
        self.game.save(storage);
        self.audio.save(storage);
    }
}

// A parameter of the page's URL (?name=value), in the browser.
pub fn url_param(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        search.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('=')).map(String::from)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = name;
        None
    }
}

// The device the app asks for: the adapter's limits (the shader's nesting
// of calls and its buffers), timestamps where there are any, and SPIR-V
// passthrough where there is (to run rust-gpu's SPIR-V as it is).
pub fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.power_preference = wgpu::PowerPreference::HighPerformance;
    setup.device_descriptor = std::sync::Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
        label: Some("wizard"),
        required_features: adapter.features() & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::PASSTHROUGH_SHADERS),
        required_limits: adapter.limits(),
        ..Default::default()
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() }
}
