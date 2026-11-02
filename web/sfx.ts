// Sound effects: made by the build's `sfx` entry point (see fut/sfx.fut),
// each once, a frame at a time, and kept as audio buffers (again for each
// new build, whose effects may differ). Played where they happen: panned
// by which side of the view they are on, quieter further off. The "fx"
// button turns them off and on (remembered).

import type { AudioHub } from "./audio.ts";
import type { FutInstance } from "./futhark-loader.ts";

// As in fut/sfx.fut.
export const SFX = {
  castFire: 0, castStrike: 1, castWind: 2, burst: 3, thunder: 4, gust: 5, hurt: 6, defeat: 7,
  shieldUp: 8, block: 9, shieldDown: 10, fizzle: 11, mark: 12, appear: 13,
} as const;

const STORE = "wizard-sfx";
const VOLUME = 0.7;

export interface Placement {
  pan: number; // -1 left .. 1 right
  gain: number; // 0..1
}

export class Sfx {
  on = true;
  private audio: AudioHub;
  private button: HTMLButtonElement;
  private out: GainNode | null = null;
  private made = new WeakMap<FutInstance, (AudioBuffer | null)[]>();
  private current: (AudioBuffer | null)[] = [];

  constructor(button: HTMLButtonElement, audio: AudioHub) {
    this.audio = audio;
    this.button = button;
    try {
      this.on = localStorage.getItem(STORE) !== "0";
    } catch {
      // storage unavailable: on
    }
    button.onclick = () => this.toggle();
    this.render();
  }

  disable(reason: string): void {
    this.on = false;
    this.button.disabled = true;
    this.button.title = reason;
    this.render();
  }

  toggle(): void {
    if (this.button.disabled) return;
    this.on = !this.on;
    try {
      localStorage.setItem(STORE, this.on ? "1" : "0");
    } catch {
      // ignore
    }
    this.render();
  }

  // Whether build i has effects still to make (once audio can start).
  wants(i: FutInstance): boolean {
    if (!this.on || !this.audio.ctx || !i.fut.entry.sfx) return false;
    const made = this.made.get(i);
    return !made || made.some((b) => b === null);
  }

  // Makes build i's next effect (call between frames: entry points must
  // not run at once). Its effects are the ones played from then on.
  async make(i: FutInstance): Promise<void> {
    const ctx = this.audio.ctx!;
    let made = this.made.get(i);
    if (!made) {
      made = new Array(Number(await i.fut.entry.sfx_kinds(0))).fill(null);
      this.made.set(i, made);
    }
    this.current = made;
    const k = made.indexOf(null);
    if (k < 0) return;
    const n = Math.round(Number(await i.fut.entry.sfx_seconds(k)) * ctx.sampleRate);
    const arr = await i.fut.entry.sfx(k, ctx.sampleRate, n);
    const data = (await arr.values()) as Float32Array;
    arr.free();
    const buf = ctx.createBuffer(2, n, ctx.sampleRate);
    const l = buf.getChannelData(0), r = buf.getChannelData(1);
    for (let j = 0; j < n; j++) {
      l[j] = data[2 * j];
      r[j] = data[2 * j + 1];
    }
    made[k] = buf;
  }

  // Plays effect k (one of SFX), placed.
  play(k: number, at: Placement = { pan: 0, gain: 1 }): void {
    const ctx = this.audio.ctx;
    const buf = this.current[k];
    if (!this.on || !ctx || !buf || ctx.state !== "running" || at.gain < 0.02) return;
    if (!this.out) {
      this.out = ctx.createGain();
      this.out.gain.value = VOLUME;
      this.out.connect(ctx.destination);
    }
    const src = ctx.createBufferSource();
    src.buffer = buf;
    const gain = ctx.createGain();
    gain.gain.value = at.gain;
    const pan = ctx.createStereoPanner();
    pan.pan.value = Math.max(-1, Math.min(1, at.pan));
    src.connect(gain).connect(pan).connect(this.out);
    src.start();
  }

  private render(): void {
    this.button.setAttribute("aria-pressed", String(this.on));
    if (!this.button.disabled) this.button.title = this.on ? "Sound effects on" : "Sound effects off";
    this.button.textContent = this.on ? "fx on" : "fx off";
  }
}
