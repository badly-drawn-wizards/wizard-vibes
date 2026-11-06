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
pub mod skills;
pub mod spellbar;
pub mod spells;
pub mod vec;

use crate::i18n::tr;
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
const STORE_SWAP_XY: &str = "wizard-swap-xy";
const STORE_SOULS: &str = "wizard-souls";
const STORE_FFA: &str = "wizard-ffa";
const STORE_SKILLS: &str = "wizard-skills";
const STORE_SPELL: &str = "wizard-spell";
const STORE_LOADOUT: &str = "wizard-loadout";
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
    // the enemies fighting, as last logged
    enemies_shown: usize,
    // dash pressed (taken by the flight this step)
    dash: bool,
    // (?open=die-...: defeated by that, so many frames into the fight)
    die_soon: Option<(u32, u32)>,
    // the souls he has gathered, all told (to spend: the start of an
    // incremental game)
    pub souls: u64,
    // free for all (the enemies fight each other too), or one against all
    ffa: bool,
    // defeated: how (see Death)
    death: Option<Death>,
    // his skills (bought with souls), the one shown in the tree, and the
    // skill tree asked for (from the defeat screen: the app opens it)
    skills: skills::Skills,
    skill_shown: usize,
    // the skill tree, when open (see Tree)
    tree: Option<Tree>,
    // the spell in each of his slots (see fit_loadout), and the loadout
    // screen, when open (the slot picked)
    loadout: Vec<Option<usize>>,
    loadout_open: Option<usize>,
    // where his spell will go, aim assisted (see assist), for the
    // crosshair's cross
    cross: Option<V3>,
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

// Defeated, he comes apart (drawn as an enemy would: see the shader's
// him_dying) while time slows to a stop over DEATH_SLOW seconds; then the
// scene holds still, the camera free to look round, and the defeat panel
// shows. Where he stood, what defeated him (a kill code, as he faced), and
// since when.
#[derive(Clone, Copy, Debug)]
struct Death {
    pos: V3,
    code: f32,
    since: Instant,
}

const DEATH_SLOW: f32 = 2.0;

// The skill tree, open (from the defeat screen, the only way to it: over
// everything, the game paused): the skill picked, and where the view is
// (the tree's space, points, its middle at the root), easing to the one
// picked unless being dragged.
#[derive(Clone, Copy, Debug)]
struct Tree {
    sel: usize,
    pan: Vec2,
    dragging: bool,
}

// (the tree's cells, points)
const TREE_CELL: f32 = 120.0;

// A way from a to b in the skill tree, bowed a little to one side: its
// points.
fn edge_curve(a: egui::Pos2, b: egui::Pos2, bow: f32) -> Vec<egui::Pos2> {
    let d = b - a;
    let side = Vec2::new(-d.y, d.x).normalized() * bow * (d.length() / (2.0 * TREE_CELL)).min(1.0);
    let mid = a + 0.5 * d + side;
    (0..=24)
        .map(|j| {
            let t = j as f32 / 24.0;
            let u = 1.0 - t;
            egui::pos2(u * u * a.x + 2.0 * u * t * mid.x + t * t * b.x, u * u * a.y + 2.0 * u * t * mid.y + t * t * b.y)
        })
        .collect()
}

// The point a fraction f of the way along pts.
fn along(pts: &[egui::Pos2], f: f32) -> egui::Pos2 {
    let x = f.clamp(0.0, 1.0) * (pts.len() - 1) as f32;
    let i = (x as usize).min(pts.len() - 2);
    pts[i] + (pts[i + 1] - pts[i]) * (x - i as f32)
}

// A hexagon round c, its corners r from it, one at the top.
fn hexagon(c: egui::Pos2, r: f32) -> Vec<egui::Pos2> {
    (0..6)
        .map(|k| {
            let a = std::f32::consts::FRAC_PI_3 * k as f32;
            c + Vec2::new(a.sin(), -a.cos()) * r
        })
        .collect()
}

