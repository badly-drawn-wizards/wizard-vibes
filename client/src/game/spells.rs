// Spells (web/spells.ts): fire bolt, lightning strike and wind blade, cast
// from the orb along the camera's view; and healing, channelled (see
// combat.rs: while it is held he heals, and cannot move). Their state lives here; each frame
// it goes to the renderer as a flat array (see fut/spells.fut), and the
// collision probe (fut/collide.fut) reports the land at each one: a fire
// bolt bursts where it hits, a wind blade breaks up, and a lightning strike
// finds the ground it hits. His fire bolt is charged first, while the cast
// is held (see combat.rs): the longer, up to CHARGE_TIME, the faster and
// bigger the bolt and the wider and harder its burst, which hurts every
// enemy within its reach, most at its heart. His wind blades come like a
// gatling gun's while the cast is held (see Gatling). At most `limit` (a
// compile-time parameter) are out at once, and each spell has its own
// cooldown on top.
//
// The enemies cast them too (see enemies.rs). Each spell has a side: his
// hit the enemies, theirs hit him, as spheres listed in `targets`; a hit is
// reported in `events` (a bolt or blade where it strikes, a burst once for
// each target in its blast, a strike once where it lands). His strikes
// lock on to a target near where he aims. Theirs are telegraphed: a ring
// marks where one will land for a moment first (MARK), so he can get out
// of the way.

use super::vec::{dist, dot, len, sub, V3};
use super::Rng;

// His spells, by number (a spell's number is not a kind of spell in flight,
// below, though the first three are the same).
pub const N_SPELLS: usize = 9;
// Each spell's name, in the language chosen.
pub fn spell_name(i: usize) -> &'static str {
    let t = crate::i18n::text();
    match i {
        0 => t.spell_fire_bolt(),
        1 => t.spell_lightning_strike(),
        2 => t.spell_wind_blade(),
        3 => t.spell_healing(),
        FROST_LANCE => t.spell_frost_lance(),
        ARCANE => t.spell_arcane_missiles(),
        METEOR => t.spell_meteor(),
        GRAVITY => t.spell_gravity_well(),
        _ => t.spell_spirit_familiar(),
    }
}
pub const FROST_LANCE: usize = 4;
pub const ARCANE: usize = 5;
pub const METEOR: usize = 6;
pub const GRAVITY: usize = 7;
pub const FAMILIAR: usize = 8;
pub const HEALING: usize = 3; // (the spell number: channelled, not cast)

// kinds, as in fut/spells.fut
pub const FIRE: u32 = 0;
pub const STRIKE: u32 = 1;
pub const WIND: u32 = 2;
pub const BURST: u32 = 3;
pub const GUST: u32 = 4; // a wind blade's burst into small blades
pub const MARK: u32 = 5; // where a strike is about to land
pub const HEAL: u32 = 6; // healing being channelled, round him
pub const LOCK: u32 = 7; // his strike locking on to a target (held)
pub const CHARGE: u32 = 8; // a fire bolt being charged at the orb (his: held)
pub const WINDUP: u32 = 9; // wind blades warming up at the orb
pub const WISP: u32 = 10; // a defeated enemy's soul, flying to him (it restores his mana)
pub const WISP_TAIL: u32 = 11; // a piece of a soul's tail (drawn only: see pack)
pub const FROST: u32 = 12; // a frost lance: passes through what it hits, slowing it
pub const MISSILE: u32 = 13; // an arcane missile: homes in on its target (vel: its velocity)
pub const FALLING: u32 = 14; // a meteor falling to where it was called (top: whence)
pub const WELL: u32 = 15; // a gravity well: pulls enemies, and their spells, in
pub const SPIRIT: u32 = 16; // his spirit familiar, circling him and casting missiles
pub const CHAIN: u32 = 17;
const NO_TARGET: i32 = -100;
// (a frost lance's speed; an arcane missile's, and how sharply it turns;
// a meteor's fall, seconds, from how high; a gravity well's reach, its
// pull on enemy spells; his familiar's circle round him, its turn a
// second, and its casting's reach and pace)
pub const FROST_SPEED: f32 = 40.0;
const MISSILE_SPEED: f32 = 22.0;
const MISSILE_TURN: f32 = 6.0;
const METEOR_FALL: f32 = 0.9;
const METEOR_HEIGHT: f32 = 28.0;
pub const WELL_REACH: f32 = 14.0;
const WELL_PULL: f32 = 30.0;
const SPIRIT_ORBIT: f32 = 1.7;
const SPIRIT_TURN: f32 = 1.3;
const SPIRIT_REACH: f32 = 30.0;
const SPIRIT_PACE: f32 = 1.2; // his strike's lightning jumping on to another enemy (drawn only): from pos to vel
// (his strike chains on from the enemy it hits to the nearest other within
// CHAIN_RANGE, and so on, CHAIN_JUMPS times, each doing CHAIN_DAMAGE of its
// damage)
const CHAIN_RANGE: f32 = 9.0;
const CHAIN_JUMPS: usize = 2;
const CHAIN_DAMAGE: f32 = 0.5;
// (a soul's tail: how much of its path it trails, units, and how far apart
// the points of its path are kept)
const TAIL: f32 = 1.8;
const TAIL_STEP: f32 = 0.05;
pub const STRIDE: usize = 10;

const LIFE: [f32; 18] = [3.0, 0.9, 1.5, 1.4, 1.1, 1.1, 1e3, LOCK_TIME, 1.0, 0.5, 12.0, 1.0, 2.0, 3.0, 1.0, 4.0, 12.0, 0.35]; // seconds, by kind
pub const WARMUP: f32 = 0.5; // seconds wind blades warm up before the first flies
pub const LOCK_TIME: f32 = 0.6; // seconds his strike takes to lock on (cast held)
const SPEED: [f32; 4] = [30.0, 0.0, 24.0, 0.0]; // units/s, by kind (fire, strike, wind)
const COOLDOWN: [f32; N_SPELLS] = [0.9, 2.0, 0.0, 0.0, 0.7, 2.5, 6.0, 8.0, 15.0]; // seconds before the spell's next cast (wind: see Gatling)
const GATLING_LIFE: f32 = 0.75; // seconds a gatling's blade flies (then fades, not bursting)
const STRIKE_RANGE: f32 = 45.0; // furthest a strike lands
const STRIKE_HEIGHT: f32 = 25.0; // it comes down from this far above the orb
const PROBE_HEIGHT: f32 = 60.0; // a strike's ground is probed from up here
const BLAST: f32 = 2.6; // a burst's reach (uncharged; twice it, charged fully)
pub const CHARGE_TIME: f32 = 1.2; // seconds a fire bolt takes to charge fully
const STRIKE_REACH: f32 = 1.6; // a strike's
const LOCK_ON: f32 = 0.12; // radians off his aim a strike still finds a target

