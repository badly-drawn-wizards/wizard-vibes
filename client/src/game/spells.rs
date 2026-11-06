// Spells (web/spells.ts): fire bolt, lightning strike and wind blade, cast
// from the orb along the camera's view; and healing, channelled (see
// combat.rs: while it is held he heals, and cannot move). Their state lives here; each frame
// it goes to the renderer as a flat array (see fut/spells.fut), and the
// collision probe (fut/collide.fut) reports the land at each one: a fire
// bolt bursts where it hits, a wind blade breaks up, and a lightning strike
// finds the ground it hits. At most `limit` (a compile-time parameter) are
// out at once, and casting has a short cooldown on top.
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
pub const STRIDE: usize = 10;

const LIFE: [f32; 7] = [3.0, 0.9, 1.5, 1.4, 1.1, 1.1, 1e3]; // seconds, by kind
const SPEED: [f32; 4] = [30.0, 0.0, 24.0, 0.0]; // units/s
const COOLDOWN: [f32; 4] = [0.25, 0.5, 0.3, 0.0]; // seconds before the next cast
const STRIKE_RANGE: f32 = 45.0; // furthest a strike lands
const STRIKE_HEIGHT: f32 = 25.0; // it comes down from this far above the orb
const PROBE_HEIGHT: f32 = 60.0; // a strike's ground is probed from up here
const BLAST: f32 = 2.6; // a burst's reach
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

// What happened, for the fight to deal with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpellEvent {
    // a spell of `kind` cast by `owner` hit `side`
    Hit { side: i32, kind: u32, owner: i32 },
    // (for the sound: a bolt bursting, a blade breaking up, a strike landing)
    Sound { what: Sound, pos: V3 },
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
    items: Vec<Spell>,
    cooldown: f32,
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
            items: Vec::new(),
            cooldown: 0.0,
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

    // Whether he could cast now (not cooling down, nor at the limit).
    pub fn ready(&self) -> bool {
        self.cooldown <= 0.0 && self.items.len() < self.limit
    }

    // Casts the selected spell from `origin` along the unit vector `dir`
    // (world space), unless cooling down or at the limit.
    pub fn cast(&mut self, origin: V3, dir: V3, rng: &mut Rng) -> bool {
        if self.cooldown > 0.0 || !self.launch(self.selected as u32, origin, dir, HIS, None, rng) {
            return false;
        }
        self.cooldown = COOLDOWN[self.selected];
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
        };
        self.next_id += 1;
        if kind == STRIKE || kind == MARK {
            let target = at.or_else(|| if owner == HIS { self.lock_on(origin, dir) } else { None });
            if let Some(t) = target {
                s.pos = t;
            } else {
                // where the view meets sea level, within range; the probe then
                // finds the actual ground beneath
                let t = if dir[1] < -0.05 { STRIKE_RANGE.min(origin[1] / -dir[1]) } else { STRIKE_RANGE };
                s.pos = [origin[0] + dir[0] * t, 0.0, origin[2] + dir[2] * t];
            }
            s.top = origin[1].max(s.pos[1]) + STRIKE_HEIGHT;
        } else {
            let v = SPEED[kind as usize];
            s.vel = [dir[0] * v, dir[1] * v, dir[2] * v];
        }
        if kind == STRIKE && s.pos[1] != 0.0 {
            self.events.push(SpellEvent::Sound { what: Sound::Strike, pos: s.pos });
        }
        self.items.push(s);
        true
    }

    // The enemy target nearest his aim, within LOCK_ON of it and in range.
    pub fn lock_on(&self, origin: V3, dir: V3) -> Option<V3> {
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
                best = Some(t.c);
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
                events.push(SpellEvent::Hit { side: t.side, kind: s.kind, owner: s.owner });
                if s.kind == FIRE {
                    Self::burst(s, events);
                } else {
                    Self::gust(s, events);
                }
            } else if (s.kind == BURST || (s.kind == STRIKE && s.pos[1] != 0.0)) && !s.done {
                // (once per target: the nearest sphere of each)
                s.done = true;
                let reach = if s.kind == BURST { BLAST } else { STRIKE_REACH };
                let mut sides: Vec<i32> = Vec::new();
                for t in targets {
                    if Self::opposed(s.owner, t) && dist(s.pos, t.c) < t.r + reach && !sides.contains(&t.side) {
                        sides.push(t.side);
                    }
                }
                for side in sides {
                    events.push(SpellEvent::Hit { side, kind: s.kind, owner: s.owner });
                }
            }
        }
    }

    // `holding`: the cast button is down. The staff comes out quickly while
    // casting and stays out between repeated casts, then lowers slowly.
    pub fn update(&mut self, dt: f32, holding: bool) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        self.since_cast += dt;
        let out = holding || self.since_cast < 0.7;
        let goal = if out { 1.0 } else { 0.0 };
        self.staff += (goal - self.staff) * (1.0 - (-dt * if out { 12.0 } else { 2.5 }).exp());
        for s in &mut self.items {
            s.age += dt;
            // (a gust stays where the blade broke; its velocity only aims the shards)
            if s.kind != GUST {
                for i in 0..3 {
                    s.pos[i] += s.vel[i] * dt;
                }
            }
            // a wind blade that flew its course bursts too
            if s.kind == WIND && s.age >= s.life {
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
        (ids, pts)
    }

    // Applies the probe's answers (per point: distance to the land, normal).
    pub fn apply_probe(&mut self, ids: &[u32], data: &[f32]) {
        for (i, &id) in ids.iter().enumerate() {
            let Some(&d) = data.get(i * 4) else { break };
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
    // [kind, x, y, z, a, b, c, age, life, seed] (see fut/spells.fut).
    pub fn pack(&self) -> Vec<f32> {
        let mut out = vec![-1.0; self.limit * STRIDE];
        for (i, s) in self.items.iter().take(self.limit).enumerate() {
            let extra = if s.kind == STRIKE || s.kind == MARK { [0.0, s.top, 0.0] } else { s.vel };
            let slot = [
                s.kind as f32, s.pos[0], s.pos[1], s.pos[2], extra[0], extra[1], extra[2], s.age, s.life, s.seed,
            ];
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
        assert!(s.cast([0.0; 3], [0.0, 0.0, 1.0], &mut rng));
        assert!(!s.ready()); // cooling down
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
    fn strike_finds_the_ground() {
        let mut rng = Rng::new(1);
        let mut s = Spells::default();
        let dir = super::super::vec::normalise([0.0, -1.0, 1.0]);
        assert!(s.cast([0.0, 10.0, 0.0], dir, &mut rng));
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
        assert!(!s.ready());
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
