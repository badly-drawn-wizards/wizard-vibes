// The fight, put together (web/combat.ts): the enemies (enemies.rs), his
// mana (mana.rs) and shield (shield.rs), casting with them, the sounds of
// it all (sfx.rs), and what it shows (the HUD state, drawn by hud.rs):
// bottom left his health (with enemies about), his mana, how many he has
// defeated and a reset button; a red flash when he is hit; and a crosshair
// at the view's centre, where spells go, which turns into a lock when a
// strike would find a target. Defeated, he stays down: a panel says so,
// until he tries again (its button, Enter, or the reset button).

use super::enemies::{EnemyEvent, Enemies, PLAYER_HP, WIZARD_SLOTS};
use super::mana::{Mana, CHARGE_DRAIN, HEAL_DRAIN, MANA, SPELL_COST};
use super::shield::Shield;
use super::sfx;
use super::spells::{
    aim_ground, Lock, Sound, SpellEvent, Spells, ARCANE, CHARGE_TIME, FAMILIAR, FIRE, FROST_LANCE, GRAVITY, HEALING, LOCK_TIME, MARK, METEOR, STRIKE, WARMUP, WIND,
};
use super::vec::V3;
use super::Rng;
use crate::types::SfxEvent;

const CAST_SOUND: [usize; super::spells::N_SPELLS] =
    [sfx::CAST_FIRE, sfx::CAST_STRIKE, sfx::CAST_WIND, sfx::SHIELD_UP, sfx::CAST_WIND, sfx::CAST_STRIKE, sfx::CAST_FIRE, sfx::GUST, sfx::APPEAR];
const HEAL_RATE: f32 = 1.4; // health a second, channelling
const METEOR_CHANNEL: f32 = 1.2; // seconds a meteor takes to call down (held)
const SOUL_MANA: f32 = 30.0;
// (the leaps of his chain lightning, without skills: see spells.rs)
const CHAIN_JUMPS: usize = 2;
// (the arcane missiles' locks: at most so many, one each so often, on an
// enemy within so many radians of his aim)
const MISSILE_LOCKS: usize = 6;
const MISSILE_LOCK_EVERY: f32 = 0.25;
const MISSILE_LOCK_CONE: f32 = 0.09; // mana a defeated enemy's soul brings him
const BUFFER: f32 = 0.35; // seconds a press of cast is remembered, to start a spell not yet ready
// the wind blades' gatling: seconds between blades, spun down and up; how
// long it takes to spin up, and down; the heat each blade adds (1:
// overheated), and how fast it cools, and overheated, and down to what it
// cools before firing again
const SLOW_SHOT: f32 = 0.33;
const FAST_SHOT: f32 = 0.125;
const SPIN_UP: f32 = 1.2;
const SPIN_DOWN: f32 = 0.6;
const SHOT_HEAT: f32 = 0.13;
const HEAT_COOL: f32 = 0.22;
const OVERHEAT_COOL: f32 = 0.35;
const HEAT_RESUME: f32 = 0.2;

// What the fight needs to know of the frame.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub dt: f32, // 0 while paused
    pub time: f32,
    pub him: V3,    // where he is
    pub vel: V3,    // and how fast he goes (a fire bolt is carried along by it)
    pub origin: V3, // his orb
    pub dir: V3,    // where he aims
    pub drawn: bool, // spells can be drawn (always, here)
    pub able: bool,  // and the game is running (not paused)
    pub n: usize,    // enemies
    pub seed: i64,
    pub casting: bool,   // the cast button is held
    pub shielding: bool, // the controller's shield button is held
}

