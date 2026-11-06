// fut/rig.fut: the wizard's rig: the staff's place, the arms' IK, and the
// pose (everything about him that moves, worked out once per pixel from the
// time, the uniforms and the harness's drive record).

use crate::prelude::*;

// Wizard

// The staff stands just outside the robe's hem and leans forward; the left
// hand grips it at grip_height and the orb sits on its tip.
pub const staff_foot: Vec3 = Vec3::new(0.6, 0.0, 0.25);
pub const staff_top: Vec3 = Vec3::new(0.52, 2.0, 0.46);
// (not const-evaluable in Rust: a function)
#[inline]
pub fn staff_dir() -> Vec3 {
    normalise(staff_top - staff_foot)
}
pub const staff_radius: f32 = 0.028;
#[inline]
pub fn staff_at(y: f32) -> Vec3 {
    staff_foot + scale(y / staff_dir().y, staff_dir())
}

// The orb on its tip.
pub const orb_radius: f32 = 0.11;
#[inline]
pub fn orb_pos() -> Vec3 {
    staff_top + scale(0.1, staff_dir())
}

pub const grip_height: f32 = 1.15;
// The elbow's rest position; only used to place the fist and the wrist.
pub const left_elbow_rest: Vec3 = Vec3::new(0.36, 1.07, 0.14);
// The left palm's centre, on the elbow's side of the staff, and the cuff
// of the sleeve, set back from it so the hand sticks out (the arm's IK
// reaches for the cuff).
#[inline]
pub fn left_palm() -> Vec3 {
    let g = staff_at(grip_height);
    let towards = left_elbow_rest - g;
    g + scale(0.045, normalise(towards - scale(dot(towards, staff_dir()), staff_dir())))
}
#[inline]
pub fn left_wrist() -> Vec3 {
    left_palm() - scale(0.1, normalise(left_palm() - left_elbow_rest))
}

// Two-bone IK: for a shoulder s reaching for target t with bone lengths l1
// and l2, returns (elbow, hand). The elbow bends towards `pole`; an
// unreachable target is approached as far as the arm allows.
pub fn two_bone(s: Vec3, t: Vec3, l1: f32, l2: f32, pole: Vec3) -> (Vec3, Vec3) {
    let d = t - s;
    let dir = normalise(d);
    let reach = clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3, length(d));
    let a = (l1 * l1 - l2 * l2 + reach * reach) / (2.0 * reach); // along dir to the elbow
    let h = 0f32.maximum(l1 * l1 - a * a).sqrt(); // out from dir to the elbow
    let side = normalise(pole - scale(dot(pole, dir), dir));
    (s + scale(a, dir) + scale(h, side), s + scale(reach, dir))
}

// Breathing: three breaths per loop (4 s each); 0 exhaled, 1 inhaled.
#[inline]
pub fn breath(tm: f32) -> f32 {
    0.5 - 0.5 * (3.0 * tm).cos()
}

// Everything about the wizard that moves, worked out once per pixel. tm is
// the loop angle (for periodic motion), secs the time in seconds.
#[derive(Clone, Copy)]
pub struct Pose {
    pub tm: f32,
    pub secs: f32,
    pub fl: Flight,
    pub moon: Vec3,       // moon direction, local
    pub moon_world: Vec3, // and in the world
    pub prm: Params,      // the drawer's uniforms
    pub breath: f32,
    pub lean: f32,  // sideways shear of the body above the hips, per unit height
    pub head: Vec3, // offset of the head, beard and hat
    pub look: f32,  // head turn (radians, towards +x; the torso's twist with it)
    pub nod: f32,   // head tilt (radians, up)
    pub twist: f32, // the torso's turn (radians, towards +x), full at the shoulders
    pub glance: f32,    // the eyes' turn in the head (radians, towards +x)
    pub glance_up: f32, // and up
    pub point: f32, // 0 idle .. 1 pointing with the right hand
    pub thumb: f32, // 0 .. 1 thumbing back over his right shoulder (flying backwards)
    pub pix: f32,   // angle one pixel spans (for texture filtering)
    pub blink: f32, // 0 eyes open .. 1 closed
    pub runes: f32, // the runes' brightness, relative to their glow parameters
    // The staff, held out to cast: its rest frame turns rigidly: rotated to
    // columns (staff_x, staff_y, staff_z) and moved by staff_t (see
    // staff_world and staff_rest).
    pub staff_x: Vec3,
    pub staff_y: Vec3,
    pub staff_z: Vec3,
    pub staff_t: Vec3,
    pub orb: Vec3,       // the orb's centre, where the staff holds it
    pub orb_tint: Vec3,  // the orb's glow colour, by spell
    pub bolt_tint: Vec3, // tints its arcs and its light, by spell
    pub orb_boost: f32,  // its glow, flaring as a spell is cast
    pub spell: i32,      // the spell in hand (0 fire, 1 lightning, 2 wind)
    pub foes: Foes,      // the enemies' states
    pub dead: bool,      // defeated (drawn coming apart: see him_dying)
    pub shield: f32,
    pub shield_hit: f32, // his shield (see shield_layer in shading.fut)
    pub l_shoulder: Vec3,
    pub l_elbow: Vec3,
    pub l_wrist: Vec3,
    // the left hand's grip on the staff, the staff's axis there, and the
    // palm (on the side facing the elbow)
    pub l_grip: Vec3,
    pub l_axis: Vec3,
    pub l_palm: Vec3,
    pub r_shoulder: Vec3,
    pub r_elbow: Vec3,
    pub r_wrist: Vec3,
}

