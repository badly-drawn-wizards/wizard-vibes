// The fight with the enemy wizards (drawn by fut/enemies.fut). Where each
// one is is worked out the same way the renderer does it (a pure function
// of the time, his position, the seed and its generation), so the spells
// can hit what is drawn. Here: their health, their defeat and replacement,
// their casting at him, and his health.
//
// Each enemy's state goes to the renderer as one float (see foes in
// fut/enemies.fut): 16 times its generation (how many have had its place
// before it: each new one has new looks and a new place) plus its phase,
// under 8 the seconds it has still to appear, 8 and over 8 plus the seconds
// since its defeat.

import { BURST, FIRE, HIS, MARK, STRIKE, type Spells, type Target, WIND } from "./spells.ts";

type V3 = [number, number, number];

const MAX = 6; // as max_enemies in fut/materials.fut
const HP = 3;
const APPEAR = 1.5; // seconds (appear_time in fut/enemies.fut)
const RESPAWN = 5; // seconds after a defeat until the next one appears
const RANGE = 40; // they only cast at him from nearer than this
export const PLAYER_HP = 10;
const DOWN = 3; // seconds he is down after being defeated

// Damage, by kind of spell. (A fire bolt does its damage by its burst,
// which a bolt that hits sets off.)
const DAMAGE: Record<number, number> = { [FIRE]: 0, [BURST]: 1, [WIND]: 1, [STRIKE]: 2 };

// hash3 in fut/hash.fut.
function hash3(x: number, y: number, z: number): number {
  let h = (Math.imul(x, 73856093) ^ Math.imul(y, 19349663) ^ Math.imul(z, 83492791)) >>> 0;
  h = Math.imul(h ^ (h >>> 16), 0x7feb352d) >>> 0;
  h = Math.imul(h ^ (h >>> 15), 0x846ca68b) >>> 0;
  h = (h ^ (h >>> 16)) >>> 0;
  return (h >>> 8) / 16777216;
}

// enemy_rnd in fut/enemies.fut.
const rnd = (seed: number, gen: number, i: number, k: number) => hash3(seed + 7919 * gen, i + 17, k + 101);

// A direction turned by `yaw` about the vertical (yaw in fut/world.fut).
const turn = (a: number, [x, y, z]: V3): V3 => [x * Math.cos(a) + z * Math.sin(a), y, z * Math.cos(a) - x * Math.sin(a)];
const add = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

// Where enemy i (of generation gen) stands, and which way it faces, when he
// is at `him` (enemy_at in fut/enemies.fut, while it is not falling).
export function enemyAt(seed: number, gen: number, i: number, secs: number, him: V3): { pos: V3; yaw: number } {
  const r = (k: number) => rnd(seed, gen, i, k);
  const radius = 9 + 12 * r(0);
  const speed = (0.04 + 0.06 * r(1)) * (r(2) < 0.5 ? -1 : 1);
  const a = 2 * Math.PI * (i / MAX + 0.12 * r(3)) + speed * secs;
  const h = -1.5 + 4 * r(4) + 0.5 * Math.sin(0.6 * secs + 6.3 * r(5));
  const off: V3 = [radius * Math.sin(a), h, radius * Math.cos(a)];
  return { pos: add(him, off), yaw: Math.atan2(-off[0], -off[2]) };
}

// His shield, raised (shield_centre and shield_radius in fut/shading.fut).
const SHIELD: Target = { c: [0, 0.85, 0], r: 1.85, side: HIS };

// The spheres that stand for a wizard on his platform at `pos` (the
// platform's top): his body, and the platform.
export function spheres(pos: V3, side: number): Target[] {
  return [
    { c: add(pos, [0, 1.1, 0]), r: 0.9, side },
    { c: add(pos, [0, -0.8, 0]), r: 1.4, side },
  ];
}

interface Slot {
  gen: number;
  hp: number;
  appear: number; // seconds still to appear
  down: number; // seconds since its defeat, or -1
  castIn: number; // seconds to its next spell
}

export class Enemies {
  hp = PLAYER_HP;
  defeated = 0; // enemies he has defeated
  hurt = 99; // seconds since he was last hit
  down = -1; // seconds since he was defeated, or -1
  shielded = false; // his shield is up: it takes what would hit him
  onChange: () => void = () => {};
  // (for the sound and the shield)
  onBlock: () => void = () => {};
  onHurt: () => void = () => {};
  onDefeat: (pos: V3) => void = () => {};
  onAppear: (pos: V3) => void = () => {};
  onCast: (kind: number, pos: V3) => void = () => {};
  private slots: Slot[] = Array.from({ length: MAX }, (_, i) => ({ gen: 0, hp: HP, appear: 0, down: -1, castIn: 5 + i * 2 }));
  private seen = { seed: 0, secs: 0, him: [0, 0, 0] as V3 }; // as of the last targets()
  private last: V3 | null = null; // where he was last frame
  private vel: V3 = [0, 0, 0]; // how fast he is going, smoothed

