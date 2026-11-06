// The game round the wizard (a port of web/main.ts's step() and the game
// parts of the page): his free flight (flight.rs), the orbit camera
// (camera.rs), the modes (mode.rs), spells (spells.rs), the fight with the
// enemy wizards (enemies.rs, combat.rs, mana.rs, shield.rs), controller
// input (gamepad.rs) as bound (binds.rs), where sounds are heard from
// (sfx.rs), the spells shown (spellbar.rs) and the quality panel
// (quality.rs), with the HUD drawn by egui (hud.rs).
//
// The app calls, each frame: step() (input, and everything advanced by dt),
// then the probe it asks for and apply_probe() with its answer, then ui()
// for the HUD; mode_ui(), quality_ui() and controls_ui() go in its pause
// menu. While a binding is being taken, the keys are given to
// capture_keys() before egui sees them.

pub mod camera;
pub mod combat;
pub mod enemies;
pub mod flight;
pub mod binds;
pub mod gamepad;
pub mod hud;
pub mod look;
pub mod mana;
pub mod mode;
pub mod quality;
pub mod sfx;
pub mod shield;
pub mod spellbar;
pub mod spells;
pub mod vec;

use crate::types::{BuildInfo, Camera, FrameOut, ParamValues, ProbeRequest, Value, PROBE_SPHERES};
use camera::Orbit;
use combat::{Combat, Moment};
use egui::{Align2, Key, Margin, Rect, Vec2};
use flight::{FreeFlight, Held, Stick};
use binds::{Action, Binds, KeyBind, ACTIONS};
use gamepad::Pads;
use spellbar::{Device, SpellBar};
use mode::{Mode, ModeName, ORDER};
use quality::Quality;
use spells::Spells;
use std::f32::consts::PI;
use vec::{turn, V3};
use web_time::Instant;

pub use quality::{Level, LEVELS};

// What is kept between runs (sessionStorage / localStorage in the TS).
const STORE_CAMERA: &str = "wizard-camera";
const STORE_FLIGHT: &str = "wizard-flight";
const STORE_MODE: &str = "wizard-mode";
// (renamed as the defaults changed: casting by mouse, not Space)
const STORE_BINDS: &str = "wizard-binds-2";
const STORE_SHOULDER: &str = "wizard-over-shoulder";
const STORE_QUALITY: &str = "wizard-quality";

