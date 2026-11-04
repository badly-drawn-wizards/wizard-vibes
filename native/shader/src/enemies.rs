// fut/enemies.fut: enemies: other wizards flying round him, each on a
// platform of its own (see platform.fut) with a magic circle of its own
// colour, and looks of its own, all hashed from the seed and its number.
// Where they are is a pure function of the time, the arena (see foes.fut)
// and where he is: each circles the arena at its own distance, height and
// speed, turned to face him, so he can fly round them.
//
// Their fights are run by the harness (web/enemies.ts), which passes each
// one's state as a float (see foe_of): how many times it has been replaced
// (which re-rolls its looks and its place), and whether it is appearing or
// defeated. 0 is one there from the start.
//
// They are drawn in the layer layers_enemies, with materials m_enemy +
// enemy_parts * i + part (part_* below). Or, with full_enemies
// (compile-time), his own figure, at rest, in their colours and proportions
// (materials m_enemy_full + full_parts * i + his material).

use crate::prelude::*;

pub const part_robe: i32 = 0;
pub const part_skin: i32 = 1;
pub const part_beard: i32 = 2;
pub const part_hat: i32 = 3;
pub const part_band: i32 = 4;
pub const part_staff: i32 = 5;
pub const part_orb: i32 = 6;
pub const part_top: i32 = 7; // the platform's top, with the circle
pub const part_side: i32 = 8; // the rest of the platform

// Everything about an enemy lies within this sphere of its own space
// (the platform below, the hat's tip above).
pub const enemy_centre: Vec3 = Vec3::new(0.0, 0.1, 0.0);
pub const enemy_radius: f32 = 3.0;

pub const appear_time: f32 = 1.5; // seconds an enemy takes to appear
pub const fall_time: f32 = 3.0; // and a defeated one to fall out of sight

// Random number k (in [0, 1)) of enemy i, generation g.
#[inline]
pub fn enemy_rnd(seed: i32, g: i32, i: i32, k: i32) -> f32 {
    hash3(seed.wrapping_add(7919i32.wrapping_mul(g)), i + 17, k + 101)
}

// Where enemy i is (in the world) and which way it faces (a yaw: its +z
// points that way); its generation; how far it has still to appear (0..1)
// and how long since it was defeated (or -1); its size, and how far it has
// tumbled over (radians, about its x axis through tumble_axis).
#[derive(Clone, Copy)]
pub struct Enemy {
    pub pos: Vec3,
    pub yaw: f32,
    pub r#gen: i32,
    pub appear: f32,
    pub dying: f32,
    pub size: f32,
    pub tumble: f32,
}

pub const tumble_axis: Vec3 = Vec3::new(0.0, 0.5, 0.0);