// What the HUD shows, as of the last update.
#[derive(Clone, Debug, Default)]
pub struct Hud {
    pub drawn: bool,
    pub n: usize,
    pub health: f32, // 0..1
    pub mana: f32,   // 0..1
    pub short: bool, // mana fell short a moment ago
    pub defeated: u32,
    // defeated: the panel's line (as when it appeared)
    pub lost: Option<String>,
    pub hurt: f32, // the red flash's opacity
    pub locked: bool,
    pub shielded: bool,
    // a boss standing: its health (0..1)
    pub boss: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct Combat {
    pub enemies: Enemies,
    pub mana: Mana,
    pub shield: Shield,
    fizzled: f32, // when the last fizzle sounded (holding the key does not drone)
    was_casting: bool, // the cast button held last update (for its presses)
    buffered: f32,     // seconds a press of cast is still remembered (see BUFFER)
    short: bool,       // the press remembered has waited on mana (it fizzles if it runs out)
    healed: f32,  // healing channelled towards the next point of health
    // the arcane missiles locking on (held), and until the next lock
    locking: bool,
    lock_tick: f32,
    here: (V3, V3), // him, and the way he looks
    started: bool,  // the fight has an arena (it starts with the first enemies)
    // where enemies were defeated (their souls to send him), and the souls
    // he has taken in since the game last counted them
    souls: Vec<V3>,
    pub souls_taken: u32,
    // what his skills add (set by the game)
    pub boosts: super::skills::Boosts,
    // sounds to play
    pub sfx: Vec<SfxEvent>,
    pub hud: Hud,
    // the structure he is near (world.rs's kind_*), and its blessing:
    // health back at a cottage's hearth (a point every HEARTH seconds),
    // mana twice as fast on a tower's ley line, spells cooling half again
    // as fast by a pyramid's ancient ward
    pub near: Option<i32>,
    hearth: f32,
}

// (seconds a point of health takes to come back by a hearth)
const HEARTH: f32 = 3.0;

impl Default for Combat {
    fn default() -> Self {
        Combat {
            enemies: Enemies::default(),
            mana: Mana::default(),
            shield: Shield::default(),
            fizzled: -1.0,
            was_casting: false,
            buffered: 0.0,
            short: false,
            healed: 0.0,
            locking: false,
            lock_tick: 0.0,
            here: ([0.0; 3], [0.0, 0.0, 1.0]),
            started: false,
            souls: Vec::new(),
            souls_taken: 0,
            boosts: Default::default(),
            sfx: Vec::new(),
            hud: Hud::default(),
            near: None,
            hearth: 0.0,
        }
    }
}

impl Combat {
    // Starts the fight afresh on the next update (once the view has settled:
    // the arena goes where he then looks).
    pub fn restart(&mut self) {
        self.started = false;
    }

    // The fight from the start: full health and mana, no spells in flight,
    // a new set of enemies.
    pub fn reset(&mut self, spells: &mut Spells) {
        self.started = true;
        // (round a point ahead, where he looks)
        let (him, [x, _, z]) = self.here;
        let d = x.hypot(z);
        let d = if d == 0.0 { 1.0 } else { d };
        self.enemies.reset([him[0] + 12.0 * x / d, him[1], him[2] + 12.0 * z / d]);
        self.mana.value = self.mana.max;
        spells.clear();
    }

    // The drive record's share: (shield, shield_hit, arena, foes, kills).
    pub fn drive(&self) -> (f32, f32, V3, [f32; WIZARD_SLOTS], [f32; WIZARD_SLOTS]) {
        (self.shield.amount, self.shield.since_hit, self.enemies.arena, self.enemies.foes(), self.enemies.kills())
    }

