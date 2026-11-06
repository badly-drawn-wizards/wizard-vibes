// The fight with the enemy wizards (web/enemies.ts; drawn by
// fut/enemies.fut). Here: where each is (it steers for its spot circling
// the arena, keeps off him, drifts round him, keeps apart from the others,
// and dodges his bolts and blades: the renderer is told where), their
// health, their defeat and replacement, their casting at him (each its own
// spell, telegraphed as his are), and his health.
//
// Each enemy's state goes to the renderer as one float (see foes in
// fut/enemies.fut): 16 times its generation (how many have had its place
// before it: each new one has new looks and a new place) plus its phase,
// under 8 the seconds it has still to appear, 8 and over 8 plus the seconds
// since its defeat.

use super::spells::{Cut, Spells, Target, BURST, CHARGE, FIRE, FROST, HIS, MARK, MISSILE, STRIKE, WARMUP, WELL_REACH, WIND, WINDUP};
use super::vec::{add, dist, dot, len, normalise, scale, sub, turn, V3};
use super::Rng;
use std::f32::consts::PI;

// The enemies (at most), and the wizards' slots the renderer is sent: theirs,
// then one more for him (defeated, he comes apart there as they do): as
// enemy_slots and wizard_slots in the shader's materials.rs.
pub const ENEMY_SLOTS: usize = 6;
pub const WIZARD_SLOTS: usize = ENEMY_SLOTS + 1;
const HP: i32 = 3;
const APPEAR: f32 = 1.5; // seconds (appear_time in fut/enemies.fut)
const RESPAWN: f32 = 5.0; // seconds after a defeat until the next one appears
const RANGE: f32 = 40.0; // they only cast at him from nearer than this
// The arena, the point they circle, stays where the fight began unless he
// goes further than this from it; then it follows him, slower than he
// flies, and never lower than this.
const ARENA_LEASH: f32 = 35.0;
const ARENA_SPEED: f32 = 6.0;
const ARENA_FLOOR: f32 = 12.0;
pub const PLAYER_HP: i32 = 10;

// Damage, by kind of spell. (A fire bolt does its damage by its burst,
// which a bolt that hits sets off.)
// A kill code for the renderer (kill_of in the shader's enemies.rs): the
// kind of death (0 a fire bolt's burst, 1 a strike, 2 a wind blade), and a
// blade's cut: its plane (a point and normal in the world) in the enemy's
// space (it at `pos`, turned by `yaw`), the normal up, moved to cut through
// the figure if it would miss (within 0.35 of its middle), each of
// azimuth, elevation and offset in six bits; and above them, in four, the
// way the blade went (level, one of sixteen: the halves are carried that
// way).
pub fn kill_code(kind: u32, cut: Option<Cut>, pos: V3, yaw: f32) -> f32 {
    let k = match kind {
        STRIKE => 1,
        WIND => 2,
        _ => 0,
    };
    let Some((p, n, v)) = cut.filter(|_| k == 2) else { return k as f32 };
    // (the way the blade went, level, in its space: one of sixteen)
    let vb = turn(-yaw, v);
    let push = ((vb[0].atan2(vb[2]) + PI) / (2.0 * PI) * 16.0).round() as u32 % 16;
    let n = normalise(turn(-yaw, n));
    let n = if n[1] < 0.0 { scale(-1.0, n) } else { n };
    let pb = turn(-yaw, sub(p, pos));
    let off = super::vec::dot(n, sub(pb, [0.0, 1.0, 0.0])).clamp(-0.35, 0.35);
    let phi = n[2].atan2(n[0]);
    let th = n[1].clamp(0.0, 1.0).asin();
    let a = ((phi + PI) / (2.0 * PI) * 63.0).round() as u32 % 64;
    let e = (th / (0.5 * PI) * 63.0).round() as u32;
    let o = ((off + 0.35) / 0.7 * 63.0).round() as u32;
    (k + 4 * (a + 64 * (e + 64 * o)) + (push << 20)) as f32
}

fn damage(kind: u32) -> i32 {
    match kind {
        BURST | WIND | FROST | MISSILE => 1,
        STRIKE => 2,
        _ => 0,
    }
}

// JavaScript's ToInt32 (as Math.imul takes its arguments), as u32 bits.
fn bits(x: i64) -> u32 {
    x as u32
}

// hash3 in fut/hash.fut.
pub fn hash3(x: i64, y: i64, z: i64) -> f32 {
    let mut h = bits(x).wrapping_mul(73856093) ^ bits(y).wrapping_mul(19349663) ^ bits(z).wrapping_mul(83492791);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16777216.0
}

