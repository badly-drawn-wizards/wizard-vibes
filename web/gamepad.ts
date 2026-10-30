// Game controller input, through the Gamepad API (polled: it has no input
// events). Uses the "standard" layout (Xbox / PlayStation style); browsers
// only expose a controller once one of its buttons is pressed on the page.
//
//   left stick     fly along / across the camera's view
//   right stick    turn the camera
//   RB / RT        climb (the trigger by how far it is pulled)
//   LB / LT        descend
//   A / L3         boost
//   Y / Start      toggle free flight

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

let toggleHeld = false;

export function readPad(): Pad | null {
  const pads = [...(navigator.getGamepads?.() ?? [])].filter((p): p is Gamepad => !!p?.connected);
  const g = pads.find((p) => p.mapping === "standard") ?? pads[0];
  if (!g) return null;
  const b = (i: number) => g.buttons[i]?.value ?? 0;
  const pressed = (i: number) => g.buttons[i]?.pressed ?? false;
  const [mx, my] = stick(g.axes[0] ?? 0, g.axes[1] ?? 0);
  const [lx, ly] = stick(g.axes[2] ?? 0, g.axes[3] ?? 0);
  const t = pressed(3) || pressed(9);
  const toggle = t && !toggleHeld;
  toggleHeld = t;
  return {
    move: [mx, -my], // (stick axes point down)
    look: [lx, -ly],
    up: Math.max(-1, Math.min(1, Math.max(b(5), b(7)) - Math.max(b(4), b(6)))),
    boost: pressed(0) || pressed(10),
    toggle,
  };
}