    // Advances the fight; casts if he may (not while down, nor from behind
    // the shield, and only with the mana for it). Before the spells' update.
    pub fn update(&mut self, m: &Moment, spells: &mut Spells, rng: &mut Rng) {
        self.here = (m.him, m.dir);
        // (his skills)
        let b = self.boosts;
        self.enemies.max_hp = PLAYER_HP + b.health;
        self.mana.max = MANA + b.mana;
        self.enemies.revive = b.revive;
        self.enemies.frost = b.frost;
        self.shield.cost = b.shield;
        spells.chain_jumps = CHAIN_JUMPS + b.chain;
        spells.familiar = b.familiar;
        spells.well_power = (b.well_reach, b.well_harm);
        let near = self.near;
        self.mana.regen = b.regen * if near == Some(wizard_shader::world::kind_tower) { 2.0 } else { 1.0 };
        spells.cool_rate = if near == Some(wizard_shader::world::kind_pyramid) { 1.5 } else { 1.0 };
        if near == Some(wizard_shader::world::kind_cottage) && self.enemies.down < 0.0 && self.enemies.hp < self.enemies.max_hp {
            self.hearth += m.dt;
            if self.hearth >= HEARTH {
                self.hearth -= HEARTH;
                self.enemies.hp += 1;
            }
        } else {
            self.hearth = 0.0;
        }
        spells.blast = b.blast;
        spells.lock_time = LOCK_TIME * b.lock;
        let able = m.able && self.enemies.down < 0.0;
        self.shield.update(m.dt, m.shielding, able, &mut self.mana);
        self.shield_sounds();
        self.enemies.shielded = self.shield.up();
        // healing: channelled while held (with the mana for it), health
        // coming back steadily; he cannot move meanwhile (see Game::step)
        if spells.selected == HEALING {
            let channel = able && !self.shield.raised_now() && m.casting && self.mana.drain(HEAL_DRAIN * m.dt);
            if channel {
                if spells.channel < 0.0 {
                    spells.channel = 0.0;
                    self.sfx.push(sfx::here(CAST_SOUND[HEALING]));
                }
                spells.channel += m.dt;
                spells.since_cast = 0.0;
                self.healed += HEAL_RATE * self.boosts.heal * m.dt;
                while self.healed >= 1.0 {
                    self.healed -= 1.0;
                    self.enemies.hp = (self.enemies.hp + 1).min(self.enemies.max_hp);
                }
            } else {
                spells.channel = -1.0;
            }
        } else {
            spells.channel = -1.0;
        }
        spells.here = m.him;
        // A press of cast is remembered a moment (BUFFER): a spell not yet
        // ready (cooling down, or short of mana) starts as soon as it is,
        // if that is within it. Each press casts once (the fire bolt and the
        // strike); only the wind blades come again while it is held.
        let pressed = m.casting && !self.was_casting;
        self.was_casting = m.casting;
        let had = self.buffered > 0.0;
        self.buffered = if pressed { BUFFER } else { self.buffered - m.dt };
        if had && self.buffered <= 0.0 && self.short {
            self.fizzle(m.time);
        }
        if pressed {
            self.short = false;
        }
        let can = able && !self.shield.raised_now();
        let selected = spells.selected;
        // the fire bolt and the frost lance: pressed, it charges at the orb
        // while held (its cost as it starts, then mana drained while it
        // grows: short of it, it stops growing; none comes back while it is
        // held), and flies when let go, as charged (a press let go before it
        // could start: as soon as it can, uncharged)
        let charged = selected == FIRE as usize || selected == FROST_LANCE;
        if charged && can {
            let cost = SPELL_COST[selected];
            spells.charging = selected;
            if spells.charge < 0.0 && self.buffered > 0.0 && spells.ready(selected) {
                if self.mana.value >= cost {
                    self.mana.spend(cost);
                    spells.charge = 0.0;
                    self.buffered = 0.0;
                    self.short = false;
                } else {
                    self.short = true;
                }
            }
            if spells.charge >= 0.0 {
                if m.casting {
                    if spells.charge < CHARGE_TIME && self.mana.drain(CHARGE_DRAIN * m.dt) {
                        spells.charge = (spells.charge + m.dt * self.boosts.charge).min(CHARGE_TIME);
                    } else {
                        // (held charged: no refill meanwhile)
                        self.mana.drain(0.0);
                    }
                    spells.since_cast = 0.0;
                } else {
                    let power = spells.charge / CHARGE_TIME;
                    if selected == FROST_LANCE {
                        spells.cast_frost(m.origin, m.dir, power, m.vel, rng);
                    } else {
                        spells.cast_fire(m.origin, m.dir, power, m.vel, rng);
                    }
                    spells.charge = -1.0;
                    self.sfx.push(sfx::here(CAST_SOUND[selected]));
                }
            }
        } else {
            spells.charge = -1.0;
        }
        spells.orb = m.origin;
        // the lightning strike: pressed and held, it locks on to what is in
        // his aim as it starts, an enemy (the ring closing in round it,
        // following it) or the ground there (staying put, however he
        // turns), and lands there once locked (its cost then); let go
        // before, and it is not cast
        // (the meteor likewise: held, called down on what it locks on to,
        // longer to lock; he cannot speed up meanwhile: see Game::step)
        let striking = selected == STRIKE as usize || selected == METEOR;
        if selected == METEOR {
            spells.lock_time = METEOR_CHANNEL * self.boosts.meteor;
        }
        if striking && can && m.casting {
            if spells.lock.is_none() && self.buffered > 0.0 && spells.ready(selected) {
                if self.mana.value >= SPELL_COST[selected] {
                    spells.lock = Some(match spells.lock_target(m.origin, m.dir) {
                        Some((c, side)) => Lock { age: 0.0, target: Some(c), side, ground: None },
                        None => Lock { age: 0.0, target: None, side: -1, ground: Some((aim_ground(m.origin, m.dir), None)) },
                    });
                    self.sfx.push(sfx::here(sfx::MARK));
                    self.buffered = 0.0;
                    self.short = false;
                } else {
                    self.short = true;
                }
            }
            if let Some(mut lock) = spells.lock {
                // (an enemy locked: followed while it is there; gone, the
                // place it was)
                if let Some(p) = lock.target {
                    let near = spells.targets.iter().filter(|t| t.side == lock.side).map(|t| t.c).min_by(|a, b| {
                        super::vec::dist(*a, p).total_cmp(&super::vec::dist(*b, p))
                    });
                    lock.target = near.or(Some(p));
                }
                lock.age += m.dt;
                spells.lock = Some(lock);
                if lock.age >= spells.lock_time {
                    spells.lock = None;
                    if self.mana.spend(SPELL_COST[selected]) {
                        let at = lock.target.or(lock.ground.and_then(|(g, y)| y.map(|y| [g[0], y, g[2]])));
                        if selected == METEOR {
                            // (the ground not yet found: where he aimed, at
                            // sea level)
                            let at = at.unwrap_or_else(|| lock.ground.map_or(m.him, |(g, _)| g));
                            spells.cast_meteor(at, rng);
                        } else {
                            spells.cast_at(m.origin, m.dir, at, rng);
                        }
                        self.sfx.push(sfx::here(CAST_SOUND[selected]));
                    } else {
                        self.fizzle(m.time);
                    }
                }
            }
        } else {
            spells.lock = None;
        }
        // the arcane missiles: held, his aim swept over the enemies locks on
        // to each it passes (a lock each MISSILE_LOCK_EVERY, so lingering on
        // one locks on to it again), each lock its cost as it is made, as
        // many as MISSILE_LOCKS; let go, a missile for each lock (none
        // locked: nothing cast, nothing spent)
        if selected == ARCANE && can {
            if m.casting {
                if !self.locking && self.buffered > 0.0 && spells.ready(ARCANE) {
                    self.locking = true;
                    self.buffered = 0.0;
                    spells.missile_locks.clear();
                    self.lock_tick = 0.0;
                }
                if self.locking {
                    self.lock_tick -= m.dt;
                    spells.since_cast = 0.0;
                    if self.lock_tick <= 0.0 && spells.missile_locks.len() < MISSILE_LOCKS + self.boosts.locks {
                        if let Some((side, _)) = spells.aimed(m.origin, m.dir, MISSILE_LOCK_CONE) {
                            if self.mana.value >= SPELL_COST[ARCANE] {
                                self.mana.spend(SPELL_COST[ARCANE]);
                                spells.missile_locks.push(side);
                                self.lock_tick = MISSILE_LOCK_EVERY;
                                self.sfx.push(sfx::here(sfx::MARK));
                            } else {
                                self.short = true;
                            }
                        }
                    }
                }
            } else if std::mem::take(&mut self.locking) {
                let locks = std::mem::take(&mut spells.missile_locks);
                if !locks.is_empty() && spells.cast_missiles(m.origin, m.dir, &locks, rng) {
                    self.sfx.push(sfx::here(CAST_SOUND[ARCANE]));
                }
            }
        } else {
            self.locking = false;
            spells.missile_locks.clear();
        }
        // the gravity well and spirit familiar: pressed, cast at once
        // (remembered a moment, as the others: see BUFFER)
        if matches!(selected, GRAVITY | FAMILIAR) && can && self.buffered > 0.0 && spells.ready(selected) {
            let cost = SPELL_COST[selected];
            if self.mana.value >= cost {
                let cast = match selected {
                    GRAVITY => spells.cast_well(m.origin, m.dir, rng),
                    _ => spells.summon(rng),
                };
                if cast {
                    self.mana.spend(cost);
                    self.sfx.push(sfx::here(CAST_SOUND[selected]));
                }
                self.buffered = 0.0;
                self.short = false;
            } else {
                self.short = true;
            }
        }
        // the wind blades, like a gatling gun: held, they warm up (none
        // flying, the gusts swirling round the orb: see Spells::pack) for
        // WARMUP, then come faster and faster as it spins up, each heating
        // it, which cools all the while; too hot, it overheats, and will not
        // fire again until it has cooled well down
        let winding = selected == WIND as usize;
        let g = &mut spells.gatling;
        g.heat = (g.heat - m.dt * if g.overheated { OVERHEAT_COOL } else { HEAT_COOL }).max(0.0);
        if g.overheated && g.heat <= HEAT_RESUME {
            g.overheated = false;
        }
        g.next = (g.next - m.dt).max(0.0);
        let holding = winding && can && m.casting && !g.overheated;
        g.warm = if holding { g.warm + m.dt } else { 0.0 };
        let spinning = holding && g.warm >= WARMUP;
        g.spin = (g.spin + m.dt * if spinning { 1.0 / SPIN_UP } else { -1.0 / SPIN_DOWN }).clamp(0.0, 1.0);
        if holding {
            spells.since_cast = 0.0;
        }
        if spinning && spells.gatling.next <= 0.0 && spells.ready(WIND as usize) {
            let cost = SPELL_COST[WIND as usize];
            if self.mana.value >= cost && spells.cast_blade(m.origin, m.dir, rng) {
                self.mana.spend(cost);
                self.sfx.push(sfx::here(CAST_SOUND[WIND as usize]));
                self.buffered = 0.0;
                self.short = false;
                let g = &mut spells.gatling;
                g.next = SLOW_SHOT + (FAST_SHOT - SLOW_SHOT) * g.spin;
                g.heat += SHOT_HEAT * self.boosts.heat;
                if g.heat >= 1.0 {
                    g.heat = 1.0;
                    g.overheated = true;
                    self.sfx.push(sfx::here(sfx::FIZZLE));
                }
            } else if self.mana.value < cost {
                self.short = true;
                if m.casting && !pressed && self.buffered <= 0.0 {
                    self.fizzle(m.time);
                }
            }
        }
        self.mana.update(m.dt);
        // (only where spells can be drawn)
        let n = if m.drawn { m.n } else { 0 };
        if n > 0 && !self.started {
            self.reset(spells);
        }
        spells.targets = self.enemies.targets(n, m.seed, m.time, m.him);
        self.enemies.update(m.dt, n, m.seed, m.time, m.him, spells, rng);
        self.enemy_events();
        for at in std::mem::take(&mut self.souls) {
            spells.soul(at);
        }
        self.show(n, m, spells);
    }

