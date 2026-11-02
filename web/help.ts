// The controls panel, bottom right: keyboard keys or controller glyphs,
// whichever was used last, and the spell in hand. Dismissing it leaves a
// help button that brings it back; that choice is remembered.

import { SPELL_NAMES } from "./spells.ts";

export type Device = "keyboard" | "pad";

const STORE = "wizard-help-closed";

const key = (k: string) => `<kbd>${k}</kbd>`;
// Xbox-style glyphs: face buttons in their colours, shoulders, sticks
const face = (b: "A" | "B" | "X" | "Y") => `<span class="glyph face ${b}">${b}</span>`;
const shoulder = (b: string) => `<span class="glyph shoulder">${b}</span>`;
const stickGlyph = (s: "L" | "R") => `<span class="glyph stick">${s}</span>`;
const menu = `<span class="glyph shoulder">≡</span>`;

const ROWS: Record<Device, [string, string][]> = {
  keyboard: [
    ["Free flight", key("F")],
    ["Fly", key("W") + key("A") + key("S") + key("D")],
    ["Climb / descend", key("E") + " / " + key("Q")],
    ["Boost", key("Shift")],
    ["Spell", key("1") + key("2") + key("3")],
    ["Cast", key("Space")],
    ["Shield", key("R") + " (hold)"],
    ["Camera", "drag · wheel"],
    ["Pause", key("P")],
    ["Music", key("M")],
  ],
  pad: [
    ["Free flight", face("Y") + " / " + menu],
    ["Fly", stickGlyph("L")],
    ["Camera", stickGlyph("R")],
    ["Climb / descend", shoulder("RT") + " / " + shoulder("LT")],
    ["Boost", stickGlyph("L") + "↓"],
    ["Spell", face("X") + face("A") + face("B")],
    ["Cast", shoulder("RB")],
    ["Shield", shoulder("LB") + " (hold)"],
  ],
};

export class Help {
  private device: Device = "keyboard";
  private spell = -1;
  private body: HTMLElement;
  private spellsEl: HTMLElement;
  private panel: HTMLElement;
  private button: HTMLElement;

  constructor(panel: HTMLElement, button: HTMLElement) {
    this.panel = panel;
    this.button = button;
    this.body = panel.querySelector(".help-body")!;
    this.spellsEl = panel.querySelector(".help-spells")!;
    panel.querySelector(".help-close")!.addEventListener("click", () => this.setClosed(true));
    button.addEventListener("click", () => this.setClosed(false));
    let closed = false;
    try {
      closed = localStorage.getItem(STORE) === "1";
    } catch {
      // storage unavailable
    }
    this.setClosed(closed);
    this.render();
  }

  private setClosed(closed: boolean): void {
    this.panel.hidden = closed;
    this.button.hidden = !closed;
    try {
      localStorage.setItem(STORE, closed ? "1" : "0");
    } catch {
      // ignore
    }
  }

  // Called every frame; only redraws on change.
  update(device: Device, spell: number): void {
    if (device === this.device && spell === this.spell) return;
    const redraw = device !== this.device;
    this.device = device;
    this.spell = spell;
    if (redraw) this.render();
    this.renderSpells();
  }

  private render(): void {
    this.body.innerHTML = ROWS[this.device].map(([what, how]) => `<div>${what}</div><div>${how}</div>`).join("");
    this.renderSpells();
  }

  private renderSpells(): void {
    const keys = this.device === "pad" ? [face("X"), face("A"), face("B")] : [key("1"), key("2"), key("3")];
    this.spellsEl.innerHTML = SPELL_NAMES.map((n, i) => `<span class="${i === this.spell ? "on" : ""}">${keys[i]} ${n}</span>`).join("");
  }
}
