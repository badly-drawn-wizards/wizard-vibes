// Mana: what casting and the shield draw on. It refills steadily, after a
// short pause from the last time it was drawn on.

export const MANA = 100;
const REGEN = 14; // per second
const PAUSE = 0.6; // seconds after spending before it refills

// What each spell costs (fire bolt, lightning strike, wind blade).
export const SPELL_COST = [12, 25, 8];

export class Mana {
  value = MANA;
  short = 99; // seconds since something wanted more than there was
  private since = 99; // seconds since it was last spent

  // Spends `amount` if there is that much (else notes the shortfall).
  spend(amount: number): boolean {
    if (this.value < amount) {
      this.short = 0;
      return false;
    }
    this.value -= amount;
    this.since = 0;
    return true;
  }

  // Spends up to `amount` (for draining over time): whether it covered it.
  drain(amount: number): boolean {
    const ok = this.value >= amount;
    this.value = Math.max(0, this.value - amount);
    this.since = 0;
    if (!ok) this.short = 0;
    return ok;
  }

  update(dt: number): void {
    this.since += dt;
    this.short += dt;
    if (this.since > PAUSE) this.value = Math.min(MANA, this.value + REGEN * dt);
  }
}
