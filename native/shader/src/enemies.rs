// fut/enemies.fut: enemies: other wizards flying round him, each on a
// platform of its own (see platform.fut) with a magic circle of its own
// colour, and looks of its own, all hashed from the seed and its number.
// Where they are comes from the harness (see foes.fut: it moves them, as
// their AI has them), each turned to face him.
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
    // what defeated it (kill_*), and a wind blade's cut (see kill_of)
    pub kill: i32,
    pub cut_n: Vec3,
    pub cut_o: f32,
    // (a wind blade's) the way its pieces are carried, in the burst's space
    pub push: Vec3,
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
    let _ = (seed, secs);
    // (a defeated one falls, faster and faster, tumbling over, and shrinks
    // away at the end; a new one grows in)
    let fall = if dying >= 0.0 { 2.5 * dying * dying } else { 0.0 };
    let size = if dying >= 0.0 { smoothstep(fall_time, fall_time - 1.0, dying) } else { 1.0 - 0.95 * appear * appear };
    let pos = foe_pos(fs, i);
    // (him, defeated: where he stood, turned his way, not falling)
    let him = i == fs.him;
    let fall = if him { 0.0 } else { fall };
    let size = if him { 1.0 } else { size };
    let to_him = fl.pos - pos;
    let (kill, cut_n, cut_o, push) = kill_of(foe_kill(fs, i));
    Enemy {
        pos: pos - vec(0.0, fall, 0.0),
        burst: pos,
        kill,
        cut_n,
        cut_o,
        push,
        yaw: if him { fl.heading } else { to_him.x.atan2(to_him.z) },
        r#gen,
        appear,
        dying,
        size,
        tumble: if dying >= 0.0 && !him { 1.4 * dying + 0.5 * dying * dying } else { 0.0 },
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
// Remains. Defeated, an enemy's figure comes apart by what defeated it (its
// kill code, see kill_of): a fire bolt's blast bursts it into giblets, a
// wind blade cuts it in two on the blade's plane, a lightning strike
// throws it off its platform smouldering (see enemy_material); in each its
// hat pops off and its staff falls away on their own. Each piece is the
// figure (or a part of it) cut to its own region and moved rigidly: out of
// its rest (the figure's space, where it came from) by a translation along
// a parabola and a turn about an axis of its own; in its last half second
// it shrinks away. Where a cut, not the figure, is the surface, it is the
// flesh inside; the cloth by a cut is soaked. (Rigid motions, and cuts
// that are half-spaces, keep the distance a bound.)
//
// Pieces: the body's (a burst: giblets 0..7, the Voronoi cells of the body
// round fixed seeds; a cut: halves 0 and 1; a strike: 0, whole), then the
// hat (piece_hat) and the staff with its orb (piece_staff). Small bits of
// flesh fly with a burst's giblets, further and faster.

pub const kill_fire: i32 = 0;
pub const kill_strike: i32 = 1;
pub const kill_wind: i32 = 2;

pub const giblets: i32 = 8; // a burst's body pieces
pub const giblet_bits: i32 = 10;
pub const piece_hat: i32 = 8;
pub const piece_staff: i32 = 9;
pub const pieces: i32 = 10;
const gravity: f32 = 9.0;

// The figure's parts, as enemy_figure takes them.
pub const parts_body: i32 = 1;
pub const parts_hat: i32 = 2; // the hat and its band
pub const parts_staff: i32 = 4; // the staff and its orb
pub const parts_all: i32 = 7;

// A kill code (from the harness: see foes.rs): the kind (kill_*) and, for
// a wind blade's cut, its plane in the burst's space: a unit normal (up,
// or level) and its offset from the figure's middle (fig_middle), clamped
// so that it cuts the figure: code = kind + 4 (azimuth + 64 (elevation +
// 64 offset)), each six bits; and above them (times 2^20, four bits) the
// way the blade went, level, one of sixteen (its pieces carried that way:
// the last of what kill_of gives, in the burst's space).
pub const fig_middle: Vec3 = Vec3::new(0.0, 1.0, 0.0);
pub fn kill_of(code: f32) -> (i32, Vec3, f32, Vec3) {
    let c = ftoi(code.maximum(0.0) + 0.5);
    let k = imod(c, 4);
    let r = idiv(c, 4);
    let phi = imod(r, 64) as f32 / 63.0 * 2.0 * PI - PI;
    let th = imod(idiv(r, 64), 64) as f32 / 63.0 * 0.5 * PI;
    let o = imod(idiv(r, 4096), 64) as f32 / 63.0 * 0.7 - 0.35;
    let a = idiv(c, 1 << 20) as f32 / 16.0 * 2.0 * PI - PI;
    let push = if k == kill_wind { vec(a.sin(), 0.0, a.cos()) } else { splat(0.0) };
    (k, vec(th.cos() * phi.cos(), th.sin(), th.cos() * phi.sin()), o, push)
}

// Giblet k's seed (k < giblets), in the figure's space: the legs, the
// body, the arms and the head, one or two each.
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
    } else {
        vec(0.0, 1.5, 0.06)
    }
}