// Something spells can hit: a sphere, and whose it is (HIS, or an enemy's
// number).
pub const HIS: i32 = -1;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub c: V3,
    pub r: f32,
    pub side: i32,
}

// (a wizard's body's sphere is this big, its platform's and a shield's
// bigger: see enemies::spheres)
pub const BODY_R: f32 = 0.9;

impl Target {
    // Whether it is a wizard's body (not the platform under it, nor a
    // shield): what aiming aims at.
    pub fn body(&self) -> bool {
        self.r <= BODY_R
    }
}

// Where his aim meets sea level, within a strike's range (the probe finds
// the ground beneath).
pub fn aim_ground(origin: V3, dir: V3) -> V3 {
    let t = if dir[1] < -0.05 { STRIKE_RANGE.min(origin[1] / -dir[1]) } else { STRIKE_RANGE };
    [origin[0] + dir[0] * t, 0.0, origin[2] + dir[2] * t]
}

// The normal of a wind blade's plane, as it is drawn (spell_light in the
// shader's spells.rs: its travel and its span, rolled by its seed and age).
fn blade_normal(s: &Spell) -> V3 {
    use super::vec::{cross, normalise};
    use wizard_shader::hash::hash2;
    let seed = s.seed as i32;
    let rnd = |n: i32| hash2(seed.wrapping_add(n.wrapping_mul(7919)), 23);
    let f = normalise(s.vel);
    let side = normalise(super::vec::add(cross(f, [0.0, 1.0, 0.0]), [0.0, 0.001, 0.0]));
    let up = cross(side, f);
    let spin = if rnd(1) < 0.5 { -1.0 } else { 1.0 };
    let a = s.age;
    let roll = (rnd(2) - 0.5) * 1.2 + spin * (0.25 + 0.35 * rnd(3)) * a + 0.12 * (a * (5.0 + 4.0 * rnd(4)) + 6.283 * rnd(5)).sin();
    let span = super::vec::add(super::vec::scale(roll.cos(), side), super::vec::scale(roll.sin(), up));
    normalise(cross(f, span))
}

// His strike locking on: how long the cast has been held, and the target
// (an enemy's centre and number), if there is one in his aim; else it
// lands where he aims.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lock {
    pub age: f32,
    pub target: Option<V3>,
    pub side: i32,
    // no enemy in his aim: where it meets the ground, and the ground's
    // height there once the probe has found it
    pub ground: Option<(V3, Option<f32>)>,
}

// (the probe's id for a lock's ground point)
const LOCK_PROBE: u32 = u32::MAX;

// A wind blade's cut: a point of its plane and the plane's normal, and how
// fast the blade was going (all in the world).
pub type Cut = (V3, V3, V3);

// What happened, for the fight to deal with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpellEvent {
    // a spell of `kind` cast by `owner` hit `side` (a wind blade: cutting
    // along a plane, through a point with a normal, in the world), its
    // damage scaled by `scale` (a burst's, by its charge and how near its
    // heart)
    Hit { side: i32, kind: u32, owner: i32, cut: Option<Cut>, scale: f32 },
    // (for the sound: a bolt bursting, a blade breaking up, a strike landing)
    Sound { what: Sound, pos: V3 },
    // a soul reached him
    Soul,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Burst,
    Gust,
    Strike,
}

#[derive(Clone, Debug)]
struct Spell {
    id: u32,
    kind: u32,
    pos: V3,
    vel: V3,
    age: f32,
    life: f32, // seconds: it has faded out by then
    seed: f32, // varies each one's look
    top: f32,  // a strike's top height
    owner: i32,
    done: bool, // its blast (a burst's or a strike's) has been dealt
    power: f32, // a fire bolt's (and its burst's) charge, 0..1
    quiet: bool, // a wind blade that fades at the end of its flight (not bursting)
    trail: Vec<V3>, // a soul's path behind it, newest last (TAIL of it)
    absorb: f32,    // a soul reaching him: how far it has been taken in (0..1), or -1
    target: i32,    // an arcane missile's: whom it homes in on (a side), or NO_TARGET
    struck: u32,    // a frost lance's: those it has passed through (a bit a side)
    next_shot: f32, // his familiar's: seconds until it casts again
}

// (seconds a soul takes to be taken in, shrinking, its tail drawn after)
const ABSORB: f32 = 0.35;

// His wind blades held: they come faster as it spins up (spin 0..1), and
// each heats it; too hot, it overheats and will not fire until it has
// cooled (see combat.rs). `next`: seconds until it may fire again.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gatling {
    // seconds it has been held, warming up (none fly until WARMUP)
    pub warm: f32,
    pub spin: f32,
    pub heat: f32,
    pub overheated: bool,
    pub next: f32,
}

#[derive(Clone, Debug)]
pub struct Spells {
    pub selected: usize,
    // the spells he has (his skills unlock them: see the game)
    pub unlocked: [bool; N_SPELLS],
    pub limit: usize,
    pub staff: f32,      // how far the staff is held out (0..1)
    pub since_cast: f32, // seconds
    pub targets: Vec<Target>,
    pub events: Vec<SpellEvent>,
    // seconds he has been channelling healing (or -1), and where he is
    pub channel: f32,
    pub here: V3,
    // his strike locking on, while the cast is held
    pub lock: Option<Lock>,
    // seconds his fire bolt has been charging (or -1), and the orb, where
    // it charges
    pub charge: f32,
    pub orb: V3,
    pub gatling: Gatling,
    // free for all: the enemies' spells hit each other too
    pub ffa: bool,
    // his bursts' reach, and his lock-on's time, times (his skills)
    pub blast: f32,
    pub lock_time: f32,
    items: Vec<Spell>,
    // each spell's cooldown left (seconds)
    cooldowns: [f32; N_SPELLS],
    // how fast the cooldowns run (an ancient ward's: faster)
    pub cool_rate: f32,
    next_id: u32,
}

