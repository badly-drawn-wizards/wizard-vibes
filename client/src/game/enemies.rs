// The fight with the enemy wizards (web/enemies.ts; drawn by
// fut/enemies.fut). Where each one is is worked out the same way the
// renderer does it (a pure function of the time, his position, the seed and
// its generation), so the spells can hit what is drawn. Here: their health,
// their defeat and replacement, their casting at him, and his health.
//
// Each enemy's state goes to the renderer as one float (see foes in
// fut/enemies.fut): 16 times its generation (how many have had its place
// before it: each new one has new looks and a new place) plus its phase,
// under 8 the seconds it has still to appear, 8 and over 8 plus the seconds
// since its defeat.

use super::spells::{Spells, Target, BURST, FIRE, HIS, MARK, STRIKE, WIND};
use super::vec::{add, dist, normalise, scale, sub, turn, V3};
use super::Rng;
use std::f32::consts::PI;

pub const MAX: usize = 6; // as max_enemies in fut/materials.fut
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
// azimuth, elevation and offset in six bits.
pub fn kill_code(kind: u32, cut: Option<(V3, V3)>, pos: V3, yaw: f32) -> f32 {
    let k = match kind {
        STRIKE => 1,
        WIND => 2,
        _ => 0,
    };
    let Some((p, n)) = cut.filter(|_| k == 2) else { return k as f32 };
    let n = normalise(turn(-yaw, n));
    let n = if n[1] < 0.0 { scale(-1.0, n) } else { n };
    let pb = turn(-yaw, sub(p, pos));
    let off = super::vec::dot(n, sub(pb, [0.0, 1.0, 0.0])).clamp(-0.35, 0.35);
    let phi = n[2].atan2(n[0]);
    let th = n[1].clamp(0.0, 1.0).asin();
    let a = ((phi + PI) / (2.0 * PI) * 63.0).round() as u32 % 64;
    let e = (th / (0.5 * PI) * 63.0).round() as u32;
    let o = ((off + 0.35) / 0.7 * 63.0).round() as u32;
    (k + 4 * (a + 64 * (e + 64 * o))) as f32
}

fn damage(kind: u32) -> i32 {
    match kind {
        BURST | WIND => 1,
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

// Where enemy i (of generation generation) stands, circling `arena`, and which way
// it faces, he being at `him` (enemy_at in fut/enemies.fut, while it is not
// falling).
pub fn enemy_at(seed: i64, generation: i64, i: usize, secs: f32, arena: V3, him: V3) -> (V3, f32) {
    let r = |k| rnd(seed, generation, i, k);
    let radius = 9.0 + 12.0 * r(0);
    let speed = (0.04 + 0.06 * r(1)) * if r(2) < 0.5 { -1.0 } else { 1.0 };
    let a = 2.0 * PI * (i as f32 / MAX as f32 + 0.12 * r(3)) + speed * secs;
    let h = -1.5 + 4.0 * r(4) + 0.5 * (0.6 * secs + 6.3 * r(5)).sin();
    let pos = add(arena, [radius * a.sin(), h, radius * a.cos()]);
    (pos, (him[0] - pos[0]).atan2(him[2] - pos[2]))
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
    Defeat(V3),
    Appear(V3),
    Cast(u32, V3),
}

#[derive(Clone, Debug)]
struct Slot {
    generation: i64,
    hp: i32,
    kill: f32, // what defeated it (a kill code: see kill_code)
    appear: f32, // seconds still to appear
    down: f32,   // seconds since its defeat, or -1
    cast_in: f32, // seconds to its next spell
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
            slots: (0..MAX).map(|i| Slot { generation: 0, hp: HP, kill: 0.0, appear: 0.0, down: -1.0, cast_in: 5.0 + i as f32 * 2.0 }).collect(),
            seen: (0, 0.0, [0.0; 3]),
            last: None,
            vel: [0.0; 3],
        }
    }
}

impl Enemies {
    // The renderer's states (foe0 .. foe5 of the drive record).
    pub fn foes(&self) -> [f32; MAX] {
        let mut f = [0.0; MAX];
        for (i, s) in self.slots.iter().enumerate() {
            let phase = if s.down >= 0.0 { 8.0 + s.down.min(7.9) } else { s.appear.min(7.9) };
            f[i] = 16.0 * s.generation as f32 + phase;
        }
        f
    }