// Giblet k's cell at q (figure space): the distance outside it.
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

// Whether enemy e (dying) has piece k.
#[inline]
pub fn has_piece(e: Enemy, k: i32) -> bool {
    k >= piece_hat || (e.kill == kill_fire && k < giblets) || (e.kill == kill_wind && k < 2) || (e.kill == kill_strike && k == 0)
}

// Piece k of enemy i (generation g, dying e), tau seconds after: where it
// is (the burst's space: its pivot's place), the turn out of it into its
// rest (an axis, cos and sin), its size, its pivot (in its rest), and how
// far from the pivot it reaches.
pub fn piece_place(seed: i32, g: i32, i: i32, e: Enemy, k: i32, tau: f32) -> (Vec3, Vec3, f32, f32, f32, Vec3, f32) {
    let h = enemy_rnd(seed, g, i, 40 + k);
    let r = move |j: i32| fract(h * (17.0 + 23.0 * j as f32) + 0.31 * j as f32);
    let size = smoothstep(fall_time, fall_time - 0.5, tau);
    let fall = vec(0.0, -0.5 * gravity * tau * tau, 0.0);
    let (pivot, v, axis, angle, reach) = if k == piece_hat {
        // (popping off, up and back, tumbling)
        let a = 2.0 * PI * r(0);
        (vec(0.0, 1.7, 0.0), vec(0.8 * (r(1) - 0.5), 3.2, -0.9), normalise(vec(a.cos(), 0.2, a.sin())), (5.0 + 4.0 * r(2)) * tau, 0.9)
    } else if k == piece_staff {
        // (tipping away and falling)
        (vec(0.45, 1.0, 0.2), vec(1.1, 1.4, 0.3 * (r(0) - 0.5)), vec(0.1, 0.0, 1.0), -(2.5 + 1.5 * r(1)) * tau, 1.2)
    } else if e.kill == kill_wind {
        // the halves: sliding apart along the cut, tilting away from it,
        // then falling
        let s = if k == 0 { 1.0 } else { -1.0 };
        let n = e.cut_n;
        let tilt = normalise(cross(vec(0.0, 1.0, 0.0), n) + vec(0.0, 0.0, 1e-3));
        (fig_middle, scale(1.3 * s, n) + vec(0.0, 0.8, 0.0), tilt, s * (1.0 + 0.8 * r(0)) * tau, 1.35)
    } else if e.kill == kill_strike {
        // whole: thrown up and off its platform, toppling as it falls
        let a = 2.0 * PI * r(0);
        (vec(0.0, 0.1, 0.0), vec(1.2 * a.cos(), 2.4, 1.2 * a.sin()), normalise(vec(-a.sin(), 0.0, a.cos())), 0.8 * tau + 0.9 * tau * tau, 1.95)
    } else {
        // a giblet: out from the chest and up, spinning
        let out = giblet_seed(k) - vec(0.0, 1.0, 0.0);
        let flat = normalise(vec(out.x, 0.0, out.z) + vec(0.01 * (r(0) - 0.5), 0.0, 0.01));
        let a = 2.0 * PI * r(2);
        let v = scale(2.2 + 2.4 * r(0), flat) + vec(0.0, 2.5 + 2.5 * r(1) + 1.5 * out.y, 0.0);
        let spin = (4.0 + 8.0 * r(3)) * if r(2) < 0.5 { 1.0 } else { -1.0 };
        (giblet_seed(k), v, normalise(vec(a.cos(), 2.0 * r(3) - 1.0, a.sin())), spin * tau, 0.85)
    };
    // (a wind blade carries the pieces on, slowing)
    let carried = if e.kill == kill_wind { scale(3.2 * (1.0 - (-1.5 * tau).exp()), e.push) } else { splat(0.0) };
    let c = pivot + scale(tau, v) + fall + carried;
    let a = -angle;
    (c, axis, a.cos(), a.sin(), size, pivot, reach)
}

