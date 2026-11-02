// The mode, cycled by F (or Y / Start on a controller): fixed (he follows
// the flight path, as in the demo), flight (free flight) and fight (free
// flight, with the enemies). Remembered for the tab; a note says which on
// each change.

export type ModeName = "fixed" | "flight" | "fight";
const ORDER: ModeName[] = ["fixed", "flight", "fight"];
const LABEL: Record<ModeName, string> = { fixed: "Fixed: on the flight path", flight: "Flight: free flight", fight: "Fight: enemies about" };
const STORE = "wizard-mode";

export class Mode {
  name: ModeName = "fixed";
  onChange: (name: ModeName) => void = () => {};
  private note: HTMLElement;
  private hide: number | undefined;

  constructor(note: HTMLElement) {
    this.note = note;
    try {
      const s = sessionStorage.getItem(STORE) as ModeName | null;
      if (s && ORDER.includes(s)) this.name = s;
    } catch {
      // storage unavailable
    }
  }

  get free(): boolean {
    return this.name !== "fixed";
  }

  get fight(): boolean {
    return this.name === "fight";
  }

  cycle(): void {
    this.name = ORDER[(ORDER.indexOf(this.name) + 1) % ORDER.length];
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
}