    // What defeated each (kill0 .. kill5 of the drive record).
    pub fn kills(&self) -> [f32; MAX] {
        let mut f = [0.0; MAX];
        for (i, s) in self.slots.iter().enumerate() {
            f[i] = s.kill;
        }
        f
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
                out.extend(spheres(enemy_at(seed, s.generation, i, secs, self.arena, him).0, i as i32));
            }
        }
        out
    }

    // The fight from the start, round `arena`: him at full health and up,
    // none defeated, and a new set of enemies appearing.
    pub fn reset(&mut self, arena: V3) {
        self.arena = [arena[0], arena[1].max(ARENA_FLOOR), arena[2]];
        self.hp = PLAYER_HP;
        self.down = -1.0;
        self.defeated = 0;
        self.hurt = 99.0;
        for (i, s) in self.slots.iter_mut().enumerate() {
            *s = Slot { generation: s.generation + 1, hp: HP, kill: 0.0, appear: APPEAR, down: -1.0, cast_in: 5.0 + i as f32 * 2.0 };
        }
    }

    // A spell of `kind` hit `side`.
    pub fn hit(&mut self, side: i32, kind: u32) {
        self.hit_with(side, kind, None, 1.0);
    }

    // The same, a wind blade's cut with it (a point and normal, world), its
    // damage scaled by `scale` (rounded; at least 1, if it does any).
    pub fn hit_with(&mut self, side: i32, kind: u32, cut: Option<(V3, V3)>, scale: f32) {
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
            }
        } else {
            let Some(s) = self.slots.get_mut(side as usize) else { return };
            if s.down >= 0.0 {
                return;
            }
            s.hp -= damage;
            if s.hp <= 0 {
                s.down = 0.0;
                self.defeated += 1;
                let (seed, secs, him) = self.seen;
                let generation = s.generation;
                let (pos, yaw) = enemy_at(seed, generation, side as usize, secs, self.arena, him);
                s.kill = kill_code(kind, cut, pos, yaw);
                self.events.push(EnemyEvent::Defeat(pos));
            }
        }
    }

    // Advances the fight by dt with n enemies, he being at `him`; they cast
    // with `spells`.
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
        for i in 0..MAX {
            let s = &mut self.slots[i];
            if s.down >= 0.0 {
                s.down += dt;
                if s.down >= RESPAWN {
                    *s = Slot { generation: s.generation + 1, hp: HP, kill: 0.0, appear: APPEAR, down: -1.0, cast_in: 4.0 + 4.0 * rng.uniform() };
                    let generation = s.generation;
                    if i < n {
                        self.events.push(EnemyEvent::Appear(enemy_at(seed, generation, i, secs, self.arena, him).0));
                    }
                }
                continue;
            }
            s.appear = (s.appear - dt).max(0.0);
            if i >= n || s.appear > 0.0 || self.down >= 0.0 {
                continue;
            }
            s.cast_in -= dt;
            if s.cast_in > 0.0 {
                continue;
            }
            s.cast_in = 6.0 + 6.0 * rng.uniform() + n as f32;
            let generation = s.generation;
            self.cast(seed, generation, i, secs, him, spells, rng);
        }
    }

    // Enemy i casts at him: its kind of spell, from its orb, aimed where he
    // will be when it arrives, give or take.
    #[allow(clippy::too_many_arguments)]
    fn cast(&mut self, seed: i64, generation: i64, i: usize, secs: f32, him: V3, spells: &mut Spells, rng: &mut Rng) {
        let (pos, yaw) = enemy_at(seed, generation, i, secs, self.arena, him);
        let orb = add(pos, turn(yaw, [0.45, 2.05, 0.2]));
        let r = rnd(seed, generation, i, 29);
        let kind = if r < 0.5 { FIRE } else if r < 0.8 { WIND } else { STRIKE };
        let chest = add(him, [0.0, 1.1, 0.0]);
        if dist(chest, orb) > RANGE {
            return;
        }
        // (off by up to 2.5 either way, mostly less)
        let mut miss = || (rng.uniform() + rng.uniform() - 1.0) * 2.5;
        if kind == STRIKE {
            // marked first, where he will be by the time it lands
            let lead = 1.1;
            let (mx, mz) = (miss(), miss());
            let at = add(add(him, scale(lead, self.vel)), [mx, 0.6, mz]);
            if spells.launch(MARK, orb, [0.0, 1.0, 0.0], i as i32, Some(at), rng) {
                self.events.push(EnemyEvent::Cast(MARK, at));
            }
            return;
        }
        let speed = if kind == FIRE { 30.0 } else { 24.0 };
        let (mx, my, mz) = (miss(), miss(), miss());
        let aim = add(add(chest, scale(dist(chest, orb) / speed, self.vel)), [mx, my / 2.0, mz]);
        if spells.launch(kind, orb, normalise(sub(aim, orb)), i as i32, None, rng) {
            self.events.push(EnemyEvent::Cast(kind, orb));
        }
    }
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
    fn positions_follow_the_formula() {
        let arena = [1.0, 12.0, 12.0];
        let (seed, generation, i, secs) = (1, 2, 3, 7.5);
        let r = |k| rnd(seed, generation, i, k);
        let (pos, yaw) = enemy_at(seed, generation, i, secs, arena, [0.0, 12.0, 0.0]);
        let radius = 9.0 + 12.0 * r(0);
        let flat = (pos[0] - arena[0]).hypot(pos[2] - arena[2]);
        assert!((flat - radius).abs() < 1e-3);
        let h = pos[1] - arena[1];
        assert!(h >= -2.0 && h <= 3.0);
        assert!((yaw - (-pos[0]).atan2(-pos[2])).abs() < 1e-6);
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
        assert!(matches!(e.events.last(), Some(EnemyEvent::Defeat(_))));
        assert_eq!(e.foes()[1], 16.0 + 8.0);
        // it comes back, a generation on
        e.update(5.0, 3, 1, 0.0, [0.0, 12.0, 0.0], &mut spells, &mut rng);
        assert_eq!(e.foes()[1], 32.0 + 1.5);
        assert!(matches!(e.events.last(), Some(EnemyEvent::Appear(_))));
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
        let code = kill_code(WIND, Some((p, n)), pos, yaw);
        let (k, nb, o) = wizard_shader::enemies::kill_of(code);
        assert_eq!(k, 2);
        assert!((nb.x.abs() - 1.0).abs() < 0.02 && nb.y.abs() < 0.03, "{nb:?}");
        assert!((o * nb.x.signum() - 0.2).abs() < 0.02, "{o}");
        // others: just the kind
        assert_eq!(wizard_shader::enemies::kill_of(kill_code(STRIKE, None, pos, yaw)).0, 1);
        assert_eq!(wizard_shader::enemies::kill_of(kill_code(BURST, None, pos, yaw)).0, 0);
        // a cut that misses the figure is moved to its edge
        let far = add(pos, turn(yaw, [3.0, 1.0, 0.0]));
        let (_, _, o) = wizard_shader::enemies::kill_of(kill_code(WIND, Some((far, n)), pos, yaw));
        assert!(o.abs() <= 0.351, "{o}");
    }
}
