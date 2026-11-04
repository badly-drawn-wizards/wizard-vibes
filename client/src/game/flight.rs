// Free flight (web/flight.ts): the keyboard flies the wizard's rock through
// the world. The physics runs here; the renderer gets the result as its
// drive record, and the probe reports the land round the rock (see
// fut/collide.fut), which pushes it back out of anything it flies into.
//
//   W / S   accelerate the way the camera looks / backwards
//   A / D   accelerate left / right (of the camera): strafe
//   E / Q   climb / descend           Shift   boost
//
// or a game controller (see gamepad.rs).
//
// The camera stays where it is in the world (only the mouse or the right
// stick turns it). Moving, he turns (gradually) to face the way it looks,
// and part of the way towards where he drifts sideways, so strafing turns
// him a little; flying backwards he still faces ahead, and thumbs back
// over his shoulder. He banks into turns, and points the way he
// accelerates: the harder, the more.

use super::vec::V3;
use crate::types::{Drive, ON_PATH};

// Analog control, e.g. from a game controller: fwd and side along and across
// the camera's view, up for climbing (each -1..1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stick {
    pub fwd: f32,
    pub side: f32,
    pub up: f32,
    pub boost: bool,
}

// The flight keys held (W/↑, S/↓, A/←, D/→, E, Q, Shift).
#[derive(Clone, Copy, Debug, Default)]
pub struct Held {
    pub fwd: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub boost: bool,
}

// The automatic flight path (as in fut/world.fut), to take off from.
const SPEED: f32 = 3.0;
const ALTITUDE: f32 = 12.0;
pub fn path_x(s: f32) -> f32 {
    14.0 * (0.12 * s).sin() + 6.0 * (0.27 * s + 1.0).sin()
}
pub fn path_dx(s: f32) -> f32 {
    14.0 * 0.12 * (0.12 * s).cos() + 6.0 * 0.27 * (0.27 * s + 1.0).cos()
}

const THRUST: f32 = 9.0; // units/s², forward
const SIDE: f32 = 7.0; // sideways
const CLIMB: f32 = 6.0;
const BACK: f32 = 0.7; // backwards, of the thrust forwards
const DRAG: f32 = 0.5; // per second: top speed is thrust / drag
const BOOST: f32 = 2.5;
const TURN_RATE: f32 = 2.5; // how fast the heading follows its aim (1/s)
const DRIFT: f32 = 0.4; // how far he turns towards his sideways drift (of its angle)
const MAX_TURN: f32 = 1.2; // rad/s
const CEILING: f32 = 80.0;

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// What is kept between runs (as flight.ts keeps it in sessionStorage).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Saved {
    pub active: bool,
    pub pos: V3,
    pub vel: V3,
    pub heading: f32,
}

#[derive(Clone, Debug)]
pub struct FreeFlight {
    pub active: bool,
    pub pos: V3,
    pub vel: V3,
    pub heading: f32,
    pub turn: f32,
    accel: V3,    // smoothed, world space
    last_vel: V3, // at the end of the last update
    climb: f32,   // vertical speed, smoothed
    time: f32,    // the animation time, as of the last update
}

impl Default for FreeFlight {
    fn default() -> Self {
        FreeFlight {
            active: false,
            pos: [0.0, ALTITUDE, 0.0],
            vel: [0.0, 0.0, SPEED],
            heading: 0.0,
            turn: 0.0,
            accel: [0.0; 3],
            last_vel: [0.0, 0.0, SPEED],
            climb: 0.0,
            time: 0.0,
        }
    }
}

impl FreeFlight {
    pub fn restore(s: &Saved) -> Self {
        FreeFlight { active: s.active, pos: s.pos, vel: s.vel, heading: s.heading, ..Default::default() }
    }

    pub fn saved(&self) -> Saved {
        Saved { active: self.active, pos: self.pos, vel: self.vel, heading: self.heading }
    }

    // Switches free flight on or off (see mode.rs); whether it changed (it
    // is switched on: the caller turns the camera behind him).
    pub fn set_active(&mut self, on: bool) -> bool {
        if on != self.active {
            self.toggle();
            return true;
        }
        false
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
        if self.active {
            // take off from where the path has him now, at its speed
            let s = self.time;
            let dx = path_dx(s);
            self.pos = [path_x(s), ALTITUDE, SPEED * s];
            self.vel = [dx, 0.0, SPEED];
            self.heading = dx.atan2(SPEED);
            self.turn = 0.0;
            self.accel = [0.0; 3];
            self.last_vel = self.vel;
        }
        self.climb = 0.0;
    }

