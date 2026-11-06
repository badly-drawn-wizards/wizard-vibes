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
pub const part_flesh: i32 = 9; // defeated: the giblets' insides, where they were cut

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
// tumbled over (radians, about its x axis through tumble_axis). Those are
// its platform's: defeated, the figure bursts where it stood (`burst`, in
// the world, not falling) into giblets (see sd_giblets).
#[derive(Clone, Copy)]
pub struct Enemy {
    pub pos: Vec3,
    pub burst: Vec3,
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
        burst: pos,
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

// A point of his space in the burst's (the figure's space as it stood when
// defeated: upright, full size, not falling).
#[inline]
pub fn burst_space(fl: Flight, e: Enemy, p: Vec3) -> Vec3 {
    yaw_inv(e.yaw, to_world(fl, p) - e.burst)
}

// ---------------------------------------------------------------------
// Giblets. Defeated, the figure bursts: it is cut into chunks (its Voronoi
// cells round fixed seeds: each chunk the figure intersected with its
// cell, so at the start they fit together), and each flies off on a
// parabola, from the chest outwards and up, falling under gravity and
// spinning about an axis of its own; in its last half second it shrinks
// away. Small bits of flesh fly with them, further and faster. The cut faces (where the cell, not the figure, is the surface)
// are the flesh inside; the cloth near a cut is soaked. Each chunk is moved
// rigidly (and the cells are half-spaces), so the distance stays a bound.

pub const giblets: i32 = 10;
pub const giblet_bits: i32 = 10; // and small bits of flesh flung with them
pub const giblet_reach: f32 = 0.85; // every chunk lies within this of its seed
const gravity: f32 = 9.0;

// Chunk k's seed, in the figure's space: the legs, the body, the arms, the
// head, the hat and the staff, each a chunk or two.
#[inline]
pub fn giblet_seed(k: i32) -> Vec3 {
    if k == 0 {
        vec(-0.15, 0.25, 0.1)
    } else if k == 1 {
        vec(0.15, 0.25, -0.1)
    } else if k == 2 {
        vec(0.0, 0.62, 0.15)
    } else if k == 3 {
        vec(0.08, 0.66, -0.16)
    } else if k == 4 {
        vec(-0.12, 0.98, 0.0)
    } else if k == 5 {
        vec(0.22, 1.18, 0.1)
    } else if k == 6 {
        vec(-0.26, 1.28, 0.16)
    } else if k == 7 {
        vec(0.0, 1.5, 0.06)
    } else if k == 8 {
        vec(0.0, 1.95, -0.02)
    } else {
        vec(0.45, 1.05, 0.2)
    }
}

// Chunk k's flight (enemy i, generation g): its velocity at the burst, the
// axis it spins about and how fast (radians a second).
#[inline]
pub fn giblet_motion(seed: i32, g: i32, i: i32, k: i32) -> (Vec3, Vec3, f32) {
    // (one hash, spread into four numbers: this runs at every step of a
    // march through the burst)
    let h = enemy_rnd(seed, g, i, 40 + k);
    let r = move |j: i32| fract(h * (17.0 + 23.0 * j as f32) + 0.31 * j as f32);
    let out = giblet_seed(k) - vec(0.0, 1.0, 0.0);
    let flat = normalise(vec(out.x, 0.0, out.z) + vec(0.01 * (r(0) - 0.5), 0.0, 0.01));
    let v = scale(2.2 + 2.4 * r(0), flat) + vec(0.0, 2.5 + 2.5 * r(1) + 1.5 * out.y, 0.0);
    let a = 2.0 * PI * r(2);
    let axis = normalise(vec(a.cos(), 2.0 * r(3) - 1.0, a.sin()));
    (v, axis, (4.0 + 8.0 * r(3)) * if r(2) < 0.5 { 1.0 } else { -1.0 })
}

// Chunk k's cell at q (figure space): the distance outside it (negative
// inside): the furthest of the half-spaces nearer its seed than another's.
#[inline]
pub fn giblet_cell(k: i32, q: Vec3) -> f32 {
    let sk = giblet_seed(k);
    let mut d = -1e9f32;
    let mut j = 0;
    while j < giblets {
        if j != k {
            let sj = giblet_seed(j);
            d = d.maximum(dot(q - scale(0.5, sk + sj), normalise(sj - sk)));
        }
        j += 1;
    }
    d
}

// Where chunk k is tau seconds after the burst: its seed's place, the
// rotation (cos, sin of the angle about its axis) and its size.
#[inline]
pub fn giblet_place(seed: i32, g: i32, i: i32, k: i32, tau: f32) -> (Vec3, Vec3, f32, f32, f32) {
    let (v, axis, spin) = giblet_motion(seed, g, i, k);
    let c = giblet_seed(k) + scale(tau, v) - vec(0.0, 0.5 * gravity * tau * tau, 0.0);
    let a = -spin * tau;
    (c, axis, a.cos(), a.sin(), smoothstep(fall_time, fall_time - 0.5, tau))
}

// A point of the burst's space in chunk k's rest (the figure's space, where
// the chunk was cut from), given the chunk's place (giblet_place).
#[inline]
pub fn giblet_rest(k: i32, place: (Vec3, Vec3, f32, f32, f32), qb: Vec3) -> Vec3 {
    let (c, axis, ca, sa, size) = place;
    let r = rotate_about(axis, ca, sa, qb - c);
    giblet_seed(k) + scale(1.0 / 0.02f32.maximum(size), r)
}

// The giblets of enemy i (looks lk), at qb in the burst's space, tau seconds
// after it: (distance, material), or `best` where none is nearer.
pub fn sd_giblets(prm: Params, i: i32, lk: Looks, g: i32, secs: f32, tau: f32, qb: Vec3, best: Hit) -> Hit {
    let seed = prm.enemy_seed;
    let flesh = m_enemy + enemy_parts * i + part_flesh;
    let mut h = best;
    let mut k = 0;
    while k < giblets {
        let place = giblet_place(seed, g, i, k, tau);
        let (c, size) = (place.0, place.4);
        let bound = length(qb - c) - size * giblet_reach;
        if bound < h.0 {
            let qr = giblet_rest(k, place, qb);
            // (the figure as it was at the burst, its casting arm still)
            let (fd, fm) = enemy_figure(prm, i, lk, secs - tau, qr);
            let cell = giblet_cell(k, qr);
            let d = size * fd.maximum(cell);
            if d < h.0 {
                h = (d, if cell >= fd { flesh } else { fm });
            }
        }
        k += 1;
    }
    // and bits: small drops of flesh and blood, flung further and faster
    let mut j = 0;
    while j < giblet_bits {
        let (c, radius) = giblet_bit(seed, g, i, j, tau);
        let d = length(qb - c) - radius;
        if d < h.0 {
            h = (d, flesh);
        }
        j += 1;
    }
    h
}

// Bit j of enemy i's giblets, tau seconds after the burst: its centre (in
// the burst's space) and radius.
#[inline]
pub fn giblet_bit(seed: i32, g: i32, i: i32, j: i32, tau: f32) -> (Vec3, f32) {
    let hj = enemy_rnd(seed, g, i, 100 + j);
    let r = move |n: i32| fract(hj * (19.0 + 29.0 * n as f32) + 0.37 * n as f32);
    let a = 2.0 * PI * r(0);
    let up = 0.2 + 0.8 * r(1);
    let across = (1.0 - up * up).sqrt();
    let v = scale(3.0 + 2.2 * r(2), vec(a.cos() * across, up, a.sin() * across));
    let c = vec(0.0, 1.1, 0.0) + scale(tau, v) - vec(0.0, 0.5 * gravity * tau * tau, 0.0);
    (c, smoothstep(fall_time, fall_time - 0.5, tau) * (0.025 + 0.05 * r(3)))
}

// The stretch of a ray (in his space) that passes near enemy e's giblets:
// through any chunk's or bit's sphere ((INF, -INF) if none): so that a ray
// through the burst's wide bound that misses every one is not marched.
pub fn giblets_span(prm: Params, e: Enemy, i: i32, fl: Flight, ro: Vec3, rd: Vec3) -> (f32, f32) {
    let (seed, g, tau) = (prm.enemy_seed, e.r#gen, e.dying);
    let at = move |c: Vec3| to_local_dir(fl, e.burst + yaw(e.yaw, c) - fl.pos);
    let (mut t0, mut t1) = (INF, -INF);
    let mut k = 0;
    while k < giblets {
        let place = giblet_place(seed, g, i, k, tau);
        let (a, b) = ray_sphere(at(place.0), place.4 * giblet_reach + 0.05, ro, rd);
        if a <= b && b >= 0.0 {
            t0 = t0.minimum(a);
            t1 = t1.maximum(b);
        }
        k += 1;
    }
    let mut j = 0;
    while j < giblet_bits {
        let (c, radius) = giblet_bit(seed, g, i, j, tau);
        let (a, b) = ray_sphere(at(c), radius + 0.05, ro, rd);
        if a <= b && b >= 0.0 {
            t0 = t0.minimum(a);
            t1 = t1.maximum(b);
        }
        j += 1;
    }
    (t0, t1)
}

// Which chunk of enemy i a point of the burst's space is on (the nearest),
// and where that is in its rest: (the point there, how far inside its cell
// (negative: inside; near 0 by a cut), the chunk's axis and rotation, for
// turning normals into its rest).
pub fn giblet_at(prm: Params, i: i32, lk: Looks, g: i32, secs: f32, tau: f32, qb: Vec3) -> (Vec3, f32, Vec3, f32, f32) {
    let seed = prm.enemy_seed;
    let mut best = (1e9f32, qb, 0f32, vec(0.0, 1.0, 0.0), 1f32, 0f32);
    let mut k = 0;
    while k < giblets {
        let place = giblet_place(seed, g, i, k, tau);
        let (c, axis, ca, sa, size) = place;
        if length(qb - c) - size * giblet_reach < best.0 {
            let qr = giblet_rest(k, place, qb);
            let (fd, _) = enemy_figure(prm, i, lk, secs - tau, qr);
            let cell = giblet_cell(k, qr);
            let d = size * fd.maximum(cell);
            if d < best.0 {
                best = (d, qr, cell, axis, ca, sa);
            }
        }
        k += 1;
    }
    (best.1, best.2, best.3, best.4, best.5)
}

// The giblets' bounding sphere tau seconds after the burst (in the burst's
// space): every chunk flies within it.
#[inline]
pub fn giblets_bound(tau: f32) -> (Vec3, f32) {
    (vec(0.0, 1.0 + 3.5 * tau - 0.5 * gravity * tau * tau, 0.0), 1.8 + 5.5 * tau)
}

// ---------------------------------------------------------------------
// The blood mist: where a figure bursts, a cloud of fine blood billowing
// out from its chest, thinning as it spreads and settles. Integrated along
// a ray (like the clouds, separately from the march): ro + t rd for t in
// [0, tend], through each burst's sphere: (radiance, transmittance).
pub fn blood_mist(prm: Params, fs: Foes, secs: f32, fl: Flight, ro: Vec3, rd: Vec3, tend: f32) -> (Vec3, f32) {
    let (mut rad, mut tr) = (splat(0.0), 1f32);
    let n = prm.enemies.min(max_enemies);
    let mut i = 0;
    while i < n {
        let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
        let tau = e.dying;
        if tau >= 0.0 && tau < fall_time {
            let radius = 0.35 + 2.6 * (1.0 - (-2.6 * tau).exp());
            let thick = 9.0 * (-1.8 * tau).exp() * smoothstep(0.0, 0.05, tau) * smoothstep(fall_time, fall_time - 0.8, tau);
            let centre = e.burst + vec(0.0, 1.1 - 0.3 * tau * tau, 0.0);
            let c = to_local_dir(fl, centre - fl.pos);
            let (a, b) = ray_sphere(c, radius, ro, rd);
            let (a, b) = (a.maximum(0.0), b.minimum(tend));
            if a < b {
                let steps = 10;
                let dt = (b - a) / steps as f32;
                let mut j = 0;
                while j < steps {
                    let p = ro + scale(a + (j as f32 + 0.5) * dt, rd);
                    let r2 = dot(p - c, p - c) / (radius * radius);
                    let w = to_world(fl, p);
                    // (billowing, in wisps: two octaves, thresholded, drifting)
                    let m = scale(3.0 / radius, w - centre) + vec(0.0, -0.8 * tau, 0.3 * tau);
                    let billow = smoothstep(0.35, 0.85, 0.65 * vnoise3(m) + 0.35 * vnoise3(scale(2.7, m)));
                    let dens = thick * (1.0 - r2).maximum(0.0) * (1.0 - r2).maximum(0.0) * billow;
                    // (fine blood: it mostly absorbs, scattering a dim red)
                    rad = rad + scale(tr * dens * dt, vec(0.06, 0.005, 0.004));
                    tr = tr * (-dens * dt).exp();
                    j += 1;
                }
            }
        }
        i += 1;
    }
    (rad, tr)
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
            let lk = looks_of(seed, e.r#gen, i);
            // the platform, and alive, the figure on it
            let h = if bound > 0.3f32.minimum(best.0) {
                umin(best, (bound, m))
            } else {
                let (pd, pm) = platform_exact(lk.kind, secs, q);
                let ph = (e.size * pd, m + if pm == m_ground { part_top } else { part_side });
                if e.dying >= 0.0 {
                    umin(best, ph)
                } else {
                    let (fd, fm) = enemy_figure(prm, i, lk, secs, q);
                    umin(best, if fd < pd { (e.size * fd, fm) } else { ph })
                }
            };
            // defeated, the giblets
            if e.dying >= 0.0 {
                let qb = burst_space(fl, e, p);
                let (gc, gr) = giblets_bound(e.dying);
                let gb = length(qb - gc) - gr;
                if gb > 0.3f32.minimum(h.0) { umin(h, (gb, m)) } else { sd_giblets(prm, i, lk, e.r#gen, secs, e.dying, qb, h) }
            } else {
                h
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
        // (defeated, where the ray passes its giblets too)
        // (first the burst's wide bound, which most rays miss)
        let (n0, n1) = if shown(e) && e.dying >= 0.0 {
            let (gc, gr) = giblets_bound(e.dying);
            let (a, b) = ray_sphere(to_local_dir(fl, e.burst + yaw(e.yaw, gc) - fl.pos), gr + 0.3, ro, rd);
            let (a, b) = if a > b || b < 0.0 { (INF, -INF) } else { giblets_span(prm, e, i, fl, ro, rd) };
            if a > b { (n0, n1) } else { (n0.minimum(a), n1.maximum(b)) }
        } else {
            (n0, n1)
        };
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
