// Spells: fire bolt, lightning strike and wind blade, cast from the orb along
// the camera's view. Their state lives here; each frame it goes to the
// renderer as a flat array (see fut/spells.fut), and the collision probe
// (fut/collide.fut) reports the land at each one: a fire bolt bursts where
// it hits, a wind blade breaks up, and a lightning strike finds the ground
// it hits. At most `limit` (a compile-time parameter) are out at once, and
// casting has a short cooldown on top.
//
// The enemies cast them too (see enemies.ts). Each spell has a side: his
// hit the enemies, theirs hit him, as spheres listed in `targets`; a hit is
// reported to `onHit` (a bolt or blade where it strikes, a burst once for
// each target in its blast, a strike once where it lands). His strikes
// lock on to a target near where he aims. Theirs are telegraphed: a ring
// marks where one will land for a moment first (MARK), so he can get out
// of the way.

type V3 = [number, number, number];

export const SPELL_NAMES = ["Fire bolt", "Lightning strike", "Wind blade"];

// kinds, as in fut/spells.fut
export const FIRE = 0;
export const STRIKE = 1;
export const WIND = 2;
export const BURST = 3;
const GUST = 4; // a wind blade's burst into small blades
export const MARK = 5; // where a strike is about to land
const STRIDE = 10;

const LIFE = [3, 0.9, 1.5, 1.4, 1.1, 1.1]; // seconds, by kind
const SPEED = [30, 0, 24]; // units/s
const COOLDOWN = [0.25, 0.5, 0.3]; // seconds before the next cast
const STRIKE_RANGE = 45; // furthest a strike lands
const STRIKE_HEIGHT = 25; // it comes down from this far above the orb
const PROBE_HEIGHT = 60; // a strike's ground is probed from up here
const BLAST = 2.6; // a burst's reach
const STRIKE_REACH = 1.6; // a strike's
const LOCK_ON = 0.12; // radians off his aim a strike still finds a target

// Something spells can hit: a sphere, and whose it is (HIS, or an enemy's
// number).
export const HIS = -1;
export interface Target {
  c: V3;
  r: number;
  side: number;
}

interface Spell {
  id: number;
  kind: number;
  pos: V3;
  vel: V3;
  age: number;
  life: number; // seconds: it has faded out by then
  seed: number; // varies each one's look
  top: number; // a strike's top height
  owner: number; // whose (HIS or an enemy's number)
  done: boolean; // its blast (a burst's or a strike's) has been dealt
}


const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const dist = (a: V3, b: V3) => Math.hypot(...sub(a, b));


export class Spells {
  selected = 1; // lightning: the orb's own colours
  limit = 6;
  staff = 0; // how far the staff is held out (0..1)
  sinceCast = 10; // seconds
  targets: Target[] = [];
  onHit: (side: number, kind: number, owner: number) => void = () => {};
  // (for the sound: a bolt bursting, a blade breaking up, a strike landing)
  onSound: (what: "burst" | "gust" | "strike", pos: V3) => void = () => {};
  private items: Spell[] = [];
  private cooldown = 0;
  private nextId = 1;

  get count(): number {
    return this.items.length;
  }

  select(i: number): void {
    if (i >= 0 && i < SPELL_NAMES.length) this.selected = i;
  }

  // Whether he could cast now (not cooling down, nor at the limit).
  get ready(): boolean {
    return this.cooldown <= 0 && this.items.length < this.limit;
  }

  // Casts the selected spell from `origin` along the unit vector `dir`
  // (world space), unless cooling down or at the limit.
  cast(origin: V3, dir: V3): boolean {
    if (this.cooldown > 0 || !this.launch(this.selected, origin, dir, HIS)) return false;
    this.cooldown = COOLDOWN[this.selected];
    this.sinceCast = 0;
    return true;
  }

  // Casts a spell of `kind` for `owner` (no cooldown), unless at the limit.
  // A strike lands at `at` if given; his lock on to a target near his aim,
  // or else land where the view meets the ground.
  launch(kind: number, origin: V3, dir: V3, owner: number, at?: V3): boolean {
    if (this.items.length >= this.limit) return false;
    const s: Spell = { id: this.nextId++, kind, pos: [...origin], vel: [0, 0, 0], age: 0, life: LIFE[kind],
      seed: (Math.random() * 1e5) | 0, top: 0, owner, done: false };
    if (kind === STRIKE || kind === MARK) {
      const target = at ?? (owner === HIS ? this.lockOn(origin, dir) : null);
      if (target) s.pos = [...target];
      else {
        // where the view meets sea level, within range; the probe then finds
        // the actual ground beneath
        const t = dir[1] < -0.05 ? Math.min(STRIKE_RANGE, origin[1] / -dir[1]) : STRIKE_RANGE;
        s.pos = [origin[0] + dir[0] * t, 0, origin[2] + dir[2] * t];
      }
      s.top = Math.max(origin[1], s.pos[1]) + STRIKE_HEIGHT;
    } else {
      s.vel = dir.map((d) => d * SPEED[kind]) as V3;
    }
    this.items.push(s);
    if (kind === STRIKE && s.pos[1] !== 0) this.onSound("strike", s.pos);
    return true;
  }

