// What the app's parts hand each other: the renderer's inputs (the camera,
// the flight state as Futhark's drive record, the spells), what a build
// says about itself, and the sounds and music asked of it.

use std::collections::HashMap;
use crate::game::enemies::WIZARD_SLOTS;

// The orbit camera round him (fut/wizard.fut's camera_ray): yaw and pitch
// in radians, distance in units.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

// As the page starts.
pub const PAGE_CAMERA: Camera = Camera { yaw: 0.5, pitch: 0.12, dist: 4.4 };
// As the benchmarks have it.
pub const BENCH_CAMERA: Camera = Camera { yaw: 0.5, pitch: 0.2, dist: 4.2 };

// The flight state (fut/rig.fut's drive record), field for field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    pub free: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub heading: f32,
    pub turn: f32,
    pub ax: f32,
    pub ay: f32,
    pub az: f32,
    pub climb: f32,
    // casting (see game/spells.rs)
    pub staff: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
    pub flash: f32,
    pub spell: f32,
    // his shield
    pub shield: f32,
    pub shield_hit: f32,
    // flying backwards: 0 .. 1, as he thumbs over his shoulder
    pub thumb: f32,
    // dancing to the music: how much (0 .. 1), and the beat (beats into
    // the song's loop)
    pub groove: f32,
    // the camera over his shoulder: 0 .. 1 (see camera_ray)
    pub shoulder: f32,
    // picture mode (1): the camera flown freely, its target moved by
    // (cam_x, cam_y, cam_z) (his frame)
    pub picture: f32,
    pub cam_x: f32,
    pub cam_y: f32,
    pub cam_z: f32,
    pub beat: f32,
    // the wizards' states (the enemies', then his: see WIZARD_SLOTS), and
    // the point they circle
    pub arena_x: f32,
    pub arena_y: f32,
    pub arena_z: f32,
    pub foe: [f32; WIZARD_SLOTS],
    // where each stands (its platform's top, in the world: fx0 .. fz6)
    pub foe_pos: [[f32; 3]; WIZARD_SLOTS],
    // what defeated each (a kill code: see kill_of in the shader's enemies.rs)
    pub kill: [f32; WIZARD_SLOTS],
}

// On the flight path (fut/rig.fut's on_path).
pub const ON_PATH: Drive = Drive {
    free: false,
    x: 0.0,
    y: 0.0,
    z: 0.0,
    heading: 0.0,
    turn: 0.0,
    ax: 0.0,
    ay: 0.0,
    az: 0.0,
    climb: 0.0,
    staff: 0.0,
    sx: 0.0,
    sy: 0.0,
    sz: 1.0,
    flash: 10.0,
    spell: 1.0,
    shield: 0.0,
    shield_hit: 10.0,
    thumb: 0.0,
    groove: 0.0,
    shoulder: 0.0,
    picture: 0.0,
    cam_x: 0.0,
    cam_y: 0.0,
    cam_z: 0.0,
    beat: 0.0,
    arena_x: 0.0,
    arena_y: 12.0,
    arena_z: 12.0,
    foe: [0.0; WIZARD_SLOTS],
    foe_pos: [[0.0; 3]; WIZARD_SLOTS],
    kill: [0.0; WIZARD_SLOTS],
};

impl Drive {
    // A field by its name in the drive record (as wizard.kernels.json names
    // them: foe0 .. foe6 for the array).
    pub fn field(&self, id: &str) -> f32 {
        match id {
            "free" => self.free as i32 as f32,
            "x" => self.x,
            "y" => self.y,
            "z" => self.z,
            "heading" => self.heading,
            "turn" => self.turn,
            "ax" => self.ax,
            "ay" => self.ay,
            "az" => self.az,
            "climb" => self.climb,
            "staff" => self.staff,
            "sx" => self.sx,
            "sy" => self.sy,
            "sz" => self.sz,
            "flash" => self.flash,
            "spell" => self.spell,
            "shield" => self.shield,
            "shield_hit" => self.shield_hit,
            "thumb" => self.thumb,
            "groove" => self.groove,
            "shoulder" => self.shoulder,
            "cam_z" => self.cam_z,
            "cam_y" => self.cam_y,
            "cam_x" => self.cam_x,
            "picture" => self.picture,
            "beat" => self.beat,
            "arena_x" => self.arena_x,
            "arena_y" => self.arena_y,
            "arena_z" => self.arena_z,
            f if f.starts_with("foe") => f[3..].parse::<usize>().ok().and_then(|i| self.foe.get(i).copied()).unwrap_or(0.0),
            f if f.len() == 3 && (f.starts_with("fx") || f.starts_with("fy") || f.starts_with("fz")) => {
                let a = (f.as_bytes()[1] - b'x') as usize;
                f[2..].parse::<usize>().ok().and_then(|i| self.foe_pos.get(i)).map_or(0.0, |p| p[a])
            }
            f if f.starts_with("kill") => f[4..].parse::<usize>().ok().and_then(|i| self.kill.get(i).copied()).unwrap_or(0.0),
            _ => panic!("no drive field {id}"),
        }
    }

