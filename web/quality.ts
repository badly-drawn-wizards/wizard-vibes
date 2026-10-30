// The quality panel: Auto / Low / Med / High. Each level is a preset of the
// compile-time step counts (params.json's "quality"); choosing one loads its
// build (the static site ships one per level) or, under the dev server,
// sets the preset so the server rebuilds. The new build is loaded and its
// shaders compiled while the current one keeps running (see swapTo).
//
// Auto watches the frame rate: once a level has settled it steps down if
// frames come slower than 40 fps (and will not go back up to that level),
// and up if they keep near the display's rate.

export type Level = "low" | "med" | "high";
export type Choice = "auto" | Level;
export const LEVELS: Level[] = ["low", "med", "high"];

const STORE = "wizard-quality";
const SETTLE_MS = 2500; // after a switch, before judging the frame rate
const WINDOW_MS = 3000; // how long a judgement averages over
const SLOW_FPS = 40;
const FAST_FPS = 57; // near a 60 Hz display's rate: room for more

export class Quality {
  choice: Choice | null; // null: none (the compile-time parameters match no preset)
  level: Level | null;
  private buttons = new Map<Choice, HTMLButtonElement>();
  private apply: (level: Level) => Promise<void>;
  private enabled: boolean;
  private busy = false;
  private since = performance.now(); // when the level last settled
  private frames = 0;
  private windowStart = 0;
  private tooSlow = new Set<Level>();

  // `level`: the level running now, if known. `apply` switches to a level
  // and resolves once it is running. `fallback`: the choice when none is
  // stored (the static site's is auto; under the dev server it is the level
  // running, since auto there rewrites the compile-time parameters).
  // Disabled, it does nothing (e.g. with a build pinned in the URL).
  constructor(host: HTMLElement, level: Level | null, apply: (level: Level) => Promise<void>, fallback: Choice | null, enabled = true) {
    this.apply = apply;
    this.enabled = enabled;
    this.level = level;
    let stored: string | null = null;
    try {
      stored = localStorage.getItem(STORE);
    } catch {
      // storage unavailable
    }
    this.choice = stored === "auto" || LEVELS.includes(stored as Level) ? (stored as Choice) : fallback;
    for (const c of ["auto", ...LEVELS] as Choice[]) {
      const b = document.createElement("button");
      b.type = "button";
      b.onclick = () => this.choose(c);
      this.buttons.set(c, b);
      host.append(b);
    }
    this.render();
  }

  // The level to start at: the chosen one, or medium for auto.
  get initial(): Level {
    return this.choice === "auto" || this.choice === null ? "med" : this.choice;
  }

  private choose(c: Choice): void {
    this.choice = c;
    try {
      localStorage.setItem(STORE, c);
    } catch {
      // ignore
    }
    this.tooSlow.clear();
    if (c !== "auto") void this.switchTo(c);
    this.render();
  }

  private async switchTo(level: Level): Promise<void> {
    if (level === this.level && !this.busy) return;
    this.busy = true;
    this.render();
    try {
      await this.apply(level);
      this.level = level;
    } finally {
      this.busy = false;
      this.since = performance.now();
      this.frames = 0;
      this.render();
    }
  }

  // Called once per drawn frame; `counting` is false while paused or hidden.
  frame(counting: boolean): void {
    if (!this.enabled || this.choice !== "auto" || this.busy || !counting) {
      this.frames = 0;
      return;
    }
    const now = performance.now();
    if (now - this.since < SETTLE_MS) return;
    if (this.frames === 0) this.windowStart = now;
    this.frames++;
    const elapsed = now - this.windowStart;
    if (elapsed < WINDOW_MS) return;
    const fps = ((this.frames - 1) * 1000) / elapsed;
    this.frames = 0;
    const i = this.level ? LEVELS.indexOf(this.level) : 1;
    if (fps < SLOW_FPS && i > 0) {
      if (this.level) this.tooSlow.add(this.level);
      void this.switchTo(LEVELS[i - 1]);
    } else if (fps > FAST_FPS && i < LEVELS.length - 1 && !this.tooSlow.has(LEVELS[i + 1])) {
      void this.switchTo(LEVELS[i + 1]);
    } else if (!this.level) void this.switchTo(LEVELS[i]);
  }

  private render(): void {
    for (const [c, b] of this.buttons) {
      const name = { auto: "Auto", low: "Low", med: "Med", high: "High" }[c];
      b.textContent = c === "auto" && this.choice === "auto" && this.level ? `Auto·${this.level}` : name;
      b.setAttribute("aria-pressed", String(c === this.choice));
      b.classList.toggle("active", c === this.level && this.choice === "auto");
      b.title = c === "auto" ? "Pick the quality by the frame rate" : `Quality: ${name}${this.busy && c === this.choice ? " (loading…)" : ""}`;
    }
    this.buttons.get("auto")!.parentElement?.classList.toggle("busy", this.busy);
  }
}