  // The enemy target nearest his aim, within LOCK_ON of it and in range.
  lockOn(origin: V3, dir: V3): V3 | null {
    let best: V3 | null = null;
    let bestAngle = LOCK_ON;
    for (const t of this.targets) {
      if (t.side === HIS) continue;
      const v = sub(t.c, origin);
      const d = Math.hypot(...v);
      if (d > STRIKE_RANGE * 1.5) continue;
      const angle = Math.acos(Math.min(1, dot(v, dir) / d)) - Math.atan(t.r / d);
      if (angle < bestAngle) {
        bestAngle = angle;
        best = t.c;
      }
    }
    return best;
  }

  // A fire bolt bursting where it is.
  private burst(s: Spell): void {
    Object.assign(s, { kind: BURST, age: 0, life: LIFE[BURST], vel: [0, 0, 0] });
    this.onSound("burst", s.pos);
  }

  // A wind blade bursting into small blades where it is (see spell_gust in
  // fut/spells.fut): it keeps its velocity, which aims the shards.
  private gust(s: Spell): void {
    Object.assign(s, { kind: GUST, age: 0, life: LIFE[GUST] });
    this.onSound("gust", s.pos);
  }

  // Whether a spell of `owner`'s can hit target t.
  private opposed(owner: number, t: Target): boolean {
    return (owner === HIS) !== (t.side === HIS);
  }

  // Bolts and blades that fly into a target; bursts' and strikes' blasts.
  private collide(): void {
    for (const s of this.items) {
      if (s.kind === FIRE || s.kind === WIND) {
        const t = this.targets.find((t) => this.opposed(s.owner, t) && dist(s.pos, t.c) < t.r);
        if (!t) continue;
        this.onHit(t.side, s.kind, s.owner);
        if (s.kind === FIRE) this.burst(s);
        else this.gust(s);
      } else if ((s.kind === BURST || (s.kind === STRIKE && s.pos[1] !== 0)) && !s.done) {
        // (once per target: the nearest sphere of each)
        s.done = true;
        const reach = s.kind === BURST ? BLAST : STRIKE_REACH;
        const sides = new Set(this.targets.filter((t) => this.opposed(s.owner, t) && dist(s.pos, t.c) < t.r + reach).map((t) => t.side));
        for (const side of sides) this.onHit(side, s.kind, s.owner);
      }
    }
  }

  // `holding`: the cast button is down. The staff comes out quickly while
  // casting and stays out between repeated casts, then lowers slowly.
  update(dt: number, holding = false): void {
    this.cooldown = Math.max(0, this.cooldown - dt);
    this.sinceCast += dt;
    const out = holding || this.sinceCast < 0.7;
    this.staff += ((out ? 1 : 0) - this.staff) * (1 - Math.exp(-dt * (out ? 12 : 2.5)));
    for (const s of this.items) {
      s.age += dt;
      // (a gust stays where the blade broke; its velocity only aims the shards)
      if (s.kind !== GUST) for (let i = 0; i < 3; i++) s.pos[i] += s.vel[i] * dt;
      // a wind blade that flew its course bursts too
      if (s.kind === WIND && s.age >= s.life) this.gust(s);
      // a mark's time is up: the strike comes down
      if (s.kind === MARK && s.age >= s.life) {
        Object.assign(s, { kind: STRIKE, age: 0, life: LIFE[STRIKE], done: false });
        this.onSound("strike", s.pos);
      }
    }
    this.collide();
    this.items = this.items.filter((s) => s.age < s.life);
  }

  // The points to probe the land at, `limit` of them (unused ones far above
  // everything), and which spell each is.
  probePoints(): { ids: number[]; pts: Float32Array } {
    const pts = new Float32Array(this.limit * 3).fill(0);
    const ids: number[] = [];
    this.items.slice(0, this.limit).forEach((s, i) => {
      ids.push(s.id);
      pts.set(s.kind === STRIKE ? [s.pos[0], PROBE_HEIGHT, s.pos[2]] : s.pos, i * 3);
    });
    for (let i = ids.length; i < this.limit; i++) pts[i * 3 + 1] = 1e5;
    return { ids, pts };
  }

  // Applies the probe's answers (per point: distance to the land, normal).
  applyProbe(ids: number[], data: Float32Array): void {
    ids.forEach((id, i) => {
      const s = this.items.find((x) => x.id === id);
      const d = data[i * 4];
      if (!s || !(d < 1e5)) return;
      if (s.kind === FIRE && d < 0.15) {
        // burst where it hit
        this.burst(s);
      } else if (s.kind === WIND && d < 0.15) {
        // it bursts against the land into small blades
        this.gust(s);
      } else if (s.kind === STRIKE && s.pos[1] === 0) {
        // the ground below the probe point: its height above the land,
        // divided by the slope
        s.pos[1] = PROBE_HEIGHT - d / Math.max(0.2, data[i * 4 + 2]);
        this.onSound("strike", s.pos);
      }
    });
  }

  // The renderer's array: `limit` slots of
  // [kind, x, y, z, a, b, c, age, life, seed] (see fut/spells.fut).
  pack(): Float32Array {
    const out = new Float32Array(this.limit * STRIDE).fill(-1);
    this.items.slice(0, this.limit).forEach((s, i) => {
      const extra = s.kind === STRIKE || s.kind === MARK ? [0, s.top, 0] : s.vel;
      out.set([s.kind, ...s.pos, ...extra, s.age, s.life, s.seed], i * STRIDE);
    });
    return out;
  }
}