    // `view` is the camera's heading in the world (radians from +z towards +x).
    pub fn update(&mut self, dt: f32, time: f32, view: f32, held: &Held, pad: Option<&Stick>) {
        self.time = time;
        if !self.active || dt <= 0.0 {
            return;
        }
        let h = held;
        let boost = if h.boost || pad.is_some_and(|p| p.boost) { BOOST } else { 1.0 };
        let fwd: V3 = [view.sin(), 0.0, view.cos()];
        let right: V3 = [-fwd[2], 0.0, fwd[0]]; // the camera's right
        let b = |x: bool| if x { 1.0 } else { 0.0 };
        let k = |a: bool, c: bool| b(a) - b(c);
        let clamp1 = |x: f32| x.clamp(-1.0, 1.0);
        let push = clamp1(b(h.fwd) - b(h.back) + pad.map_or(0.0, |p| p.fwd));
        let thrust = push * if push < 0.0 { BACK } else { 1.0 } * THRUST * boost;
        let side = clamp1(k(h.right, h.left) + pad.map_or(0.0, |p| p.side)) * SIDE * boost;
        let mut a: V3 = [0.0; 3];
        for i in 0..3 {
            a[i] = fwd[i] * thrust + right[i] * side - DRAG * self.vel[i];
        }
        a[1] += clamp1(k(h.up, h.down) + pad.map_or(0.0, |p| p.up)) * CLIMB * boost;
        let steering = (thrust / THRUST).abs().max((side / SIDE).abs());
        for i in 0..3 {
            self.vel[i] += a[i] * dt;
            self.pos[i] += self.vel[i] * dt;
        }
        if self.pos[1] > CEILING {
            self.pos[1] = CEILING;
            self.vel[1] = self.vel[1].min(0.0);
        }

        // face the way the camera looks, turned part of the way towards
        // where he drifts sideways (the drift's angle from the view, a
        // backwards drift mirrored forwards: he does not turn round), as he
        // moves or steers
        let wrap = |d: f32| d.sin().atan2(d.cos());
        let hs = self.vel[0].hypot(self.vel[2]);
        let mut drift = if hs > 0.5 { wrap(self.vel[0].atan2(self.vel[2]) - view) } else { 0.0 };
        if drift.abs() > std::f32::consts::FRAC_PI_2 {
            drift = drift.signum() * std::f32::consts::PI - drift;
        }
        let aim = view + DRIFT * drift * smoothstep(0.5, 3.0, hs);
        let weight = smoothstep(0.5, 3.0, hs).max(steering);
        let turn = (wrap(aim - self.heading) * TURN_RATE).clamp(-MAX_TURN, MAX_TURN) * weight;
        self.heading += turn * dt;
        self.turn += (turn - self.turn) * (1.0 - (-dt / 0.15).exp());

        // the acceleration he feels (thrust, drag, turning and the land pushing
        // back alike: pressing into the ground is no acceleration), smoothed
        let f = 1.0 - (-dt / 0.2).exp();
        for i in 0..3 {
            self.accel[i] += ((self.vel[i] - self.last_vel[i]) / dt - self.accel[i]) * f;
        }
        self.last_vel = self.vel;
        self.climb += (self.vel[1] - self.climb) * (1.0 - (-dt / 0.3).exp());
    }

    // Pushes the rock out of the land the probe found it in: per sphere,
    // [distance to the land (negative inside), normal x, y, z].
    pub fn collide(&mut self, probe: &[f32]) {
        if !self.active {
            return;
        }
        for p in probe.chunks_exact(4) {
            let d = p[0];
            if !(d < 0.0) {
                continue;
            }
            let n = [p[1], p[2], p[3]];
            let vn = super::vec::dot(self.vel, n);
            for i in 0..3 {
                self.pos[i] -= n[i] * d;
                if vn < 0.0 {
                    self.vel[i] -= n[i] * vn; // stop going in; slide along
                }
            }
        }
    }

    // Where he is and which way he faces, in free flight or on the path.
    pub fn place(&self, time: f32) -> (V3, f32) {
        if self.active {
            return (self.pos, self.heading);
        }
        ([path_x(time), ALTITUDE, SPEED * time], path_dx(time).atan2(SPEED))
    }

