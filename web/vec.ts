// Small 3-vector helpers (world space: y up).

export type V3 = [number, number, number];

export const add = (a: V3, b: V3): V3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: V3, b: V3): V3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (s: number, a: V3): V3 => [s * a[0], s * a[1], s * a[2]];
export const dot = (a: V3, b: V3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const len = (a: V3) => Math.hypot(a[0], a[1], a[2]);
export const dist = (a: V3, b: V3) => len(sub(a, b));
export const normalise = (a: V3): V3 => scale(1 / (len(a) || 1), a);

// A direction turned by `yaw` about the vertical (yaw in fut/world.fut).
export const turn = (a: number, [x, y, z]: V3): V3 => [x * Math.cos(a) + z * Math.sin(a), y, z * Math.cos(a) - x * Math.sin(a)];
