// The fight, put together: the enemies (enemies.ts), his mana (mana.ts) and
// shield (shield.ts), casting with them, the sounds of it all (sfx.ts), and
// what it shows: bottom left his health (with enemies about), his mana, how
// many he has defeated and a reset button; a red flash when he is hit; and
// a crosshair at the view's centre, where spells go, which turns into a
// lock when a strike would find a target. Defeated, he stays down: a panel
// says so, until he tries again (its button, Enter, or the reset button).

import { Enemies, PLAYER_HP } from "./enemies.ts";
import { MANA, Mana, SPELL_COST } from "./mana.ts";
import { type Placement, SFX, type Sfx } from "./sfx.ts";
import { Shield } from "./shield.ts";
import { FIRE, MARK, type Spells, STRIKE } from "./spells.ts";
import { dot, len, sub, type V3 } from "./vec.ts";

const CAST_SOUND = [SFX.castFire, SFX.castStrike, SFX.castWind];

// What the fight needs to know of the frame.
export interface Moment {
  dt: number; // 0 while paused
  time: number;
  him: V3; // where he is
  aim: { origin: V3; dir: V3 }; // his orb, and where he aims
  drawn: boolean; // spells can be drawn (WebGPU: the fight is on)
  able: boolean; // and the game is running (not paused)
  n: number; // enemies
  seed: number;
  casting: boolean; // the cast button is held
  shielding: boolean; // the controller's shield button is held
}

export class Combat {
  readonly enemies = new Enemies();
  readonly mana = new Mana();
  readonly shield = new Shield(this.mana);
  private spells: Spells;
  private sfx: Sfx;
  private fizzled = -1; // when the last fizzle sounded (holding the key does not drone)
  private here: { him: V3; dir: V3 } = { him: [0, 0, 0], dir: [0, 0, 1] };
  private started = false; // the fight has an arena (it starts with the first enemies)
  private el = {
    vitals: document.getElementById("vitals")!,
    health: document.querySelector<HTMLElement>("#vitals .health")!,
    mana: document.querySelector<HTMLElement>("#vitals .mana")!,
    score: document.querySelector<HTMLElement>("#vitals .score")!,
    foot: document.querySelector<HTMLElement>("#vitals .foot")!,
    defeat: document.getElementById("defeat")!,
    crosshair: document.getElementById("crosshair")!,
    hurt: document.getElementById("hurt")!,
  };

  constructor(spells: Spells, sfx: Sfx) {
    this.spells = spells;
    this.sfx = sfx;
    const { enemies, shield } = this;
    const at = (pos: V3) => this.placed(pos);
    spells.onHit = (side, kind) => enemies.hit(side, kind);
    spells.onSound = (what, pos) => sfx.play(what === "burst" ? SFX.burst : what === "gust" ? SFX.gust : SFX.thunder, at(pos));
    enemies.onBlock = () => {
      shield.block();
      sfx.play(SFX.block);
    };
    enemies.onHurt = () => sfx.play(SFX.hurt);
    enemies.onDefeat = (pos) => sfx.play(SFX.defeat, at(pos));
    enemies.onAppear = (pos) => sfx.play(SFX.appear, at(pos));
    enemies.onCast = (kind, pos) => sfx.play(kind === MARK ? SFX.mark : kind === FIRE ? SFX.castFire : SFX.castWind, at(pos));
    shield.onRaise = () => sfx.play(SFX.shieldUp);
    shield.onDrop = () => sfx.play(SFX.shieldDown);
    document.querySelector<HTMLButtonElement>("#vitals .reset")!.onclick = () => this.reset();
    this.el.defeat.querySelector("button")!.onclick = () => this.reset();
    window.addEventListener("keydown", (e) => {
      if (e.code === "Enter" && this.enemies.down >= 0 && !(e.target as HTMLElement | null)?.closest?.("input, select, textarea, button")) this.reset();
    });
  }

