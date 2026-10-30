// Free flight: the keyboard flies the wizard's rock through the world. The
// physics runs here; the renderer gets the result as its `drive` record, and
// the `probe` entry point reports the land around the rock (see
// fut/collide.fut), which pushes it back out of anything it flies into.
//
//   W / S   accelerate / brake        A / D   accelerate left / right
//   E / Q   climb / descend           Shift   boost
//
// He always faces the way he travels (turning towards it gradually), banks
// into turns, and points the way he accelerates: the harder, the more.

export interface Drive {
  free: boolean;
  x: number;
  y: number;
  z: number;
  heading: number;
  turn: number;
  ax: number;
  ay: number;
  az: number;
}

export const ON_PATH: Drive = { free: false, x: 0, y: 0, z: 0, heading: 0, turn: 0, ax: 0, ay: 0, az: 0 };

// The automatic flight path (as in fut/world.fut), to take off from.
const SPEED = 3;
const ALTITUDE = 12;
const pathX = (s: number) => 14 * Math.sin(0.12 * s) + 6 * Math.sin(0.27 * s + 1);
const pathDx = (s: number) => 14 * 0.12 * Math.cos(0.12 * s) + 6 * 0.27 * Math.cos(0.27 * s + 1);

const THRUST = 9; // units/s², forward
const SIDE = 7; // sideways
const CLIMB = 6;
const BRAKE = 10;
const DRAG = 0.5; // per second: top speed is thrust / drag
const BOOST = 2.5;
const TURN_RATE = 2.5; // how fast the heading follows the velocity (1/s)
const MAX_TURN = 1.2; // rad/s
const CEILING = 80;

type V3 = [number, number, number];
const STORE = "wizard-flight";

const KEYS: Record<string, string> = {
  KeyW: "fwd", ArrowUp: "fwd", KeyS: "back", ArrowDown: "back",
  KeyA: "left", ArrowLeft: "left", KeyD: "right", ArrowRight: "right",
  KeyE: "up", KeyQ: "down", ShiftLeft: "boost", ShiftRight: "boost",
};

const smoothstep = (a: number, b: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
};

export class FreeFlight {
  active = false;
  pos: V3 = [0, ALTITUDE, 0];
  vel: V3 = [0, 0, SPEED];
  heading = 0;
  turn = 0;
  private accel: V3 = [0, 0, 0]; // smoothed, world space
  private lastVel: V3 = [0, 0, SPEED]; // at the end of the last update
  private held = new Set<string>();

  constructor() {
    window.addEventListener("keydown", (e) => {
      if (isTyping(e)) return;
      if (e.code === "KeyF" && !e.ctrlKey && !e.metaKey && !e.altKey) this.toggle();
      const k = KEYS[e.code];
      if (k && this.active) {
        this.held.add(k);
        e.preventDefault();
      }
    });
    window.addEventListener("keyup", (e) => {
      const k = KEYS[e.code];
      if (k) this.held.delete(k);
    });
    window.addEventListener("blur", () => this.held.clear());
    try {
      const s = JSON.parse(sessionStorage.getItem(STORE) ?? "null");
      if (s) Object.assign(this, { active: s.active, pos: s.pos, vel: s.vel, heading: s.heading });
    } catch {
      // storage unavailable: start on the path
    }
  }

  // Called with the animation time when free flight is switched on.
  onToggle: (active: boolean) => void = () => {};
  private time = 0;

  toggle(): void {
    this.active = !this.active;
    if (this.active) {
      // take off from where the path has him now, at its speed
      const s = this.time;
      const dx = pathDx(s);
      this.pos = [pathX(s), ALTITUDE, SPEED * s];
      this.vel = [dx, 0, SPEED];
      this.heading = Math.atan2(dx, SPEED);
      this.turn = 0;
      this.accel = [0, 0, 0];
      this.lastVel = [...this.vel];
    }
    this.save();
    this.onToggle(this.active);
  }