// Where skill k is in the tree's space (points).
fn tree_at(k: usize) -> Vec2 {
    let s = &skills::SKILLS[k];
    Vec2::new(s.at.0, s.at.1) * TREE_CELL
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
            pads: Pads::new(get(STORE_SWAP_XY).as_deref() != Some("0")),
            look: look::MouseLook::default(),
            rng: Rng::new(seed),
            time: 0.0,
            paused: false,
            menu: None,
            pad_connected: false,
            shoulder: 0.0,
            over_shoulder: get(STORE_SHOULDER).as_deref() != Some("0"),
            picture: None,
            enemies_shown: 0,
            dash: false,
            die_soon: None,
            souls: get(STORE_SOULS).and_then(|v| v.parse().ok()).unwrap_or(0),
            ffa: get(STORE_FFA).as_deref() == Some("1"),
            death: None,
            skills: get(STORE_SKILLS).map_or_else(skills::Skills::default, |v| skills::Skills::restored(&v)),
            skill_shown: 0,
            tree: None,
            loadout: get(STORE_LOADOUT).map_or_else(|| vec![Some(0)], |v| {
                v.split(',').map(|x| x.parse::<usize>().ok().filter(|&i| i < spells::N_SPELLS)).collect()
            }),
            loadout_open: None,
            cross: None,
            orb: [0.55, 2.1, 0.5],
            last_pos: None,
            vel: [0.0; 3],
            view: None,
            defeat_since: None,
        };
        // (the spell he last had in hand, kept from round to round and run to
        // run, if he has it)
        g.spells.unlocked = g.skills.boosts().spells;
        if let Some(i) = get(STORE_SPELL).and_then(|v| v.parse().ok()) {
            g.spells.select(i);
        }
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
        storage.set_string(STORE_SWAP_XY, if self.pads.swap_xy { "1" } else { "0" }.to_string());
        storage.set_string(STORE_SOULS, self.souls.to_string());
        storage.set_string(STORE_FFA, if self.ffa { "1" } else { "0" }.to_string());
        storage.set_string(STORE_SKILLS, self.skills.saved());
        storage.set_string(STORE_SPELL, self.spells.selected.to_string());
        storage.set_string(STORE_LOADOUT, self.loadout.iter().map(|s| s.map_or("-".to_string(), |i| i.to_string())).collect::<Vec<_>>().join(","));
        // (none chosen: the drawer's own settings, recompiled)
        storage.set_string(STORE_QUALITY, self.quality.choice.map_or("custom", |c| c.key()).to_string());
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

    // Aim assist: with an enemy near his aim (within ASSIST_CONE of the
    // view), a fire bolt or wind blade is turned part of the way (the more,
    // the nearer the aim) towards where it will be when the spell gets
    // there (its motion against his, for a bolt he carries along). Where
    // the spell then goes shows as the crosshair's cross (see cross).
    fn assist(&mut self, origin: V3, dir: V3) -> V3 {
        const ASSIST_CONE: f32 = 0.14;
        const ASSIST: f32 = 0.6;
        self.cross = None;
        let sel = self.spells.selected as u32;
        let speed = if sel == spells::FIRE {
            30.0 * (1.0 + 0.6 * (self.spells.charge.max(0.0) / spells::CHARGE_TIME))
        } else if sel == spells::WIND {
            24.0
        } else {
            return dir;
        };
        let (pos, heading) = self.flight.place(self.time);
        let eye = vec::add(pos, turn(heading, self.orbit.eye(self.shoulder)));
        let view = turn(heading, self.orbit.view_dir());
        let best = self
            .spells
            .targets
            .iter()
            .filter(|t| t.side >= 0 && t.r < 1.0)
            .map(|t| {
                let v = vec::sub(t.c, eye);
                ((vec::dot(v, view) / vec::len(v)).clamp(-1.0, 1.0).acos(), t)
            })
            .filter(|(a, _)| *a < ASSIST_CONE)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((angle, t)) = best else { return dir };
        let ev = self.combat.enemies.velocity(t.side).unwrap_or([0.0; 3]);
        let rel = if sel == spells::FIRE { vec::sub(ev, self.vel) } else { ev };
        let mut at = t.c;
        for _ in 0..3 {
            let time = vec::dist(at, origin) / speed;
            at = vec::add(t.c, vec::scale(time, rel));
        }
        let k = ASSIST * (1.0 - angle / ASSIST_CONE).clamp(0.0, 1.0);
        let to = vec::normalise(vec::sub(at, origin));
        let d = vec::normalise(vec::add(vec::scale(1.0 - k, dir), vec::scale(k, to)));
        self.cross = Some(vec::add(origin, vec::scale(vec::dist(at, origin), d)));
        d
    }

    // Where world point `p` is in the view (points), if in front of the
    // camera (as camera_ray has it; his bank aside).
    fn on_screen(&self, p: V3, rect: Rect) -> Option<egui::Pos2> {
        let (pos, heading) = self.flight.place(self.time);
        let v = vec::sub(turn(-heading, vec::sub(p, pos)), self.orbit.eye(self.shoulder));
        let f = self.orbit.view_dir();
        let r = vec::normalise([-f[2], 0.0, f[0]]);
        let u = vec::cross(r, f);
        let z = vec::dot(v, f);
        if z <= 0.1 {
            return None;
        }
        let h = rect.height();
        let x = 2.414 * vec::dot(v, r) / z;
        let y = 2.414 * vec::dot(v, u) / z;
        Some(rect.center() + Vec2::new(x * 0.5 * h, -y * 0.5 * h))
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
        // the mouse: a click on the view takes it (not while paused), and it
        // then turns the camera
        let picture = self.picture.is_some();
        if view.clicked() && (!self.paused || picture) && !self.look.held() {
            self.look.grab(ctx);
        }
        // (a click on the view, outside the dialogs, dismisses them: the
        // pause menu resumes, the defeat panel tries again)
        if view.clicked() && !picture {
            if self.paused {
                self.paused = false;
            }
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
                    boost: down(Action::Dash),
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
            // (defeated, and time stopped: the middle button opens the
            // loadout)
            if mouse && self.settled() && !self.paused && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)) {
                self.open_loadout();
            }
            // (the loadout open: up and down pick a slot, left and right
            // change its spell)
            if let Some(k) = self.loadout_open {
                let n = self.loadout.len();
                for (key, _) in &keys {
                    match key {
                        Key::W | Key::ArrowUp => self.loadout_open = Some((k + n - 1) % n),
                        Key::S | Key::ArrowDown => self.loadout_open = Some((k + 1) % n),
                        Key::A | Key::ArrowLeft => self.cycle_slot(k, -1),
                        Key::D | Key::ArrowRight => self.cycle_slot(k, 1),
                        Key::Enter => self.close_loadout(),
                        _ => {}
                    }
                }
                acts.retain(|a| *a == Action::Pause);
            }
            // (the skill tree open: the keys move about it, buy, go back)
            if self.tree.is_some() {
                for (key, _) in &keys {
                    match key {
                        Key::W | Key::ArrowUp => self.tree_move(Vec2::new(0.0, -1.0)),
                        Key::S | Key::ArrowDown => self.tree_move(Vec2::new(0.0, 1.0)),
                        Key::A | Key::ArrowLeft => self.tree_move(Vec2::new(-1.0, 0.0)),
                        Key::D | Key::ArrowRight => self.tree_move(Vec2::new(1.0, 0.0)),
                        Key::Enter | Key::Space => self.tree_buy(),
                        _ => {}
                    }
                }
                acts.retain(|a| *a == Action::Pause);
            }
            for a in acts {
                match a {
                    Action::Mode => self.cycle_mode(),
                    Action::Pause => self.pause_pressed(),
                    Action::Spell(k) => self.select_slot(k),
                    Action::Dash => self.dash = true,
                    // (defeated, and time stopped: casting opens the skill
                    // tree, the shield tries again)
                    Action::Cast if self.settled() && !self.paused => self.ask_skills(),
                    Action::Shield if self.settled() && !self.paused => self.combat.reset(&mut self.spells),
                    _ => {}
                }
            }
            for (key, _) in keys {
                match key {
                    Key::Escape => self.pause_pressed(),
                    Key::Enter if self.settled() && !self.paused => self.combat.reset(&mut self.spells),
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
        if self.paused && !picture || self.tree.is_some() || self.loadout_open.is_some() || !ctx.input(|i| i.focused) {
            self.look.release(ctx);
        }
        if !self.paused {
            self.time += dt * self.time_scale();
        }
        self.orbit.idle(dt);
        // (read every frame, so nothing held seems newly pressed later; but
        // only heeded while the window has the focus: natively a controller
        // reaches every window)
        let read = self.pads.read(&self.binds);
        self.pad_connected = read.is_some();
        let pad = read.filter(|_| ctx.input(|i| i.focused));
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
            if p.pause || (p.back && self.paused && !self.menu_holds && self.tree.is_none()) {
                self.pause_pressed();
            }
            if p.active {
                self.spellbar.device = Device::Pad;
            }
            if let Some(k) = self.loadout_open {
                // (the loadout: up and down pick a slot, left and right
                // change its spell, B (or A) goes back)
                let n = self.loadout.len();
                if p.nav[0] {
                    self.loadout_open = Some((k + n - 1) % n);
                }
                if p.nav[1] {
                    self.loadout_open = Some((k + 1) % n);
                }
                if p.nav[2] {
                    self.cycle_slot(k, -1);
                }
                if p.nav[3] {
                    self.cycle_slot(k, 1);
                }
                if p.back || p.confirm {
                    self.close_loadout();
                }
            } else if self.tree.is_some() {
                // (the skill tree: the D-pad or the left stick move about it,
                // A buys, B goes back)
                for (on, d) in p.nav.iter().zip([Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)]) {
                    if *on {
                        self.tree_move(d);
                    }
                }
                if p.confirm {
                    self.tree_buy();
                }
                if p.back {
                    self.close_tree();
                }
            } else if self.picture.is_some() {
                // (in picture mode, the sticks fly the camera: below)
            } else if self.paused {
                // (in the pause menu, the rest is its navigation)
                self.menu = Some(*p);
            } else {
                // (A tries again once he is defeated)
                if p.pressed == Some(binds::PadButton::X) && self.settled() {
                    self.ask_skills();
                }
                if p.pressed == Some(binds::PadButton::Y) && self.settled() {
                    self.open_loadout();
                }
                if p.confirm && self.settled() {
                    self.combat.reset(&mut self.spells);
                }
                if let Some(k) = p.spell {
                    self.select_slot(k);
                }
            }
        }
        let n = self.enemy_count(params);
        if n != self.enemies_shown {
            log::info!("fight: {n} enemies");
            self.enemies_shown = n;
        }
        let pad_cast = pad.is_some_and(|p| p.cast);
        let game_dt = if self.paused { 0.0 } else { dt * self.time_scale() };
        let (origin, dir) = self.cast_from();
        // (how fast he goes: from where he was last step; a jump, as the
        // mode changes, not counted)
        let him = self.flight.place(self.time).0;
        if game_dt > 0.0 {
            let v = self.last_pos.map_or([0.0; 3], |p| vec::scale(1.0 / game_dt, vec::sub(him, p)));
            self.vel = if vec::len(v) < 60.0 { v } else { [0.0; 3] };
            self.last_pos = Some(him);
        }
        let dir = self.assist(origin, dir);
        self.spells.ffa = self.ffa;
        self.combat.boosts = self.skills.boosts();
        // (the spells he has, in his slots; one not in them out of his hand)
        self.spells.unlocked = self.combat.boosts.spells;
        self.fit_loadout();
        if !self.loadout.contains(&Some(self.spells.selected)) {
            if let Some(s) = self.loadout.iter().flatten().next() {
                self.spells.selected = *s;
            }
        }
        self.combat.enemies.ffa = self.ffa;
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
        if let Some((kind, frames)) = self.die_soon.take() {
            if frames > 0 {
                self.die_soon = Some((kind, frames - 1));
            } else {
                let e = &mut self.combat.enemies;
                e.hit_with(spells::HIS, kind, Some(([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 20.0])), 99.0, 0);
                log::info!("defeated, as asked: hp {} down {} shielded {}", e.hp, e.down, e.shielded);
            }
        }
        self.spells.update(game_dt, casting || pad_cast);
        self.combat.handle(&mut self.spells);
        self.souls += std::mem::take(&mut self.combat.souls_taken) as u64;
        self.spellbar.spell = self.spells.selected;
        self.spellbar.unlocked = self.spells.unlocked;
        self.spellbar.loadout = self.loadout.clone();
        self.spellbar.cooling = std::array::from_fn(|i| self.spells.cooling(i));
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
        // (channelling, he cannot speed up: no input; he drifts on, slowing
        // as the air's drag takes his speed)
        let channelling = self.spells.channel >= 0.0;
        let (held, stick) = if channelling || self.death.is_some() { (flight::Held::default(), None) } else { (held, stick) };
        let dash = std::mem::take(&mut self.dash) || pad.is_some_and(|p| p.dash);
        self.flight.update(game_dt, self.time, heading + self.orbit.cam.yaw + PI, &held, stick.as_ref(), dash);
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
        drive.foe_pos = self.combat.enemies.positions();
        // (defeated: he comes apart in the slot after the enemies')
        let e = &self.combat.enemies;
        match (e.down >= 0.0, self.death) {
            (true, None) => {
                let (pos, heading) = self.flight.place(self.time);
                let (kind, cut) = e.his_end.unwrap_or((spells::BURST, None));
                self.death = Some(Death { pos, code: enemies::kill_code(kind, cut, pos, heading), since: Instant::now() });
            }
            (false, Some(_)) => self.death = None,
            _ => {}
        }
        // (else that slot says nothing of the kind: an enemy there before,
        // with more of them, may have been defeated)
        if n < enemies::MAX {
            match &self.death {
                Some(d) => {
                    drive.foe[n] = 16.0 * e.his_deaths as f32 + 8.0 + e.down.min(7.9);
                    drive.kill[n] = d.code;
                    drive.foe_pos[n] = d.pos;
                }
                None => drive.foe[n] = 0.0,
            }
        }


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
        self.souls += std::mem::take(&mut self.combat.souls_taken) as u64;
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
        .on_hover_text(tr("Mode (F)"));
        if let Some(m) = chosen {
            self.set_mode(m);
        }
    }

    // The skill tree, asked for from the defeat screen.
    // His slots as his skills give them, each holding one of his spells (or
    // none): more slots, filled with spells not yet in one; fewer, the last
    // gone; a spell he no longer has, out.
    fn fit_loadout(&mut self) {
        let have = self.combat.boosts.spells;
        let n = self.combat.boosts.slots.clamp(1, 4);
        self.loadout.resize(n, None);
        for s in self.loadout.iter_mut() {
            if s.is_some_and(|i| !have[i]) {
                *s = None;
            }
        }
        for k in 0..n {
            if self.loadout[k].is_none() {
                self.loadout[k] = (0..spells::N_SPELLS).find(|&i| have[i] && !self.loadout.contains(&Some(i)));
            }
        }
    }

    // Takes the spell in slot k in hand.
    fn select_slot(&mut self, k: usize) {
        if let Some(Some(s)) = self.loadout.get(k) {
            self.spells.select(*s);
        }
    }

    // The loadout screen (from the defeat screen): slot k's spell, the next
    // (by) of those he has: swapped with a slot that held it.
    fn cycle_slot(&mut self, k: usize, by: i32) {
        let have = self.combat.boosts.spells;
        let mut opts: Vec<Option<usize>> = (0..spells::N_SPELLS).filter(|&i| have[i]).map(Some).collect();
        opts.push(None);
        let Some(cur) = self.loadout.get(k).copied() else { return };
        let at = opts.iter().position(|o| *o == cur).unwrap_or(0) as i32;
        let next = opts[(at + by).rem_euclid(opts.len() as i32) as usize];
        if let Some(j) = self.loadout.iter().position(|o| next.is_some() && *o == next) {
            self.loadout[j] = cur;
        }
        self.loadout[k] = next;
    }

    // A screen opened as the page asks (?open=tree, loadout, or die-fire,
    // die-lightning, die-wind: for looking at them without a defeat first).
    pub fn open_screen(&mut self, what: &str) {
        match what {
            "tree" => self.ask_skills(),
            "loadout" => self.open_loadout(),
            // (defeated at once, in a fight, by that: for the defeat screen)
            "die-fire" | "die-lightning" | "die-wind" => {
                self.set_mode(ModeName::Fight);
                let kind = match what {
                    "die-fire" => spells::BURST,
                    "die-lightning" => spells::STRIKE,
                    _ => spells::WIND,
                };
                self.die_soon = Some((kind, 30));
                log::info!("to be defeated by {kind}");
            }
            _ => {}
        }
    }

    // The loadout screen, from the defeat screen (paused while open), and
    // back.
    fn open_loadout(&mut self) {
        self.paused = true;
        self.loadout_open = Some(0);
    }

    fn close_loadout(&mut self) {
        if self.loadout_open.take().is_some() {
            self.paused = false;
        }
    }

    // (at the skill last looked at)
    fn ask_skills(&mut self) {
        let sel = self.skill_shown.min(skills::SKILLS.len() - 1);
        self.paused = true;
        self.tree = Some(Tree { sel, pan: tree_at(sel), dragging: false });
    }

    // Back to the defeat screen.
    fn close_tree(&mut self) {
        if self.tree.take().is_some() {
            self.paused = false;
        }
    }

    // Whether something covers the game that is not the pause menu (the
    // skill tree, or picture mode): the app shows no menu.
    pub fn overlay(&self) -> bool {
        self.picture.is_some() || self.tree.is_some() || self.loadout_open.is_some()
    }

    // The skill tree, open: picks the skill that way (a direction on
    // screen), the nearest roughly that way.
    fn tree_move(&mut self, dir: Vec2) {
        let Some(t) = &mut self.tree else { return };
        let from = tree_at(t.sel);
        let mut best = (f32::MAX, t.sel);
        for k in 0..skills::SKILLS.len() {
            let d = tree_at(k) - from;
            let len = d.length();
            if k == t.sel || len < 1.0 {
                continue;
            }
            let cos = d.dot(dir) / len;
            if cos < 0.4 {
                continue;
            }
            let score = len * (1.0 + 2.0 * (1.0 - cos));
            if score < best.0 {
                best = (score, k);
            }
        }
        t.sel = best.1;
        t.dragging = false;
        self.skill_shown = t.sel;
    }

    // Buys the next level of the skill picked, if he can.
    fn tree_buy(&mut self) {
        let Some(t) = self.tree else { return };
        if self.skills.buy(&skills::SKILLS[t.sel], &mut self.souls) {
            self.combat.sfx.push(sfx::here(sfx::APPEAR));
        }
    }

    // Defeated: the overlay's seconds since, and by what (a kill code's
    // kind: 0 fire, 1 lightning, 2 wind).
    pub fn defeat_overlay(&self) -> Option<(f32, u32)> {
        self.death.map(|d| (d.since.elapsed().as_secs_f32(), (d.code.max(0.0) as u32) % 4))
    }

    // How fast time runs: slowing to a stop as he comes apart.
    fn time_scale(&self) -> f32 {
        match &self.death {
            Some(d) => (1.0 - d.since.elapsed().as_secs_f32() / DEATH_SLOW).max(0.0).powi(2),
            None => 1.0,
        }
    }

    // Defeated, and time stopped: the defeat panel is up (and he may try
    // again).
    fn settled(&self) -> bool {
        self.death.is_some_and(|d| d.since.elapsed().as_secs_f32() >= DEATH_SLOW)
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
        if self.loadout_open.is_some() {
            self.close_loadout();
        } else if self.tree.is_some() {
            self.close_tree();
        } else if self.picture.is_some() {
            self.leave_picture();
        } else {
            self.paused = !self.paused;
        }
    }

    // Whom the enemies fight (for the pause menu: a row of its navigation):
    // him alone, or each other too.
    pub fn sides_ui(&mut self, ui: &mut egui::Ui, nav: &mut crate::nav::Nav) {
        for (name, ffa, tip) in [
            ("1 vs all", false, "The enemies all fight him"),
            ("Free for all", true, "The enemies fight each other too"),
        ] {
            let r = hud::fill_toggle(ui, tr(name), self.ffa == ffa).on_hover_text(tr(tip));
            nav.item(ui, &r);
            if r.clicked() {
                self.ffa = ffa;
            }
        }
    }

    // The camera's choice (for the pause menu: a row of its navigation):
    // over his shoulder as he flies, or centred on him.
    pub fn camera_ui(&mut self, ui: &mut egui::Ui, nav: &mut crate::nav::Nav) {
        for (name, over, tip) in [
            ("Over the shoulder", true, "The camera looks past his right shoulder"),
            ("Centred", false, "The camera looks at him, centred"),
        ] {
            let r = hud::fill_toggle(ui, tr(name), self.over_shoulder == over).on_hover_text(tr(tip));
            nav.item(ui, &r);
            if r.clicked() {
                self.over_shoulder = over;
            }
        }
        let r = hud::fill_toggle(ui, tr("Picture"), false).on_hover_text(tr("Hold the scene still and fly the camera freely to frame a picture (Escape to come back)"));
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

    // The drawer's own settings run, not a quality level (none chosen).
    pub fn quality_custom(&mut self) {
        self.quality.custom();
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
        // (the loadout, open: over everything)
        if self.loadout_open.is_some() {
            self.loadout_ui(ctx, rect);
            return;
        }
        // (the skill tree, open: over everything)
        if self.tree.is_some() {
            self.tree_ui(ctx, rect);
            return;
        }
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
                            ui.label(hud::text(tr("Picture mode"), hud::FONT, hud::FG));
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
                    let cross = self.cross.and_then(|p| self.on_screen(p, rect)).unwrap_or(rect.center());
                    let vitals = (h.n > 0).then_some(h.health);
                    hud::crosshair(&painter, rect.center(), cross, h.locked, h.shielded, vitals, h.mana, h.short);
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

        // the vitals, bottom left (bare, on the view)
        let mut reset = false;
        if h.drawn {
            egui::Area::new(egui::Id::unique("wizard-vitals"))
                .pivot(Align2::LEFT_BOTTOM)
                .fixed_pos(rect.left_bottom() + Vec2::new(16.0, -16.0))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                    let width = 200.0f32.min(0.4 * rect.width());
                    let health = ctx.animate_value_with_time(egui::Id::unique("wizard-health"), h.health, 0.15);
                    let mana = ctx.animate_value_with_time(egui::Id::unique("wizard-mana"), h.mana, 0.15);
                    if h.n > 0 {
                        ui.horizontal(|ui| {
                            hud::icon(ui, hud::Icon::Heart, hud::FONT, hud::FG, egui::Sense::hover());
                            hud::bar(ui, width, health, egui::Color32::from_rgb(0xc0, 0x28, 0x3c), egui::Color32::from_rgb(0xff, 0x6a, 0x5a), hud::BORDER);
                        })
                        .response
                        .on_hover_text(tr("Health"));
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
                    .on_hover_text(tr("Mana"));
                });
            // the tally, top right: enemies he has defeated (in a fight),
            // and the souls he has gathered, all told
            egui::Area::new(egui::Id::unique("wizard-tally"))
                .pivot(Align2::RIGHT_TOP)
                .fixed_pos(rect.right_top() + Vec2::new(-16.0, 16.0))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        if h.n > 0 {
                            hud::icon(ui, hud::Icon::Skull, 1.3 * hud::FONT, hud::FG, egui::Sense::hover());
                            ui.label(hud::text(h.defeated.to_string(), 1.2 * hud::FONT, hud::FG));
                            ui.add_space(10.0);
                        }
                        hud::icon(ui, hud::Icon::Soul, 1.3 * hud::FONT, egui::Color32::from_rgb(0xb8, 0xe4, 0xff), egui::Sense::hover());
                        ui.label(hud::text(self.souls.to_string(), 1.2 * hud::FONT, hud::FG));
                    });
                });
        }

        // defeated: a panel says so, until he tries again
        match (&h.lost, self.defeat_since) {
            (Some(_), None) if self.settled() => self.defeat_since = Some(Instant::now()),
            (None, Some(_)) => self.defeat_since = None,
            _ => {}
        }
        if let (Some(stats), Some(since)) = (&h.lost, self.defeat_since) {
            // the word across a dark band a quarter of the way down, fading
            // in and growing a little; what he did, beneath; and at the
            // bottom, what the buttons do now
            let t = since.elapsed().as_secs_f32();
            let alpha = (t / 1.6).min(1.0);
            if alpha < 1.0 {
                ctx.request_repaint();
            }
            let y = rect.top() + 0.25 * rect.height();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("wizard-defeat")));
            // (the magic behind it is the shader's: see the overlay; here
            // the word, glowing in the colour of what defeated him, and what
            // did)
            let kind = self.defeat_overlay().map_or(0, |(_, k)| k);
            let (glow, line) = match kind {
                0 => (egui::Color32::from_rgb(0xff, 0x8a, 0x3a), "Burnt to cinders"),
                1 => (egui::Color32::from_rgb(0x9c, 0xc0, 0xff), "Struck down"),
                2 => (egui::Color32::from_rgb(0x7a, 0xf0, 0xff), "Torn apart by the wind"),
                _ => (egui::Color32::from_rgb(0xd0, 0x9c, 0xff), "Undone"),
            };
            let size = 54.0 * (1.0 + 0.04 * alpha);
            let word: String = tr("Defeated").to_uppercase().chars().map(|c| format!("{c} ")).collect::<String>().trim_end().to_string();
            let font = egui::FontId::proportional(size);
            let centre = egui::pos2(rect.center().x, y);
            // (a glow: the word faint in its colour, about it)
            for k in 0..12 {
                let a = std::f32::consts::TAU * k as f32 / 12.0;
                let off = Vec2::new(a.cos(), a.sin()) * (3.0 + 2.0 * (k % 2) as f32);
                let c = glow.gamma_multiply(0.12 * alpha);
                let g = painter.layout_no_wrap(word.clone(), font.clone(), c);
                painter.galley(Align2::CENTER_CENTER.anchor_size(centre + off, g.size()).min, g, c);
            }
            let ink = egui::Color32::from_rgb(0xff, 0xf6, 0xe8).gamma_multiply(alpha);
            let g = painter.layout_no_wrap(word, font, ink);
            painter.galley(Align2::CENTER_CENTER.anchor_size(centre, g.size()).min, g, ink);
            let g = painter.layout_no_wrap(tr(line).to_string(), egui::FontId::proportional(20.0), glow.gamma_multiply(alpha));
            painter.galley(Align2::CENTER_TOP.anchor_size(centre + Vec2::new(0.0, 0.6 * size), g.size()).min, g, glow);
            let g = painter.layout_no_wrap(stats.clone(), hud::mono(hud::FONT), hud::DIM.gamma_multiply(alpha));
            painter.galley(Align2::CENTER_TOP.anchor_size(centre + Vec2::new(0.0, 0.6 * size + 32.0), g.size()).min, g, hud::DIM);
            let pad = self.spellbar.device == Device::Pad;
            let mut skills = false;
            let mut loadout = false;
            egui::Area::new(egui::Id::unique("wizard-defeat-hints"))
                .pivot(Align2::CENTER_BOTTOM)
                .fixed_pos(rect.center_bottom() - Vec2::new(0.0, 28.0))
                .show(ctx, |ui| {
                    ui.set_opacity(alpha);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 18.0;
                        let skill_glyph = if pad { hud::Chip::Face('X') } else { hud::Chip::Mouse(false) };
                        if hud::glyph_button(ui, skill_glyph, tr("Skill tree"), 1.05 * hud::FONT).clicked() {
                            skills = true;
                        }
                        let loadout_glyph = if pad { hud::Chip::Face('Y') } else { hud::Chip::MouseMiddle };
                        if hud::glyph_button(ui, loadout_glyph, tr("Loadout"), 1.05 * hud::FONT).clicked() {
                            loadout = true;
                        }
                        let again_glyph = if pad { hud::Chip::Face('A') } else { hud::Chip::Mouse(true) };
                        if hud::glyph_button(ui, again_glyph, tr("Try again"), 1.05 * hud::FONT).clicked() {
                            reset = true;
                        }
                    });
                });
            if skills {
                self.ask_skills();
            }
            if loadout {
                self.open_loadout();
            }
        }
        if reset {
            self.combat.reset(&mut self.spells);
        }

        self.spellbar.ui(ctx, rect, &self.binds);
    }

    // The loadout, open: over everything, dark; his slots, each its key (or
    // button) and the spell in it between arrows that change it, the slot
    // picked lit; and what the buttons do.
    fn loadout_ui(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(sel) = self.loadout_open else { return };
        let pad = self.spellbar.device == Device::Pad;
        let mut change: Option<(usize, i32)> = None;
        let mut pick = None;
        let mut back = false;
        egui::Area::new(egui::Id::unique("wizard-loadout")).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (area, _) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(area, 0, hud::bg(0.88));
            let title = p.layout_no_wrap(tr("Loadout").to_string(), egui::FontId::proportional(26.0), hud::FG);
            p.galley(Align2::CENTER_TOP.anchor_size(area.center_top() + Vec2::new(0.0, 24.0), title.size()).min, title, hud::FG);
            let n = self.loadout.len();
            let row_h = 64.0;
            let top = area.center().y - 0.5 * row_h * n as f32;
            for k in 0..n {
                let y = top + (k as f32 + 0.5) * row_h;
                let row = Rect::from_center_size(egui::pos2(area.center().x, y), Vec2::new(420.0, row_h - 10.0));
                let on = k == sel;
                p.rect_filled(row, 8, if on { hud::bg(1.0) } else { hud::bg(0.5) });
                p.rect_stroke(row, 8, egui::Stroke::new(if on { 2.0 } else { 1.0 }, if on { hud::FOCUS } else { hud::BORDER }), egui::StrokeKind::Inside);
                if ui.interact(row, ui.scope_id().with(("slot", k)), egui::Sense::click()).clicked() {
                    pick = Some(k);
                }
                // its key or button
                let glyph_at = egui::pos2(row.left() + 30.0, y);
                if pad {
                    if let Some(b) = self.binds.pad(Action::Spell(k)) {
                        hud::paint_pad_glyph(&p, glyph_at, 10.0, b.chip());
                    }
                } else if let Some(key) = self.binds.key(Action::Spell(k)) {
                    hud::paint_key_glyph(&p, glyph_at, 20.0, key.shown());
                }
                // the spell, between arrows
                let mid = egui::pos2(row.center().x + 10.0, y);
                for (dir, x) in [(-1, row.left() + 80.0), (1, row.right() - 30.0)] {
                    let r = Rect::from_center_size(egui::pos2(x, y), Vec2::splat(30.0));
                    let resp = ui.interact(r, ui.scope_id().with(("arrow", k, dir)), egui::Sense::click());
                    let c = if resp.hovered() { hud::FG } else { hud::DIM };
                    let s = 7.0 * dir as f32;
                    p.add(egui::Shape::convex_polygon(
                        vec![r.center() + Vec2::new(s, 0.0), r.center() + Vec2::new(-s, -7.0), r.center() + Vec2::new(-s, 7.0)],
                        c,
                        egui::Stroke::NONE,
                    ));
                    if resp.clicked() {
                        change = Some((k, dir));
                    }
                }
                match self.loadout[k] {
                    Some(i) => {
                        hud::paint_spell(&p, mid - Vec2::new(70.0, 0.0), 14.0, i, hud::SPELL_COLOURS[i]);
                        let g = p.layout_no_wrap(tr(spells::SPELL_NAMES[i]).to_string(), egui::FontId::proportional(18.0), hud::FG);
                        p.galley(Align2::LEFT_CENTER.anchor_size(mid - Vec2::new(45.0, 0.0), g.size()).min, g, hud::FG);
                    }
                    None => {
                        let g = p.layout_no_wrap(tr("(empty)").to_string(), egui::FontId::proportional(18.0), hud::DIM);
                        p.galley(Align2::LEFT_CENTER.anchor_size(mid - Vec2::new(45.0, 0.0), g.size()).min, g, hud::DIM);
                    }
                }
            }
            egui::Area::new(egui::Id::unique("wizard-loadout-hints"))
                .order(egui::Order::Foreground)
                .pivot(Align2::CENTER_BOTTOM)
                .fixed_pos(area.center_bottom() - Vec2::new(0.0, 24.0))
                .show(ctx, |ui| {
                    let g = if pad { hud::Chip::Face('B') } else { hud::Chip::Key("Esc") };
                    back = hud::glyph_button(ui, g, tr("Back"), 1.05 * hud::FONT).clicked();
                });
        });
        if let Some(k) = pick {
            self.loadout_open = Some(k);
        }
        if let Some((k, d)) = change {
            self.loadout_open = Some(k);
            self.cycle_slot(k, d);
        }
        if back {
            self.close_loadout();
        }
    }

    // The skill tree, open: over everything, dark; its skills (logos in
    // rings, their levels as pips, the lines from those they need), the
    // view panned by dragging (snapping to the skill nearest the middle as
    // it is let go) and easing to the one picked; the souls he has; what
    // the one picked does, and costs; and what the buttons do.
    fn tree_ui(&mut self, ctx: &egui::Context, rect: Rect) {
        use skills::SKILLS;
        let Some(mut t) = self.tree else { return };
        let pad = self.spellbar.device == Device::Pad;
        egui::Area::new(egui::Id::unique("wizard-tree")).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (area, bg) = ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
            let p = ui.painter().clone();
            p.rect_filled(area, 0, hud::bg(0.88));
            // (dragged: panned, and let go, the nearest to the middle picked)
            if bg.dragged() {
                t.pan -= bg.drag_delta();
                t.dragging = true;
            } else if t.dragging {
                t.dragging = false;
                t.sel = (0..SKILLS.len()).min_by(|&a, &b| (tree_at(a) - t.pan).length().total_cmp(&(tree_at(b) - t.pan).length())).unwrap_or(0);
                self.skill_shown = t.sel;
            }
            if !t.dragging {
                let k = 1.0 - (-ui.input(|i| i.stable_dt) * 10.0).exp();
                t.pan += (tree_at(t.sel) - t.pan) * k;
                if (tree_at(t.sel) - t.pan).length() > 0.5 {
                    ctx.request_repaint();
                }
            }
            let mid = area.center() + Vec2::new(0.0, -20.0);
            let at = |k: usize| mid + tree_at(k) - t.pan;
            let blue = egui::Color32::from_rgb(0x6f, 0xb4, 0xff);
            // the ways between them: curving a little; learnt, glowing, with
            // sparks flowing out along them from the root; open to learn,
            // a line that breathes; else faint and dashed
            let now = ui.input(|i| i.time) as f32;
            for (k, s) in SKILLS.iter().enumerate() {
                if let Some(n) = s.needs.and_then(|n| SKILLS.iter().position(|x| x.id == n)) {
                    let pts = edge_curve(at(n), at(k), 32.0);
                    if self.skills.level(s.id) > 0 {
                        p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(9.0, blue.gamma_multiply(0.12))));
                        p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(4.0, blue.gamma_multiply(0.35))));
                        p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(1.6, egui::Color32::from_rgb(0xd8, 0xec, 0xff))));
                        for j in 0..3 {
                            let f = (now * 0.45 + j as f32 / 3.0 + 0.13 * k as f32).fract();
                            let q = along(&pts, f);
                            let a = (std::f32::consts::PI * f).sin();
                            p.circle_filled(q, 4.5, blue.gamma_multiply(0.25 * a));
                            p.circle_filled(q, 2.0, egui::Color32::WHITE.gamma_multiply(a));
                        }
                        ctx.request_repaint();
                    } else if self.skills.cost(s).is_some() {
                        let b = 0.55 + 0.25 * (now * 2.4 + k as f32).sin();
                        p.add(egui::Shape::line(pts.clone(), egui::Stroke::new(5.0, hud::FG.gamma_multiply(0.06 * b))));
                        p.add(egui::Shape::line(pts, egui::Stroke::new(1.6, hud::FG.gamma_multiply(b))));
                        ctx.request_repaint();
                    } else {
                        for seg in pts.chunks(2).step_by(2) {
                            if let [a, b] = seg {
                                p.line_segment([*a, *b], egui::Stroke::new(1.2, hud::BORDER));
                            }
                        }
                    }
                }
            }
            for (k, s) in SKILLS.iter().enumerate() {
                let c = at(k);
                let r = 30.0;
                let resp = ui.interact(egui::Rect::from_center_size(c, Vec2::splat(2.0 * r)), ui.scope_id().with(("skill", s.id)), egui::Sense::click());
                if resp.clicked() {
                    if t.sel == k {
                        self.tree = Some(t);
                        self.tree_buy();
                    }
                    t.sel = k;
                    t.dragging = false;
                    self.skill_shown = k;
                }
                let level = self.skills.level(s.id);
                let can = self.skills.cost(s).is_some_and(|c| c <= self.souls);
                let open = self.skills.cost(s).is_some() || level > 0;
                let (ring, ink) = if level > 0 {
                    (blue, egui::Color32::WHITE)
                } else if can {
                    (hud::FG, hud::FG)
                } else if open {
                    (hud::DIM, hud::DIM)
                } else {
                    (hud::BORDER, hud::BORDER)
                };
                // (an unlock, a spell or a slot, a hexagon; the rest round)
                let edge = egui::Stroke::new(if k == t.sel { 3.0 } else { 1.5 }, if k == t.sel { hud::FOCUS } else { ring });
                if skills::unlocks(s) {
                    let hex = hexagon(c, 1.12 * r);
                    p.add(egui::Shape::convex_polygon(hex.clone(), hud::bg(1.0), egui::Stroke::NONE));
                    if level > 0 {
                        p.add(egui::Shape::convex_polygon(hex.clone(), blue.gamma_multiply(0.25), egui::Stroke::NONE));
                    }
                    p.add(egui::Shape::closed_line(hex, edge));
                } else {
                    p.circle_filled(c, r, hud::bg(1.0));
                    if level > 0 {
                        p.circle_filled(c, r, blue.gamma_multiply(0.25));
                    }
                    p.circle_stroke(c, r, edge);
                }
                hud::paint_skill(&p, c, 0.6 * r, s.id, ink);
                for l in 0..s.levels {
                    let x = c.x + (l as f32 - 0.5 * (s.levels - 1) as f32) * 10.0;
                    let pc = egui::pos2(x, c.y + r + 8.0);
                    if l < level {
                        p.circle_filled(pc, 3.0, blue);
                    } else {
                        p.circle_stroke(pc, 3.0, egui::Stroke::new(1.0, hud::DIM));
                    }
                }
            }
            // the title and his souls, top; what the one picked is, bottom
            let title = p.layout_no_wrap(tr("Skill tree").to_string(), egui::FontId::proportional(26.0), hud::FG);
            p.galley(Align2::CENTER_TOP.anchor_size(area.center_top() + Vec2::new(0.0, 24.0), title.size()).min, title, hud::FG);
            let souls = p.layout_no_wrap(self.souls.to_string(), hud::mono(1.3 * hud::FONT), hud::FG);
            let sp = area.right_top() + Vec2::new(-28.0 - souls.size().x, 30.0);
            hud::paint_icon(&p, egui::Rect::from_center_size(sp + Vec2::new(-14.0, 0.5 * souls.size().y), Vec2::splat(18.0)), hud::Icon::Soul, egui::Color32::from_rgb(0xb8, 0xe4, 0xff));
            p.galley(sp, souls, hud::FG);
            let s = &SKILLS[t.sel];
            let (name, what) = skills::shown(s);
            let level = self.skills.level(s.id);
            let state = match self.skills.cost(s) {
                Some(c) => crate::i18n::trf("Next level: {} souls", &[&c]),
                None if level >= s.levels => tr("Learnt in full").to_string(),
                None => crate::i18n::trf("Needs {}", &[&s.needs.and_then(skills::skill).map_or("", |n| skills::shown(n).0)]),
            };
            let lines = [(format!("{name}  {level}/{}", s.levels), 1.4 * hud::FONT, hud::FG), (what.to_string(), hud::FONT, hud::FG), (state, hud::FONT, hud::DIM)];
            let mut y = area.bottom() - 150.0;
            for (text, size, colour) in lines {
                let g = p.layout_no_wrap(text, egui::FontId::proportional(size), colour);
                p.galley(Align2::CENTER_TOP.anchor_size(egui::pos2(area.center().x, y), g.size()).min, g, colour);
                y += size + 10.0;
            }
            // what the buttons do
            let mut back = false;
            let mut buy = false;
            egui::Area::new(egui::Id::unique("wizard-tree-hints"))
                .order(egui::Order::Foreground)
                .pivot(Align2::CENTER_BOTTOM)
                .fixed_pos(area.center_bottom() - Vec2::new(0.0, 24.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 18.0;
                        let g = if pad { hud::Chip::Face('A') } else { hud::Chip::Key("Enter") };
                        buy = hud::glyph_button(ui, g, tr("Learn"), 1.05 * hud::FONT).clicked();
                        let g = if pad { hud::Chip::Face('B') } else { hud::Chip::Key("Esc") };
                        back = hud::glyph_button(ui, g, tr("Back"), 1.05 * hud::FONT).clicked();
                    });
                });
            if self.tree.is_some() {
                self.tree = Some(Tree { sel: t.sel, pan: t.pan, dragging: t.dragging });
            }
            if buy {
                self.tree_buy();
            }
            if back {
                self.close_tree();
            }
        });
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
            ui.label(hud::text(tr("Keyboard"), 12.0, hud::DIM));
            ui.label(hud::text(tr("Controller"), 12.0, hud::DIM));
            ui.end_row();
            for a in ACTIONS {
                ui.label(a.label());
                nav.grid_row();
                for device in [Device::Keyboard, Device::Pad] {
                    let waiting = self.capture.is_some_and(|c| c.action == a && c.device == device);
                    let chip = match device {
                        Device::Keyboard => self.binds.key(a).map(|k| Chip::Key(k.shown())),
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
                ("Camera (click to take the mouse)", Chip::Text(tr("mouse · wheel")), Chip::Stick('R')),
                ("Try again (defeated)", Chip::Key("Enter"), Chip::Face('A')),
                ("Music", Chip::Key("M"), Chip::Text("—")),
            ] {
                ui.label(hud::text(tr(what), 12.5, hud::DIM));
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
        ui.horizontal(|ui| {
            let r = hud::fill_toggle(ui, tr("Swap X and Y"), self.pads.swap_xy).on_hover_text(tr("For controllers that report X and Y the other way round"));
            nav.item(ui, &r);
            if r.clicked() {
                self.pads.swap_xy = !self.pads.swap_xy;
            }
        });
        ui.add_space(6.0);
        nav.row();
        let r = ui.button(tr("Reset to defaults"));
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
    fn the_loadout_fills_its_slots_and_swaps() {
        let mut g = Game::new(None);
        g.combat.boosts.spells = [true, true, true, false, false, false, false, false, false];
        g.combat.boosts.slots = 3;
        g.loadout = vec![Some(0)];
        g.fit_loadout();
        assert_eq!(g.loadout, vec![Some(0), Some(1), Some(2)]);
        // (slot 0 to the next spell: lightning, which slot 1 held, takes the
        // fire bolt)
        g.cycle_slot(0, 1);
        assert_eq!(g.loadout, vec![Some(1), Some(0), Some(2)]);
        g.select_slot(1);
        assert_eq!(g.spells.selected, 0);
        // (fewer slots, a spell lost: out)
        g.combat.boosts.slots = 2;
        g.combat.boosts.spells = [true, false, true, false, false, false, false, false, false];
        g.fit_loadout();
        assert_eq!(g.loadout, vec![Some(2), Some(0)]);
    }

    #[test]
    fn a_death_asked_for_comes() {
        let ctx = egui::Context::default();
        let mut g = Game::new(None);
        let p = params(3.0);
        g.open_screen("die-wind");
        for _ in 0..40 {
            frame(&ctx, &mut g, &p, vec![]);
        }
        assert!(g.combat.enemies.down >= 0.0, "hp {}", g.combat.enemies.hp);
        assert_eq!(g.defeat_overlay().map(|d| d.1), Some(2));
    }

    #[test]
    fn plays_a_fight() {
        let ctx = egui::Context::default();
        let mut g = Game::new(None);
        let p = params(3.0);
        let o = frame(&ctx, &mut g, &p, vec![]);
        assert_eq!(o.drive, Drive { staff: o.drive.staff, shoulder: o.drive.shoulder, foe_pos: o.drive.foe_pos, sx: o.drive.sx, sy: o.drive.sy, sz: o.drive.sz, flash: 10.0 + 1.0 / 60.0, spell: 0.0, shield_hit: 10.0 + 1.0 / 60.0, ..ON_PATH });
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
                    ui.label(hud::text(tr("press…"), 12.0, hud::FOCUS));
                }
                (false, Some(c)) => hud::chip(ui, c),
                (false, None) => {
                    ui.label(hud::text(tr("—"), 12.0, hud::DIM));
                }
            }
        });
    });
    let r = ui.interact(inner.response.rect, ui.scope_id().with(("bind", format!("{:?}", id.0), id.1 == Device::Pad)), egui::Sense::click());
    if r.hovered() && !waiting {
        ui.painter().rect_stroke(inner.response.rect, 4, egui::Stroke::new(1.0, hud::DIM), egui::StrokeKind::Inside);
    }
    r.on_hover_text(tr(if waiting { "Press a key, or a button (Escape gives up)" } else { "Choose, then press the key or button to bind" }))
}
