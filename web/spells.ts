// Spells: fire bolt, lightning strike and wind blade, cast from the orb along
// the camera's view. Their state lives here; each frame it goes to the
// renderer as a flat array (see fut/spells.fut), and the collision probe
// (fut/collide.fut) reports the land at each one: a fire bolt bursts where
// it hits, a wind blade breaks up, and a lightning strike finds the ground
// it hits. At most `limit` (a compile-time parameter) are out at once, and
// casting has a short cooldown on top.

type V3 = [number, number, number];

export const SPELL_NAMES = ["Fire bolt", "Lightning strike", "Wind blade"];

// kinds, as in fut/spells.fut
const FIRE = 0;
const STRIKE = 1;
const WIND = 2;
const BURST = 3;
const STRIDE = 8;

const LIFE = [3, 0.9, 1.5, 1.4]; // seconds, by kind
const SPEED = [30, 0, 24]; // units/s
const COOLDOWN = [0.25, 0.5, 0.3]; // seconds before the next cast
const STRIKE_RANGE = 45; // furthest a strike lands
const STRIKE_HEIGHT = 25; // it comes down from this far above the orb
const PROBE_HEIGHT = 60; // a strike's ground is probed from up here

interface Spell {
  id: number;
  kind: number;
  pos: V3;
  vel: V3;
  age: number;
  seed: number;
  top: number; // a strike's top height
}

export class Spells {
  selected = 1; // lightning: the orb's own colours
  limit = 6;
  staff = 0; // how far the staff is held out (0..1)
  sinceCast = 10; // seconds
  private items: Spell[] = [];
  private cooldown = 0;
  private nextId = 1;

  get count(): number {
    return this.items.length;
  }

  select(i: number): void {
    if (i >= 0 && i < SPELL_NAMES.length) this.selected = i;
  }

  // Casts the selected spell from `origin` along the unit vector `dir`
  // (world space), unless cooling down or at the limit.
  cast(origin: V3, dir: V3): boolean {
    if (this.cooldown > 0 || this.items.length >= this.limit) return false;
    const kind = this.selected;
    const s: Spell = { id: this.nextId++, kind, pos: [...origin], vel: [0, 0, 0], age: 0, seed: (Math.random() * 1e6) | 0, top: 0 };
    if (kind === STRIKE) {
      // where the view meets sea level, within range; the probe then finds
      // the actual ground beneath
      const t = dir[1] < -0.05 ? Math.min(STRIKE_RANGE, origin[1] / -dir[1]) : STRIKE_RANGE;
      s.pos = [origin[0] + dir[0] * t, 0, origin[2] + dir[2] * t];
      s.top = origin[1] + STRIKE_HEIGHT;
    } else {
      s.vel = dir.map((d) => d * SPEED[kind]) as V3;
    }
    this.items.push(s);
    this.cooldown = COOLDOWN[kind];
    this.sinceCast = 0;
    return true;
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
      for (let i = 0; i < 3; i++) s.pos[i] += s.vel[i] * dt;
    }
    this.items = this.items.filter((s) => s.age < LIFE[s.kind]);
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
        s.kind = BURST;
        s.age = 0;
        s.vel = [0, 0, 0];
      } else if (s.kind === WIND && d < 0.1) {
        s.age = LIFE[WIND];
      } else if (s.kind === STRIKE && s.pos[1] === 0) {
        // the ground below the probe point: its height above the land,
        // divided by the slope
        s.pos[1] = PROBE_HEIGHT - d / Math.max(0.2, data[i * 4 + 2]);
      }
    });
  }

  // The renderer's array: `limit` slots of [kind, x, y, z, a, b, c, age].
  pack(): Float32Array {
    const out = new Float32Array(this.limit * STRIDE).fill(-1);
    this.items.slice(0, this.limit).forEach((s, i) => {
      const extra = s.kind === STRIKE ? [s.seed % 100000, s.top, 0] : s.vel;
      out.set([s.kind, ...s.pos, ...extra, s.age], i * STRIDE);
    });
    return out;
  }
}