    // Deals with what the spells did (hits and sounds): after their update,
    // and after the probe.
    pub fn handle(&mut self, spells: &mut Spells) {
        for e in std::mem::take(&mut spells.events) {
            match e {
                SpellEvent::Hit { side, kind, cut, scale, owner } => {
                    self.enemies.hit_with(side, kind, cut, scale, owner);
                    self.enemy_events();
                }
                SpellEvent::Soul => {
                    self.souls_taken += self.boosts.souls;
                    self.mana.value = (self.mana.value + SOUL_MANA + self.boosts.soul_mana).min(self.mana.max);
                    self.sfx.push(sfx::here(sfx::SHIELD_UP));
                }
                SpellEvent::Sound { what, pos } => {
                    let kind = match what {
                        Sound::Burst => sfx::BURST,
                        Sound::Gust => sfx::GUST,
                        Sound::Strike => sfx::THUNDER,
                    };
                    self.sfx.push(self.placed(kind, pos));
                }
            }
        }
        for at in std::mem::take(&mut self.souls) {
            spells.soul(at);
        }
        // (the HUD as it now stands: the health and defeat a hit changed)
        self.hud.health = self.enemies.hp as f32 / self.enemies.max_hp as f32;
        self.hud.mana = self.mana.value / self.mana.max;
        self.hud.defeated = self.enemies.defeated;
        self.hud.boss = self.enemies.boss(self.hud.n);
    }

