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

pub const SPELL_NAMES: [&str; 4] = ["Fire bolt", "Lightning strike", "Wind blade", "Healing"];
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
pub const STRIDE: usize = 10;

const LIFE: [f32; 11] = [3.0, 0.9, 1.5, 1.4, 1.1, 1.1, 1e3, LOCK_TIME, 1.0, 0.5, 12.0]; // seconds, by kind
pub const WARMUP: f32 = 0.5; // seconds wind blades warm up before the first flies
pub const LOCK_TIME: f32 = 0.6; // seconds his strike takes to lock on (cast held)
const SPEED: [f32; 4] = [30.0, 0.0, 24.0, 0.0]; // units/s
const COOLDOWN: [f32; 4] = [0.9, 2.0, 0.0, 0.0]; // seconds before the spell's next cast (wind: see Gatling)
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

// What happened, for the fight to deal with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpellEvent {
    // a spell of `kind` cast by `owner` hit `side` (a wind blade: cutting
    // along a plane, through a point with a normal, in the world), its
    // damage scaled by `scale` (a burst's, by its charge and how near its
    // heart)
    Hit { side: i32, kind: u32, owner: i32, cut: Option<(V3, V3)>, scale: f32 },
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
}

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
    items: Vec<Spell>,
    // each spell's cooldown left (seconds)
    cooldowns: [f32; 4],
    next_id: u32,
}

impl Default for Spells {
    fn default() -> Self {
        Spells {
            selected: 1, // lightning: the orb's own colours
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
            items: Vec::new(),
            cooldowns: [0.0; 4],
            next_id: 1,
        }
    }
}

impl Spells {
    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn select(&mut self, i: usize) {
        if i < SPELL_NAMES.len() {
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

    // His bolts and blades in flight (where, and how fast), for the
    // enemies to dodge.
    pub fn his_missiles(&self) -> Vec<(V3, V3)> {
        self.items.iter().filter(|s| s.owner == HIS && (s.kind == FIRE || s.kind == WIND)).map(|s| (s.pos, s.vel)).collect()
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

    // Whether a spell of `owner`'s can hit target t.
    fn opposed(owner: i32, t: &Target) -> bool {
        (owner == HIS) != (t.side == HIS)
    }

    // Bolts and blades that fly into a target; bursts' and strikes' blasts.
    fn collide(&mut self) {
        let targets = &self.targets;
        let events = &mut self.events;
        for s in &mut self.items {
            if s.kind == FIRE || s.kind == WIND {
                let Some(t) = targets.iter().find(|t| Self::opposed(s.owner, t) && dist(s.pos, t.c) < t.r) else {
                    continue;
                };
                let cut = if s.kind == WIND { Some((s.pos, blade_normal(s))) } else { None };
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
                let reach = if s.kind == BURST { BLAST * (1.0 + s.power) } else { STRIKE_REACH };
                let mut hits: Vec<(i32, f32)> = Vec::new();
                for t in targets {
                    let d = (dist(s.pos, t.c) - t.r).max(0.0);
                    if !Self::opposed(s.owner, t) || d >= reach {
                        continue;
                    }
                    let scale = if s.kind == BURST { (1.0 + 2.0 * s.power) * (1.0 - 0.5 * d / reach) } else { 1.0 };
                    match hits.iter_mut().find(|(side, _)| *side == t.side) {
                        Some(h) => h.1 = h.1.max(scale),
                        None => hits.push((t.side, scale)),
                    }
                }
                for (side, scale) in hits {
                    events.push(SpellEvent::Hit { side, kind: s.kind, owner: s.owner, cut: None, scale });
                }
            }
        }
    }

    // `holding`: the cast button is down. The staff comes out quickly while
    // casting and stays out between repeated casts, then lowers slowly.
    pub fn update(&mut self, dt: f32, holding: bool) {
        for c in &mut self.cooldowns {
            *c = (*c - dt).max(0.0);
        }
        self.since_cast += dt;
        let out = holding || self.since_cast < 0.7;
        let goal = if out { 1.0 } else { 0.0 };
        self.staff += (goal - self.staff) * (1.0 - (-dt * if out { 12.0 } else { 2.5 }).exp());
        for s in &mut self.items {
            s.age += dt;
            // (a soul flies to him, faster and faster, turning towards him;
            // reaching him, it is gone)
            if s.kind == WISP {
                let to = sub(super::vec::add(self.here, [0.0, 1.3, 0.0]), s.pos);
                let d = len(to);
                if d < 0.6 {
                    s.age = s.life;
                    self.events.push(SpellEvent::Soul);
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
            // blade broke: its velocity only aims the shards)
            if s.kind != GUST && s.kind != CHARGE && s.kind != WINDUP {
                for i in 0..3 {
                    s.pos[i] += s.vel[i] * dt;
                }
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
            if s.kind == FIRE && d < 0.15 {
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
        for (i, s) in self.items.iter().take(self.limit).enumerate() {
            let extra = if s.kind == STRIKE || s.kind == MARK {
                [0.0, s.top, 0.0]
            } else if s.kind == BURST {
                [s.power, 0.0, 0.0]
            } else {
                s.vel
            };
            let slot = [
                s.kind as f32, s.pos[0], s.pos[1], s.pos[2], extra[0], extra[1], extra[2], s.age, s.life, s.seed,
            ];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        // (locking on, the ring round the target: likewise)
        if let (Some(l), true) = (&self.lock, self.limit > 0) {
            let at = l.target.or(l.ground.and_then(|(g, y)| y.map(|y| [g[0], y, g[2]])));
            if let Some(p) = at {
                let i = self.items.len().min(self.limit - 1);
                let slot = [LOCK as f32, p[0], p[1], p[2], 0.0, p[1] + STRIKE_HEIGHT, 0.0, l.age, LOCK_TIME, 11.0];
                out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
            }
        }
        // (his wind blades warming up, at the orb: likewise)
        let g = self.gatling;
        if g.warm > 0.0 && g.warm < WARMUP && self.limit > 0 {
            let i = self.items.len().min(self.limit - 1);
            let [x, y, z] = self.orb;
            let slot = [WINDUP as f32, x, y, z, 0.0, 0.0, 0.0, g.warm, WARMUP, 3.0];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        // (charging, the fire bolt at the orb: likewise)
        if self.charge >= 0.0 && self.limit > 0 {
            let i = self.items.len().min(self.limit - 1);
            let [x, y, z] = self.orb;
            let slot = [CHARGE as f32, x, y, z, 0.0, 0.0, 0.0, self.charge, CHARGE_TIME, 5.0];
            out[i * STRIDE..(i + 1) * STRIDE].copy_from_slice(&slot);
        }
        // (channelling, the healing round him: in the first free slot, or
        // the last)
        if self.channel >= 0.0 && self.limit > 0 {
            let i = self.items.len().min(self.limit - 1);
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
}