  // Starts the fight afresh on the next update (once the view has settled:
  // the arena goes where he then looks).
  restart(): void {
    this.started = false;
  }

  // The fight from the start: full health and mana, no spells in flight,
  // a new set of enemies.
  reset(): void {
    this.started = true;
    // (round a point ahead, where he looks)
    const [x, , z] = this.here.dir;
    const d = Math.hypot(x, z) || 1;
    this.enemies.reset([this.here.him[0] + (12 * x) / d, this.here.him[1], this.here.him[2] + (12 * z) / d]);
    this.mana.value = MANA;
    this.spells.clear();
  }

  // The renderer's share (the drive record's shield and foe fields).
  drive(): { shield: number; shield_hit: number } & Record<string, number> {
    return { shield: this.shield.amount, shield_hit: this.shield.sinceHit, ...this.enemies.foes() };
  }

  // Advances the fight; casts if he may (not while down, nor from behind
  // the shield, and only with the mana for it). Before the spells' update.
  update(m: Moment): void {
    const { enemies, mana, shield, spells } = this;
    this.here = { him: m.him, dir: m.aim.dir };
    const able = m.able && enemies.down < 0;
    shield.update(m.dt, m.shielding, able);
    enemies.shielded = shield.up;
    if (able && !shield.raisedNow && m.casting && spells.ready) {
      if (mana.spend(SPELL_COST[spells.selected])) {
        spells.cast(m.aim.origin, m.aim.dir);
        this.sfx.play(CAST_SOUND[spells.selected]);
      } else if (m.time - this.fizzled > 0.5 || m.time < this.fizzled) {
        this.fizzled = m.time;
        this.sfx.play(SFX.fizzle);
      }
    }
    mana.update(m.dt);
    // (only where spells can be drawn)
    const n = m.drawn ? m.n : 0;
    if (n > 0 && !this.started) this.reset();
    spells.targets = enemies.targets(n, m.seed, m.time, m.him);
    enemies.update(m.dt, n, m.seed, m.time, m.him, spells);
    this.show(n, m);
  }

  // Where a sound at world point `pos` is heard from: panned by which side
  // of the view it is on, quieter further from him.
  private placed(pos: V3): Placement {
    const v = sub(pos, this.here.him);
    const d = len(v) || 1;
    const [x, , z] = this.here.dir;
    const right: V3 = [-z, 0, x];
    return { pan: (0.8 * dot(v, right)) / ((len(right) || 1) * d), gain: 1 / (1 + d / 15) };
  }

  private show(n: number, m: Moment): void {
    const { el, enemies, mana, shield } = this;
    el.vitals.hidden = el.crosshair.hidden = !m.drawn;
    el.health.hidden = el.foot.hidden = n === 0;
    el.health.querySelector<HTMLElement>(".bar span")!.style.width = `${(100 * enemies.hp) / PLAYER_HP}%`;
    el.mana.querySelector<HTMLElement>(".bar span")!.style.width = `${(100 * mana.value) / MANA}%`;
    el.mana.classList.toggle("short", mana.short < 0.4);
    el.score.textContent = `${enemies.defeated} defeated`;
    const lost = n > 0 && enemies.down >= 0;
    if (el.defeat.hidden === lost) {
      el.defeat.hidden = !lost;
      const k = enemies.defeated;
      el.defeat.querySelector(".stats")!.textContent = k === 0 ? "No enemies defeated" : `${k} ${k === 1 ? "enemy" : "enemies"} defeated`;
    }
    el.hurt.style.opacity = String(lost ? 0.6 : 0.7 * Math.exp(-4 * enemies.hurt));
    if (lost) el.crosshair.hidden = true;
    el.crosshair.classList.toggle("locked", this.spells.selected === STRIKE && !!this.spells.lockOn(m.aim.origin, m.aim.dir));
    el.crosshair.classList.toggle("shielded", shield.raisedNow);
  }
}