// Free flight, driven from the harness (web/flight.ts): see fut/rig.fut.
// (The drive record is `Drive`, in generated.rs.)

pub const on_path: Drive = Drive {
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
    cam_x: 0.0,
    cam_y: 0.0,
    cam_z: 0.0,
    picture: 0.0,
    fx0: 0.0,
    fx1: 0.0,
    fx2: 0.0,
    fx3: 0.0,
    fx4: 0.0,
    fx5: 0.0,
    fy0: 0.0,
    fy1: 0.0,
    fy2: 0.0,
    fy3: 0.0,
    fy4: 0.0,
    fy5: 0.0,
    fz0: 0.0,
    fz1: 0.0,
    fz2: 0.0,
    fz3: 0.0,
    fz4: 0.0,
    fz5: 0.0,
    beat: 0.0,
    arena_x: 0.0,
    arena_y: 12.0,
    arena_z: 12.0,
    foe0: 0.0,
    foe1: 0.0,
    foe2: 0.0,
    foe3: 0.0,
    foe4: 0.0,
    foe5: 0.0,
    kill0: 0.0,
    kill1: 0.0,
    kill2: 0.0,
    kill3: 0.0,
    kill4: 0.0,
    kill5: 0.0,
};

pub fn flight_of(prm: Params, dr: Drive, secs: f32) -> Flight {
    if !dr.free {
        flight_at(prm.bank, secs)
    } else {
        Flight { pos: vec(dr.x, dr.y, dr.z), heading: dr.heading, turn: dr.turn, bank: clamp(-0.35, 0.35, prm.bank * dr.turn) }
    }
}

// Rotation of v by angle (cos c, sin s) about the unit axis k.
#[inline]
pub fn rotate_about(k: Vec3, c: f32, s: f32, v: Vec3) -> Vec3 {
    scale(c, v) + scale(s, cross(k, v)) + scale((1.0 - c) * dot(k, v), k)
}

// The staff's rest frame to his, and back.
#[inline]
pub fn staff_world(pose: Pose, q: Vec3) -> Vec3 {
    pose.staff_t + scale(q.x, pose.staff_x) + scale(q.y, pose.staff_y) + scale(q.z, pose.staff_z)
}
#[inline]
pub fn staff_rest(pose: Pose, p: Vec3) -> Vec3 {
    let v = p - pose.staff_t;
    vec(dot(v, pose.staff_x), dot(v, pose.staff_y), dot(v, pose.staff_z))
}

// How the staff is held: leaning with him, turned to lead with the orb as
// he casts, brought across, tapping as he dances. (pose_at's; and on its
// own, for where the orb is: see held_orb.)
#[derive(Clone, Copy)]
pub struct StaffHold {
    tilt_c: f32,
    tilt_s: f32,
    pivot: Vec3,
    pub axis: Vec3,
    pub c: f32,
    pub s: f32,
    shift: Vec3,
    pub leaned_dir: Vec3,
}

impl StaffHold {
    // A point of the staff at rest (his frame), as held.
    #[inline]
    pub fn at(self, q: Vec3) -> Vec3 {
        let leaned = staff_foot + rotate_about(vec(0.0, 0.0, 1.0), self.tilt_c, self.tilt_s, q - staff_foot);
        self.pivot + rotate_about(self.axis, self.c, self.s, leaned - self.pivot) + self.shift
    }
}