// enemy_rnd in fut/enemies.fut.
pub fn rnd(seed: i64, generation: i64, i: usize, k: i64) -> f32 {
    hash3(seed + 7919 * generation, i as i64 + 17, k + 101)
}

// Where enemy i (of generation generation) would like to be: circling
// `arena` at its own distance, height and speed (it steers there, unless
// keeping away from him or dodging: see Enemies::update).
pub fn home(seed: i64, generation: i64, i: usize, secs: f32, arena: V3) -> V3 {
    let r = |k| rnd(seed, generation, i, k);
    let radius = 9.0 + 12.0 * r(0);
    let speed = (0.04 + 0.06 * r(1)) * if r(2) < 0.5 { -1.0 } else { 1.0 };
    let a = 2.0 * PI * (i as f32 / ENEMY_SLOTS as f32 + 0.12 * r(3)) + speed * secs;
    let h = -1.5 + 4.0 * r(4) + 0.5 * (0.6 * secs + 6.3 * r(5)).sin();
    add(arena, [radius * a.sin(), h, radius * a.cos()])
}

// Which spell enemy i (of generation generation) casts, always (its orb
// shows it: the renderer picks it the same way, see looks_of).
pub fn spell_of(seed: i64, generation: i64, i: usize) -> u32 {
    let r = rnd(seed, generation, i, 29);
    if r < 0.5 { FIRE } else if r < 0.8 { WIND } else { STRIKE }
}

// Which way one at `pos` faces him (a yaw), as the renderer turns it.
pub fn facing(pos: V3, him: V3) -> f32 {
    (him[0] - pos[0]).atan2(him[2] - pos[2])
}

// Its orb, where it casts from.
fn orb_of(pos: V3, him: V3) -> V3 {
    add(pos, turn(facing(pos, him), [0.45, 2.05, 0.2]))
}

// His shield, raised (shield_centre and shield_radius in fut/shading.fut).
const SHIELD_C: V3 = [0.0, 0.85, 0.0];
const SHIELD_R: f32 = 1.85;

// The spheres that stand for a wizard on his platform at `pos` (the
// platform's top): his body, and the platform.
pub fn spheres(pos: V3, side: i32) -> [Target; 2] {
    [Target { c: add(pos, [0.0, 1.1, 0.0]), r: 0.9, side }, Target { c: add(pos, [0.0, -0.8, 0.0]), r: 1.4, side }]
}

// What happened, for the sound and the shield.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnemyEvent {
    Block,
    Hurt,
    // (and whether he defeated it)
    Defeat(V3, bool),
    Appear(V3),
    Cast(u32, V3),
}

// What an enemy is doing about its spell: biding its time; charging a fire
// bolt (for so long), or warming up its wind blades (each shown at its
// orb: `telegraph`); or loosing a run of wind blades.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Idle,
    Charging { secs: f32 },
    Warming,
    Blades { left: u32, next: f32, shot: u32 },
}

// How an enemy moves: as fast as this, turning its velocity towards what it
// wants this quickly (a second), no nearer him than NEAR, nor nearer
// another than APART; it sees his bolts and blades coming this far ahead
// (seconds), and dodges those that would pass within DODGE_BY.
const TOP_SPEED: f32 = 7.0;
const STEER: f32 = 2.5;
const NEAR: f32 = 8.0;
const APART: f32 = 4.5;
const LOOKAHEAD: f32 = 0.9;
const DODGE_BY: f32 = 2.4;
// (its fire bolts' charge, at most; its wind blades' warm-up, and how many
// in a run, and how far apart, spun down and up)
const ENEMY_POWER: f32 = 0.5;
// Aggro: how much each holds against him, and against each other, at
// rest; what a hit adds against whoever cast it; and how fast it settles
// back (a time constant, seconds). Free for all, it picks whom to cast at
// by it (nearer counting for more).
const AGGRO_HIM: f32 = 3.0;
const AGGRO_OTHER: f32 = 1.0;
const AGGRO_HIT: f32 = 2.0;
const AGGRO_SETTLE: f32 = 6.0;
// (a frost lance's slow: how long, and to what share of its speed and its
// casting's pace)
const FROST_SLOW: f32 = 2.5;
const SLOWED: f32 = 0.35;
const BLADES: u32 = 5;
const SLOW_BLADE: f32 = 0.3;
const FAST_BLADE: f32 = 0.14;

