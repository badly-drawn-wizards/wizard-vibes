// The fight, put together (web/combat.ts): the enemies (enemies.rs), his
// mana (mana.rs) and shield (shield.rs), casting with them, the sounds of
// it all (sfx.rs), and what it shows (the HUD state, drawn by hud.rs):
// bottom left his health (with enemies about), his mana, how many he has
// defeated and a reset button; a red flash when he is hit; and a crosshair
// at the view's centre, where spells go, which turns into a lock when a
// strike would find a target. Defeated, he stays down: a panel says so,
// until he tries again (its button, Enter, or the reset button).

use super::enemies::{EnemyEvent, Enemies, PLAYER_HP};
use super::mana::{Mana, CHARGE_DRAIN, HEAL_DRAIN, MANA, SPELL_COST};
use super::shield::Shield;
use super::sfx;
use super::spells::{aim_ground, Lock, Sound, SpellEvent, Spells, CHARGE_TIME, FIRE, HEALING, LOCK_TIME, MARK, STRIKE};
use super::vec::V3;
use super::Rng;
use crate::types::SfxEvent;

const CAST_SOUND: [usize; 4] = [sfx::CAST_FIRE, sfx::CAST_STRIKE, sfx::CAST_WIND, sfx::SHIELD_UP];
const HEAL_RATE: f32 = 1.4; // health a second, channelling

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
}

#[derive(Clone, Debug)]
pub struct Combat {
    pub enemies: Enemies,
    pub mana: Mana,
    pub shield: Shield,
    fizzled: f32, // when the last fizzle sounded (holding the key does not drone)
    healed: f32,  // healing channelled towards the next point of health
    here: (V3, V3), // him, and the way he looks
    started: bool,  // the fight has an arena (it starts with the first enemies)
    // sounds to play
    pub sfx: Vec<SfxEvent>,
    pub hud: Hud,
}