#[inline]
pub fn staff_hold(prm: Params, dr: Drive, fl: Flight) -> StaffHold {
    let groove = saturate(dr.groove);
    let on_beat = 0.5 + 0.5 * (2.0 * PI * dr.beat).cos();
    let sway = 0.05 * groove * (PI * dr.beat).sin();
    let lean = clamp(-0.15, 0.15, prm.lean * fl.turn) + sway;
    // The staff leans with him, half as far, tilting about its foot (a shear
    // of lean per unit height is a tilt of atan lean).
    let tilt = -((0.5 * lean).atan());
    let leaned = move |q: Vec3| staff_foot + rotate_about(vec(0.0, 0.0, 1.0), tilt.cos(), tilt.sin(), q - staff_foot);
    let leaned_dir = rotate_about(vec(0.0, 0.0, 1.0), tilt.cos(), tilt.sin(), staff_dir());
    // Casting: the staff turns about his grip to lead with the orb (raised a
    // little above the aim) and is pushed forward, with a thrust on each cast.
    let aim = normalise(vec(dr.sx, dr.sy, dr.sz) + vec(0.0, 0.0, 1e-4));
    let want = normalise(aim + vec(0.0, 0.9, 0.0));
    let k = cross(leaned_dir, want);
    let staff_axis = if length(k) > 1e-4 { normalise(k) } else { vec(1.0, 0.0, 0.0) };
    let angle = 0.85 * dr.staff * clamp(-1.0, 1.0, dot(leaned_dir, want)).acos();
    let kick = dr.staff * (-12.0 * dr.flash).exp();
    let staff_pivot = leaned(staff_at(grip_height));
    // Aiming to his right (local -x), he brings the staff across in front of
    // his chest, the further the more he aims that way.
    let across = dr.staff * smoothstep(0.0, 0.7, -aim.x);
    // (dancing, the staff lifts between beats and taps down on them)
    let staff_shift = scale(0.16 * dr.staff + 0.07 * kick, normalise(vec(aim.x, 0.0, aim.z) + vec(0.0, 0.0, 1e-4)))
        + vec(-0.6 * across, 0.08 * dr.staff + 0.05 * across + 0.035 * groove * (1.0 - on_beat) * (1.0 - dr.staff), 0.22 * across);
    let (staff_cos, staff_sin) = (angle.cos(), angle.sin());
    StaffHold { tilt_c: tilt.cos(), tilt_s: tilt.sin(), pivot: staff_pivot, axis: staff_axis, c: staff_cos, s: staff_sin, shift: staff_shift, leaned_dir }
}

// Following the camera's view (over his shoulder: by the drive's shoulder,
// 0..1), he turns to look the same way: first his torso, then his head,
// then his eyes, each up to a soft limit (and his head and eyes up and
// down); looking back at him, not at all.
#[derive(Clone, Copy)]
pub struct Follow {
    pub twist: f32,
    pub turn: f32,
    pub eyes: f32,
    pub nod: f32,
    pub eyes_up: f32,
}

// x, softly limited to `l` (as x for small x).
#[inline]
fn soft(x: f32, l: f32) -> f32 {
    x / (1.0 + x.abs() / l)
}

#[inline]
pub fn follow_view(dr: Drive) -> Follow {
    let v = normalise(vec(dr.sx, dr.sy, dr.sz) + vec(0.0, 0.0, 1e-4));
    let w = saturate(dr.shoulder) * smoothstep(-0.6, -0.2, v.z);
    let a = w * v.x.atan2(v.z);
    let up = w * clamp(-1.0, 1.0, v.y).asin();
    let twist = soft(a, 0.45);
    let turn = soft(a - twist, 0.7);
    let eyes = soft(a - twist - turn, 0.4);
    let nod = soft(up, 0.45);
    Follow { twist, turn, eyes, nod, eyes_up: soft(up - nod, 0.3) }
}

// How much of the torso's twist a height has: none at the hips, all at the
// shoulders.
#[inline]
pub fn twist_at(y: f32) -> f32 {
    smoothstep(0.7, 1.3, y)
}

// The orb as he holds the staff (his frame), at `secs`.
#[inline]
pub fn held_orb(prm: Params, dr: Drive, secs: f32) -> Vec3 {
    staff_hold(prm, dr, flight_of(prm, dr, secs)).at(orb_pos())
}