  // The renderer's states (foe0 .. foe5 of the drive record).
  foes(): Record<string, number> {
    const f: Record<string, number> = {};
    this.slots.forEach((s, i) => {
      const phase = s.down >= 0 ? 8 + Math.min(s.down, 7.9) : Math.min(s.appear, 7.9);
      f[`foe${i}`] = 16 * s.gen + phase;
    });
    return f;
  }

  // The spheres spells can hit: his (or his shield), and the first n
  // enemies' that stand.
  targets(n: number, seed: number, secs: number, him: V3): Target[] {
    const out = this.down >= 0 ? [] : this.shielded ? [{ ...SHIELD, c: add(him, SHIELD.c) }] : spheres(him, HIS);
    this.seen = { seed, secs, him };
    this.slots.slice(0, n).forEach((s, i) => {
      if (s.down < 0 && s.appear < 0.5) out.push(...spheres(enemyAt(seed, s.gen, i, secs, him).pos, i));
    });
    return out;
  }

  // A spell of `kind` cast by `owner` hit `side`.
  hit(side: number, kind: number): void {
    const damage = DAMAGE[kind] ?? 0;
    if (side === HIS) {
      if (this.down >= 0) return;
      if (this.shielded) {
        this.onBlock();
        return;
      }
      if (damage === 0) return;
      this.hp = Math.max(0, this.hp - damage);
      this.hurt = 0;
      this.onHurt();
      if (this.hp === 0) this.down = 0;
    } else {
      const s = this.slots[side];
      if (!s || s.down >= 0) return;
      s.hp -= damage;
      if (s.hp <= 0) {
        s.down = 0;
        this.defeated++;
        const { seed, secs, him } = this.seen;
        this.onDefeat(enemyAt(seed, s.gen, side, secs, him).pos);
      }
    }
    this.onChange();
  }

  // Advances the fight by dt with n enemies, he being at `him`; they cast
  // with `spells`.
  update(dt: number, n: number, seed: number, secs: number, him: V3, spells: Spells): void {
    if (this.last && dt > 0) {
      const v = [0, 1, 2].map((k) => (him[k] - this.last![k]) / dt) as V3;
      const k = 1 - Math.exp(-dt * 4);
      this.vel = this.vel.map((x, j) => x + (v[j] - x) * k) as V3;
    }
    this.last = [...him];
    this.hurt += dt;
    if (this.down >= 0) {
      this.down += dt;
      if (this.down >= DOWN) {
        this.down = -1;
        this.hp = PLAYER_HP;
        this.onChange();
      }
    }
    this.slots.forEach((s, i) => {
      if (s.down >= 0) {
        s.down += dt;
        if (s.down >= RESPAWN) {
          Object.assign(s, { gen: s.gen + 1, hp: HP, appear: APPEAR, down: -1, castIn: 4 + 4 * Math.random() });
          if (i < n) this.onAppear(enemyAt(seed, s.gen, i, secs, him).pos);
        }
        return;
      }
      s.appear = Math.max(0, s.appear - dt);
      if (i >= n || s.appear > 0 || this.down >= 0) return;
      s.castIn -= dt;
      if (s.castIn > 0) return;
      s.castIn = 6 + 6 * Math.random() + n;
      this.cast(seed, s.gen, i, secs, him, spells);
    });
  }

  // Enemy i casts at him: its kind of spell, from its orb, aimed where he
  // will be when it arrives, give or take.
  private cast(seed: number, gen: number, i: number, secs: number, him: V3, spells: Spells): void {
    const { pos, yaw } = enemyAt(seed, gen, i, secs, him);
    const orb = add(pos, turn(yaw, [0.45, 2.05, 0.2]));
    const r = rnd(seed, gen, i, 29);
    const kind = r < 0.5 ? FIRE : r < 0.8 ? WIND : STRIKE;
    const chest = add(him, [0, 1.1, 0]);
    if (Math.hypot(chest[0] - orb[0], chest[1] - orb[1], chest[2] - orb[2]) > RANGE) return;
    // (off by up to 2.5 either way, mostly less)
    const miss = () => (Math.random() + Math.random() - 1) * 2.5;
    if (kind === STRIKE) {
      // marked first, where he will be by the time it lands
      const lead = 1.1;
      const at = add(add(him, this.vel.map((v) => v * lead) as V3), [miss(), 0.6, miss()]);
      if (spells.launch(MARK, orb, [0, 1, 0], i, at)) this.onCast(MARK, at);
      return;
    }
    const speed = kind === FIRE ? 30 : 24;
    const flight = Math.hypot(chest[0] - orb[0], chest[1] - orb[1], chest[2] - orb[2]) / speed;
    const aim = add(add(chest, this.vel.map((v) => v * flight) as V3), [miss(), miss() / 2, miss()]);
    const d = [aim[0] - orb[0], aim[1] - orb[1], aim[2] - orb[2]];
    const len = Math.hypot(...d);
    if (spells.launch(kind, orb, d.map((x) => x / len) as V3, i)) this.onCast(kind, orb);
  }
}