// A point of the burst's space in a piece's rest, given its place.
#[inline]
pub fn piece_rest(place: (Vec3, Vec3, f32, f32, f32, Vec3, f32), qb: Vec3) -> Vec3 {
    let (c, axis, ca, sa, size, pivot, _) = place;
    pivot + scale(1.0 / 0.02f32.maximum(size), rotate_about(axis, ca, sa, qb - c))
}

// Piece k at qr (its rest): (distance there, material, how far inside its
// cut (negative inside; near 0 by a cut; -1e9 where it has none)).
pub fn piece_at(prm: Params, i: i32, lk: Looks, e: Enemy, secs: f32, k: i32, qr: Vec3) -> (f32, i32, f32) {
    let parts = if k == piece_hat { parts_hat } else if k == piece_staff { parts_staff } else { parts_body };
    let (fd, fm) = enemy_figure(prm, i, lk, secs, qr, parts);
    let cut = if k >= piece_hat || e.kill == kill_strike {
        -1e9
    } else if e.kill == kill_wind {
        let w = dot(e.cut_n, qr - fig_middle) - e.cut_o;
        if k == 0 { -w } else { w }
    } else {
        giblet_cell(k, qr)
    };
    let flesh = m_enemy + enemy_parts * i + part_flesh;
    (fd.maximum(cut), if cut >= fd { flesh } else { fm }, cut)
}

// Bit j of a burst, tau seconds after: its centre (the burst's space) and
// radius.
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