pub fn enemy_at(seed: i32, fs: Foes, i: i32, secs: f32, fl: Flight) -> Enemy {
    let state = foe_state(fs, i);
    let gf = (state / 16.0).floor();
    let phase = state - 16.0 * gf;
    let r#gen = ftoi(gf);
    let appear = if phase < 8.0 { saturate(phase / appear_time) } else { 0.0 };
    let dying = if phase >= 8.0 { phase - 8.0 } else { -1.0 };
    let r = move |k: i32| enemy_rnd(seed, r#gen, i, k);
    let radius = 9.0 + 12.0 * r(0);
    let speed = (0.04 + 0.06 * r(1)) * (if r(2) < 0.5 { -1.0 } else { 1.0 });
    // (spread round him, then each drifting round at its own speed)
    let a = 2.0 * PI * (i as f32 / max_enemies as f32 + 0.12 * r(3)) + speed * secs;
    let h = -1.5 + 4.0 * r(4) + 0.5 * (0.6 * secs + 6.3 * r(5)).sin();
    let off = vec(radius * a.sin(), h, radius * a.cos());
    // (a defeated one falls, faster and faster, tumbling over, and shrinks
    // away at the end; a new one grows in)
    let fall = if dying >= 0.0 { 2.5 * dying * dying } else { 0.0 };
    let size = if dying >= 0.0 { smoothstep(fall_time, fall_time - 1.0, dying) } else { 1.0 - 0.95 * appear * appear };
    let pos = fs.arena + off;
    let to_him = fl.pos - pos;
    Enemy {
        pos: pos - vec(0.0, fall, 0.0),
        yaw: to_him.x.atan2(to_him.z),
        r#gen,
        appear,
        dying,
        size,
        tumble: if dying >= 0.0 { 1.4 * dying + 0.5 * dying * dying } else { 0.0 },
    }
}

// Whether it is drawn at all.
#[inline]
pub fn shown(e: Enemy) -> bool {
    e.dying < fall_time
}

// A point of his space in enemy e's (where it is modelled at full size,
// upright), and how distances there scale back to his.
pub fn enemy_space(fl: Flight, e: Enemy, p: Vec3) -> Vec3 {
    let q = yaw_inv(e.yaw, to_world(fl, p) - e.pos) - tumble_axis;
    let (c, s) = (e.tumble.cos(), e.tumble.sin());
    let q = vec(q.x, c * q.y + s * q.z, c * q.z - s * q.y) + tumble_axis;
    scale(1.0 / 0.02f32.maximum(e.size), q)
}

// A colour from hue (0..1, round the wheel), saturation and value.
pub fn hsv(h: f32, s: f32, v: f32) -> Vec3 {
    let k = move |n: f32| {
        let t = 6.0 * fract(n / 6.0 + h);
        v - v * s * saturate(t.minimum((4.0 - t).minimum(1.0)))
    };
    vec(k(5.0), k(3.0), k(1.0))
}

#[derive(Clone, Copy)]
pub struct Looks {
    pub kind: i32,
    pub robe: Vec3,
    pub hat: Vec3,
    pub beard: Vec3,
    pub skin: Vec3,
    pub circle: Vec3,
    pub carpet: Vec3,
    pub hem: f32,
    pub hat_h: f32,
    pub droop: f32,
    pub beard_len: f32,
    pub phase: f32,
}

pub fn looks_of(seed: i32, g: i32, i: i32) -> Looks {
    let r = move |k: i32| enemy_rnd(seed, g, i, k);
    let robe_hue = r(10);
    let hat_hue = if r(11) < 0.5 { robe_hue } else { fract(robe_hue + 0.35 + 0.3 * r(12)) };
    let b = r(13);
    let beard = if b < 0.3 {
        vec(0.85, 0.85, 0.85)
    } else if b < 0.5 {
        vec(0.4, 0.4, 0.42)
    } else if b < 0.7 {
        vec(0.03, 0.03, 0.03)
    } else if b < 0.85 {
        vec(0.5, 0.17, 0.05)
    } else {
        vec(0.18, 0.09, 0.04)
    };
    Looks {
        kind: ftoi(2f32.minimum(3.0 * r(14))),
        robe: hsv(robe_hue, 0.6 + 0.35 * r(15), 0.2 + 0.3 * r(16)),
        hat: hsv(hat_hue, 0.5 + 0.4 * r(17), 0.1 + 0.25 * r(18)),
        beard,
        skin: mix(vec(0.85, 0.6, 0.48), vec(0.3, 0.17, 0.1), r(19)),
        circle: hsv(r(20), 0.75 + 0.25 * r(21), 1.0),
        carpet: hsv(r(22), 0.85, 0.25 + 0.25 * r(23)),
        hem: 0.36 + 0.14 * r(24),
        hat_h: 0.5 + 0.45 * r(25),
        droop: 0.3 * r(26),
        beard_len: 0.2 + 0.4 * r(27),
        phase: 6.3 * r(28),
    }
}

// The staff, the hand on it and the orb on its tip.
pub const enemy_grip: Vec3 = Vec3::new(0.45, 1.05, 0.18);
pub const enemy_orb: Vec3 = Vec3::new(0.45, 2.05, 0.2);

// The figure, in its own space: (distance, part).
pub fn sd_enemy_figure(lk: Looks, secs: f32, q: Vec3) -> (f32, i32) {
    // (cut flat at the platform: the cone's rounded foot poked through the carpet)
    let robe = sd_round_cone_ab(q, vec(0.0, 0.06, 0.0), vec(0.0, 1.32, 0.0), lk.hem, 0.15).maximum(-q.y);
    // one hand on the staff, the other raised and lowered as if casting
    let cast = 0.5 + 0.5 * (1.3 * secs + lk.phase).sin();
    let hand = vec(-0.3, 1.12 + 0.38 * cast, 0.28 + 0.12 * cast);
    let arms = sd_capsule(q, vec(0.17, 1.3, 0.0), enemy_grip - vec(0.06, 0.0, 0.0), 0.065)
        .minimum(sd_capsule(q, vec(-0.17, 1.3, 0.0), hand - scale(0.06, normalise(hand - vec(-0.17, 1.3, 0.0))), 0.065));
    let cloth = smin(0.05, robe, arms);
    let head = smin(0.03, sd_sphere(q - vec(0.0, 1.53, 0.01), 0.15), sd_sphere(q - vec(0.0, 1.51, 0.16), 0.04));
    let skin = head.minimum(sd_sphere(q - enemy_grip, 0.05).minimum(sd_sphere(q - hand, 0.05)));
    let beard = sd_round_cone_ab(q, vec(0.0, 1.47, 0.09), vec(0.0, 1.47 - lk.beard_len, 0.15), 0.11, 0.015);
    let hq = q - vec(0.0, 1.64, 0.0);
    let hat = (sd_cylinder(hq, 0.32, 0.01) - 0.008).minimum(sd_round_cone_ab(hq, vec(0.0, 0.0, 0.0), vec(0.0, lk.hat_h, -lk.droop), 0.21, 0.012));
    let band = sd_torus(hq - vec(0.0, 0.05, 0.0), 0.2, 0.024);
    let staff = sd_capsule(q, vec(0.45, 0.0, 0.18), enemy_orb - vec(0.0, 0.08, 0.0), 0.025);
    let orb = sd_sphere(q - enemy_orb, 0.09);
    let h = (cloth, part_robe);
    let h = umin((skin, part_skin), h);
    let h = umin((beard, part_beard), h);
    let h = umin((hat, part_hat), h);
    let h = umin((band, part_band), h);
    let h = umin((staff, part_staff), h);
    umin((orb, part_orb), h)
}

// With full_enemies: the pose of an enemy with looks lk (his, at rest,
// breathing in its own time, in its colours and proportions).
pub fn enemy_pose(prm: Params, lk: Looks, secs: f32) -> Pose {
    let prm = Params {
        robe_colour: lk.robe,
        hat_colour: lk.hat,
        skin_colour: lk.skin,
        beard_tint: lk.beard,
        robe_hem: lk.hem + 0.06,
        hat_droop: lk.droop,
        ..prm
    };
    pose_at(secs + lk.phase, prm, on_path)
}

// The figure of enemy i (looks lk), at q in its space: (distance, material).
pub fn enemy_figure(prm: Params, i: i32, lk: Looks, secs: f32, q: Vec3) -> Hit {
    let m = m_enemy + enemy_parts * i;
    if full_enemies {
        let ep = enemy_pose(prm, lk, secs);
        let (d, fm) = bounded_figure(figure_layers, ep, 0.1, q);
        umin((sd_sphere(q - ep.orb, orb_radius), m + part_orb), (d, m_enemy_full + full_parts * i + hit_material(fm)))
    } else {
        let (d, part) = sd_enemy_figure(lk, secs, q);
        (d, m + part)
    }
}

// The enemies near his space's point p (tagged materials).
#[inline(never)]
pub fn enemies_dist(prm: Params, fs: Foes, secs: f32, fl: Flight, p: Vec3) -> Hit {
    let seed = prm.enemy_seed;
    let mut best = nothing;
    let n = prm.enemies.min(max_enemies);
    let mut i = 0;
    while i < n {
        let e = enemy_at(seed, fs, i, secs, fl);
        best = if !(shown(e)) {
            best
        } else {
            let q = enemy_space(fl, e, p);
            let bound = e.size * (length(q - enemy_centre) - enemy_radius);
            let m = m_enemy + enemy_parts * i;
            if bound > 0.3f32.minimum(best.0) {
                umin(best, (bound, m))
            } else {
                let lk = looks_of(seed, e.r#gen, i);
                let (pd, pm) = platform_exact(lk.kind, secs, q);
                let (fd, fm) = enemy_figure(prm, i, lk, secs, q);
                let h = if fd < pd { (e.size * fd, fm) } else { (e.size * pd, m + if pm == m_ground { part_top } else { part_side }) };
                umin(best, h)
            }
        };
        i += 1;
    }
    tag(layers_enemies, best)
}

// Sphere-traces the enemies alone along ro + t rd up to tmax: (distance,
// tagged material), or infinity. Only the stretch of the ray through their
// bounding spheres is marched (most rays miss them all, at the cost of a
// ray-sphere test each).
pub fn march_enemies(prm: Params, fs: Foes, secs: f32, fl: Flight, ro: Vec3, rd: Vec3, tmax: f32) -> (f32, i32) {
    let (mut t0, mut t1) = (INF, -INF);
    let n = prm.enemies.min(max_enemies);
    let mut i = 0;
    while i < n {
        let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
        // (tumbling moves the centre round the axis: a wider sphere covers it)
        let c = to_local_dir(fl, e.pos + tumble_axis - fl.pos);
        let (a, b) = ray_sphere(c, e.size * enemy_radius + 0.6, ro, rd);
        let (n0, n1) = if !(shown(e)) || a > b || b < 0.0 { (t0, t1) } else { (t0.minimum(a), t1.maximum(b)) };
        t0 = n0;
        t1 = n1;
        i += 1;
    }
    let t1 = t1.minimum(tmax);
    if t0 >= t1 {
        (INF, 0)
    } else {
        let (mut t, mut i, mut hit, mut m) = (0.02f32.maximum(t0), 0i32, false, 0i32);
        while i < 128 && t < t1 && !hit {
            let (d, mat) = enemies_dist(prm, fs, secs, fl, ro + scale(t, rd));
            let (nt, ni, nh, nm) = if d < 0.0005 * t { (t, i + 1, true, mat) } else { (t + d, i + 1, false, m) };
            t = nt;
            i = ni;
            hit = nh;
            m = nm;
        }
        if hit { (t, m) } else { (INF, 0) }
    }
}