impl Default for Spells {
    fn default() -> Self {
        Spells {
            selected: 0, // the fire bolt: the one he starts with
            unlocked: [true; N_SPELLS],
            limit: 6,
            staff: 0.0,
            since_cast: 10.0,
            targets: Vec::new(),
            events: Vec::new(),
            channel: -1.0,
            here: [0.0; 3],
            lock: None,
            charge: -1.0,
            orb: [0.0; 3],
            gatling: Gatling::default(),
            ffa: false,
            blast: 1.0,
            lock_time: LOCK_TIME,
            items: Vec::new(),
            cooldowns: [0.0; N_SPELLS],
            cool_rate: 1.0,
            next_id: 1,
        }
    }
}

impl Spells {
    pub fn count(&self) -> usize {
        self.items.len()
    }

    // Takes spell i in hand, if he has it (see unlocked).
    pub fn select(&mut self, i: usize) {
        if i < N_SPELLS && self.unlocked[i] {
            self.selected = i;
        }
    }

    // Ends every spell in flight.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    // Whether he could cast spell `i` now (not cooling down, nor at the
    // limit).
    pub fn ready(&self, i: usize) -> bool {
        self.cooldowns[i] <= 0.0 && self.items.len() < self.limit
    }

    // How much of spell i's cooldown is left (1 just cast .. 0 ready).
    pub fn cooling(&self, i: usize) -> f32 {
        if COOLDOWN[i] > 0.0 { (self.cooldowns[i] / COOLDOWN[i]).clamp(0.0, 1.0) } else { 0.0 }
    }

    // Casts one of his gatling's wind blades from `origin` along the unit
    // vector `dir` (world space), unless at the limit: it flies briefly,
    // and fades rather than bursting at the end.
    pub fn cast_blade(&mut self, origin: V3, dir: V3, rng: &mut Rng) -> bool {
        if !self.launch(WIND, origin, dir, HIS, None, rng) {
            return false;
        }
        let s = self.items.last_mut().unwrap();
        s.life = GATLING_LIFE;
        s.quiet = true;
        self.since_cast = 0.0;
        true
    }

    // Casts his fire bolt, charged by `power` (0..1: faster, and its burst
    // bigger and harder), carried along by his own velocity `carry` too,
    // with its cooldown.
    pub fn cast_fire(&mut self, origin: V3, dir: V3, power: f32, carry: V3, rng: &mut Rng) -> bool {
        if !self.launch(FIRE, origin, dir, HIS, None, rng) {
            return false;
        }
        let s = self.items.last_mut().unwrap();
        s.power = power.clamp(0.0, 1.0);
        let k = 1.0 + 0.6 * s.power;
        s.vel = super::vec::add(s.vel.map(|v| v * k), carry);
        // (the renderer reads its charge from its seed: see pack)
        s.seed += 100000.0 * (100.0 * s.power).round();
        self.cooldowns[FIRE as usize] = COOLDOWN[FIRE as usize];
        self.since_cast = 0.0;
        true
    }

    // His frost lance from `origin` along `dir`, carried on by his own
    // velocity, with its cooldown.
    pub fn cast_frost(&mut self, origin: V3, dir: V3, carry: V3, rng: &mut Rng) -> bool {
        if !self.launch(FROST, origin, dir, HIS, None, rng) {
            return false;
        }
        let s = self.items.last_mut().unwrap();
        s.vel = super::vec::add(super::vec::scale(FROST_SPEED, dir), carry);
        self.cooled(FROST_LANCE);
        true
    }

    // His arcane missiles: three, fanned out from `origin` along `dir`,
    // homing in on `target` (a side) if there is one, with their cooldown.
    pub fn cast_missiles(&mut self, origin: V3, dir: V3, target: Option<i32>, rng: &mut Rng) -> bool {
        let side = super::vec::normalise([dir[2], 0.0, -dir[0]]);
        let mut any = false;
        for k in [-1.0f32, 0.0, 1.0] {
            let d = super::vec::normalise(super::vec::add(dir, super::vec::add(super::vec::scale(0.35 * k, side), [0.0, 0.25 * (1.0 - k.abs()), 0.0])));
            if self.launch(MISSILE, origin, d, HIS, None, rng) {
                let s = self.items.last_mut().unwrap();
                s.vel = super::vec::scale(MISSILE_SPEED, d);
                s.target = target.unwrap_or(NO_TARGET);
                any = true;
            }
        }
        if any {
            self.cooled(ARCANE);
        }
        any
    }

    // His meteor, called down on `at`: it falls from high above it, and
    // bursts there, as a fully charged bolt and more.
    pub fn cast_meteor(&mut self, at: V3, rng: &mut Rng) -> bool {
        let from = super::vec::add(at, [6.0, METEOR_HEIGHT, -4.0]);
        if !self.launch(FALLING, from, [0.0, -1.0, 0.0], HIS, None, rng) {
            return false;
        }
        let s = self.items.last_mut().unwrap();
        s.vel = super::vec::scale(1.0 / METEOR_FALL, sub(at, from));
        s.life = METEOR_FALL;
        s.power = 1.0;
        self.cooled(METEOR);
        true
    }

    // His gravity well, at `at`, with its cooldown.
    pub fn cast_well(&mut self, at: V3, rng: &mut Rng) -> bool {
        if !self.launch(WELL, at, [0.0, 1.0, 0.0], HIS, None, rng) {
            return false;
        }
        self.items.last_mut().unwrap().vel = [0.0; 3];
        self.cooled(GRAVITY);
        true
    }

    // His spirit familiar, summoned beside him (the one before, gone), with
    // its cooldown.
    pub fn summon(&mut self, rng: &mut Rng) -> bool {
        self.items.retain(|s| s.kind != SPIRIT);
        let at = super::vec::add(self.here, [SPIRIT_ORBIT, 2.4, 0.0]);
        if !self.launch(SPIRIT, at, [0.0, 1.0, 0.0], HIS, None, rng) {
            return false;
        }
        self.cooled(FAMILIAR);
        true
    }

    // Where his gravity wells are (the enemies are pulled to them).
    pub fn wells(&self) -> Vec<V3> {
        self.items.iter().filter(|s| s.kind == WELL && s.age > 0.2).map(|s| s.pos).collect()
    }