// A small random number generator (splitmix64) for what the TS takes from
// Math.random: spells' looks, the enemies' timing and aim.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    // Uniform in [0, 1).
    pub fn uniform(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub struct Game {
    orbit: Orbit,
    flight: FreeFlight,
    mode: Mode,
    spells: Spells,
    combat: Combat,
    spellbar: SpellBar,
    quality: Quality,
    binds: Binds,
    // a binding being taken (the next key, or button, pressed), and the
    // modifiers held as it began
    capture: Option<Capture>,
    // the modifiers held last frame (one pressed alone can be bound)
    mods: egui::Modifiers,
    // the mouse's buttons count (bound: casting, the shield): it is taken,
    // and the click that took it let go of
    mouse_armed: bool,
    // the pause menu has a slider held (the controller's B lets it go,
    // rather than resuming): set by the app
    pub menu_holds: bool,
    pads: Pads,
    look: look::MouseLook,
    rng: Rng,
    // the scene's time (stopped while paused)
    time: f32,
    paused: bool,
    // the controller's presses for the pause menu this frame (taken by the
    // app: see menu_input)
    menu: Option<gamepad::Pad>,
    pad_connected: bool,
    // the camera over his shoulder (0..1, easing between), if so chosen
    // (else centred on him), in every mode
    shoulder: f32,
    over_shoulder: bool,
    // picture mode, if on (see Picture)
    picture: Option<Picture>,
    // his orb as the renderer last posed it (his frame, before his heading:
    // see set_orb), and where he was last step and how fast he went
    orb: V3,
    last_pos: Option<V3>,
    vel: V3,
    // the view's area, as of the last step (the HUD goes round it)
    view: Option<Rect>,
    // when the defeat panel appeared (it fades in)
    defeat_since: Option<Instant>,
}

// Picture mode: the scene held still (paused, without the menu) while the
// camera flies freely (the keys or the left stick, as he would; the mouse,
// taken, or the right stick look), to frame a picture. The camera's target
// moves (his frame) and the camera stays close round it; the camera as it
// was, and the view he aimed along, kept for afterwards. Escape, P, B or
// Start go back to the menu.
#[derive(Clone, Copy, Debug)]
struct Picture {
    at: V3,
    saved: crate::types::Camera,
    view: V3,
    since: Instant,
}

// (how fast the picture's camera flies, units a second, and boosted)
const PICTURE_SPEED: f32 = 4.0;
const PICTURE_BOOST: f32 = 4.0;
// (the camera's target at rest, his frame: default_target in the shader)
const TARGET: V3 = [0.0, 1.15, 0.0];

#[derive(Clone, Copy, Debug)]
struct Capture {
    action: Action,
    device: Device,
    since: Instant,
    mods: egui::Modifiers,
}

// (how long a binding waits for its key, or button)
const CAPTURE_SECS: f32 = 6.0;

impl Game {
    pub fn new(storage: Option<&dyn eframe::Storage>) -> Game {
        let get = |key: &str| storage.and_then(|s| s.get_string(key));
        let orbit = storage
            .and_then(|s| eframe::get_value::<Camera>(s, STORE_CAMERA))
            .map_or_else(Orbit::default, Orbit::restored);
        let flight = storage
            .and_then(|s| eframe::get_value::<flight::Saved>(s, STORE_FLIGHT))
            .map_or_else(FreeFlight::default, |s| FreeFlight::restore(&s));
        let mode = Mode::new(get(STORE_MODE).and_then(|m| ModeName::from_key(&m)).unwrap_or(ModeName::Fixed));
        let binds = storage
            .and_then(|s| eframe::get_value::<std::collections::BTreeMap<String, (String, String)>>(s, STORE_BINDS))
            .map_or_else(Binds::default, |b| Binds::restored(&b));
        let quality = Quality::new(get(STORE_QUALITY).and_then(|c| Level::from_key(&c)));
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        let mut g = Game {
            orbit,
            flight,
            mode,
            spells: Spells::default(),
            combat: Combat::default(),
            spellbar: SpellBar::default(),
            quality,
            binds,
            capture: None,
            mods: egui::Modifiers::NONE,
            mouse_armed: false,
            menu_holds: false,
            pads: Pads::default(),
            look: look::MouseLook::default(),
            rng: Rng::new(seed),
            time: 0.0,
            paused: false,
            menu: None,
            pad_connected: false,
            shoulder: 0.0,
            over_shoulder: get(STORE_SHOULDER).as_deref() != Some("0"),
            picture: None,
            orb: [0.55, 2.1, 0.5],
            last_pos: None,
            vel: [0.0; 3],
            view: None,
            defeat_since: None,
        };
        // (the flight as the mode has it, the camera as it was: main.ts sets
        // this before free flight's take-off turns the camera)
        g.flight.set_active(g.mode.free());
        g
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORE_CAMERA, &self.orbit.cam);
        eframe::set_value(storage, STORE_FLIGHT, &self.flight.saved());
        storage.set_string(STORE_MODE, self.mode.name.key().to_string());
        eframe::set_value(storage, STORE_BINDS, &self.binds.saved());
        storage.set_string(STORE_SHOULDER, if self.over_shoulder { "1" } else { "0" }.to_string());
        if let Some(c) = self.quality.choice {
            storage.set_string(STORE_QUALITY, c.key().to_string());
        }
    }

    pub fn set_build(&mut self, info: &BuildInfo) {
        self.spells.limit = info.spell_limit.max(1) as usize;
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    // Pauses or resumes (the pause menu shows while paused).
    pub fn set_paused(&mut self, on: bool) {
        self.paused = on;
    }

    // The controller's presses this frame while paused (for the pause
    // menu's navigation), taken once.
    pub fn menu_input(&mut self) -> Option<gamepad::Pad> {
        self.menu.take()
    }

    // Starts the fight again (from the pause menu).
    pub fn reset_fight(&mut self) {
        self.combat.reset(&mut self.spells);
    }

    // The device used last (whose glyphs are shown).
    pub fn device(&self) -> Device {
        self.spellbar.device
    }

    // Whether a binding is being taken (the keys are then capture_keys's).
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    // While a binding is being taken: the keys pressed (taken out of
    // `events`, so nothing else acts on them), and the modifiers held.
    // Escape gives up; a key (or a modifier pressed alone) is taken for the
    // keyboard's binding.
    pub fn capture_keys(&mut self, events: &mut Vec<egui::Event>, mods: egui::Modifiers) {
        let Some(c) = self.capture else { return };
        let mut taken = None;
        for e in events.iter() {
            match e {
                egui::Event::Key { key, pressed: true, repeat: false, .. } => {
                    taken = Some(if *key == Key::Escape { None } else { Some(KeyBind::Key(*key)) });
                    break;
                }
                // (a mouse button, for the keyboard's binding)
                egui::Event::PointerButton { button, pressed: true, .. } if c.device == Device::Keyboard => {
                    taken = Some(Some(KeyBind::Mouse(*button)));
                    break;
                }
                _ => {}
            }
        }
        if taken.is_none() && c.device == Device::Keyboard {
            if let Some(m) = KeyBind::modifier_pressed(c.mods, mods) {
                taken = Some(Some(m));
            }
        }
        events.retain(|e| !matches!(e, egui::Event::Key { .. }) && !(c.device == Device::Keyboard && matches!(e, egui::Event::PointerButton { .. })));
        if let Some(k) = taken {
            if let (Some(k), Device::Keyboard) = (k, c.device) {
                self.binds.set_key(c.action, k);
            }
            self.capture = None;
        }
    }

    // Whether a game controller is connected (for the status line).
    pub fn pad_connected(&self) -> bool {
        self.pad_connected
    }

    pub fn camera(&self) -> Camera {
        self.orbit.cam
    }

    // The flight and the fight as the mode, just changed, has them: free
    // flight starts from behind him; a fight starts afresh.
    fn follow_mode(&mut self) {
        if self.flight.set_active(self.mode.free()) && self.flight.active {
            self.orbit.cam.yaw = PI;
            self.orbit.cam.pitch = 0.2;
            self.orbit.cam.dist = 3.2;
            self.orbit.interacted = true;
        }
        if self.mode.fight() {
            self.combat.restart();
        }
    }

    fn set_mode(&mut self, m: ModeName) {
        if self.mode.set(m) {
            self.follow_mode();
        }
    }

    fn cycle_mode(&mut self) {
        if self.mode.cycle() {
            self.follow_mode();
        }
    }

    // Enemies for the renderer: the drawer's count in fight mode, else none.
    fn enemy_count(&self, params: &ParamValues) -> usize {
        if self.mode.fight() { params.f32("enemies").clamp(0.0, 6.0) as usize } else { 0 }
    }

    // Where the renderer has his orb (posed as he holds the staff, and
    // banked, but not yet turned to his heading nor moved to where he is),
    // so spells leave from it: from the app, each frame.
    pub fn set_orb(&mut self, orb: V3) {
        self.orb = orb;
    }

    // The orb, and the way to cast from it, in the world: towards the point
    // AIM_AT along the camera's view (from over his shoulder, the view's
    // centre, where the crosshair is, is not in line with the orb).
    fn cast_from(&self) -> (V3, V3) {
        const AIM_AT: f32 = 25.0;
        let (pos, heading) = self.flight.place(self.time);
        let orb = self.orb;
        let aim = vec::add(self.orbit.eye(self.shoulder), vec::scale(AIM_AT, self.orbit.view_dir()));
        let dir = vec::normalise(vec::sub(aim, orb));
        (vec::add(pos, turn(heading, orb)), turn(heading, dir))
    }

    // Advances everything by dt (input, camera, flight, spells, the fight).
    // `params` must be the drawer's values, without this game's overrides
    // (it reads the drawer's enemy count from them).
    pub fn step(&mut self, ctx: &egui::Context, view: &egui::Response, dt: f32, params: &ParamValues) -> FrameOut {
        self.view = Some(view.rect);
        // (pressing on the view takes the keys back from the drawer's controls)
        if view.is_pointer_button_down_on() {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            self.orbit.interacted = true;
        }
        // the mouse: a click on the view takes it (not while paused, nor
        // defeated), and it then turns the camera
        let defeated = self.combat.hud.lost.is_some();
        let picture = self.picture.is_some();
        if view.clicked() && (!self.paused || picture) && (!defeated || picture) && !self.look.held() {
            self.look.grab(ctx);
        }
        let (dx, dy) = self.look.take(ctx);
        if dx != 0.0 || dy != 0.0 {
            self.orbit.look(dx, dy);
        }
        if (view.hovered() || self.look.held()) && !picture {
            // (egui's wheel is points up; the TS's deltaY is pixels down,
            // about 100 a notch to egui's 50)
            let dy = ctx.input(|i| i.smooth_scroll_delta.y);
            if dy != 0.0 {
                self.orbit.wheel(-2.0 * dy);
            }
        }

        // the keyboard (unless a text field or the like has it)
        let mut held = Held::default();
        let mut casting = false;
        let binds = self.binds.clone();
        // (the mouse's buttons count once it is taken, and the click that
        // took it has been let go of)
        if !self.look.held() {
            self.mouse_armed = false;
        } else if !ctx.input(|i| i.pointer.any_down()) {
            self.mouse_armed = true;
        }
        let mouse = self.mouse_armed;
        if !ctx.egui_wants_keyboard_input() {
            let (keys, typed_any, mods) = ctx.input(|i| {
                let pressed: Vec<(Key, egui::Modifiers)> = i
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                        _ => None,
                    })
                    .collect();
                let any = i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }));
                // (as bound; the arrows fly too)
                let down = |a: Action| binds.key(a).is_some_and(|k| (mouse || !k.is_mouse()) && k.down(i));
                held = Held {
                    fwd: down(Action::Forward) || i.key_down(Key::ArrowUp),
                    back: down(Action::Back) || i.key_down(Key::ArrowDown),
                    left: down(Action::Left) || i.key_down(Key::ArrowLeft),
                    right: down(Action::Right) || i.key_down(Key::ArrowRight),
                    up: down(Action::Climb),
                    down: down(Action::Descend),
                    boost: down(Action::Boost),
                };
                casting = down(Action::Cast);
                self.combat.shield.wanted = down(Action::Shield);
                (pressed, any, i.modifiers)
            });
            if typed_any {
                self.spellbar.device = Device::Keyboard;
            }
            // (the actions pressed: their keys, without Ctrl, Alt or
            // Command, or a modifier bound alone)
            let mut acts: Vec<Action> = keys
                .iter()
                .filter(|(_, m)| !m.ctrl && !m.command && !m.alt && !m.mac_cmd)
                .filter_map(|(k, _)| binds.key_action(KeyBind::Key(*k)))
                .collect();
            acts.extend(KeyBind::modifier_pressed(self.mods, mods).and_then(|m| binds.key_action(m)));
            if mouse {
                acts.extend(ctx.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            egui::Event::PointerButton { button, pressed: true, .. } => binds.key_action(KeyBind::Mouse(*button)),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                }));
            }
            self.mods = mods;
            for a in acts {
                match a {
                    Action::Mode => self.cycle_mode(),
                    Action::Pause => self.pause_pressed(),
                    Action::Spell(s) => self.spells.select(s),
                    _ => {}
                }
            }
            for (key, _) in keys {
                match key {
                    Key::Escape => self.pause_pressed(),
                    Key::Enter if self.combat.enemies.down >= 0.0 && !self.paused => self.combat.reset(&mut self.spells),
                    _ => {}
                }
            }
        } else {
            self.combat.shield.wanted = false;
        }

        // (let go of by the browser, Escape: paused; paused, or defeated,
        // or the window left: let go of)
        if self.look.lost() {
            self.paused = true;
        }
        let picture = self.picture.is_some();
        if (self.paused || defeated) && !picture || !ctx.input(|i| i.focused) {
            self.look.release(ctx);
        }
        if !self.paused {
            self.time += dt;
        }
        self.orbit.idle(dt);
        let pad = self.pads.read(&self.binds);
        self.pad_connected = pad.is_some();
        // (a binding being taken: the button pressed, if it is the
        // controller's; nothing else acts on the controller meanwhile, nor
        // on the press that ends it)
        let capturing = self.capture.is_some();
        if let Some(c) = self.capture {
            if c.since.elapsed().as_secs_f32() > CAPTURE_SECS {
                self.capture = None;
            } else if let (Some(b), Device::Pad) = (pad.and_then(|p| p.pressed), c.device) {
                self.binds.set_pad(c.action, b);
                self.capture = None;
            }
        }
        if let Some(p) = pad.as_ref().filter(|_| !capturing) {
            // (the mode's button cycles the mode; pause's pauses, B resumes,
            // unless the menu has a slider held)
            if p.mode {
                self.cycle_mode();
            }
            if p.pause || (p.back && self.paused && !self.menu_holds) {
                self.pause_pressed();
            }
            if p.active {
                self.spellbar.device = Device::Pad;
            }
            if self.picture.is_some() {
                // (in picture mode, the sticks fly the camera: below)
            } else if self.paused {
                // (in the pause menu, the rest is its navigation)
                self.menu = Some(*p);
            } else {
                // (A tries again once he is defeated)
                if p.confirm && self.combat.enemies.down >= 0.0 {
                    self.combat.reset(&mut self.spells);
                }
                if let Some(s) = p.spell {
                    self.spells.select(s);
                }
            }
        }
        let n = self.enemy_count(params);
        let pad_cast = pad.is_some_and(|p| p.cast);
        let game_dt = if self.paused { 0.0 } else { dt };
        let (origin, dir) = self.cast_from();
        // (how fast he goes: from where he was last step; a jump, as the
        // mode changes, not counted)
        let him = self.flight.place(self.time).0;
        if game_dt > 0.0 {
            let v = self.last_pos.map_or([0.0; 3], |p| vec::scale(1.0 / game_dt, vec::sub(him, p)));
            self.vel = if vec::len(v) < 60.0 { v } else { [0.0; 3] };
            self.last_pos = Some(him);
        }
        let moment = Moment {
            dt: game_dt,
            time: self.time,
            him,
            vel: self.vel,
            origin,
            dir,
            drawn: true,
            able: !self.paused,
            n,
            seed: params.f32("enemy_seed") as i64,
            casting: casting || pad_cast,
            shielding: pad.is_some_and(|p| p.shield),
        };
        self.combat.update(&moment, &mut self.spells, &mut self.rng);
        self.spells.update(game_dt, casting || pad_cast);
        self.combat.handle(&mut self.spells);
        self.spellbar.spell = self.spells.selected;
        self.spellbar.cooling = [0, 1, 2, 3].map(|i| self.spells.cooling(i));
        self.spellbar.heat = self.spells.gatling.heat;
        self.spellbar.overheated = self.spells.gatling.overheated;
        // right stick: look right turns the view right (the camera's yaw is its
        // azimuth round him, so it decreases), up raises the view
        // (and the D-pad zooms)
        if let Some(p) = &pad {
            self.orbit.nudge(-2.5 * p.look[0] * dt, -1.5 * p.look[1] * dt);
            if !self.paused {
                self.orbit.zoom(1.5 * p.zoom * dt);
            }
        }
        // The camera orbits in his frame, which turns with him; in free flight
        // keep it still in the world instead, so W (along the camera) does not
        // chase its own tail as he turns to face the way he goes. (camera_ray
        // in fut/wizard.fut looks along -(sin yaw, cos yaw) in his frame.)
        let heading = self.flight.heading;
        let stick = pad.map(|p| Stick { fwd: p.mov[1], side: p.mov[0], up: p.up, boost: p.boost });
        // (picture mode: the keys and the left stick fly the camera, along
        // its view, across it and up; he stays put)
        if let Some(pic) = &mut self.picture {
            let b = |x: bool| x as i32 as f32;
            let s = stick.unwrap_or_default();
            let fwd = (b(held.fwd) - b(held.back) + s.fwd).clamp(-1.0, 1.0);
            let side = (b(held.right) - b(held.left) + s.side).clamp(-1.0, 1.0);
            let up = (b(held.up) - b(held.down) + s.up).clamp(-1.0, 1.0);
            let speed = PICTURE_SPEED * if held.boost || s.boost { PICTURE_BOOST } else { 1.0 };
            let f = self.orbit.view_dir();
            let r = vec::normalise([-f[2], 0.0, f[0]]);
            let step = vec::add(vec::add(vec::scale(fwd, f), vec::scale(side, r)), [0.0, up, 0.0]);
            pic.at = vec::add(pic.at, vec::scale(speed * dt, step));
        }
        let (held, stick) = if self.picture.is_some() { (Held::default(), None) } else { (held, stick) };
        // (channelling healing, he cannot move: no input, and he is brought
        // to a stop)
        let channelling = self.spells.channel >= 0.0;
        let (held, stick) = if channelling { (flight::Held::default(), None) } else { (held, stick) };
        self.flight.update(game_dt, self.time, heading + self.orbit.cam.yaw + PI, &held, stick.as_ref());
        if channelling {
            self.flight.brake(game_dt);
        }
        if self.flight.active {
            self.orbit.cam.yaw -= self.flight.heading - heading;
        }

        // the drive record: the flight, casting (in his frame: along the
        // camera's view), and the fight
        let mut drive = self.flight.drive();
        let goal = if self.over_shoulder && self.picture.is_none() { 1.0 } else { 0.0 };
        self.shoulder += (goal - self.shoulder) * (1.0 - (-dt / 0.35).exp());
        drive.shoulder = self.shoulder;
        // (picture mode: the camera's target moved, nothing keeping it above
        // his platform; his aim as it was)
        let [sx, sy, sz] = match &self.picture {
            Some(p) => {
                drive.picture = 1.0;
                [drive.cam_x, drive.cam_y, drive.cam_z] = p.at;
                p.view
            }
            None => self.orbit.view_dir(),
        };
        drive.staff = self.spells.staff;
        drive.sx = sx;
        drive.sy = sy;
        drive.sz = sz;
        drive.flash = self.spells.since_cast;
        drive.spell = self.spells.selected as f32;
        let (shield, shield_hit, arena, foes, kills) = self.combat.drive();
        drive.shield = shield;
        drive.shield_hit = shield_hit;
        [drive.arena_x, drive.arena_y, drive.arena_z] = arena;
        drive.foe = foes;
        drive.kill = kills;


        // (and for a lock on the ground, its height)
        let probe = (self.flight.active || self.spells.count() > 0 || self.spells.lock.is_some()).then(|| {
            let (ids, pts) = self.spells.probe_points();
            ProbeRequest { ids, pts }
        });
        FrameOut {
            time: self.time,
            camera: self.orbit.cam,
            drive,
            spells: self.spells.pack(),
            probe,
            overrides: vec![("enemies".to_string(), Value::Num(n as f64))],
            sfx: std::mem::take(&mut self.combat.sfx).into_iter().filter(|e| e.gain >= 0.02).collect(),
            paused: self.paused,
        }
    }

    // The probe's answer to `req`: PROBE_SPHERES spheres round the rock and
    // him, then the request's points, 4 floats each. (The sounds it sets off
    // go out with the next frame.)
    pub fn apply_probe(&mut self, req: &ProbeRequest, data: &[f32]) {
        let spheres = (PROBE_SPHERES * 4).min(data.len());
        self.flight.collide(&data[..spheres]);
        self.spells.apply_probe(&req.ids, &data[spheres..]);
        self.combat.handle(&mut self.spells);
    }

    // The mode buttons (for the pause menu: a row of its navigation).
    pub fn mode_ui(&mut self, ui: &mut egui::Ui, nav: &mut crate::nav::Nav) {
        let mut chosen = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for m in ORDER {
                let r = ui.add_enabled_ui(self.mode.allowed(m), |ui| {
                    hud::toggle_button(ui, m.title(), m == self.mode.name, false)
                });
                let r = r.inner.on_hover_text(self.mode.tooltip(m)).on_disabled_hover_text(self.mode.tooltip(m));
                nav.item(ui, &r);
                if r.clicked() {
                    chosen = Some(m);
                }
            }
        })
        .response
        .on_hover_text("Mode (F)");
        if let Some(m) = chosen {
            self.set_mode(m);
        }
    }

    // Whether in picture mode (the scene still, the camera flown freely:
    // the app shows no menu nor bar).
    pub fn picture(&self) -> bool {
        self.picture.is_some()
    }

    // Into picture mode, from the pause menu: the camera from where it is.
    pub fn enter_picture(&mut self) {
        let eye = self.orbit.eye(self.shoulder);
        let saved = self.orbit.cam;
        self.picture = Some(Picture { at: vec::sub(eye, TARGET), saved, view: self.orbit.view_dir(), since: Instant::now() });
        self.orbit.cam.dist = 0.05;
        self.orbit.interacted = true;
        self.shoulder = 0.0;
        self.paused = true;
    }

    // Back to the pause menu, the camera as it was.
    fn leave_picture(&mut self) {
        if let Some(p) = self.picture.take() {
            self.orbit.cam = p.saved;
        }
    }

    // Escape, P, B or Start: picture mode left (to the menu), or else the
    // pause toggled.
    fn pause_pressed(&mut self) {
        if self.picture.is_some() {
            self.leave_picture();
        } else {
            self.paused = !self.paused;
        }
    }

    // The camera's choice (for the pause menu: a row of its navigation):
    // over his shoulder as he flies, or centred on him.
    pub fn camera_ui(&mut self, ui: &mut egui::Ui, nav: &mut crate::nav::Nav) {
        for (name, over, tip) in [
            ("Over the shoulder", true, "The camera looks past his right shoulder"),
            ("Centred", false, "The camera looks at him, centred"),
        ] {
            let r = hud::fill_toggle(ui, name, self.over_shoulder == over).on_hover_text(tip);
            nav.item(ui, &r);
            if r.clicked() {
                self.over_shoulder = over;
            }
        }
        let r = hud::fill_toggle(ui, "Picture", false).on_hover_text("Hold the scene still and fly the camera freely to frame a picture (Escape to come back)");
        nav.item(ui, &r);
        if r.clicked() {
            self.enter_picture();
        }
    }

    // The quality buttons (for the pause menu: a row of its navigation;
    // nothing while it is disabled).
    pub fn quality_ui(&mut self, ui: &mut egui::Ui, nav: &mut crate::nav::Nav) {
        self.quality.ui(ui, nav);
    }

    // Sets the quality panel up: `level` the level running now, if known;
    // `fallback` the choice when none is stored (the level running);
    // `enabled` false hides it (e.g. no presets, or a pinned build).
    pub fn quality_setup(&mut self, level: Option<Level>, fallback: Option<Level>, enabled: bool) {
        self.quality.setup(level, fallback, enabled);
    }

    // A level the app should load now (polled once a frame): it then calls
    // quality_done once that level runs, or failed to load.
    pub fn quality_request(&mut self) -> Option<Level> {
        self.quality.take_request()
    }

    pub fn quality_done(&mut self, level: Option<Level>) {
        self.quality.done(level);
    }

    // The HUD: the mode's note, the vitals, the crosshair, the hurt flash,
    // the defeat panel and the controls panel.
    pub fn ui(&mut self, ctx: &egui::Context) {
        let rect = self.view.unwrap_or_else(|| ctx.content_rect());
        // (picture mode: nothing in the way but a note of how to leave, a
        // few seconds)
        if let Some(p) = &self.picture {
            let t = p.since.elapsed().as_secs_f32();
            let alpha = if t < 3.0 { 1.0 } else { (1.0 - (t - 3.0) / 0.6).max(0.0) };
            if alpha > 0.0 {
                ctx.request_repaint();
                egui::Area::new(egui::Id::unique("wizard-picture-note"))
                    .pivot(Align2::CENTER_TOP)
                    .fixed_pos(egui::pos2(rect.center().x, rect.top() + 16.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        ui.set_opacity(alpha);
                        hud::panel(0.6, 6, Margin::symmetric(14, 6)).show(ui, |ui| {
                            ui.label(hud::text("Picture mode · click to look, fly as he would · Esc to return", hud::FONT, hud::FG));
                        });
                    });
            }
            return;
        }
        let h = self.combat.hud.clone();

        // the red flash, and the crosshair at the view's centre (not in the
        // way of the pointer)
        egui::Area::new(egui::Id::unique("wizard-overlay"))
            .fixed_pos(rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter_at(rect);
                hud::hurt(&painter, rect, h.hurt);
                if h.drawn && h.lost.is_none() {
                    hud::crosshair(&painter, rect.center(), h.locked, h.shielded);
                }
            });

        // the mode's note: it shows for 1.6 s on a change, then fades
        if let Some(t) = self.mode.noted {
            let t = t.elapsed().as_secs_f32();
            let alpha = if t < 1.6 { (t / 0.1).min(1.0) } else { (1.0 - (t - 1.6) / 0.4).max(0.0) };
            if alpha > 0.0 {
                ctx.request_repaint();
                egui::Area::new(egui::Id::unique("wizard-mode-note"))
                    .pivot(Align2::CENTER_TOP)
                    .fixed_pos(egui::pos2(rect.center().x, rect.top() + 0.22 * rect.height()))
                    .interactable(false)
                    .show(ctx, |ui| {
                        ui.set_opacity(alpha);
                        hud::panel(0.6, 6, Margin::symmetric(14, 6)).show(ui, |ui| {
                            ui.label(hud::text(self.mode.name.label(), hud::FONT, hud::FG));
                        });
                    });
            } else {
                self.mode.noted = None;
            }
        }

        // the vitals, bottom left
        let mut reset = false;
        if h.drawn {
            egui::Area::new(egui::Id::unique("wizard-vitals"))
                .pivot(Align2::LEFT_BOTTOM)
                .fixed_pos(rect.left_bottom() + Vec2::new(16.0, -16.0))
                .show(ctx, |ui| {
                    hud::panel(0.6, 6, Margin::symmetric(12, 8)).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                        let width = 200.0f32.min(0.4 * rect.width());
                        let health = ctx.animate_value_with_time(egui::Id::unique("wizard-health"), h.health, 0.15);
                        let mana = ctx.animate_value_with_time(egui::Id::unique("wizard-mana"), h.mana, 0.15);
                        if h.n > 0 {
                            ui.horizontal(|ui| {
                                hud::icon(ui, hud::Icon::Heart, hud::FONT, hud::FG, egui::Sense::hover());
                                hud::bar(
                                    ui,
                                    width,
                                    health,
                                    egui::Color32::from_rgb(0xc0, 0x28, 0x3c),
                                    egui::Color32::from_rgb(0xff, 0x6a, 0x5a),
                                    hud::BORDER,
                                );
                            })
                            .response
                            .on_hover_text("Health");
                        }
                        ui.horizontal(|ui| {
                            hud::icon(ui, hud::Icon::Star, hud::FONT, hud::FG, egui::Sense::hover());
                            hud::bar(
                                ui,
                                width,
                                mana,
                                egui::Color32::from_rgb(0x2a, 0x4f, 0xd0),
                                egui::Color32::from_rgb(0x6f, 0xb4, 0xff),
                                if h.short { hud::SHORT } else { hud::BORDER },
                            );
                        })
                        .response
                        .on_hover_text("Mana");
                        if h.n > 0 {
                            ui.horizontal(|ui| {
                                ui.label(hud::text(format!("{} defeated", h.defeated), 0.85 * hud::FONT, hud::DIM));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let b = hud::plain_button(ui, "↺ Reset", 0.85 * hud::FONT, Vec2::new(8.0, 2.0));
                                    if b.on_hover_text("Start the fight again (View / Back on a controller)").clicked() {
                                        reset = true;
                                    }
                                });
                            });
                        }
                    });
                });
        }

        // defeated: a panel says so, until he tries again
        match (&h.lost, self.defeat_since) {
            (Some(_), None) => self.defeat_since = Some(Instant::now()),
            (None, Some(_)) => self.defeat_since = None,
            _ => {}
        }
        if let (Some(stats), Some(since)) = (&h.lost, self.defeat_since) {
            let alpha = (since.elapsed().as_secs_f32() / 0.6).min(1.0);
            if alpha < 1.0 {
                ctx.request_repaint();
            }
            egui::Area::new(egui::Id::unique("wizard-defeat"))
                .pivot(Align2::CENTER_CENTER)
                .fixed_pos(egui::pos2(rect.center().x, rect.top() + 0.4 * rect.height()))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    // (ease-out)
                    ui.set_opacity(1.0 - (1.0 - alpha).powi(2));
                    hud::panel(0.8, 8, Margin::symmetric(32, 20)).show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 10.0;
                            ui.label(hud::text("Defeated", 1.8 * hud::FONT, hud::DEFEAT).extra_letter_spacing(0.1 * 1.8 * hud::FONT));
                            ui.label(hud::text(stats.clone(), hud::FONT, hud::FG));
                            if hud::plain_button(ui, "Try again", 1.05 * hud::FONT, Vec2::new(16.0, 4.0)).clicked() {
                                reset = true;
                            }
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.label(hud::text("or press Enter (", 0.8 * hud::FONT, hud::DIM));
                                hud::chip(ui, hud::Chip::Face('A'));
                                ui.label(hud::text(" on a controller)", 0.8 * hud::FONT, hud::DIM));
                            });
                        });
                    });
                });
        }
        if reset {
            self.combat.reset(&mut self.spells);
        }

        self.spellbar.ui(ctx, rect, &self.binds);
    }

    // The controls tab of the pause menu: each action's key and controller
    // button, either of which can be chosen (a row of the navigation each)
    // and then bound to the next key, or button, pressed; and the controls
    // that are fixed.
    pub fn controls_ui(&mut self, ui: &mut egui::Ui, nav: &mut crate::nav::Nav) {
        use hud::Chip;
        let now_mods = ui.input(|i| i.modifiers);
        egui::Grid::new("controls").num_columns(3).min_col_width(130.0).spacing([24.0, 6.0]).show(ui, |ui| {
            ui.label("");
            ui.label(hud::text("Keyboard", 12.0, hud::DIM));
            ui.label(hud::text("Controller", 12.0, hud::DIM));
            ui.end_row();
            for a in ACTIONS {
                ui.label(a.label());
                nav.row();
                for device in [Device::Keyboard, Device::Pad] {
                    let waiting = self.capture.is_some_and(|c| c.action == a && c.device == device);
                    let chip = match device {
                        Device::Keyboard => self.binds.key(a).map(|k| Chip::Key(k.name())),
                        Device::Pad => self.binds.pad(a).map(|b| b.chip()),
                    };
                    let r = bind_cell(ui, (a, device), chip, waiting);
                    nav.item(ui, &r);
                    if r.clicked() {
                        self.capture = if waiting { None } else { Some(Capture { action: a, device, since: Instant::now(), mods: now_mods }) };
                    }
                }
                ui.end_row();
            }
            // (fixed)
            for (what, key, pad) in [
                ("Fly (controller)", Chip::Text("—"), Chip::Stick('L')),
                ("Camera (click to take the mouse)", Chip::Text("mouse · wheel"), Chip::Stick('R')),
                ("Try again (defeated)", Chip::Key("Enter"), Chip::Face('A')),
                ("Music", Chip::Key("M"), Chip::Text("—")),
            ] {
                ui.label(hud::text(what, 12.5, hud::DIM));
                for c in [key, pad] {
                    ui.horizontal(|ui| {
                        ui.add_space(7.0);
                        hud::chip(ui, c);
                    });
                }
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        nav.row();
        let r = ui.button("Reset to defaults");
        nav.item(ui, &r);
        if r.clicked() {
            self.binds = Binds::default();
            self.capture = None;
        }
        if self.capture.is_some() {
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Drive, ParamValues, Value, ON_PATH};

    fn params(enemies: f64) -> ParamValues {
        let mut p = ParamValues::default();
        p.0.insert("enemies".into(), Value::Num(enemies));
        p.0.insert("enemy_seed".into(), Value::Num(1.0));
        p
    }

    // A frame of the app: the view's area, step, the probe (land far
    // below), the HUD.
    fn frame(ctx: &egui::Context, g: &mut Game, p: &ParamValues, events: Vec<egui::Event>) -> FrameOut {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, 720.0))),
            events,
            ..Default::default()
        };
        let mut out = None;
        let mut full = ctx.run_ui(input, |ui| {
            let view = ui.allocate_response(ui.available_size(), egui::Sense::click_and_drag());
            let o = g.step(ui.ctx(), &view, 1.0 / 60.0, p);
            if let Some(req) = &o.probe {
                let n = PROBE_SPHERES + req.ids.len().max(req.pts.len() / 3);
                g.apply_probe(req, &vec![50.0; n * 4]);
            }
            g.ui(ui.ctx());
            out = Some(o);
        });
        full.textures_delta.clear();
        out.unwrap()
    }

    // A key pressed and let go.
    fn key(k: Key) -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: Default::default() })
            .to_vec()
    }

    #[test]
    fn picture_mode_holds_the_scene_and_flies_the_camera() {
        let ctx = egui::Context::default();
        let mut g = Game::new(None);
        let p = params(0.0);
        frame(&ctx, &mut g, &p, key(Key::P));
        let cam = g.orbit.cam;
        g.enter_picture();
        let o0 = frame(&ctx, &mut g, &p, vec![]);
        let (t, at0) = (o0.time, [o0.drive.cam_x, o0.drive.cam_y, o0.drive.cam_z]);
        // W held: the camera's target moves along its view; the scene's
        // time does not
        let w = egui::Event::Key { key: Key::W, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() };
        let mut o = frame(&ctx, &mut g, &p, vec![w]);
        for _ in 0..30 {
            o = frame(&ctx, &mut g, &p, vec![]);
        }
        assert_eq!(o.time, t);
        assert_eq!(o.drive.picture, 1.0);
        let at = [o.drive.cam_x, o.drive.cam_y, o.drive.cam_z];
        let d = vec::sub(at, at0);
        let along = vec::dot(d, g.orbit.view_dir());
        assert!(along > 1.5 && (vec::len(d) - along).abs() < 1e-3, "{d:?}");
        // Escape: back to the menu (still paused), the camera as it was
        let o = frame(&ctx, &mut g, &p, key(Key::Escape));
        assert!(!g.picture() && g.paused() && o.paused);
        assert_eq!(g.orbit.cam, cam);
        assert_eq!(frame(&ctx, &mut g, &p, vec![]).drive.picture, 0.0);
    }

    #[test]
    fn plays_a_fight() {
        let ctx = egui::Context::default();
        let mut g = Game::new(None);
        let p = params(3.0);
        let o = frame(&ctx, &mut g, &p, vec![]);
        assert_eq!(o.drive, Drive { staff: o.drive.staff, shoulder: o.drive.shoulder, sx: o.drive.sx, sy: o.drive.sy, sz: o.drive.sz, flash: 10.0 + 1.0 / 60.0, spell: 1.0, shield_hit: 10.0 + 1.0 / 60.0, ..ON_PATH });
        assert!(o.probe.is_none());
        assert_eq!(o.overrides, vec![("enemies".to_string(), Value::Num(0.0))]);
        // F twice: fight
        frame(&ctx, &mut g, &p, key(Key::F));
        let o = frame(&ctx, &mut g, &p, key(Key::F));
        assert!(o.drive.free);
        assert_eq!(g.mode.name, ModeName::Fight);
        assert_eq!(o.overrides, vec![("enemies".to_string(), Value::Num(3.0))]);
        let req = o.probe.unwrap();
        assert_eq!(req.pts.len(), 6 * 3);
        // the enemies appear, and cast at him
        let mut sounds = Vec::new();
        for _ in 0..60 * 12 {
            let o = frame(&ctx, &mut g, &p, vec![]);
            sounds.extend(o.sfx.iter().map(|e| e.kind));
            assert_eq!(o.spells.len(), 6 * spells::STRIDE);
        }
        assert!(o.drive.foe[0] >= 16.0);
        assert!(sounds.iter().any(|&k| k == sfx::CAST_FIRE || k == sfx::CAST_WIND || k == sfx::MARK), "{sounds:?}");
        // P pauses the scene's time
        let t = frame(&ctx, &mut g, &p, key(Key::P)).time;
        let o = frame(&ctx, &mut g, &p, vec![]);
        assert!(o.paused);
        assert_eq!(o.time, t);
    }
}