impl Default for Combat {
    fn default() -> Self {
        Combat {
            enemies: Enemies::default(),
            mana: Mana::default(),
            shield: Shield::default(),
            fizzled: -1.0,
            healed: 0.0,
            here: ([0.0; 3], [0.0, 0.0, 1.0]),
            started: false,
            sfx: Vec::new(),
            hud: Hud::default(),
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
        self.mana.value = MANA;
        spells.clear();
    }

    // The drive record's share: (shield, shield_hit, arena, foes, kills).
    pub fn drive(&self) -> (f32, f32, V3, [f32; 6], [f32; 6]) {
        (self.shield.amount, self.shield.since_hit, self.enemies.arena, self.enemies.foes(), self.enemies.kills())
    }

    // Advances the fight; casts if he may (not while down, nor from behind
    // the shield, and only with the mana for it). Before the spells' update.
    pub fn update(&mut self, m: &Moment, spells: &mut Spells, rng: &mut Rng) {
        self.here = (m.him, m.dir);
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
                self.healed += HEAL_RATE * m.dt;
                while self.healed >= 1.0 {
                    self.healed -= 1.0;
                    self.enemies.hp = (self.enemies.hp + 1).min(PLAYER_HP);
                }
            } else {
                spells.channel = -1.0;
            }
        } else {
            spells.channel = -1.0;
        }
        spells.here = m.him;
        // the fire bolt: held, it charges at the orb (its cost as it
        // starts, then mana drained while it grows: short of it, it stops
        // growing; none comes back while it is held), and flies when let
        // go, as charged
        let firing = spells.selected == FIRE as usize;
        if firing && able && !self.shield.raised_now() {
            if m.casting {
                if spells.charge < 0.0 {
                    if spells.ready() {
                        if self.mana.spend(SPELL_COST[FIRE as usize]) {
                            spells.charge = 0.0;
                        } else {
                            self.fizzle(m.time);
                        }
                    }
                } else if spells.charge < CHARGE_TIME && self.mana.drain(CHARGE_DRAIN * m.dt) {
                    spells.charge = (spells.charge + m.dt).min(CHARGE_TIME);
                } else {
                    // (held charged: no refill meanwhile)
                    self.mana.drain(0.0);
                }
                if spells.charge >= 0.0 {
                    spells.since_cast = 0.0;
                }
            } else if spells.charge >= 0.0 {
                spells.cast_fire(m.origin, m.dir, spells.charge / CHARGE_TIME, m.vel, rng);
                spells.charge = -1.0;
                self.sfx.push(sfx::here(CAST_SOUND[FIRE as usize]));
            }
        } else {
            spells.charge = -1.0;
        }
        spells.orb = m.origin;
        // the lightning strike: held, it locks on to what is in his aim as
        // it starts, an enemy (the ring closing in round it, following it)
        // or the ground there (staying put, however he turns), and lands
        // there once locked; let go before, and it is not cast
        let striking = spells.selected == STRIKE as usize;
        if striking && able && !self.shield.raised_now() && m.casting && (spells.lock.is_some() || spells.ready()) {
            let mut lock = spells.lock.unwrap_or_else(|| match spells.lock_target(m.origin, m.dir) {
                Some((c, side)) => Lock { age: 0.0, target: Some(c), side, ground: None },
                None => Lock { age: 0.0, target: None, side: -1, ground: Some((aim_ground(m.origin, m.dir), None)) },
            });
            // (an enemy locked: followed while it is there; gone, the
            // place it was)
            if let Some(p) = lock.target {
                let near = spells.targets.iter().filter(|t| t.side == lock.side).map(|t| t.c).min_by(|a, b| {
                    super::vec::dist(*a, p).total_cmp(&super::vec::dist(*b, p))
                });
                lock.target = near.or(Some(p));
            }
            if spells.lock.is_none() {
                self.sfx.push(sfx::here(sfx::MARK));
            }
            lock.age += m.dt;
            spells.lock = Some(lock);
            if lock.age >= LOCK_TIME {
                spells.lock = None;
                if self.mana.spend(SPELL_COST[STRIKE as usize]) {
                    let at = lock.target.or(lock.ground.and_then(|(g, y)| y.map(|y| [g[0], y, g[2]])));
                    spells.cast_at(m.origin, m.dir, at, rng);
                    self.sfx.push(sfx::here(CAST_SOUND[STRIKE as usize]));
                } else {
                    self.fizzle(m.time);
                }
            }
        } else if striking {
            spells.lock = None;
        }
        if !striking {
            spells.lock = None;
        }
        if spells.selected != HEALING && !striking && !firing && able && !self.shield.raised_now() && m.casting && spells.ready() {
            if self.mana.spend(SPELL_COST[spells.selected]) {
                spells.cast(m.origin, m.dir, rng);
                self.sfx.push(sfx::here(CAST_SOUND[spells.selected]));
            } else {
                self.fizzle(m.time);
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
        self.show(n, m, spells);
    }

    // Deals with what the spells did (hits and sounds): after their update,
    // and after the probe.
    pub fn handle(&mut self, spells: &mut Spells) {
        for e in std::mem::take(&mut spells.events) {
            match e {
                SpellEvent::Hit { side, kind, cut, scale, .. } => {
                    self.enemies.hit_with(side, kind, cut, scale);
                    self.enemy_events();
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
        // (the HUD as it now stands: the health and defeat a hit changed)
        self.hud.health = self.enemies.hp as f32 / PLAYER_HP as f32;
        self.hud.mana = self.mana.value / MANA;
        self.hud.defeated = self.enemies.defeated;
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
                EnemyEvent::Defeat(pos) => self.placed(sfx::DEFEAT, pos),
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
        h.health = e.hp as f32 / PLAYER_HP as f32;
        h.mana = self.mana.value / MANA;
        h.short = self.mana.short < 0.4;
        h.defeated = e.defeated;
        let lost = n > 0 && e.down >= 0.0;
        if lost != h.lost.is_some() {
            h.lost = lost.then(|| match e.defeated {
                0 => "No enemies defeated".to_string(),
                1 => "1 enemy defeated".to_string(),
                k => format!("{k} enemies defeated"),
            });
        }
        h.hurt = if lost { 0.6 } else { 0.7 * (-4.0 * e.hurt).exp() };
        h.locked = spells.selected == STRIKE as usize && spells.lock.map_or_else(|| spells.lock_on(m.origin, m.dir).is_some(), |l| l.target.is_some());
        h.shielded = self.shield.raised_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::mana::CHARGE_DRAIN;

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
        s.selected = 2;
        let mut rng = Rng::new(1);
        let m = Moment { casting: true, n: 0, ..moment() };
        c.update(&m, &mut s, &mut rng);
        assert_eq!(s.count(), 1);
        assert_eq!(c.mana.value, MANA - SPELL_COST[2]);
        assert_eq!(c.sfx.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![sfx::CAST_WIND]);
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
        assert_eq!(c.hud.hurt, 0.6);
        c.reset(&mut s);
        c.update(&moment(), &mut s, &mut rng);
        assert!(c.hud.lost.is_none());
    }
}