    // The drive record's flight fields (the rest as on the path).
    pub fn drive(&self) -> Drive {
        if !self.active {
            return ON_PATH;
        }
        // the felt acceleration in his frame (+x his left, +z ahead)
        let (s, c) = self.heading.sin_cos();
        let [wx, wy, wz] = self.accel;
        let lx = wx * c - wz * s;
        let lz = wz * c + wx * s;
        let mag = (lx * lx + wy * wy + lz * lz).sqrt();
        let amount = smoothstep(1.5, 7.0, mag) / if mag == 0.0 { 1.0 } else { mag };
        // flying backwards: his speed backwards along his facing
        let back = -(self.vel[0] * s + self.vel[2] * c);
        Drive {
            free: true,
            x: self.pos[0],
            y: self.pos[1],
            z: self.pos[2],
            heading: self.heading,
            turn: self.turn,
            ax: lx * amount,
            ay: wy * amount,
            az: lz * amount,
            climb: self.climb,
            thumb: smoothstep(1.0, 4.0, back),
            ..ON_PATH
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_off_from_the_path() {
        let mut f = FreeFlight::default();
        f.update(0.016, 5.0, 0.0, &Held::default(), None);
        assert!(f.set_active(true));
        assert_eq!(f.pos, [path_x(5.0), 12.0, 15.0]);
        assert_eq!(f.place(5.0), (f.pos, path_dx(5.0).atan2(3.0)));
        assert!(!f.set_active(true));
        assert_eq!(f.drive().free, true);
        f.set_active(false);
        assert_eq!(f.drive(), ON_PATH);
    }

    #[test]
    fn thrust_reaches_top_speed() {
        let mut f = FreeFlight { active: true, vel: [0.0; 3], ..Default::default() };
        let held = Held { fwd: true, ..Default::default() };
        for _ in 0..2000 {
            f.update(0.02, 0.0, 0.0, &held, None);
        }
        // thrust / drag along +z, and facing it
        assert!((f.vel[2] - THRUST / DRAG).abs() < 0.01, "{:?}", f.vel);
        assert!(f.heading.abs() < 1e-3);
        // S: backwards, to its own top speed, still facing ahead, thumbing
        let held = Held { back: true, ..Default::default() };
        for _ in 0..2000 {
            f.update(0.02, 0.0, 0.0, &held, None);
        }
        assert!((f.vel[2] + BACK * THRUST / DRAG).abs() < 0.01, "{:?}", f.vel);
        assert!(f.heading.abs() < 1e-3);
        assert_eq!(f.drive().thumb, 1.0);
    }

    #[test]
    fn strafing_turns_him_part_way() {
        // D held, the view fixed along +z: he slides to the camera's right
        // (-x), and turns only part of the way towards it, not round and round
        let mut f = FreeFlight { active: true, vel: [0.0; 3], ..Default::default() };
        let held = Held { right: true, ..Default::default() };
        for _ in 0..2000 {
            f.update(0.02, 0.0, 0.0, &held, None);
        }
        assert!((f.vel[0] + SIDE / DRAG).abs() < 0.01 && f.vel[2].abs() < 0.01, "{:?}", f.vel);
        let want = -DRIFT * std::f32::consts::FRAC_PI_2;
        assert!((f.heading - want).abs() < 1e-3, "{} not {want}", f.heading);
    }

    #[test]
    fn one_step() {
        // one step of 0.1 s from rest, W and D held, looking along +x
        let mut f = FreeFlight { active: true, vel: [0.0; 3], last_vel: [0.0; 3], ..Default::default() };
        let held = Held { fwd: true, right: true, ..Default::default() };
        let view = std::f32::consts::FRAC_PI_2;
        f.update(0.1, 0.0, view, &held, None);
        // fwd (1, 0, 0) * 9 + right (0, 0, 1) * 7
        assert!((f.vel[0] - 0.9).abs() < 1e-5 && (f.vel[2] - 0.7).abs() < 1e-5, "{:?}", f.vel);
        assert!((f.pos[0] - 0.09).abs() < 1e-5 && (f.pos[2] - 0.07).abs() < 1e-5);
    }

    #[test]
    fn collision_pushes_out() {
        let mut f = FreeFlight { active: true, pos: [0.0, 1.0, 0.0], vel: [1.0, -2.0, 0.0], ..Default::default() };
        let mut probe = vec![1.0, 0.0, 1.0, 0.0];
        probe.extend([-0.5, 0.0, 1.0, 0.0]);
        f.collide(&probe);
        assert_eq!(f.pos, [0.0, 1.5, 0.0]);
        assert_eq!(f.vel, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn ceiling() {
        let mut f = FreeFlight { active: true, pos: [0.0, 79.99, 0.0], vel: [0.0, 5.0, 0.0], ..Default::default() };
        f.update(0.1, 0.0, 0.0, &Held { up: true, ..Default::default() }, None);
        assert_eq!(f.pos[1], CEILING);
        assert_eq!(f.vel[1], 0.0);
    }
}