    // The fizzle of a cast short of mana (not more than every half second,
    // so holding the button does not drone).
    fn fizzle(&mut self, time: f32) {
        if time - self.fizzled > 0.5 || time < self.fizzled {
            self.fizzled = time;
            self.sfx.push(sfx::here(sfx::FIZZLE));
        }
    }

    fn enemy_events(&mut self) {
        for e in std::mem::take(&mut self.enemies.events) {
            let ev = match e {
                EnemyEvent::Block => {
                    self.shield.block(&mut self.mana);
                    self.shield_sounds();
                    sfx::here(sfx::BLOCK)
                }
                EnemyEvent::Hurt => sfx::here(sfx::HURT),
                EnemyEvent::Revived => sfx::here(sfx::APPEAR),
                EnemyEvent::Defeat(pos, his, rank) => {
                    // (its souls: more from an elite, many from a boss,
                    // spread about where it stood)
                    if his {
                        let k = rank.souls();
                        for j in 0..k {
                            let a = std::f32::consts::TAU * j as f32 / k as f32;
                            let r = if k > 1 { 0.6 * rank.size() } else { 0.0 };
                            self.souls.push(super::vec::add(pos, [r * a.cos(), 1.3 * rank.size(), r * a.sin()]));
                        }
                    }
                    self.placed(sfx::DEFEAT, pos)
                }
                EnemyEvent::Appear(pos) => self.placed(sfx::APPEAR, pos),
                EnemyEvent::Cast(kind, pos) => {
                    let k = if kind == MARK {
                        sfx::MARK
                    } else if kind == FIRE {
                        sfx::CAST_FIRE
                    } else {
                        sfx::CAST_WIND
                    };
                    self.placed(k, pos)
                }
            };
            self.sfx.push(ev);
        }
    }

    fn shield_sounds(&mut self) {
        for k in std::mem::take(&mut self.shield.sounds) {
            self.sfx.push(sfx::here(k));
        }
    }

    // Where a sound at world point `pos` is heard from.
    fn placed(&self, kind: usize, pos: V3) -> SfxEvent {
        sfx::placed(kind, pos, self.here.0, self.here.1)
    }