#[derive(Clone, Debug)]
struct Slot {
    generation: i64,
    hp: i32,
    kill: f32, // what defeated it (a kill code: see kill_code)
    appear: f32, // seconds still to appear
    down: f32,   // seconds since its defeat, or -1
    cast_in: f32, // seconds to its next spell
    pos: V3,     // where it is (its platform's top; defeated, where it was)
    vel: V3,
    act: Act,
    act_t: f32, // seconds into it
    telegraph: Option<u32>,
    dodge: f32, // seconds of dodging left
    dodge_dir: V3,
    strafe: f32, // which way it drifts round him (-1 or 1), and for how long more
    strafe_t: f32,
    target: i32, // whom it casts at: him (HIS), or free for all another
    chosen: bool, // (free for all) its next target chosen
    slow: f32,    // seconds still slowed (a frost lance): it moves, and casts, the slower
    aggro: [f32; ENEMY_SLOTS + 1], // against each other one, and (last) him
}

impl Slot {
    fn new(generation: i64, appear: f32, cast_in: f32, pos: V3) -> Slot {
        Slot {
            generation,
            hp: HP,
            kill: 0.0,
            appear,
            down: -1.0,
            cast_in,
            pos,
            vel: [0.0; 3],
            act: Act::Idle,
            act_t: 0.0,
            telegraph: None,
            dodge: 0.0,
            dodge_dir: [0.0; 3],
            strafe: 1.0,
            strafe_t: 0.0,
            target: HIS,
            chosen: false,
            slow: 0.0,
            aggro: aggro_at_rest(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Enemies {
    pub hp: i32,
    pub defeated: u32, // enemies he has defeated
    pub hurt: f32,     // seconds since he was last hit
    pub down: f32,     // seconds since he was defeated (until a reset), or -1
    pub shielded: bool, // his shield is up: it takes what would hit him
    pub arena: V3,
    pub events: Vec<EnemyEvent>,
    slots: Vec<Slot>,
    seen: (i64, f32, V3), // seed, secs, him as of the last targets()
    last: Option<V3>,     // where he was last frame
    vel: V3,              // how fast he is going, smoothed
    placed: bool,         // the slots have been put where they belong
    pub ffa: bool,        // free for all: they fight each other too
    // what defeated him last (its kind, and a wind blade's cut), and how
    // many times he has been (each comes apart its own way)
    pub his_end: Option<(u32, Option<Cut>)>,
    pub his_deaths: i64,
    pub max_hp: i32, // his health, full (his skills raise it)
}

impl Default for Enemies {
    fn default() -> Self {
        Enemies {
            hp: PLAYER_HP,
            defeated: 0,
            hurt: 99.0,
            down: -1.0,
            shielded: false,
            arena: [0.0, ARENA_FLOOR, 12.0],
            events: Vec::new(),
            slots: (0..ENEMY_SLOTS).map(|i| Slot::new(0, 0.0, 5.0 + i as f32 * 2.0, [0.0; 3])).collect(),
            seen: (0, 0.0, [0.0; 3]),
            last: None,
            vel: [0.0; 3],
            placed: false,
            ffa: false,
            his_end: None,
            his_deaths: 0,
            max_hp: PLAYER_HP,
        }
    }
}

impl Enemies {
    // The renderer's states (foe0 .. foe6 of the drive record; his slot left for the game to fill).
    pub fn foes(&self) -> [f32; WIZARD_SLOTS] {
        let mut f = [0.0; WIZARD_SLOTS];
        for (i, s) in self.slots.iter().enumerate() {
            let phase = if s.down >= 0.0 { 8.0 + s.down.min(7.9) } else { s.appear.min(7.9) };
            f[i] = 16.0 * s.generation as f32 + phase;
        }
        f
    }

    // What defeated each (kill0 .. kill6 of the drive record).
    pub fn kills(&self) -> [f32; WIZARD_SLOTS] {
        let mut f = [0.0; WIZARD_SLOTS];
        for (i, s) in self.slots.iter().enumerate() {
            f[i] = s.kill;
        }
        f
    }

    // Where each stands (fx0 .. fz6 of the drive record).
    pub fn positions(&self) -> [V3; WIZARD_SLOTS] {
        let mut f = [[0.0; 3]; WIZARD_SLOTS];
        for (i, s) in self.slots.iter().enumerate() {
            f[i] = s.pos;
        }
        f
    }

    // How fast enemy `side` is going (for aiming at it), if it stands.
    pub fn velocity(&self, side: i32) -> Option<V3> {
        self.slots.get(side as usize).filter(|s| s.down < 0.0).map(|s| s.vel)
    }

    // The spheres spells can hit: his (or his shield), and the first n
    // enemies' that stand.
    pub fn targets(&mut self, n: usize, seed: i64, secs: f32, him: V3) -> Vec<Target> {
        let mut out = Vec::new();
        if self.down < 0.0 {
            if self.shielded {
                out.push(Target { c: add(him, SHIELD_C), r: SHIELD_R, side: HIS });
            } else {
                out.extend(spheres(him, HIS));
            }
        }
        self.seen = (seed, secs, him);
        for (i, s) in self.slots.iter().take(n).enumerate() {
            if s.down < 0.0 && s.appear < 0.5 {
                out.extend(spheres(s.pos, i as i32));
            }
        }
        out
    }

    // The fight from the start, round `arena`: him at full health and up,
    // none defeated, and a new set of enemies appearing.
    pub fn reset(&mut self, arena: V3) {
        self.arena = [arena[0], arena[1].max(ARENA_FLOOR), arena[2]];
        self.hp = self.max_hp;
        self.down = -1.0;
        self.defeated = 0;
        self.hurt = 99.0;
        let (seed, secs, _) = self.seen;
        for (i, s) in self.slots.iter_mut().enumerate() {
            let generation = s.generation + 1;
            *s = Slot::new(generation, APPEAR, 5.0 + i as f32 * 2.0, home(seed, generation, i, secs, self.arena));
        }
        self.placed = true;
    }

    // A spell of `kind` hit `side`.
    pub fn hit(&mut self, side: i32, kind: u32) {
        self.hit_with(side, kind, None, 1.0, HIS);
    }

    // The same, a wind blade's cut with it (a point and normal, world), its
    // damage scaled by `scale` (rounded; at least 1, if it does any), cast
    // by `owner` (only his defeats count).
    pub fn hit_with(&mut self, side: i32, kind: u32, cut: Option<Cut>, scale: f32, owner: i32) {
        let damage = match damage(kind) {
            0 => 0,
            d => ((d as f32 * scale).round() as i32).max(1),
        };
        if side == HIS {
            if self.down >= 0.0 {
                return;
            }
            if self.shielded {
                self.events.push(EnemyEvent::Block);
                return;
            }
            if damage == 0 {
                return;
            }
            self.hp = (self.hp - damage).max(0);
            self.hurt = 0.0;
            self.events.push(EnemyEvent::Hurt);
            if self.hp == 0 {
                self.down = 0.0;
                self.his_end = Some((kind, cut));
                self.his_deaths += 1;
            }
        } else {
            let him = self.seen.2;
            let Some(s) = self.slots.get_mut(side as usize) else { return };
            if s.down >= 0.0 {
                return;
            }
            s.hp -= damage;
            if kind == FROST {
                s.slow = FROST_SLOW;
            }
            if s.hp > 0 {
                s.aggro[if owner == HIS { ENEMY_SLOTS } else { (owner as usize).min(ENEMY_SLOTS) }] += AGGRO_HIT;
            }
            if s.hp <= 0 {
                s.down = 0.0;
                // (the one who hit it is held against, the more)
                s.aggro[if owner == HIS { ENEMY_SLOTS } else { (owner as usize).min(ENEMY_SLOTS) }] += AGGRO_HIT;
                s.vel = [0.0; 3];
                let his = owner == HIS;
                if his {
                    self.defeated += 1;
                }
                s.kill = kill_code(kind, cut, s.pos, facing(s.pos, him));
                self.events.push(EnemyEvent::Defeat(s.pos, his));
            }
        }
    }

    // Advances the fight by dt with n enemies, he being at `him`; they move
    // and cast with `spells` (seeing his spells there to dodge).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, n: usize, seed: i64, secs: f32, him: V3, spells: &mut Spells, rng: &mut Rng) {
        if let Some(last) = self.last
            && dt > 0.0
        {
            let k = 1.0 - (-dt * 4.0).exp();
            for j in 0..3 {
                let v = (him[j] - last[j]) / dt;
                self.vel[j] += (v - self.vel[j]) * k;
            }
        }
        self.last = Some(him);
        self.hurt += dt;
        if self.down >= 0.0 {
            self.down += dt;
        }
        // (the arena follows him once he is far from it)
        let away = sub(him, self.arena);
        let flat = away[0].hypot(away[2]);
        let far = flat - ARENA_LEASH;
        if far > 0.0 {
            let k = far.min(ARENA_SPEED * dt) / if flat == 0.0 { 1.0 } else { flat };
            self.arena = add(self.arena, scale(k, [away[0], 0.0, away[2]]));
        }
        self.arena[1] += (him[1].max(ARENA_FLOOR) - self.arena[1]) * (1.0 - (-dt / 4.0).exp());
        if !self.placed {
            for (i, s) in self.slots.iter_mut().enumerate() {
                s.pos = home(seed, s.generation, i, secs, self.arena);
            }
            self.placed = true;
        }
        let others: Vec<(bool, V3, V3)> = self.slots.iter().take(n).map(|s| (s.down < 0.0 && s.appear <= 0.0, s.pos, s.vel)).collect();
        let (ffa, his_vel, he_stands) = (self.ffa, self.vel, self.down < 0.0);
        for i in 0..ENEMY_SLOTS {
            let arena = self.arena;
            let missiles = spells.missiles_at(i as i32);
        let wells = spells.wells();
            let s = &mut self.slots[i];
            if s.down >= 0.0 {
                // (a telegraph cut short)
                if let Some(id) = s.telegraph.take() {
                    spells.end(id);
                }
                s.down += dt;
                if s.down >= RESPAWN {
                    let generation = s.generation + 1;
                    let pos = home(seed, generation, i, secs, arena);
                    *s = Slot::new(generation, APPEAR, 4.0 + 4.0 * rng.uniform(), pos);
                    if i < n {
                        self.events.push(EnemyEvent::Appear(pos));
                    }
                }
                continue;
            }
            s.appear = (s.appear - dt).max(0.0);
            if i >= n || dt == 0.0 {
                continue;
            }
            // (aggro settling back)
            let k = 1.0 - (-dt / AGGRO_SETTLE).exp();
            let rest = aggro_at_rest();
            for (a, r) in s.aggro.iter_mut().zip(rest) {
                *a += (r - *a) * k;
            }
            // it moves (not while still appearing)
            if s.appear <= 0.0 {
                let r = |k| rnd(seed, s.generation, i, k);
                let skill = 0.35 + 0.6 * r(31);
                // (its spot circling the arena; then kept off him, sliding
                // round him as it goes)
                let want_at = home(seed, s.generation, i, secs, arena);
                let mut want = sub(want_at, s.pos);
                let off = sub(s.pos, him);
                let d = len(off).max(1e-3);
                if d < NEAR {
                    want = add(want, scale(2.0 * (NEAR - d) / d, off));
                }
                s.strafe_t -= dt;
                if s.strafe_t <= 0.0 {
                    s.strafe = if rng.uniform() < 0.5 { -1.0 } else { 1.0 };
                    s.strafe_t = 1.5 + 3.0 * rng.uniform();
                }
                let round = normalise([off[2], 0.0, -off[0]]);
                want = add(want, scale(1.5 * s.strafe * skill, round));
                // (apart from the others)
                for (j, &(up, p, _)) in others.iter().enumerate() {
                    let v = sub(s.pos, p);
                    let dd = len(v);
                    if j != i && up && dd < APART && dd > 1e-3 {
                        want = add(want, scale(3.0 * (APART - dd) / dd, v));
                    }
                }
                // (his bolts and blades coming its way: out of their path,
                // if it notices, the more skilled the sooner)
                if s.dodge <= 0.0 {
                    let chest = add(s.pos, [0.0, 1.1, 0.0]);
                    for &(p, v) in &missiles {
                        let rel = sub(chest, p);
                        let vv = dot(v, v).max(1e-6);
                        let t = dot(rel, v) / vv;
                        if t <= 0.0 || t > LOOKAHEAD * skill + 0.2 {
                            continue;
                        }
                        let miss = sub(rel, scale(t, v));
                        if len(miss) < DODGE_BY && rng.uniform() < skill * dt * 8.0 {
                            let side = if len(miss) > 1e-3 { normalise(miss) } else { normalise([v[2], 0.0, -v[0]]) };
                            s.dodge = 0.45;
                            s.dodge_dir = scale(TOP_SPEED * 1.4, side);
                        }
                    }
                }
                let target_vel = if s.dodge > 0.0 {
                    s.dodge -= dt;
                    s.dodge_dir
                } else {
                    let l = len(want);
                    if l > TOP_SPEED { scale(TOP_SPEED / l, want) } else { want }
                };
                // (slowed by frost; drawn into his gravity wells, the more
                // the nearer, whatever it wants)
                s.slow = (s.slow - dt).max(0.0);
                let mut target_vel = if s.slow > 0.0 { scale(SLOWED, target_vel) } else { target_vel };
                for w in &wells {
                    let to = sub(*w, s.pos);
                    let d = len(to);
                    if d < WELL_REACH && d > 0.3 {
                        let k = 1.0 - d / WELL_REACH;
                        target_vel = add(scale(1.0 - k, target_vel), scale(k * 12.0 / d, to));
                    }
                }
                let k = 1.0 - (-dt * STEER * if s.dodge > 0.0 { 3.0 } else { 1.0 }).exp();
                for j in 0..3 {
                    s.vel[j] += (target_vel[j] - s.vel[j]) * k;
                }
                s.pos = add(s.pos, scale(dt, s.vel));
                s.pos[1] = s.pos[1].clamp(arena[1] - 4.0, arena[1] + 6.0);
            }
            if s.appear > 0.0 || (self.down >= 0.0 && !ffa) {
                continue;
            }
            // it casts: its own spell, telegraphed as his are, at him (or,
            // free for all, at whoever it chose)
            let orb = orb_of(s.pos, him);
            if let Some(id) = s.telegraph {
                spells.place(id, orb);
            }
            // (its target gone: him, if he stands; else no one)
            if s.target >= 0 && !others.get(s.target as usize).is_some_and(|o| o.0) {
                s.target = HIS;
            }
            if s.target == HIS && !he_stands {
                s.act = Act::Idle;
                if let Some(id) = s.telegraph.take() {
                    spells.end(id);
                }
                continue;
            }
            let (at, at_vel) = if s.target == HIS { (him, his_vel) } else { (others[s.target as usize].1, others[s.target as usize].2) };
            let chest = add(at, [0.0, 1.1, 0.0]);
            let in_range = dist(chest, orb) <= RANGE;
            let kind = spell_of(seed, s.generation, i);
            // (slowed, it casts the slower too)
            let dt = if s.slow > 0.0 { SLOWED * dt } else { dt };
            s.act_t += dt;
            match s.act {
                Act::Idle => {
                    s.cast_in -= dt;
                    if s.cast_in > 0.0 {
                        continue;
                    }
                    // (free for all: whoever is nearest, give or take)
                    if ffa && !s.chosen {
                        s.chosen = true;
                        // (by aggro, nearer counting for more, give or take)
                        let score = |a: f32, p: V3, rng: &mut Rng| a * (0.7 + 0.6 * rng.uniform()) / (1.0 + dist(p, s.pos) / 15.0);
                        let mut best = (if he_stands { score(s.aggro[ENEMY_SLOTS], him, rng) } else { -1.0 }, HIS);
                        for (j, &(up, p, _)) in others.iter().enumerate() {
                            let sc = score(s.aggro[j], p, rng);
                            if j != i && up && sc > best.0 {
                                best = (sc, j as i32);
                            }
                        }
                        s.target = best.1;
                        continue;
                    }
                    if !in_range {
                        continue;
                    }
                    s.chosen = false;
                    s.act_t = 0.0;
                    if kind == FIRE {
                        let secs = 0.6 + 0.8 * rng.uniform();
                        s.telegraph = spells.telegraph(CHARGE, orb, secs + 0.1, i as i32, rng);
                        s.act = Act::Charging { secs };
                    } else if kind == WIND {
                        s.telegraph = spells.telegraph(WINDUP, orb, WARMUP + 0.1, i as i32, rng);
                        s.act = Act::Warming;
                    } else {
                        // (a strike is marked first: its own telegraph)
                        s.cast_in = 6.0 + 6.0 * rng.uniform() + n as f32;
                        let lead = 1.1;
                        let (mx, mz) = (miss(rng), miss(rng));
                        let at = add(add(at, scale(lead, at_vel)), [mx, 0.6, mz]);
                        if spells.launch(MARK, orb, [0.0, 1.0, 0.0], i as i32, Some(at), rng) {
                            self.events.push(EnemyEvent::Cast(MARK, at));
                        }
                    }
                }
                Act::Charging { secs: c } if s.act_t >= c => {
                    if let Some(id) = s.telegraph.take() {
                        spells.end(id);
                    }
                    let power = ENEMY_POWER * (c / 1.4).min(1.0);
                    let speed = 30.0 * (1.0 + 0.6 * power);
                    let aim = aim_at(chest, orb, at_vel, speed, rng);
                    if spells.launch(FIRE, orb, aim, i as i32, None, rng) {
                        spells.empower_last(power);
                        self.events.push(EnemyEvent::Cast(FIRE, orb));
                    }
                    s.act = Act::Idle;
                    s.cast_in = 5.0 + 5.0 * rng.uniform() + n as f32;
                }
                Act::Warming if s.act_t >= WARMUP => {
                    if let Some(id) = s.telegraph.take() {
                        spells.end(id);
                    }
                    s.act = Act::Blades { left: BLADES, next: 0.0, shot: 0 };
                }
                Act::Blades { left, next, shot } => {
                    let next = next - dt;
                    if next > 0.0 {
                        s.act = Act::Blades { left, next, shot };
                        continue;
                    }
                    let aim = aim_at(chest, orb, at_vel, 24.0, rng);
                    if spells.launch(WIND, orb, aim, i as i32, None, rng) {
                        self.events.push(EnemyEvent::Cast(WIND, orb));
                    }
                    let spun = (shot as f32 / (BLADES - 1) as f32).min(1.0);
                    s.act = if left > 1 {
                        Act::Blades { left: left - 1, next: SLOW_BLADE + (FAST_BLADE - SLOW_BLADE) * spun, shot: shot + 1 }
                    } else {
                        s.cast_in = 6.0 + 6.0 * rng.uniform() + n as f32;
                        Act::Idle
                    };
                }
                _ => {}
            }
        }
    }
}

// Aggro at rest: AGGRO_OTHER against each other one, AGGRO_HIM against him.
fn aggro_at_rest() -> [f32; ENEMY_SLOTS + 1] {
    let mut a = [AGGRO_OTHER; ENEMY_SLOTS + 1];
    a[ENEMY_SLOTS] = AGGRO_HIM;
    a
}

// (off by up to 2.5 either way, mostly less)
fn miss(rng: &mut Rng) -> f32 {
    (rng.uniform() + rng.uniform() - 1.0) * 2.5
}

// The way to cast from `orb` at `chest` (moving at `vel`) so a spell going
// `speed` meets it, give or take.
fn aim_at(chest: V3, orb: V3, vel: V3, speed: f32, rng: &mut Rng) -> V3 {
    let (mx, my, mz) = (miss(rng), miss(rng), miss(rng));
    let aim = add(add(chest, scale(dist(chest, orb) / speed, vel)), [mx, my / 2.0, mz]);
    normalise(sub(aim, orb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_js() {
        // the TS's hash3, run in node
        for ((x, y, z), v) in [((1, 17, 101), 0.4066948890686035), ((-5, 3, 9), 0.4896174669265747), ((7919 * 40 + 3, 22, 130), 0.46717369556427)] {
            assert_eq!(hash3(x, y, z), v as f32);
        }
    }

    #[test]
    fn home_circles_the_arena() {
        let arena = [1.0, 12.0, 12.0];
        let (seed, generation, i, secs) = (1, 2, 3, 7.5);
        let r = |k| rnd(seed, generation, i, k);
        let pos = home(seed, generation, i, secs, arena);
        let radius = 9.0 + 12.0 * r(0);
        let flat = (pos[0] - arena[0]).hypot(pos[2] - arena[2]);
        assert!((flat - radius).abs() < 1e-3);
        let h = pos[1] - arena[1];
        assert!(h >= -2.0 && h <= 3.0);
    }

    #[test]
    fn they_dodge_what_comes_at_them() {
        // one enemy, a bolt flying straight at it: it moves out of the way
        let mut e = Enemies::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(5);
        let him = [0.0, 12.0, 0.0];
        e.reset([0.0, 12.0, 12.0]);
        e.update(2.0, 1, 1, 0.0, him, &mut spells, &mut rng);
        let start = e.slots[0].pos;
        let chest = add(start, [0.0, 1.1, 0.0]);
        let from = add(chest, scale(-12.0, normalise(sub(chest, him))));
        spells.launch(FIRE, from, normalise(sub(chest, from)), HIS, None, &mut rng);
        let mut dodged = false;
        for _ in 0..20 {
            e.update(1.0 / 60.0, 1, 1, 0.0, him, &mut spells, &mut rng);
            spells.update(1.0 / 60.0, false);
            dodged |= e.slots[0].dodge > 0.0;
        }
        assert!(dodged);
        let missed = len(sub(e.slots[0].pos, start));
        assert!(missed > 0.5, "{missed}");
    }

    #[test]
    fn foes_and_defeat() {
        let mut e = Enemies::default();
        e.reset([0.0, 0.0, 0.0]);
        assert_eq!(e.arena[1], ARENA_FLOOR);
        assert_eq!(e.foes()[0], 16.0 + 1.5);
        let mut spells = Spells::default();
        let mut rng = Rng::new(5);
        e.update(2.0, 3, 1, 0.0, [0.0, 12.0, 0.0], &mut spells, &mut rng);
        assert_eq!(e.foes()[1], 16.0);
        let t = e.targets(3, 1, 0.0, [0.0, 12.0, 0.0]);
        assert_eq!(t.len(), 2 + 3 * 2);
        for _ in 0..3 {
            e.hit(1, WIND);
        }
        assert_eq!(e.defeated, 1);
        assert!(matches!(e.events.last(), Some(EnemyEvent::Defeat(_, true))));
        assert_eq!(e.foes()[1], 16.0 + 8.0);
        // it comes back, a generation on
        e.update(5.0, 3, 1, 0.0, [0.0, 12.0, 0.0], &mut spells, &mut rng);
        assert_eq!(e.foes()[1], 32.0 + 1.5);
        assert!(matches!(e.events.last(), Some(EnemyEvent::Appear(_))));
    }

    #[test]
    fn their_defeats_of_each_other_are_not_his() {
        let mut e = Enemies::default();
        e.reset([0.0; 3]);
        e.slots[0].appear = 0.0;
        e.hit_with(0, BURST, None, 1.0, 1);
        // (a hit held against the one who cast it)
        assert_eq!(e.slots[0].aggro[1], AGGRO_OTHER + AGGRO_HIT);
        e.hit_with(0, STRIKE, None, 2.0, 1);
        assert_eq!(e.defeated, 0);
        assert!(matches!(e.events.last(), Some(EnemyEvent::Defeat(_, false))));
    }

    #[test]
    fn his_health() {
        let mut e = Enemies::default();
        e.shielded = true;
        e.hit(HIS, STRIKE);
        assert_eq!(e.hp, PLAYER_HP);
        assert_eq!(e.events, vec![EnemyEvent::Block]);
        e.shielded = false;
        e.hit(HIS, FIRE); // no damage of its own
        assert_eq!(e.hp, PLAYER_HP);
        for _ in 0..5 {
            e.hit(HIS, STRIKE);
        }
        assert_eq!(e.hp, 0);
        assert_eq!(e.down, 0.0);
        e.hit(HIS, STRIKE);
        assert_eq!(e.events.iter().filter(|x| **x == EnemyEvent::Hurt).count(), 5);
    }

    #[test]
    fn arena_follows() {
        let mut e = Enemies::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(5);
        e.arena = [0.0, 12.0, 0.0];
        e.update(1.0, 0, 1, 0.0, [100.0, 12.0, 0.0], &mut spells, &mut rng);
        assert!((e.arena[0] - 6.0).abs() < 1e-4);
    }

    #[test]
    fn a_kill_code_round_trips_through_the_renderer() {
        // a level cut (normal along the enemy's x, it turned a quarter),
        // 0.2 above its middle: decoded as the renderer does
        let yaw = 0.5 * PI;
        let n = turn(yaw, [1.0, 0.0, 0.0]);
        let pos = [3.0, 10.0, -2.0];
        let p = add(pos, turn(yaw, [0.2, 1.0, 0.0]));
        let code = kill_code(WIND, Some((p, n, [0.0; 3])), pos, yaw);
        let (k, nb, o, _) = wizard_shader::enemies::kill_of(code);
        assert_eq!(k, 2);
        assert!((nb.x.abs() - 1.0).abs() < 0.02 && nb.y.abs() < 0.03, "{nb:?}");
        assert!((o * nb.x.signum() - 0.2).abs() < 0.02, "{o}");
        // others: just the kind
        assert_eq!(wizard_shader::enemies::kill_of(kill_code(STRIKE, None, pos, yaw)).0, 1);
        assert_eq!(wizard_shader::enemies::kill_of(kill_code(BURST, None, pos, yaw)).0, 0);
        // a cut that misses the figure is moved to its edge
        let far = add(pos, turn(yaw, [3.0, 1.0, 0.0]));
        let (_, _, o, _) = wizard_shader::enemies::kill_of(kill_code(WIND, Some((far, n, [0.0; 3])), pos, yaw));
        assert!(o.abs() <= 0.351, "{o}");
    }
}