// A binding's cell in the controls tab: its glyph (or a prompt while it
// waits for a key or button), clickable.
fn bind_cell(ui: &mut egui::Ui, id: (Action, Device), chip: Option<hud::Chip>, waiting: bool) -> egui::Response {
    let stroke = egui::Stroke::new(1.0, if waiting { hud::FOCUS } else { hud::BORDER });
    let inner = egui::Frame::new().stroke(stroke).corner_radius(4).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.set_min_width(84.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            match (waiting, chip) {
                (true, _) => {
                    ui.label(hud::text("press…", 12.0, hud::FOCUS));
                }
                (false, Some(c)) => hud::chip(ui, c),
                (false, None) => {
                    ui.label(hud::text("—", 12.0, hud::DIM));
                }
            }
        });
    });
    let r = ui.interact(inner.response.rect, ui.scope_id().with(("bind", format!("{:?}", id.0), id.1 == Device::Pad)), egui::Sense::click());
    if r.hovered() && !waiting {
        ui.painter().rect_stroke(inner.response.rect, 4, egui::Stroke::new(1.0, hud::DIM), egui::StrokeKind::Inside);
    }
    r.on_hover_text(if waiting { "Press a key, or a button (Escape gives up)" } else { "Choose, then press the key or button to bind" })
}