    // The enemy nearest his aim (within `cone` radians of it, in reach): its
    // side and centre.
    pub fn aimed(&self, origin: V3, dir: V3, cone: f32) -> Option<(i32, V3)> {
        self.targets
            .iter()
            .filter(|t| t.side >= 0 && t.body() && dist(t.c, origin) < STRIKE_RANGE * 1.5)
            .map(|t| {
                let v = sub(t.c, origin);
                ((dot(v, dir) / len(v)).clamp(-1.0, 1.0).acos(), t)
            })
            .filter(|(a, _)| *a < cone)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, t)| (t.side, t.c))
    }

    fn cooled(&mut self, spell: usize) {
        self.cooldowns[spell] = COOLDOWN[spell];
        self.since_cast = 0.0;
    }

    // Casts his strike at `at` (a locked target), or where he aims if none,
    // unless at the limit; with its cooldown.
    pub fn cast_at(&mut self, origin: V3, dir: V3, at: Option<V3>, rng: &mut Rng) -> bool {
        if !self.launch(STRIKE, origin, dir, HIS, at, rng) {
            return false;
        }
        self.cooldowns[STRIKE as usize] = COOLDOWN[STRIKE as usize];
        self.since_cast = 0.0;
        true
    }

    // Casts a spell of `kind` for `owner` (no cooldown), unless at the limit.
    // A strike lands at `at` if given; his lock on to a target near his aim,
    // or else land where the view meets the ground.
    pub fn launch(&mut self, kind: u32, origin: V3, dir: V3, owner: i32, at: Option<V3>, rng: &mut Rng) -> bool {
        if self.items.len() >= self.limit {
            return false;
        }
        let mut s = Spell {
            id: self.next_id,
            kind,
            pos: origin,
            vel: [0.0; 3],
            age: 0.0,
            life: LIFE[kind as usize],
            seed: (rng.uniform() * 1e5).floor(),
            top: 0.0,
            owner,
            done: false,
            power: 0.0,
            quiet: false,
            trail: Vec::new(),
            absorb: -1.0,
            target: NO_TARGET,
            struck: 0,
            next_shot: 0.5,
        };
        self.next_id += 1;
        if kind == STRIKE || kind == MARK {
            let target = at.or_else(|| if owner == HIS { self.lock_on(origin, dir) } else { None });
            if let Some(t) = target {
                s.pos = t;
            } else {
                // where the view meets sea level, within range; the probe then
                // finds the actual ground beneath
                s.pos = aim_ground(origin, dir);
            }
            s.top = origin[1].max(s.pos[1]) + STRIKE_HEIGHT;
        } else {
            let v = SPEED.get(kind as usize).copied().unwrap_or(0.0);
            s.vel = [dir[0] * v, dir[1] * v, dir[2] * v];
        }
        if kind == STRIKE && s.pos[1] != 0.0 {
            self.events.push(SpellEvent::Sound { what: Sound::Strike, pos: s.pos });
        }
        self.items.push(s);
        true
    }

    // An enemy's telegraph (CHARGE or WINDUP), at `at` for `secs`: its id
    // (to move it with the enemy's orb, and end it), unless at the limit.
    pub fn telegraph(&mut self, kind: u32, at: V3, secs: f32, owner: i32, rng: &mut Rng) -> Option<u32> {
        if !self.launch(kind, at, [0.0, 1.0, 0.0], owner, None, rng) {
            return None;
        }
        let s = self.items.last_mut().unwrap();
        s.life = secs;
        // (drawn where it is, not at his orb)
        s.vel = [1.0, 0.0, 0.0];
        Some(s.id)
    }

    // The last spell launched, charged by `power` (an enemy's fire bolt:
    // faster, and its burst bigger and harder).
    pub fn empower_last(&mut self, power: f32) {
        if let Some(s) = self.items.last_mut() {
            s.power = power.clamp(0.0, 1.0);
            let k = 1.0 + 0.6 * s.power;
            s.vel = s.vel.map(|v| v * k);
            s.seed += 100000.0 * (100.0 * s.power).round();
        }
    }

    // Moves spell `id` to `at` (a telegraph following its enemy's orb).
    pub fn place(&mut self, id: u32, at: V3) {
        if let Some(s) = self.items.iter_mut().find(|s| s.id == id) {
            s.pos = at;
        }
    }

    // Ends spell `id`.
    pub fn end(&mut self, id: u32) {
        self.items.retain(|s| s.id != id);
    }

    // A defeated enemy's soul, rising from `at` and then flying to him.
    pub fn soul(&mut self, at: V3) {
        let rng = &mut Rng::new(at[0].to_bits() as u64 ^ ((at[2].to_bits() as u64) << 32) ^ self.next_id as u64);
        if self.launch(WISP, at, [0.0, 1.0, 0.0], HIS, None, rng) {
            let s = self.items.last_mut().unwrap();
            s.vel = [2.0 * (rng.uniform() - 0.5), 4.0, 2.0 * (rng.uniform() - 0.5)];
        }
    }

    // The bolts and blades in flight that could hit `side` (where, and how
    // fast), for it to dodge.
    pub fn missiles_at(&self, side: i32) -> Vec<(V3, V3)> {
        let ffa = self.ffa;
        let t = Target { c: [0.0; 3], r: 0.0, side };
        self.items.iter().filter(|s| (s.kind == FIRE || s.kind == WIND) && Self::opposed(ffa, s.owner, &t)).map(|s| (s.pos, s.vel)).collect()
    }

    // The enemy target nearest his aim, within LOCK_ON of it and in range.
    pub fn lock_on(&self, origin: V3, dir: V3) -> Option<V3> {
        self.lock_target(origin, dir).map(|(c, _)| c)
    }

    // The same, and whose it is.
    pub fn lock_target(&self, origin: V3, dir: V3) -> Option<(V3, i32)> {
        let mut best = None;
        let mut best_angle = LOCK_ON;
        for t in &self.targets {
            if t.side == HIS {
                continue;
            }
            let v = sub(t.c, origin);
            let d = len(v);
            if d > STRIKE_RANGE * 1.5 {
                continue;
            }
            let angle = (dot(v, dir) / d).min(1.0).acos() - (t.r / d).atan();
            if angle < best_angle {
                best_angle = angle;
                best = Some((t.c, t.side));
            }
        }
        best
    }

    // A fire bolt bursting where it is.
    fn burst(s: &mut Spell, events: &mut Vec<SpellEvent>) {
        s.kind = BURST;
        s.age = 0.0;
        s.life = LIFE[BURST as usize];
        s.vel = [0.0; 3];
        events.push(SpellEvent::Sound { what: Sound::Burst, pos: s.pos });
    }

    // A wind blade bursting into small blades where it is (see spell_gust in
    // fut/spells.fut): it keeps its velocity, which aims the shards.
    fn gust(s: &mut Spell, events: &mut Vec<SpellEvent>) {
        s.kind = GUST;
        s.age = 0.0;
        s.life = LIFE[GUST as usize];
        events.push(SpellEvent::Sound { what: Sound::Gust, pos: s.pos });
    }

    // Whether a spell of `owner`'s can hit target t: his theirs, and
    // theirs his; free for all (ffa), anyone's anyone else's.
    fn opposed(ffa: bool, owner: i32, t: &Target) -> bool {
        if ffa { owner != t.side } else { (owner == HIS) != (t.side == HIS) }
    }

    // Bolts and blades that fly into a target; bursts' and strikes' blasts.
    fn collide(&mut self) {
        let blast = self.blast;
        let ffa = self.ffa;
        let targets = &self.targets;
        let events = &mut self.events;
        let mut chains: Vec<(V3, V3)> = Vec::new();
        for s in &mut self.items {
            // a frost lance: through each it meets, slowing it (see
            // enemies.rs), on until its time is up
            if s.kind == FROST {
                for t in targets {
                    let bit = 1u32 << (t.side + 1).clamp(0, 31);
                    if Self::opposed(ffa, s.owner, t) && dist(s.pos, t.c) < t.r && s.struck & bit == 0 {
                        s.struck |= bit;
                        events.push(SpellEvent::Hit { side: t.side, kind: FROST, owner: s.owner, cut: None, scale: 1.0 });
                    }
                }
                continue;
            }
            // an arcane missile: gone into what it meets
            if s.kind == MISSILE {
                if let Some(t) = targets.iter().find(|t| Self::opposed(ffa, s.owner, t) && dist(s.pos, t.c) < t.r) {
                    events.push(SpellEvent::Hit { side: t.side, kind: MISSILE, owner: s.owner, cut: None, scale: 1.0 });
                    s.age = s.life;
                }
                continue;
            }
            if s.kind == FIRE || s.kind == WIND {
                let Some(t) = targets.iter().find(|t| Self::opposed(ffa, s.owner, t) && dist(s.pos, t.c) < t.r) else {
                    continue;
                };
                let cut = if s.kind == WIND { Some((s.pos, blade_normal(s), s.vel)) } else { None };
                events.push(SpellEvent::Hit { side: t.side, kind: s.kind, owner: s.owner, cut, scale: 1.0 });
                if s.kind == FIRE {
                    Self::burst(s, events);
                } else {
                    Self::gust(s, events);
                }
            } else if (s.kind == BURST || (s.kind == STRIKE && s.pos[1] != 0.0)) && !s.done {
                // (once per target: by its sphere nearest the heart; a burst's
                // damage by its charge, and down to half at the edge of its
                // reach)
                s.done = true;
                let reach = if s.kind == BURST { BLAST * (1.0 + s.power) * if s.owner == HIS { blast } else { 1.0 } } else { STRIKE_REACH };
                let mut hits: Vec<(i32, f32)> = Vec::new();
                for t in targets {
                    let d = (dist(s.pos, t.c) - t.r).max(0.0);
                    if !Self::opposed(ffa, s.owner, t) || d >= reach {
                        continue;
                    }
                    let scale = if s.kind == BURST { (1.0 + 2.0 * s.power) * (1.0 - 0.5 * d / reach) } else { 1.0 };
                    match hits.iter_mut().find(|(side, _)| *side == t.side) {
                        Some(h) => h.1 = h.1.max(scale),
                        None => hits.push((t.side, scale)),
                    }
                }
                // (his strike, having hit an enemy: chained on from it)
                if s.kind == STRIKE && s.owner == HIS && !hits.is_empty() {
                    let mut from = s.pos;
                    let mut struck: Vec<i32> = hits.iter().map(|h| h.0).collect();
                    for _ in 0..CHAIN_JUMPS {
                        let next = targets
                            .iter()
                            .filter(|t| t.side >= 0 && t.body() && !struck.contains(&t.side) && dist(t.c, from) < CHAIN_RANGE)
                            .min_by(|a, b| dist(a.c, from).total_cmp(&dist(b.c, from)));
                        let Some(t) = next else { break };
                        events.push(SpellEvent::Hit { side: t.side, kind: STRIKE, owner: HIS, cut: None, scale: CHAIN_DAMAGE });
                        chains.push((super::vec::add(from, [0.0, 1.0, 0.0]), t.c));
                        struck.push(t.side);
                        from = t.c;
                    }
                }
                for (side, scale) in hits {
                    events.push(SpellEvent::Hit { side, kind: s.kind, owner: s.owner, cut: None, scale });
                }
            }
        }
        // (the chains' arcs, to be seen)
        let mut rng = Rng::new(self.next_id as u64 * 7919);
        for (a, b) in chains {
            if self.launch(CHAIN, a, [0.0, 1.0, 0.0], HIS, None, &mut rng) {
                let s = self.items.last_mut().unwrap();
                s.vel = b;
                s.life = 0.35;
            }
        }
    }

    // The enemies' spells, gone (time rewound from under them).
    pub fn clear_hostile(&mut self) {
        let gone: Vec<u32> = self.items.iter().filter(|s| s.owner != HIS).map(|s| s.id).collect();
        for id in gone {
            self.end(id);
        }
    }

    // `holding`: the cast button is down. The staff comes out quickly while
    // casting and stays out between repeated casts, then lowers slowly.
    pub fn update(&mut self, dt: f32, holding: bool) {
        for c in &mut self.cooldowns {
            *c = (*c - dt * self.cool_rate).max(0.0);
        }
        self.since_cast += dt;
        let out = holding || self.since_cast < 0.7;
        let goal = if out { 1.0 } else { 0.0 };
        self.staff += (goal - self.staff) * (1.0 - (-dt * if out { 12.0 } else { 2.5 }).exp());
        let wells = self.wells();
        let mut shots: Vec<(V3, V3, i32)> = Vec::new();
        let (here, targets) = (self.here, &self.targets);
        for s in &mut self.items {
            s.age += dt;
            // (an arcane missile turns towards its target, if it still stands)
            if s.kind == MISSILE {
                if let Some(t) = targets.iter().filter(|t| t.side == s.target && t.body()).min_by(|a, b| dist(a.c, s.pos).total_cmp(&dist(b.c, s.pos))) {
                    let want = super::vec::scale(MISSILE_SPEED / dist(t.c, s.pos).max(1e-3), sub(t.c, s.pos));
                    let k = 1.0 - (-dt * MISSILE_TURN).exp();
                    for j in 0..3 {
                        s.vel[j] += (want[j] - s.vel[j]) * k;
                    }
                }
            }
            // (an enemy's bolt or blade near his gravity well, drawn in)
            if s.owner != HIS && (s.kind == FIRE || s.kind == WIND) {
                for w in &wells {
                    let to = sub(*w, s.pos);
                    let d = len(to);
                    if d < 0.5 * WELL_REACH && d > 0.1 {
                        let pull = WELL_PULL * dt * (1.0 - d / (0.5 * WELL_REACH)) / d;
                        for j in 0..3 {
                            s.vel[j] += to[j] * pull;
                        }
                    }
                }
            }
            // (his familiar circles him, and casts at the nearest enemy in
            // reach every so often)
            if s.kind == SPIRIT {
                let a = SPIRIT_TURN * s.age;
                let at = super::vec::add(here, [SPIRIT_ORBIT * a.cos(), 2.4 + 0.2 * (3.0 * a).sin(), SPIRIT_ORBIT * a.sin()]);
                s.vel = super::vec::scale(1.0 / dt.max(1e-4), sub(at, s.pos));
                s.pos = at;
                s.next_shot -= dt;
                if s.next_shot <= 0.0 {
                    s.next_shot = SPIRIT_PACE;
                    let near = targets.iter().filter(|t| t.side >= 0 && t.body() && dist(t.c, at) < SPIRIT_REACH).min_by(|a, b| dist(a.c, at).total_cmp(&dist(b.c, at)));
                    if let Some(t) = near {
                        shots.push((at, super::vec::normalise(sub(t.c, at)), t.side));
                    }
                }
                continue;
            }
            // (a meteor, landed: it bursts, a fully charged bolt's burst)
            if s.kind == FALLING && s.age >= s.life {
                s.pos = super::vec::add(s.pos, super::vec::scale(dt, s.vel));
                Self::burst(s, &mut self.events);
                s.power = 1.0;
                continue;
            }
            // (a soul flies to him, faster and faster, turning towards him;
            // reaching him, it is taken in: drawn into him, shrinking, its
            // tail after it; then gone)
            if s.kind == WISP {
                let heart = super::vec::add(self.here, [0.0, 1.3, 0.0]);
                let to = sub(heart, s.pos);
                let d = len(to);
                if s.absorb < 0.0 && d < 0.6 {
                    s.absorb = 0.0;
                }
                if s.absorb >= 0.0 {
                    s.absorb += dt / ABSORB;
                    let k = 1.0 - (-dt * 12.0).exp();
                    for j in 0..3 {
                        s.pos[j] += to[j] * k;
                    }
                    if s.absorb >= 1.0 {
                        s.age = s.life;
                        self.events.push(SpellEvent::Soul);
                    }
                    continue;
                }
                let speed = 4.0 + 14.0 * s.age.min(1.5);
                let want = super::vec::scale(speed / d, to);
                let k = 1.0 - (-dt * 3.0).exp();
                for j in 0..3 {
                    s.vel[j] += (want[j] - s.vel[j]) * k;
                }
            }
            // (telegraphs stay where they are put; a gust stays where the
            // blade broke: its velocity only aims the shards; a chain's is
            // where it ends)
            if s.kind != GUST && s.kind != CHARGE && s.kind != WINDUP && s.kind != CHAIN {
                for i in 0..3 {
                    s.pos[i] += s.vel[i] * dt;
                }
            }
            // (a soul's path, as its tail: a point every TAIL_STEP, as much
            // as TAIL of it)
            if s.kind == WISP && s.trail.last().is_none_or(|&l| dist(l, s.pos) >= TAIL_STEP) {
                s.trail.push(s.pos);
                let mut length = 0.0;
                let mut keep = 0;
                for k in (1..s.trail.len()).rev() {
                    length += dist(s.trail[k], s.trail[k - 1]);
                    if length > TAIL {
                        keep = k - 1;
                        break;
                    }
                }
                s.trail.drain(..keep);
            }
            // a wind blade that flew its course bursts too (a gatling's
            // fades)
            if s.kind == WIND && !s.quiet && s.age >= s.life {
                Self::gust(s, &mut self.events);
            }
            // a mark's time is up: the strike comes down
            if s.kind == MARK && s.age >= s.life {
                s.kind = STRIKE;
                s.age = 0.0;
                s.life = LIFE[STRIKE as usize];
                s.done = false;
                self.events.push(SpellEvent::Sound { what: Sound::Strike, pos: s.pos });
            }
        }
        // (his familiar's missiles)
        let mut rng = Rng::new(self.next_id as u64 * 104729);
        for (at, dir, side) in shots {
            if self.launch(MISSILE, at, dir, HIS, None, &mut rng) {
                let s = self.items.last_mut().unwrap();
                s.vel = super::vec::scale(MISSILE_SPEED, dir);
                s.target = side;
            }
        }
        self.collide();
        self.items.retain(|s| s.age < s.life);
    }

    // The points to probe the land at, `limit` of them (unused ones far above
    // everything), and which spell each is.
    pub fn probe_points(&self) -> (Vec<u32>, Vec<f32>) {
        let mut pts = vec![0.0; self.limit * 3];
        let mut ids = Vec::new();
        for (i, s) in self.items.iter().take(self.limit).enumerate() {
            ids.push(s.id);
            let p = if s.kind == STRIKE { [s.pos[0], PROBE_HEIGHT, s.pos[2]] } else { s.pos };
            pts[i * 3..i * 3 + 3].copy_from_slice(&p);
        }
        for i in ids.len()..self.limit {
            pts[i * 3 + 1] = 1e5;
        }
        // (a lock on the ground: the ground there, in a free slot)
        if let Some(Lock { ground: Some((g, _)), .. }) = self.lock {
            let i = ids.len();
            if i < self.limit {
                ids.push(LOCK_PROBE);
                pts[i * 3..i * 3 + 3].copy_from_slice(&[g[0], PROBE_HEIGHT, g[2]]);
            }
        }
        (ids, pts)
    }

    // Applies the probe's answers (per point: distance to the land, normal).
    pub fn apply_probe(&mut self, ids: &[u32], data: &[f32]) {
        for (i, &id) in ids.iter().enumerate() {
            let Some(&d) = data.get(i * 4) else { break };
            if id == LOCK_PROBE {
                if let (Some(Lock { ground: Some((_, y)), .. }), true) = (&mut self.lock, d < 1e5) {
                    let ny = data.get(i * 4 + 2).copied().unwrap_or(1.0);
                    *y = Some(PROBE_HEIGHT - d / ny.max(0.2));
                }
                continue;
            }
            let Some(s) = self.items.iter_mut().find(|x| x.id == id) else { continue };
            if !(d < 1e5) {
                continue;
            }
            // (a frost lance or an arcane missile meeting the land: gone)
            if (s.kind == FROST || s.kind == MISSILE) && d < 0.15 {
                s.age = s.life;
            } else if s.kind == FIRE && d < 0.15 {
                // burst where it hit
                Self::burst(s, &mut self.events);
            } else if s.kind == WIND && d < 0.15 {
                // it bursts against the land into small blades
                Self::gust(s, &mut self.events);
            } else if s.kind == STRIKE && s.pos[1] == 0.0 {
                // the ground below the probe point: its height above the land,
                // divided by the slope
                let ny = data.get(i * 4 + 2).copied().unwrap_or(1.0);
                s.pos[1] = PROBE_HEIGHT - d / ny.max(0.2);
                self.events.push(SpellEvent::Sound { what: Sound::Strike, pos: s.pos });
            }
        }
    }

    // The renderer's array: `limit` slots of
    // [kind, x, y, z, a, b, c, age, life, seed] (see the shader's
    // spells.rs: a fire bolt's seed carries its charge, in hundredths, in
    // its hundred-thousands; a charge is drawn at the orb as the renderer
    // poses it, not at its x, y, z).
    pub fn pack(&self) -> Vec<f32> {
        let mut out = vec![-1.0; self.limit * STRIDE];
        let mut tails = Vec::new();
        for (i, s) in self.items.iter().take(self.limit).enumerate() {
            let extra = if s.kind == STRIKE || s.kind == MARK {
                [0.0, s.top, 0.0]
            } else if s.kind == BURST {
                [s.power, 0.0, 0.0]
            } else if s.kind == WISP {
                // (a soul: its head, and the first of three points along its
                // tail; the rest as two more pieces, in free slots; taken in,
                // all of it shrinking, the tail drawn in from its end)
                let k = 1.0 - s.absorb.clamp(0.0, 1.0);
                let [a, b, c] = tail_points(&s.trail, s.pos, k);
                tails.push([WISP_TAIL as f32, a[0], a[1], a[2], b[0], b[1], b[2], 0.03 * k, 0.014 * k, s.seed]);
                tails.push([WISP_TAIL as f32, b[0], b[1], b[2], c[0], c[1], c[2], 0.014 * k, 0.003 * k, s.seed]);
                a
            } else {
                s.vel
            };
            // (a soul's age: how far it has been taken in, of 1)
            let (age, life) = if s.kind == WISP { (s.absorb.max(0.0), 1.0) } else { (s.age, s.life) };
            let slot = [s.kind as f32, s.pos[0], s.pos[1], s.pos[2], extra[0], extra[1], extra[2], age, life, s.seed];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        // (the souls' tails, in the free slots there are)
        let mut free = self.items.len();
        for t in tails {
            if free < self.limit {
                out[free * STRIDE..(free + 1) * STRIDE].copy_from_slice(&t);
                free += 1;
            }
        }
        let free = free.min(self.limit.max(1) - 1);
        // (locking on, the ring round the target: likewise)
        if let (Some(l), true) = (&self.lock, self.limit > 0) {
            let at = l.target.or(l.ground.and_then(|(g, y)| y.map(|y| [g[0], y, g[2]])));
            if let Some(p) = at {
                let i = free;
                let slot = [LOCK as f32, p[0], p[1], p[2], 0.0, p[1] + STRIKE_HEIGHT, 0.0, l.age, self.lock_time, 11.0];
                out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
            }
        }
        // (his wind blades warming up, at the orb: likewise)
        let g = self.gatling;
        if g.warm > 0.0 && g.warm < WARMUP && self.limit > 0 {
            let i = free;
            let [x, y, z] = self.orb;
            let slot = [WINDUP as f32, x, y, z, 0.0, 0.0, 0.0, g.warm, WARMUP, 3.0];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        // (charging, the fire bolt at the orb: likewise)
        if self.charge >= 0.0 && self.limit > 0 {
            let i = free;
            let [x, y, z] = self.orb;
            let slot = [CHARGE as f32, x, y, z, 0.0, 0.0, 0.0, self.charge, CHARGE_TIME, 5.0];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        // (channelling, the healing round him: in the first free slot, or
        // the last)
        if self.channel >= 0.0 && self.limit > 0 {
            let i = free;
            let [x, y, z] = self.here;
            let slot = [HEAL as f32, x, y, z, 0.0, 0.0, 0.0, self.channel, LIFE[HEAL as usize], 7.0];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(s: &Spells) -> Vec<(i32, u32)> {
        s.events
            .iter()
            .filter_map(|e| match *e {
                SpellEvent::Hit { side, kind, .. } => Some((side, kind)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn bolt_hits_and_bursts() {
        let mut rng = Rng::new(1);
        let mut s = Spells { selected: 0, ..Default::default() };
        s.targets = vec![Target { c: [0.0, 0.0, 10.0], r: 0.9, side: 0 }, Target { c: [0.0, 0.0, 9.0], r: 1.4, side: 0 }];
        assert!(s.cast_fire([0.0; 3], [0.0, 0.0, 1.0], 0.0, [0.0; 3], &mut rng));
        assert!(!s.ready(FIRE as usize)); // cooling down
        for _ in 0..20 {
            s.update(1.0 / 60.0, false);
        }
        // the bolt hit (no damage itself), then its burst hit once per side
        assert_eq!(hits(&s), vec![(0, FIRE), (0, BURST)]);
        let p = s.pack();
        assert_eq!(p[0], BURST as f32);
        assert_eq!(p[STRIDE], -1.0);
        assert_eq!(p.len(), 6 * STRIDE);
    }

    #[test]
    fn a_charged_burst_reaches_further_and_harder() {
        // a target 4 beyond a burst's heart: out of an uncharged one's
        // reach, in a fully charged one's (and harder near its heart)
        for (power, far, near) in [(0.0, None, 1.0), (1.0, Some(2.0), 3.0)] {
            let mut s = Spells::default();
            s.targets = vec![Target { c: [4.5, 0.0, 0.0], r: 0.5, side: 0 }, Target { c: [0.0, 0.5, 0.0], r: 0.5, side: 1 }];
            let mut rng = Rng::new(1);
            s.cast_fire([0.0; 3], [0.0, 0.0, 1.0], power, [0.0; 3], &mut rng);
            Spells::burst(&mut s.items[0], &mut Vec::new());
            s.items[0].pos = [0.0; 3];
            s.update(0.01, false);
            let scale = |side: i32| {
                s.events.iter().find_map(|e| match e {
                    SpellEvent::Hit { side: k, scale, .. } if *k == side => Some(*scale),
                    _ => None,
                })
            };
            assert_eq!(scale(1), Some(near), "{power}");
            match (far, scale(0)) {
                (None, got) => assert_eq!(got, None),
                (Some(want), Some(got)) => assert!((got - want).abs() < 0.3, "{got}"),
                (Some(_), None) => panic!("missed"),
            }
        }
    }

    #[test]
    fn strike_finds_the_ground() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        let dir = super::super::vec::normalise([0.0, -1.0, 1.0]);
        assert!(s.cast_at([0.0, 10.0, 0.0], dir, None, &mut rng));
        let (ids, pts) = s.probe_points();
        assert_eq!(ids.len(), 1);
        assert!((pts[0]).abs() < 1e-4 && pts[1] == PROBE_HEIGHT && (pts[2] - 10.0).abs() < 1e-4);
        assert_eq!(pts[4], 1e5);
        assert!(s.events.is_empty());
        s.apply_probe(&ids, &[58.0, 0.0, 1.0, 0.0]);
        assert!(matches!(s.events[..], [SpellEvent::Sound { what: Sound::Strike, pos }] if pos[1] == 2.0));
        assert_eq!(s.items[0].pos[1], 2.0);
    }

    #[test]
    fn lock_on_and_limit() {
        let mut rng = Rng::new(3);
        let mut s = Spells { limit: 2, ..Default::default() };
        s.targets = vec![Target { c: [0.5, 0.0, 20.0], r: 0.9, side: 2 }, Target { c: [0.0, 0.0, 5.0], r: 0.9, side: HIS }];
        assert_eq!(s.lock_on([0.0; 3], [0.0, 0.0, 1.0]), Some([0.5, 0.0, 20.0]));
        assert_eq!(s.lock_on([0.0; 3], [1.0, 0.0, 0.0]), None);
        assert!(s.launch(FIRE, [0.0; 3], [1.0, 0.0, 0.0], 0, None, &mut rng));
        assert!(s.launch(FIRE, [0.0; 3], [1.0, 0.0, 0.0], 0, None, &mut rng));
        assert!(!s.launch(FIRE, [0.0; 3], [1.0, 0.0, 0.0], 0, None, &mut rng));
        assert!(!s.ready(FIRE as usize));
    }

    #[test]
    fn mark_turns_into_strike() {
        let mut rng = Rng::new(3);
        let mut s = Spells::default();
        assert!(s.launch(MARK, [0.0; 3], [0.0, 1.0, 0.0], 0, Some([1.0, 2.0, 3.0]), &mut rng));
        s.targets = vec![Target { c: [1.0, 3.0, 3.0], r: 0.9, side: HIS }];
        for _ in 0..12 {
            s.update(0.1, false);
        }
        assert_eq!(s.pack()[0], STRIKE as f32);
        assert!(s.events.contains(&SpellEvent::Sound { what: Sound::Strike, pos: [1.0, 2.0, 3.0] }));
        assert_eq!(hits(&s), vec![(HIS, STRIKE)]);
    }

    #[test]
    fn frost_pierces_one_after_another() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        s.targets = vec![Target { c: [0.0, 0.0, 6.0], r: 0.9, side: 0 }, Target { c: [0.0, 0.0, 12.0], r: 0.9, side: 1 }];
        assert!(s.cast_frost([0.0; 3], [0.0, 0.0, 1.0], [0.0; 3], &mut rng));
        assert!(!s.ready(FROST_LANCE));
        for _ in 0..30 {
            s.update(1.0 / 60.0, false);
        }
        assert_eq!(hits(&s), vec![(0, FROST), (1, FROST)]);
    }

    #[test]
    fn missiles_home_in() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        // (off to the side of where they are cast)
        s.targets = vec![Target { c: [6.0, 0.0, 8.0], r: 0.9, side: 2 }];
        assert!(s.cast_missiles([0.0; 3], [0.0, 0.0, 1.0], Some(2), &mut rng));
        for _ in 0..120 {
            s.update(1.0 / 60.0, false);
        }
        assert_eq!(hits(&s), vec![(2, MISSILE); 3]);
    }

    #[test]
    fn a_meteor_falls_and_bursts() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        s.targets = vec![Target { c: [1.0, 0.5, 0.0], r: 0.9, side: 0 }];
        assert!(s.cast_meteor([0.0; 3], &mut rng));
        for _ in 0..90 {
            s.update(1.0 / 60.0, false);
        }
        assert!(hits(&s).contains(&(0, BURST)), "{:?}", hits(&s));
    }

    #[test]
    fn the_familiar_shoots() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        s.targets = vec![Target { c: [0.0, 2.0, 10.0], r: 0.9, side: 0 }];
        assert!(s.summon(&mut rng));
        for _ in 0..300 {
            s.update(1.0 / 60.0, false);
        }
        assert!(!hits(&s).is_empty());
    }

    #[test]
    fn a_well_is_where_it_was_cast() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        assert!(s.cast_well([3.0, 0.0, 4.0], &mut rng));
        assert!(s.wells().is_empty());
        s.update(0.5, false);
        assert_eq!(s.wells(), vec![[3.0, 0.0, 4.0]]);
    }
}

// Three points along a soul's tail (its path behind `head`, oldest first),
// a third, two thirds and all the way along the first `keep` of it: all of
// it the renderer is sent.
fn tail_points(trail: &[V3], head: V3, keep: f32) -> [V3; 3] {
    let mut pts: Vec<V3> = trail.iter().rev().copied().collect();
    pts.insert(0, head);
    let total = keep * pts.windows(2).map(|w| dist(w[0], w[1])).sum::<f32>();
    let at = |f: f32| {
        let want = f * total;
        let mut gone = 0.0;
        for w in pts.windows(2) {
            let d = dist(w[0], w[1]);
            if gone + d >= want && d > 0.0 {
                let t = (want - gone) / d;
                return [0, 1, 2].map(|k| w[0][k] + (w[1][k] - w[0][k]) * t);
            }
            gone += d;
        }
        *pts.last().unwrap()
    };
    [at(1.0 / 3.0), at(2.0 / 3.0), at(1.0)]
}