    fn show(&mut self, n: usize, m: &Moment, spells: &Spells) {
        let e = &self.enemies;
        let h = &mut self.hud;
        h.drawn = m.drawn;
        h.n = n;
        h.health = e.hp as f32 / e.max_hp as f32;
        h.mana = self.mana.value / self.mana.max;
        h.short = self.mana.short < 0.4;
        h.defeated = e.defeated;
        let lost = n > 0 && e.down >= 0.0;
        if lost != h.lost.is_some() {
            h.lost = lost.then(|| match e.defeated {
                0 => crate::i18n::text().defeat_none_defeated().to_string(),
                1 => crate::i18n::text().defeat_one_defeated().to_string(),
                k => crate::i18n::text().defeat_many_defeated(k),
            });
        }
        // (defeated too: the flash fades, the defeat screen's own colours after)
        h.hurt = 0.7 * (-4.0 * e.hurt).exp();
        h.locked = spells.selected == STRIKE as usize && spells.lock.map_or_else(|| spells.lock_on(m.origin, m.dir).is_some(), |l| l.target.is_some());
        h.shielded = self.shield.raised_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::mana::CHARGE_DRAIN;
    use crate::game::spells::HIS;

    fn moment() -> Moment {
        Moment {
            dt: 1.0 / 60.0,
            time: 0.0,
            him: [0.0, 12.0, 0.0],
            vel: [0.0; 3],
            origin: [0.5, 14.0, 0.5],
            dir: [0.0, 0.0, 1.0],
            drawn: true,
            able: true,
            n: 3,
            seed: 1,
            casting: false,
            shielding: false,
        }
    }

    #[test]
    fn the_strike_locks_on_while_held() {
        let mut c = Combat::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(1);
        spells.select(STRIKE as usize);
        let held = Moment { casting: true, n: 0, ..moment() };
        // held under the lock-on time: locking, nothing cast
        for _ in 0..(LOCK_TIME * 0.5 * 60.0) as usize {
            c.update(&held, &mut spells, &mut rng);
        }
        assert!(spells.lock.is_some());
        assert_eq!(spells.count(), 0);
        // let go: no strike
        c.update(&Moment { casting: false, n: 0, ..moment() }, &mut spells, &mut rng);
        assert!(spells.lock.is_none());
        assert_eq!(spells.count(), 0);
        // (no enemy in his aim: locked on the ground where he aims, the
        // ring drawn once the probe has found its height)
        c.update(&held, &mut spells, &mut rng);
        let (ids, _) = spells.probe_points();
        assert_eq!(ids.last(), Some(&u32::MAX));
        assert!(spells.pack().chunks(10).all(|s| s[0] != 7.0));
        let mut data = vec![0.0; ids.len() * 4];
        let n = ids.len() - 1;
        data[n * 4] = 58.0;
        data[n * 4 + 2] = 1.0;
        spells.apply_probe(&ids, &data);
        assert!(spells.pack().chunks(10).any(|s| s[0] == 7.0 && (s[2] - 2.0).abs() < 1e-4));
        // held through it: one strike, there
        for _ in 0..(LOCK_TIME * 60.0) as usize + 2 {
            c.update(&held, &mut spells, &mut rng);
        }
        assert_eq!(spells.count(), 1);
        assert!(spells.pack().chunks(10).any(|s| s[0] == 1.0 && (s[2] - 2.0).abs() < 1e-4));
    }

    #[test]
    fn healing_is_channelled() {
        let mut c = Combat::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(1);
        spells.select(HEALING);
        c.enemies.hp = 4;
        // two seconds held: health back, mana drawn, no spell cast
        let m = Moment { casting: true, n: 0, ..moment() };
        for _ in 0..120 {
            c.update(&m, &mut spells, &mut rng);
        }
        assert!(c.enemies.hp >= 6, "{}", c.enemies.hp);
        assert!(c.mana.value < MANA - 20.0);
        assert!(spells.channel > 1.9);
        assert_eq!(spells.count(), 0);
        // let go: it stops
        let m = Moment { casting: false, n: 0, ..moment() };
        c.update(&m, &mut spells, &mut rng);
        assert!(spells.channel < 0.0);
    }

    #[test]
    fn casting_costs_mana() {
        let mut c = Combat::default();
        let mut s = Spells::default();
        s.selected = FIRE as usize;
        let mut rng = Rng::new(1);
        // (a tap: its cost as it starts, the bolt as it is let go)
        c.update(&Moment { casting: true, n: 0, dt: 0.0, ..moment() }, &mut s, &mut rng);
        let m = Moment { casting: false, n: 0, ..moment() };
        c.update(&m, &mut s, &mut rng);
        assert_eq!(s.count(), 1);
        assert_eq!(c.mana.value, MANA - SPELL_COST[FIRE as usize]);
        assert_eq!(c.sfx.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![sfx::CAST_FIRE]);
        // not from behind the shield
        let m = Moment { casting: true, shielding: true, n: 0, dt: 1.0, ..moment() };
        c.update(&m, &mut s, &mut rng);
        assert_eq!(s.count(), 1);
        assert!(c.hud.shielded);
    }

    #[test]
    fn the_fire_bolt_charges_while_held() {
        let mut c = Combat::default();
        let mut s = Spells::default();
        s.selected = FIRE as usize;
        let mut rng = Rng::new(1);
        // two seconds held: charged fully (no further), its cost and the
        // drain while it grew spent, nothing cast yet
        let held = Moment { casting: true, n: 0, ..moment() };
        for _ in 0..120 {
            c.update(&held, &mut s, &mut rng);
        }
        assert_eq!(s.charge, CHARGE_TIME);
        assert_eq!(s.count(), 0);
        let spent = MANA - c.mana.value;
        let want = SPELL_COST[FIRE as usize] + CHARGE_DRAIN * CHARGE_TIME;
        assert!((spent - want).abs() < 0.5, "{spent} {want}");
        // let go: it flies, faster for its charge, and carried along by
        // his own velocity; its charge, for the renderer, in its seed
        c.update(&Moment { casting: false, n: 0, vel: [5.0, 0.0, 0.0], ..moment() }, &mut s, &mut rng);
        assert_eq!(s.charge, -1.0);
        assert_eq!(s.count(), 1);
        let bolt = s.pack();
        assert!((bolt[4] - 5.0).abs() < 1e-3 && (bolt[6] - 30.0 * 1.6).abs() < 1e-3, "{:?}", &bolt[4..7]);
        assert_eq!((bolt[9] as i32) / 100000, 100);
    }

    #[test]
    fn a_press_is_remembered_and_casts_once() {
        let mut c = Combat::default();
        let mut s = Spells::default();
        s.selected = FIRE as usize;
        let mut rng = Rng::new(1);
        let up = Moment { casting: false, n: 0, ..moment() };
        let down = Moment { casting: true, n: 0, ..moment() };
        // a tap: a bolt
        c.update(&down, &mut s, &mut rng);
        c.update(&up, &mut s, &mut rng);
        s.update(up.dt, false);
        assert_eq!(s.count(), 1);
        // tapped again while it cools down, 0.2 s before it is ready: the
        // press waits, and the bolt goes as soon as it can
        for _ in 0..((0.9 - 0.2) * 60.0) as usize {
            c.update(&up, &mut s, &mut rng);
            s.update(up.dt, false);
        }
        c.update(&down, &mut s, &mut rng);
        c.update(&up, &mut s, &mut rng);
        s.update(up.dt, false);
        assert_eq!(s.count(), 1);
        for _ in 0..15 {
            c.update(&up, &mut s, &mut rng);
            s.update(up.dt, false);
        }
        assert_eq!(s.count(), 2);
        // held on and on: one bolt (when let go), not one after another
        let mut s = Spells::default();
        s.selected = FIRE as usize;
        let mut c = Combat::default();
        for _ in 0..240 {
            c.update(&down, &mut s, &mut rng);
            s.update(down.dt, true);
        }
        c.update(&up, &mut s, &mut rng);
        assert_eq!(s.count(), 1);
    }

    #[test]
    fn the_wind_blades_spin_up_and_overheat() {
        let mut c = Combat::default();
        let mut s = Spells::default();
        (s.selected, s.limit) = (WIND as usize, 16);
        let mut rng = Rng::new(1);
        let held = Moment { casting: true, n: 0, ..moment() };
        // held: blades one after another, faster once spun up
        let mut casts = Vec::new();
        for f in 0..240 {
            let n = c.sfx.len();
            c.update(&held, &mut s, &mut rng);
            s.update(held.dt, true);
            if c.sfx[n..].iter().any(|e| e.kind == sfx::CAST_WIND) {
                casts.push(f);
            }
            if s.gatling.overheated {
                break;
            }
        }
        assert!(casts.len() >= 5, "{casts:?}");
        // (none while it warms up)
        assert!(casts[0] as f32 >= WARMUP * 60.0 - 1.0, "{casts:?}");
        let gap = |k: usize| casts[k + 1] - casts[k];
        assert!(gap(0) > gap(casts.len() - 2), "{casts:?}");
        // until it overheats: no more, held or not, until it has cooled
        assert!(s.gatling.overheated);
        let n = casts.len();
        for _ in 0..30 {
            let k = c.sfx.len();
            c.update(&held, &mut s, &mut rng);
            assert!(!c.sfx[k..].iter().any(|e| e.kind == sfx::CAST_WIND));
        }
        let up = Moment { casting: false, n: 0, ..moment() };
        for _ in 0..240 {
            c.update(&up, &mut s, &mut rng);
        }
        assert!(!s.gatling.overheated && s.gatling.heat == 0.0 && s.gatling.spin == 0.0);
        assert_eq!(casts.len(), n);
    }

    #[test]
    fn a_soul_restores_mana() {
        let mut c = Combat::default();
        let mut s = Spells::default();
        c.mana.value = 10.0;
        s.here = [0.0, 12.0, 0.0];
        s.soul([8.0, 12.0, 8.0]);
        let mut got = false;
        for _ in 0..600 {
            s.update(1.0 / 60.0, false);
            c.handle(&mut s);
            if c.mana.value >= 10.0 + SOUL_MANA - 1e-3 {
                got = true;
                break;
            }
        }
        assert!(got && s.count() == 0, "{}", c.mana.value);
    }

    #[test]
    fn the_strike_stays_on_the_ground_it_locked() {
        let mut c = Combat::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(1);
        spells.select(STRIKE as usize);
        let held = Moment { casting: true, n: 0, ..moment() };
        c.update(&held, &mut spells, &mut rng);
        let at = spells.lock.unwrap().ground.unwrap().0;
        // he turns: the lock stays where it was
        c.update(&Moment { dir: [1.0, -0.3, 0.0], ..held }, &mut spells, &mut rng);
        assert_eq!(spells.lock.unwrap().ground.unwrap().0, at);
    }

    #[test]
    fn fight_starts_ahead() {
        let mut c = Combat::default();
        let mut s = Spells::default();
        let mut rng = Rng::new(1);
        c.update(&moment(), &mut s, &mut rng);
        // round a point 12 ahead of him
        assert!((c.enemies.arena[2] - 12.0).abs() < 1e-4);
        assert_eq!(c.hud.n, 3);
        assert!(c.hud.lost.is_none());
        // beaten
        for _ in 0..5 {
            c.enemies.hit(super::super::spells::HIS, STRIKE);
        }
        c.update(&moment(), &mut s, &mut rng);
        assert_eq!(c.hud.lost.as_deref(), Some("No enemies defeated"));
        assert!(c.hud.hurt > 0.0);
        c.reset(&mut s);
        c.update(&moment(), &mut s, &mut rng);
        assert!(c.hud.lost.is_none());
    }

    #[test]
    fn a_chrono_revive_brings_him_back_once_a_run() {
        let mut c = Combat::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(1);
        c.boosts.revive = 0.5;
        c.update(&moment(), &mut spells, &mut rng);
        let e = &mut c.enemies;
        e.hp = 1;
        e.hit(HIS, STRIKE);
        // (back, with half his health, time to rewind, a moment unhurt)
        assert_eq!((e.down, e.hp, e.rewind), (-1.0, e.max_hp / 2, true));
        e.hit(HIS, STRIKE);
        assert_eq!(e.hp, e.max_hp / 2);
        e.grace = 0.0;
        e.hp = 1;
        e.hit(HIS, STRIKE);
        assert!(e.down >= 0.0);
        // (a new run: his again; but giving up is giving up)
        e.reset([0.0; 3]);
        e.give_up();
        assert!(e.down >= 0.0 && e.gave_up && e.his_end.is_none());
    }

    #[test]
    fn structures_bless_him_near_them() {
        use wizard_shader::world::{kind_cottage, kind_pyramid, kind_tower};
        let mut c = Combat::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(1);
        c.update(&moment(), &mut spells, &mut rng);
        let regen = c.mana.regen;
        c.near = Some(kind_tower);
        c.update(&moment(), &mut spells, &mut rng);
        assert_eq!(c.mana.regen, 2.0 * regen);
        c.near = Some(kind_pyramid);
        c.update(&moment(), &mut spells, &mut rng);
        assert_eq!((c.mana.regen, spells.cool_rate), (regen, 1.5));
        c.near = Some(kind_cottage);
        c.enemies.hp = 3;
        for _ in 0..(3.1 * 60.0) as usize {
            c.update(&moment(), &mut spells, &mut rng);
        }
        assert_eq!((c.enemies.hp, spells.cool_rate), (4, 1.0));
    }

    #[test]
    fn missiles_lock_on_as_the_aim_sweeps_and_fly_when_let_go() {
        let mut c = Combat::default();
        let mut spells = Spells::default();
        let mut rng = Rng::new(1);
        spells.unlocked = [true; super::super::spells::N_SPELLS];
        spells.select(ARCANE);
        let o = moment().origin;
        let at = |x: f32| super::super::vec::add(o, [x, 0.0, 15.0]);
        let targets = vec![
            super::super::spells::Target { c: at(-3.0), r: 0.9, side: 0, body: true },
            super::super::spells::Target { c: at(3.0), r: 0.9, side: 1, body: true },
        ];
        let aim = |x: f32| super::super::vec::normalise(super::super::vec::sub(at(x), o));
        // (held: swept from one to the other, then lingering on the second;
        // the targets set afresh each time: the fight's own, none, replace
        // them as it updates)
        for f in 0..120 {
            spells.targets = targets.clone();
            let x = if f < 40 { -3.0 + 6.0 * f as f32 / 40.0 } else { 3.0 };
            c.update(&Moment { casting: true, n: 0, dir: aim(x), ..moment() }, &mut spells, &mut rng);
            spells.update(1.0 / 60.0, true);
        }
        let locks = spells.missile_locks.clone();
        assert!(locks.contains(&0) && locks.iter().filter(|&&s| s == 1).count() >= 3, "{locks:?}");
        assert_eq!(spells.count(), 0);
        // (let go: a missile each)
        c.update(&Moment { casting: false, n: 0, dir: aim(3.0), ..moment() }, &mut spells, &mut rng);
        assert_eq!(spells.count(), locks.len());
        assert!(spells.missile_locks.is_empty());
    }
}
