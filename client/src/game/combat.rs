// The fight, put together (web/combat.ts): the enemies (enemies.rs), his
// mana (mana.rs) and shield (shield.rs), casting with them, the sounds of
// it all (sfx.rs), and what it shows (the HUD state, drawn by hud.rs):
// bottom left his health (with enemies about), his mana, how many he has
// defeated and a reset button; a red flash when he is hit; and a crosshair
// at the view's centre, where spells go, which turns into a lock when a
// strike would find a target. Defeated, he stays down: a panel says so,
// until he tries again (its button, Enter, or the reset button).

use super::enemies::{EnemyEvent, Enemies, PLAYER_HP};
use super::mana::{Mana, HEAL_DRAIN, MANA, SPELL_COST};
use super::shield::Shield;
use super::sfx;
use super::spells::{Lock, Sound, SpellEvent, Spells, FIRE, HEALING, LOCK_TIME, MARK, STRIKE};
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

    // The drive record's share: (shield, shield_hit, arena, foes).
    pub fn drive(&self) -> (f32, f32, V3, [f32; 6]) {
        (self.shield.amount, self.shield.since_hit, self.enemies.arena, self.enemies.foes())
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
        // the lightning strike: held, it locks on to the enemy in his aim
        // (the ring closing in round it, following it), and lands on it
        // once locked; let go before, and it is not cast
        let striking = spells.selected == STRIKE as usize;
        if striking && able && !self.shield.raised_now() && m.casting && (spells.lock.is_some() || spells.ready()) {
            let aimed = spells.lock_target(m.origin, m.dir);
            let mut lock = spells.lock.unwrap_or(Lock { age: 0.0, target: None, side: -1 });
            // (keep to the target locked, while it is still there; else the
            // one in his aim)
            let kept = lock.target.and_then(|_| spells.targets.iter().filter(|t| t.side == lock.side).map(|t| t.c).min_by(|a, b| {
                let p = lock.target.unwrap();
                super::vec::dist(*a, p).total_cmp(&super::vec::dist(*b, p))
            }));
            match (kept, aimed) {
                (Some(c), _) => lock.target = Some(c),
                (None, Some((c, side))) => {
                    lock.target = Some(c);
                    lock.side = side;
                    lock.age = 0.0;
                }
                (None, None) => lock.target = None,
            }
            if spells.lock.is_none() {
                self.sfx.push(sfx::here(sfx::MARK));
            }
            lock.age += m.dt;
            spells.lock = Some(lock);
            if lock.age >= LOCK_TIME {
                spells.lock = None;
                if self.mana.spend(SPELL_COST[STRIKE as usize]) {
                    spells.cast_at(m.origin, m.dir, lock.target, rng);
                    self.sfx.push(sfx::here(CAST_SOUND[STRIKE as usize]));
                } else if m.time - self.fizzled > 0.5 || m.time < self.fizzled {
                    self.fizzled = m.time;
                    self.sfx.push(sfx::here(sfx::FIZZLE));
                }
            }
        } else if striking {
            spells.lock = None;
        }
        if !striking {
            spells.lock = None;
        }
        if spells.selected != HEALING && !striking && able && !self.shield.raised_now() && m.casting && spells.ready() {
            if self.mana.spend(SPELL_COST[spells.selected]) {
                spells.cast(m.origin, m.dir, rng);
                self.sfx.push(sfx::here(CAST_SOUND[spells.selected]));
            } else if m.time - self.fizzled > 0.5 || m.time < self.fizzled {
                self.fizzled = m.time;
                self.sfx.push(sfx::here(sfx::FIZZLE));
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
                SpellEvent::Hit { side, kind, .. } => {
                    self.enemies.hit(side, kind);
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
        h.locked = spells.selected == STRIKE as usize && spells.lock_on(m.origin, m.dir).is_some();
        h.shielded = self.shield.raised_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment() -> Moment {
        Moment {
            dt: 1.0 / 60.0,
            time: 0.0,
            him: [0.0, 12.0, 0.0],
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
        // held through it: one strike
        for _ in 0..(LOCK_TIME * 60.0) as usize + 2 {
            c.update(&held, &mut spells, &mut rng);
        }
        assert_eq!(spells.count(), 1);
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
        s.selected = 0;
        let mut rng = Rng::new(1);
        let m = Moment { casting: true, n: 0, ..moment() };
        c.update(&m, &mut s, &mut rng);
        assert_eq!(s.count(), 1);
        assert_eq!(c.mana.value, MANA - SPELL_COST[0]);
        assert_eq!(c.sfx.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![sfx::CAST_FIRE]);
        // not from behind the shield
        let m = Moment { casting: true, shielding: true, n: 0, dt: 1.0, ..moment() };
        c.update(&m, &mut s, &mut rng);
        assert_eq!(s.count(), 1);
        assert!(c.hud.shielded);
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