  update(dt: number, time: number): void {
    this.time = time;
    if (!this.active || dt <= 0) return;
    const h = this.held;
    const boost = h.has("boost") ? BOOST : 1;
    const fwd: V3 = [Math.sin(this.heading), 0, Math.cos(this.heading)];
    const right: V3 = [-fwd[2], 0, fwd[0]]; // his right: local -x
    const k = (a: string, b: string) => (h.has(a) ? 1 : 0) - (h.has(b) ? 1 : 0);
    const a: V3 = [0, 0, 0];
    const thrust = h.has("fwd") ? THRUST * boost : 0;
    const side = k("right", "left") * SIDE * boost;
    for (let i = 0; i < 3; i++) a[i] = fwd[i] * thrust + right[i] * side - DRAG * this.vel[i];
    a[1] += k("up", "down") * CLIMB * boost;
    if (h.has("back")) {
      // brake, down to a stop but not into reverse
      const sp = Math.hypot(...this.vel);
      if (sp > 1e-3) for (let i = 0; i < 3; i++) a[i] -= (this.vel[i] / sp) * Math.min(BRAKE, sp / dt);
    }
    for (let i = 0; i < 3; i++) {
      this.vel[i] += a[i] * dt;
      this.pos[i] += this.vel[i] * dt;
    }
    if (this.pos[1] > CEILING) {
      this.pos[1] = CEILING;
      this.vel[1] = Math.min(0, this.vel[1]);
    }

    // face the way he travels
    const hs = Math.hypot(this.vel[0], this.vel[2]);
    let turn = 0;
    if (hs > 0.5) {
      let d = Math.atan2(this.vel[0], this.vel[2]) - this.heading;
      d = Math.atan2(Math.sin(d), Math.cos(d));
      turn = Math.max(-MAX_TURN, Math.min(MAX_TURN, d * TURN_RATE));
    }
    this.heading += turn * dt;
    this.turn += (turn - this.turn) * (1 - Math.exp(-dt / 0.15));

    // the acceleration he feels (thrust, drag, turning and the land pushing
    // back alike: pressing into the ground is no acceleration), smoothed
    const f = 1 - Math.exp(-dt / 0.2);
    for (let i = 0; i < 3; i++) this.accel[i] += ((this.vel[i] - this.lastVel[i]) / dt - this.accel[i]) * f;
    this.lastVel = [...this.vel];
    this.save();
  }

  // Pushes the rock out of the land the probe found it in: per sphere,
  // [distance to the land (negative inside), normal x, y, z].
  lastProbe?: Float32Array; // (for debugging)
  collide(probe: Float32Array): void {
    this.lastProbe = probe;
    if (!this.active) return;
    for (let s = 0; s + 3 < probe.length; s += 4) {
      const d = probe[s];
      if (!(d < 0)) continue;
      const n = [probe[s + 1], probe[s + 2], probe[s + 3]];
      const vn = this.vel[0] * n[0] + this.vel[1] * n[1] + this.vel[2] * n[2];
      for (let i = 0; i < 3; i++) {
        this.pos[i] -= n[i] * d;
        if (vn < 0) this.vel[i] -= n[i] * vn; // stop going in; slide along
      }
    }
  }

  drive(): Drive {
    if (!this.active) return ON_PATH;
    // the felt acceleration in his frame (+x his left, +z ahead)
    const c = Math.cos(this.heading), s = Math.sin(this.heading);
    const [wx, wy, wz] = this.accel;
    const lx = wx * c - wz * s, lz = wz * c + wx * s;
    const mag = Math.hypot(lx, wy, lz);
    const amount = smoothstep(1.5, 7, mag) / (mag || 1);
    return {
      free: true, x: this.pos[0], y: this.pos[1], z: this.pos[2], heading: this.heading, turn: this.turn,
      ax: lx * amount, ay: wy * amount, az: lz * amount,
    };
  }

  private save(): void {
    try {
      sessionStorage.setItem(STORE, JSON.stringify({ active: this.active, pos: this.pos, vel: this.vel, heading: this.heading }));
    } catch {
      // ignore
    }
  }
}

function isTyping(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA");
}