    // Sets a field by its name (as field() reads it); whether there is one.
    pub fn set(&mut self, id: &str, v: f32) -> bool {
        let f = match id {
            "free" => {
                self.free = v != 0.0;
                return true;
            }
            "x" => &mut self.x,
            "y" => &mut self.y,
            "z" => &mut self.z,
            "heading" => &mut self.heading,
            "turn" => &mut self.turn,
            "ax" => &mut self.ax,
            "ay" => &mut self.ay,
            "az" => &mut self.az,
            "climb" => &mut self.climb,
            "staff" => &mut self.staff,
            "sx" => &mut self.sx,
            "sy" => &mut self.sy,
            "sz" => &mut self.sz,
            "flash" => &mut self.flash,
            "spell" => &mut self.spell,
            "shield" => &mut self.shield,
            "shield_hit" => &mut self.shield_hit,
            "thumb" => &mut self.thumb,
            "groove" => &mut self.groove,
            "shoulder" => &mut self.shoulder,
            "cam_z" => &mut self.cam_z,
            "cam_y" => &mut self.cam_y,
            "cam_x" => &mut self.cam_x,
            "picture" => &mut self.picture,
            "beat" => &mut self.beat,
            "arena_x" => &mut self.arena_x,
            "arena_y" => &mut self.arena_y,
            "arena_z" => &mut self.arena_z,
            f if f.starts_with("kill") => match f[4..].parse::<usize>().ok().and_then(|i| self.kill.get_mut(i)) {
                Some(x) => x,
                None => return false,
            },
            f if f.starts_with("foe") => match f[3..].parse::<usize>().ok().and_then(|i| self.foe.get_mut(i)) {
                Some(x) => x,
                None => return false,
            },
            f if f.len() == 3 && (f.starts_with("fx") || f.starts_with("fy") || f.starts_with("fz")) => {
                let a = (f.as_bytes()[1] - b'x') as usize;
                match f[2..].parse::<usize>().ok().and_then(|i| self.foe_pos.get_mut(i)) {
                    Some(p) => &mut p[a],
                    None => return false,
                }
            }
            _ => return false,
        };
        *f = v;
        true
    }
}

// A parameter's value, as params.json has them: numbers (sliders, ints,
// selects, flags), checkboxes and colours (linear RGB).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Num(f64),
    Colour([f64; 3]),
}

impl Value {
    pub fn f32(&self) -> f32 {
        match self {
            Value::Bool(b) => *b as i32 as f32,
            Value::Num(x) => *x as f32,
            Value::Colour(c) => c[0] as f32,
        }
    }
}

// The uniforms in force, by id (the drawer's, with the app's overrides,
// e.g. no enemies outside fight mode).
#[derive(Clone, Debug, Default)]
pub struct ParamValues(pub HashMap<String, Value>);

impl ParamValues {
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.0.get(id)
    }
    // A number (0 if unknown).
    pub fn f32(&self, id: &str) -> f32 {
        self.0.get(id).map(Value::f32).unwrap_or(0.0)
    }
}

// What the shader says about itself (constants of native/shader).
#[derive(Clone, Debug)]
pub struct BuildInfo {
    // how many spells may be in flight at once
    pub spell_limit: u32,
    // the sound effects' sample rate, and each effect's length in seconds
    pub sfx_rate: f32,
    pub sfx_seconds: Vec<f32>,
}

// A sound effect to play: which (an index into BuildInfo::sfx, as fut/sfx.fut
// numbers them), panned -1 (left) .. 1 (right), at a gain.
#[derive(Clone, Copy, Debug)]
pub struct SfxEvent {
    pub kind: usize,
    pub pan: f32,
    pub gain: f32,
}

// A chunk of music asked of the GPU: `frames` stereo frames from `t0`
// seconds into song `song` (wrapped into its loop by the renderer), at
// `rate` a second. Chunks are asked for in order (`chunk` counts them from
// 0): the soundboard's state carries from each to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicRequest {
    pub t0: f32,
    pub rate: f32,
    pub frames: u32,
    pub chunk: u32,
    pub song: u32,
}

// The collision probe asked for this frame: after the six spheres round the
// rock and him (fut/collide.fut), these world points (x, y, z each), with
// the ids of the spells they belong to. Its result, 4 floats a sphere then
// a point: [distance from the surface to the land, normal x, y, z].
#[derive(Clone, Debug, Default)]
pub struct ProbeRequest {
    pub ids: Vec<u32>,
    pub pts: Vec<f32>,
}

pub const PROBE_SPHERES: usize = 6;

// What the game hands the renderer and the audio each frame.
#[derive(Clone, Debug)]
pub struct FrameOut {
    // the scene's time, in seconds
    pub time: f32,
    pub camera: Camera,
    pub drive: Drive,
    // the spells in flight, fut/spells.fut's spell_stride floats each
    pub spells: Vec<f32>,
    // whether to run the collision probe this frame, and at which points
    pub probe: Option<ProbeRequest>,
    // uniforms the game sets over the drawer's (e.g. enemies)
    pub overrides: Vec<(String, Value)>,
    // sounds to start
    pub sfx: Vec<SfxEvent>,
    pub paused: bool,
}
