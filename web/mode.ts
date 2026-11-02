// The mode, cycled by F (or Y / Start on a controller) or chosen in the top
// bar: fixed (he follows the flight path, as in the demo), flight (free
// flight) and fight (free flight, with the enemies). Remembered for the
// tab; a note says which on each change.

export type ModeName = "fixed" | "flight" | "fight";
const ORDER: ModeName[] = ["fixed", "flight", "fight"];
const LABEL: Record<ModeName, string> = { fixed: "Fixed: on the flight path", flight: "Flight: free flight", fight: "Fight: enemies about" };
const STORE = "wizard-mode";

const TITLE: Record<ModeName, string> = { fixed: "Fixed", flight: "Flight", fight: "Fight" };

export class Mode {
  name: ModeName = "fixed";
  onChange: (name: ModeName) => void = () => {};
  private note: HTMLElement;
  private hide: number | undefined;
  private buttons = new Map<ModeName, HTMLButtonElement>();
  private allowed = new Set<ModeName>(ORDER);

  constructor(note: HTMLElement, panel: HTMLElement) {
    this.note = note;
    try {
      const s = sessionStorage.getItem(STORE) as ModeName | null;
      if (s && ORDER.includes(s)) this.name = s;
    } catch {
      // storage unavailable
    }
    for (const m of ORDER) {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = TITLE[m];
      b.title = `${LABEL[m]} (F cycles)`;
      b.onclick = () => this.set(m);
      this.buttons.set(m, b);
      panel.append(b);
    }
    this.render();
  }

  // Rules a mode out (e.g. fight, where spells cannot be drawn).
  forbid(m: ModeName, reason: string): void {
    this.allowed.delete(m);
    const b = this.buttons.get(m)!;
    b.disabled = true;
    b.title = reason;
    if (this.name === m) this.name = "flight";
    this.render();
  }

  get free(): boolean {
    return this.name !== "fixed";
  }

  get fight(): boolean {
    return this.name === "fight";
  }

  cycle(): void {
    let i = ORDER.indexOf(this.name);
    do i = (i + 1) % ORDER.length;
    while (!this.allowed.has(ORDER[i]));
    this.set(ORDER[i]);
  }

  set(m: ModeName): void {
    if (!this.allowed.has(m) || m === this.name) return;
    this.name = m;
    this.render();
    try {
      sessionStorage.setItem(STORE, this.name);
    } catch {
      // ignore
    }
    this.note.textContent = LABEL[this.name];
    this.note.classList.add("on");
    clearTimeout(this.hide);
    this.hide = window.setTimeout(() => this.note.classList.remove("on"), 1600);
    this.onChange(this.name);
  }

  private render(): void {
    for (const [m, b] of this.buttons) b.setAttribute("aria-pressed", String(m === this.name));
  }
}