// The remains of enemy i (looks lk, dying e) at qb in the burst's space:
// (distance, material), or `best` where none is nearer.
pub fn sd_remains(prm: Params, i: i32, lk: Looks, e: Enemy, secs: f32, qb: Vec3, best: Hit) -> Hit {
    let (seed, g, tau) = (prm.enemy_seed, e.r#gen, e.dying);
    let mut h = best;
    let mut k = 0;
    while k < pieces {
        if has_piece(e, k) {
            let place = piece_place(seed, g, i, e, k, tau);
            let (c, size, reach) = (place.0, place.4, place.6);
            if length(qb - c) - size * reach < h.0 {
                // (the figure as it was at the burst, its casting arm still)
                let (d, m, _) = piece_at(prm, i, lk, e, secs - tau, k, piece_rest(place, qb));
                if size * d < h.0 {
                    h = (size * d, m);
                }
            }
        }
        k += 1;
    }
    if e.kill == kill_fire {
        let flesh = m_enemy + enemy_parts * i + part_flesh;
        let mut j = 0;
        while j < giblet_bits {
            let (c, radius) = giblet_bit(seed, g, i, j, tau);
            let d = length(qb - c) - radius;
            if d < h.0 {
                h = (d, flesh);
            }
            j += 1;
        }
    }
    h
}

// Which piece of enemy i a point of the burst's space is on (the nearest),
// and where that is in its rest: (the point there, how far inside its cut,
// the piece's axis and turn, for turning normals into its rest).
pub fn remains_at(prm: Params, i: i32, lk: Looks, e: Enemy, secs: f32, qb: Vec3) -> (Vec3, f32, Vec3, f32, f32) {
    let (seed, g, tau) = (prm.enemy_seed, e.r#gen, e.dying);
    let mut best = (1e9f32, qb, -1e9f32, vec(0.0, 1.0, 0.0), 1f32, 0f32);
    let mut k = 0;
    while k < pieces {
        if has_piece(e, k) {
            let place = piece_place(seed, g, i, e, k, tau);
            let (c, axis, ca, sa, size, _, reach) = place;
            if length(qb - c) - size * reach < best.0 {
                let qr = piece_rest(place, qb);
                let (d, _, cut) = piece_at(prm, i, lk, e, secs - tau, k, qr);
                if size * d < best.0 {
                    best = (size * d, qr, cut, axis, ca, sa);
                }
            }
        }
        k += 1;
    }
    (best.1, best.2, best.3, best.4, best.5)
}

// The remains' bounding sphere tau seconds after the burst (the burst's
// space): every piece and bit flies within it.
#[inline]
pub fn giblets_bound(tau: f32) -> (Vec3, f32) {
    (vec(0.0, 1.0 + 3.5 * tau - 0.5 * gravity * tau * tau, 0.0), 2.2 + 5.5 * tau)
}

// The stretch of a ray (in his space) that passes near enemy e's remains:
// through any piece's or bit's sphere ((INF, -INF) if none).
pub fn giblets_span(prm: Params, e: Enemy, i: i32, fl: Flight, ro: Vec3, rd: Vec3) -> (f32, f32) {
    let (seed, g, tau) = (prm.enemy_seed, e.r#gen, e.dying);
    let at = move |c: Vec3| to_local_dir(fl, e.burst + yaw(e.yaw, c) - fl.pos);
    let (mut t0, mut t1) = (INF, -INF);
    let mut k = 0;
    while k < pieces {
        if has_piece(e, k) {
            let place = piece_place(seed, g, i, e, k, tau);
            let (a, b) = ray_sphere(at(place.0), place.4 * place.6 + 0.05, ro, rd);
            if a <= b && b >= 0.0 {
                t0 = t0.minimum(a);
                t1 = t1.maximum(b);
            }
        }
        k += 1;
    }
    if e.kill == kill_fire {
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
    }
    (t0, t1)
}

// ---------------------------------------------------------------------
// What the remains leave in the air: a blood mist where a figure bursts (a
// cloud of fine blood billowing out from its chest, thinning as it spreads
// and settles; a little where one is cut), and behind a burst's giblets,
// trails of smoke. Integrated along a ray (like the clouds, separately from
// the march): ro + t rd for t in [0, tend]: (radiance, transmittance).
pub fn blood_mist(prm: Params, fs: Foes, secs: f32, fl: Flight, ro: Vec3, rd: Vec3, tend: f32) -> (Vec3, f32) {
    let (mut rad, mut tr) = (splat(0.0), 1f32);
    // (and him, defeated: see him_dying)
    let n = prm.enemies.min(max_enemies) + if him_dying(fs) { 1 } else { 0 };
    let mut i = 0;
    while i < n {
        let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
        let tau = e.dying;
        if tau >= 0.0 && tau < fall_time && e.kill != kill_strike {
            let much = if e.kill == kill_fire { 1.0 } else { 0.35 };
            let radius = (0.35 + 2.6 * (1.0 - (-2.6 * tau).exp())) * (0.6 + 0.4 * much);
            let thick = much * 9.0 * (-1.8 * tau).exp() * smoothstep(0.0, 0.05, tau) * smoothstep(fall_time, fall_time - 0.8, tau);
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
            // smoke trailing each giblet of a burst: its path over the last
            // half second, a soft dark tube, thinner and wider the older
            // (only for rays through the remains' wide bound)
            let (gc, gr) = giblets_bound(tau);
            let (ga, gb) = ray_sphere(to_local_dir(fl, e.burst + yaw(e.yaw, gc) - fl.pos), gr + 0.5, ro, rd);
            if e.kill == kill_fire && ga <= gb && gb >= 0.0 {
                let (seed, g) = (prm.enemy_seed, e.r#gen);
                let at = move |q: Vec3| to_local_dir(fl, e.burst + yaw(e.yaw, q) - fl.pos);
                let mut k = 0;
                while k < giblets {
                    let mut s = 0;
                    while s < 2 {
                        let (ta, tb) = ((tau - 0.25 * (s + 1) as f32).maximum(0.0), (tau - 0.25 * s as f32).maximum(0.0));
                        if tb > ta {
                            let pa = at(piece_place(seed, g, i, e, k, ta).0);
                            let pb = at(piece_place(seed, g, i, e, k, tb).0);
                            let (dist, t, _) = crate::spells::ray_segment(ro, rd, tend, pa, pb);
                            if t < tend {
                                let width = 0.12 + 0.25 * (tau - tb) + 0.15 * s as f32;
                                let dens = (2.4 - 0.8 * s as f32) * (-1.0 * tau).exp() * (-(dist * dist) / (width * width)).exp();
                                // (the ray's way through the tube: about its width; grey,
                                // lit a little by the burst)
                                let depth = dens * 1.8 * width;
                                rad = rad + scale(tr * depth, vec(0.09, 0.08, 0.075));
                                tr = tr * (-depth).exp();
                            }
                        }
                        s += 1;
                    }
                    k += 1;
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
    // the spell it casts (as his: 0 fire, 1 lightning, 2 wind), which its
    // orb shows
    pub spell: i32,
}

// The looks of slot i: an enemy's, or, the slot after theirs, his (see
// him_dying).
pub fn looks_in(prm: Params, fs: Foes, g: i32, i: i32) -> Looks {
    if i == fs.him {
        Looks {
            kind: prm.platform,
            robe: prm.robe_colour,
            hat: prm.hat_colour,
            beard: prm.beard_tint,
            skin: prm.skin_colour,
            circle: prm.orb_glow_colour,
            carpet: prm.robe_colour,
            hem: prm.robe_hem - 0.06,
            hat_h: 0.8,
            droop: prm.hat_droop,
            beard_len: 0.4,
            phase: 0.0,
            spell: 1,
        }
    } else {
        looks_of(prm.enemy_seed, g, i)
    }
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
        // (as the client's enemies.rs picks it)
        spell: {
            let k = r(29);
            if k < 0.5 { 0 } else if k < 0.8 { 2 } else { 1 }
        },
    }
}

// The staff, the hand on it and the orb on its tip.
pub const enemy_grip: Vec3 = Vec3::new(0.45, 1.05, 0.18);
pub const enemy_orb: Vec3 = Vec3::new(0.45, 2.05, 0.2);

// The figure, in its own space: (distance, part).
// The parts in `parts` (parts_*).
pub fn sd_enemy_figure(lk: Looks, secs: f32, q: Vec3, parts: i32) -> (f32, i32) {
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
    let h = if parts & parts_body != 0 { umin((skin, part_skin), umin((beard, part_beard), (cloth, part_robe))) } else { nothing };
    let h = if parts & parts_hat != 0 { umin((band, part_band), umin((hat, part_hat), h)) } else { h };
    if parts & parts_staff != 0 { umin((orb, part_orb), umin((staff, part_staff), h)) } else { h }
}

// With full_enemies: the pose of an enemy with looks lk (his, at rest,
// breathing in its own time, in its colours and proportions).
pub fn enemy_pose(prm: Params, lk: Looks, secs: f32) -> Pose {
    let dr = Drive { spell: lk.spell as f32, ..on_path };
    let prm = Params {
        robe_colour: lk.robe,
        hat_colour: lk.hat,
        skin_colour: lk.skin,
        beard_tint: lk.beard,
        robe_hem: lk.hem + 0.06,
        hat_droop: lk.droop,
        ..prm
    };
    pose_at(secs + lk.phase, prm, dr)
}

// The figure of enemy i (looks lk), at q in its space, its parts in `parts`
// (parts_*): (distance, material).
pub fn enemy_figure(prm: Params, i: i32, lk: Looks, secs: f32, q: Vec3, parts: i32) -> Hit {
    let m = m_enemy + enemy_parts * i;
    if full_enemies {
        let ep = enemy_pose(prm, lk, secs);
        // (his figure's layers: the body and gear without the hat, the hat
        // alone, the staff)
        let body = parts & parts_body != 0;
        let hat = parts & parts_hat != 0;
        let mask = (if body { layers_figure | layers_gear } else { 0 })
            | (if hat { layers_figure } else { 0 })
            | (if parts & parts_staff != 0 { layers_staff } else { 0 })
            | (if body && !hat { no_hat } else { 0 })
            | (if hat && !body { only_hat } else { 0 });
        // (its orb is glass, as his, drawn apart: see enemy_orb_at)
        let (d, fm) = if mask & figure_layers != 0 { bounded_figure(mask, ep, 0.1, q) } else { nothing };
        let _ = m;
        (d, m_enemy_full + full_parts * i + hit_material(fm))
    } else {
        let (d, part) = sd_enemy_figure(lk, secs, q, parts);
        (d, m + part)
    }
}

// Enemy i's orb, in his space, while it stands: its centre and radius
// (where its pose's staff holds it, at its size), and its spell's colour,
// which its glass takes (see orbs_hit in shading.rs).
pub fn enemy_orb_at(prm: Params, fs: Foes, i: i32, secs: f32, fl: Flight) -> Option<(Vec3, f32, Vec3)> {
    let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
    if e.dying >= 0.0 || !shown(e) || i == fs.him {
        return None;
    }
    let lk = looks_in(prm, fs, e.r#gen, i);
    let dr = Drive { spell: lk.spell as f32, ..on_path };
    let s = secs + lk.phase;
    let q = staff_hold(prm, dr, flight_of(prm, dr, s)).at(orb_pos());
    let world = e.pos + yaw(e.yaw, scale(e.size, q));
    let tint = if lk.spell == 0 { vec(1.0, 0.42, 0.12) } else if lk.spell == 2 { vec(0.4, 0.88, 1.0) } else { vec(0.7, 0.78, 1.0) };
    Some((to_local_dir(fl, world - fl.pos), orb_radius * e.size, tint))
}

// (how much brighter the haze in their orbs is than in his)
pub const ENEMY_HAZE: f32 = 2.5;

// What fills enemy i's orb, at p in his space (inside it: the same flames,
// whirlwind or lightning as his, by its spell, and the haze), as his own
// orb's media has them; taken in the orb's own frame (round orb_pos, as
// his are), turned with the enemy and at its size.
pub fn enemy_orb_media(prm: Params, fs: Foes, i: i32, secs: f32, tm: f32, fl: Flight, centre: Vec3, p: Vec3) -> MediaSample {
    let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
    let lk = looks_in(prm, fs, e.r#gen, i);
    let q = orb_pos() + scale(1.0 / e.size.maximum(0.05), yaw_inv(e.yaw, to_world_dir(fl, p - centre)));
    let fx = if lk.spell == 0 {
        fire_media(secs, q)
    } else if lk.spell == 2 {
        wind_media(secs, q)
    } else {
        let l = lightning_media(tm, q);
        MediaSample { radiance: scale(prm.lightning, l.radiance), ..l }
    };
    let tint = if lk.spell == 0 { vec(1.0, 0.42, 0.12) } else if lk.spell == 2 { vec(0.4, 0.88, 1.0) } else { prm.orb_glow_colour };
    // (brighter than his haze: his orb is lit the more by its glow and light)
    MediaSample { radiance: fx.radiance + scale(ENEMY_HAZE * globe_haze, tint), ..fx }
}

// The enemies near his space's point p (tagged materials).
#[inline(never)]
pub fn enemies_dist(prm: Params, fs: Foes, secs: f32, fl: Flight, p: Vec3) -> Hit {
    let seed = prm.enemy_seed;
    let mut best = nothing;
    // (and him, defeated: only his remains, see him_dying)
    let n = prm.enemies.min(max_enemies) + if him_dying(fs) { 1 } else { 0 };
    let mut i = 0;
    while i < n {
        let e = enemy_at(seed, fs, i, secs, fl);
        best = if !(shown(e)) {
            best
        } else {
            let q = enemy_space(fl, e, p);
            let bound = e.size * (length(q - enemy_centre) - enemy_radius);
            let m = m_enemy + enemy_parts * i;
            let lk = looks_in(prm, fs, e.r#gen, i);
            // the platform, and alive, the figure on it
            let h = if i == fs.him {
                best
            } else if bound > 0.3f32.minimum(best.0) {
                umin(best, (bound, m))
            } else {
                let (pd, pm) = platform_exact(lk.kind, secs, q);
                let ph = (e.size * pd, m + if pm == m_ground { part_top } else { part_side });
                if e.dying >= 0.0 {
                    umin(best, ph)
                } else {
                    let (fd, fm) = enemy_figure(prm, i, lk, secs, q, parts_all);
                    umin(best, if fd < pd { (e.size * fd, fm) } else { ph })
                }
            };
            // defeated, its remains
            if e.dying >= 0.0 {
                let qb = burst_space(fl, e, p);
                let (gc, gr) = giblets_bound(e.dying);
                let gb = length(qb - gc) - gr;
                if gb > 0.3f32.minimum(h.0) { umin(h, (gb, m)) } else { sd_remains(prm, i, lk, e, secs, qb, h) }
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
    let n = prm.enemies.min(max_enemies) + if him_dying(fs) { 1 } else { 0 };
    let mut i = 0;
    while i < n {
        let e = enemy_at(prm.enemy_seed, fs, i, secs, fl);
        // (tumbling moves the centre round the axis: a wider sphere covers it;
        // him: only his remains)
        let c = to_local_dir(fl, e.pos + tumble_axis - fl.pos);
        let (a, b) = ray_sphere(c, e.size * enemy_radius + 0.6, ro, rd);
        let (n0, n1) = if !(shown(e)) || a > b || b < 0.0 || i == fs.him { (t0, t1) } else { (t0.minimum(a), t1.maximum(b)) };
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
