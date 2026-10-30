// Orbit camera driven by pointer drag and wheel. State survives hot reloads
// via sessionStorage.

export interface Camera {
  yaw: number;
  pitch: number;
  dist: number;
  interacted: boolean;
}

const KEY = "wizard-camera";

export function loadCamera(): Camera {
  try {
    const saved = JSON.parse(sessionStorage.getItem(KEY) ?? "null");
    if (saved) return { ...saved, interacted: true };
  } catch {
    // storage unavailable; use defaults
  }
  return { yaw: 0.5, pitch: 0.12, dist: 4.4, interacted: false };
}

function save(cam: Camera): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify({ yaw: cam.yaw, pitch: cam.pitch, dist: cam.dist }));
  } catch {
    // ignore
  }
}

export function attachCamera(cam: Camera, el: HTMLElement): void {
  let drag: { x: number; y: number } | null = null;
  el.addEventListener("pointerdown", (e) => {
    drag = { x: e.clientX, y: e.clientY };
    el.setPointerCapture(e.pointerId);
    cam.interacted = true;
  });
  el.addEventListener("pointermove", (e) => {
    if (!drag) return;
    cam.yaw -= (e.clientX - drag.x) * 0.006;
    cam.pitch = Math.max(-0.05, Math.min(1.2, cam.pitch + (e.clientY - drag.y) * 0.006));
    drag = { x: e.clientX, y: e.clientY };
    save(cam);
  });
  el.addEventListener("pointerup", () => (drag = null));
  el.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      cam.interacted = true;
      cam.dist = Math.max(2.5, Math.min(12, cam.dist * Math.exp(e.deltaY * 0.001)));
      save(cam);
    },
    { passive: false },
  );
}

// Slow idle orbit until the user touches the camera: one revolution per
// three 12 s animation loops, so the idle view loops too.
export function idle(cam: Camera, dt: number): void {
  if (!cam.interacted) cam.yaw += (dt * 2 * Math.PI) / 36;
}
