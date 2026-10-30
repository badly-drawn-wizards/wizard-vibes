// Game controller input, through the Gamepad API (polled: it has no input
// events). Browsers only expose a controller once one of its buttons is
// pressed on the page.
//
//   left stick     fly along / across the camera's view
//   right stick    turn the camera
//   RB / RT        climb (the trigger by how far it is pulled)
//   LB / LT        descend
//   A / L3         boost
//   X / Start      toggle free flight

export interface Pad {
  move: [number, number]; // x right, y forward
  look: [number, number]; // x right, y up
  up: number; // -1..1
  boost: boolean;
  toggle: boolean; // pressed since the last read
}

const DEAD = 0.15;

// A stick with a radial dead zone, rescaled so it still reaches 1.
function stick(x: number, y: number): [number, number] {
  const m = Math.hypot(x, y);
  if (m < DEAD) return [0, 0];
  const s = Math.min(1, (m - DEAD) / (1 - DEAD)) / m;
  return [x * s, y * s];
}

// Button indices by role (-1: not a button in that layout).
type Layout = Record<"a" | "x" | "lb" | "rb" | "lt" | "rt" | "start" | "l3", number>;

// The browser's "standard" layout.
const STANDARD: Layout = { a: 0, x: 2, lb: 4, rb: 5, lt: 6, rt: 7, start: 9, l3: 10 };

// Xbox controllers the browser does not map (e.g. over Bluetooth on Linux)
// report their raw HID buttons, with gaps where the codes have them; the
// triggers are axes there, so only the bumpers climb and descend.
const XBOX_RAW: Layout = { a: 0, x: 3, lb: 6, rb: 7, lt: -1, rt: -1, start: 11, l3: 13 };

function layout(g: Gamepad): Layout {
  if (g.mapping === "standard") return STANDARD;
  return /xbox|x-box|045e/i.test(g.id) || g.buttons.length >= 15 ? XBOX_RAW : STANDARD;
}

let toggleHeld = false;
const seen = new Set<string>();

export function readPad(): Pad | null {
  const pads = [...(navigator.getGamepads?.() ?? [])].filter((p): p is Gamepad => !!p?.connected);
  const g = pads.find((p) => p.mapping === "standard") ?? pads[0];
  if (!g) return null;
  const k = layout(g);
  if (!seen.has(g.id)) {
    seen.add(g.id);
    console.info(`controller: ${g.id} (${g.mapping || "no standard mapping"}, ${g.buttons.length} buttons, ${g.axes.length} axes; using the ${k === STANDARD ? "standard" : "raw Xbox"} layout)`);
  }
  const b = (i: number) => (i < 0 ? 0 : (g.buttons[i]?.value ?? 0));
  const pressed = (i: number) => i >= 0 && (g.buttons[i]?.pressed ?? false);
  const [mx, my] = stick(g.axes[0] ?? 0, g.axes[1] ?? 0);
  const [lx, ly] = stick(g.axes[2] ?? 0, g.axes[3] ?? 0);
  const t = pressed(k.x) || pressed(k.start);
  const toggle = t && !toggleHeld;
  toggleHeld = t;
  return {
    move: [mx, -my], // (stick axes point down)
    look: [lx, -ly],
    up: Math.max(-1, Math.min(1, Math.max(b(k.rb), b(k.rt)) - Math.max(b(k.lb), b(k.lt)))),
    boost: pressed(k.a) || pressed(k.l3),
    toggle,
  };
}
