// His shield: held up (R, or LB on a controller) it stops the enemies'
// spells, at a cost in mana to raise, to hold and for each spell it stops;
// it drops when the mana runs out. He cannot cast while it is up. The
// renderer draws it (shield_layer in fut/shading.fut) from `amount` and
// `sinceHit`.

import type { Mana } from "./mana.ts";

const RAISE_COST = 10;
const HOLD_COST = 15; // per second
const BLOCK_COST = 6;
const UP_RATE = 7; // how fast it rises and falls (1/s)

export class Shield {
  amount = 0; // 0 down .. 1 up
  sinceHit = 10; // seconds since it stopped a spell
  wanted = false; // R held
  private raised = false; // paid for and held
  private mana: Mana;
  onRaise: () => void = () => {};
  onDrop: () => void = () => {};

  constructor(mana: Mana) {
    this.mana = mana;
    window.addEventListener("keydown", (e) => {
      if (e.code === "KeyR" && !(e.target as HTMLElement | null)?.closest?.("input, select, textarea")) this.wanted = true;
    });
    window.addEventListener("keyup", (e) => {
      if (e.code === "KeyR") this.wanted = false;
    });
    window.addEventListener("blur", () => (this.wanted = false));
  }

  // Up enough to stop spells.
  get up(): boolean {
    return this.raised && this.amount > 0.5;
  }

  // Up at all (casting is blocked from the moment it is raised).
  get raisedNow(): boolean {
    return this.raised;
  }

  // A spell hit it.
  block(): void {
    this.sinceHit = 0;
    if (!this.mana.drain(BLOCK_COST)) this.drop();
  }

  // `held`: the controller's button, as well as the key; `can`: whether he
  // may raise it now (not while down, say).
  update(dt: number, held: boolean, can: boolean): void {
    this.sinceHit += dt;
    const want = (this.wanted || held) && can;
    if (want && !this.raised && this.mana.spend(RAISE_COST)) {
      this.raised = true;
      this.onRaise();
    } else if (!want && this.raised) this.drop();
    if (this.raised && !this.mana.drain(HOLD_COST * dt)) this.drop();
    this.amount += ((this.raised ? 1 : 0) - this.amount) * (1 - Math.exp(-dt * UP_RATE));
    if (this.amount < 1e-3) this.amount = 0;
  }

  private drop(): void {
    if (!this.raised) return;
    this.raised = false;
    this.onDrop();
  }
}