pub fn pose_at(secs: f32, prm: Params, dr: Drive) -> Pose {
    let tm = loop_angle(secs);
    let fl = flight_of(prm, dr, secs);
    let b = breath(tm);
    // Turning towards local +x makes fl.turn positive. He leans into the turn,
    // and in sharper turns points the way with his right hand and looks there.
    // (in free flight he points the way he accelerates instead)
    let accel = vec(dr.ax, dr.ay, dr.az);
    let side = if dr.free { sgn(dr.ax) } else { sgn(fl.turn) };
    // Flying backwards, he jabs his right thumb back over his shoulder the
    // way he goes instead.
    let thumb = if dr.free { saturate(dr.thumb) } else { 0.0 };
    let point = (1.0 - thumb) * if dr.free { saturate(length(accel)) } else { smoothstep(0.03, 0.12, fl.turn.abs()) };
    // Dancing to the music (groove 0 .. 1, on its beat): the shoulders dip
    // and the head nods on each beat, the body sways over two, the free
    // hand conducts, the staff taps, the runes flash.
    let groove = saturate(dr.groove);
    let on_beat = 0.5 + 0.5 * (2.0 * PI * dr.beat).cos(); // 1 on the beat, 0 between
    let sway = 0.05 * groove * (PI * dr.beat).sin();
    let lean = clamp(-0.15, 0.15, prm.lean * fl.turn) + sway;
    let shift = move |y: f32| vec(lean * 0f32.maximum(y - 0.45), 0.0, 0.0);
    // inhaling lifts the shoulders; the head rises and nods forward a little
    let bd = b * prm.breath;
    let dip = groove * on_beat;
    let lift = vec(0.0, 0.012 * bd - 0.022 * dip, 0.004 * bd) + shift(1.33);
    let head = vec(0.0, 0.012 * bd - 0.04 * dip, 0.007 * bd + 0.025 * dip) + shift(1.6);
    // (he turns to look the way the camera does: see follow_view)
    let follow = follow_view(dr);
    let l_shoulder = yaw(follow.twist, vec(0.2, 1.33, 0.0) + lift);
    let r_shoulder = yaw(follow.twist, vec(-0.2, 1.33, 0.0) + lift);
    let hold = staff_hold(prm, dr, fl);
    let held = move |q: Vec3| hold.at(q);
    let (staff_axis, staff_cos, staff_sin, leaned_dir) = (hold.axis, hold.c, hold.s, hold.leaned_dir);
    // (that, as a rotation and offset)
    let staff_t = held(splat(0.0));
    let column = move |e: Vec3| held(e) - staff_t;
    // The left hand stays on the staff, closed round it with the palm facing
    // the elbow, wherever the staff and the elbow are: the arm reaches for the
    // staff, then the hand is built round the staff's grip facing the elbow
    // that gives, and the arm reaches again for its wrist.
    let l_grip = held(staff_at(grip_height));
    let l_axis = rotate_about(staff_axis, staff_cos, staff_sin, leaned_dir);
    let hand_at = move |elbow: Vec3| {
        let towards = elbow - l_grip;
        let palm = l_grip + scale(0.045, normalise(towards - scale(dot(towards, l_axis), l_axis)));
        (palm, palm - scale(0.1, normalise(palm - elbow)))
    };
    let pole = vec(0.5, -1.0, -0.5);
    let (e0, _) = two_bone(l_shoulder, held(left_wrist()), 0.34, 0.29, pole);
    let (l_elbow, l_wrist) = two_bone(l_shoulder, hand_at(e0).1, 0.34, 0.29, pole);
    let l_palm = hand_at(l_elbow).0;
    // the orb takes the colour of the spell in hand, and flares as it is cast
    let spell = ftoi(round_even(dr.spell));
    let orb_tint = if spell == 0 {
        vec(1.0, 0.42, 0.12)
    } else if spell == 2 {
        vec(0.4, 0.88, 1.0)
    } else if spell == 3 {
        vec(0.45, 1.0, 0.5)
    } else {
        prm.orb_glow_colour
    };
    let bolt_tint = if spell == 0 {
        vec(1.5, 0.65, 0.25)
    } else if spell == 2 {
        vec(0.7, 1.1, 1.3)
    } else if spell == 3 {
        vec(0.7, 1.3, 0.75)
    } else {
        splat(1.0)
    };
    let orb_boost = 1.0 + 2.0 * (-6.0 * dr.flash).exp();
    // the right hand drifts idly in front of the belly, or points into the turn
    let idle = vec(0.025 * tm.sin(), 0.018 * (2.0 * tm).sin() + 0.01 * b, 0.02 * tm.cos());
    // (dancing, it conducts: up and out, swinging across over two beats and
    // bouncing on each)
    let conduct = vec(-0.06 * (PI * dr.beat).sin(), 0.14 + 0.06 * on_beat, 0.08);
    let rest = yaw(follow.twist * twist_at(1.02), vec(-0.25, 1.02, 0.27) + idle + scale(groove, conduct) + shift(1.02));
    // (pointing to his left means reaching across the body, so aim higher and more
    // forward to keep the arm clear of the beard)
    let aim_dir = if dr.free {
        // never behind him (he can't reach), and reaching across, higher and
        // further forward, like the turns
        let a = normalise(accel + vec(0.0, 0.0, 1e-4));
        let across = 0f32.maximum(a.x);
        vec(a.x, a.y + 0.3 + 0.25 * across, 0.35f32.maximum(a.z) + 0.4 * across)
    } else if side < 0.0 {
        vec(-0.85, 0.3, 0.75)
    } else {
        vec(0.5, 0.55, 0.9)
    };
    let aim = r_shoulder + scale(0.62, normalise(aim_dir));
    // (thumbing back, a thumbs-up tossed up and back: the upper arm out to
    // the side, level, the forearm upright (a goalpost), the fist at his
    // head's height out clear of his hat's brim; the thumb, square to the
    // forearm as in a thumbs-up, then points back over his shoulder)
    let jab = r_shoulder + vec(-0.3, 0.26, 0.05);
    let target = mix(mix(rest, aim, point), jab, thumb);
    let pole = normalise(mix(vec(-0.6, -1.0, -0.5), vec(-1.0, -0.35, 0.1), thumb));
    let (r_elbow, r_wrist) = two_bone(r_shoulder, target, 0.31, 0.27, pole);
    let moon_world = moon_dir(prm.moon_azimuth, prm.moon_elevation);
    let foes = Foes {
            states: (dr.foe0, dr.foe1, dr.foe2, dr.foe3, dr.foe4, dr.foe5),
            kills: (dr.kill0, dr.kill1, dr.kill2, dr.kill3, dr.kill4, dr.kill5),
            arena: vec(dr.arena_x, dr.arena_y, dr.arena_z),
            him: prm.enemies.min(max_enemies).max(0),
            pos: (
                vec(dr.fx0, dr.fy0, dr.fz0),
                vec(dr.fx1, dr.fy1, dr.fz1),
                vec(dr.fx2, dr.fy2, dr.fz2),
                vec(dr.fx3, dr.fy3, dr.fz3),
                vec(dr.fx4, dr.fy4, dr.fz4),
                vec(dr.fx5, dr.fy5, dr.fz5),
            ),
    };
    let point_look = 0.45 * point * side * (if dr.free { 1f32.minimum(2.0 * dr.ax.abs()) } else { 1.0 }) - 0.4 * thumb;
    Pose {
        tm,
        secs,
        fl,
        prm,
        moon: to_local_dir(fl, moon_world),
        moon_world,
        breath: b,
        lean,
        head,
        // (and thumbing back, he glances over that shoulder: his right, -x)
        look: point_look + follow.twist + follow.turn,
        nod: follow.nod,
        twist: follow.twist,
        glance: 0.8 * point_look + follow.eyes,
        glance_up: follow.eyes_up,
        foes,
        dead: him_dying(foes),
        point,
        thumb,
        pix: 0.0008,
        // doubling for every 4 units/s of climb
        // (and they flash on the beat, dancing)
        runes: (if dr.free { clamp(0.1, 3.0, 2f32.powf(dr.climb / 4.0)) } else { 1.0 }) * (1.0 + 1.5 * groove * (-6.0 * fract(dr.beat)).exp()),
        // three quick blinks per loop (every 4 s), each about 0.2 s long
        blink: {
            let d = (fract(3.0 * tm / (2.0 * PI) + 0.15) - 0.5) * 28.0;
            (-(d * d)).exp()
        },
        l_shoulder,
        l_elbow,
        l_wrist,
        l_grip,
        l_axis,
        l_palm,
        r_shoulder,
        r_elbow,
        r_wrist,
        staff_x: column(vec(1.0, 0.0, 0.0)),
        staff_y: column(vec(0.0, 1.0, 0.0)),
        staff_z: column(vec(0.0, 0.0, 1.0)),
        staff_t,
        orb: held(orb_pos()),
        orb_tint,
        bolt_tint,
        orb_boost,
        spell,
        shield: dr.shield,
        shield_hit: dr.shield_hit,
    }
}
