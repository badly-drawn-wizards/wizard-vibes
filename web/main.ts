// WebGPU harness: Futhark renders the frame into a storage buffer on its own
// device; we copy that buffer straight into the canvas texture on the GPU.

import { attachCamera, idle, loadCamera, nudgeCamera } from "./camera.ts";
import { readPad } from "./gamepad.ts";
import { buildRecord, type FutharkArray, type FutInstance, loadFuthark, rawBuffer, unloadFuthark } from "./futhark-loader.ts";
import { clearError, setStatus, showError } from "./overlay.ts";
import { createDrawer, type Params } from "./params-drawer.ts";
import { type Drive, FreeFlight, isTyping, ON_PATH } from "./flight.ts";
import { type Device, Help } from "./help.ts";
import { Spells } from "./spells.ts";

// The drawer's uniforms, passed to render as its params record: rebuilt
// (per loaded build) only when they change.
const params: Params = await createDrawer(document.getElementById("params")!);
const records = new WeakMap<FutInstance, { version: number; rec: { ptr: number; free(): void } }>();
function paramsRecord(i: FutInstance) {
  const r = records.get(i);
  if (r && r.version === params.version) return r.rec;
  r?.rec.free();
  const rec = buildRecord(i, "params", params.values);
  records.set(i, { version: params.version, rec });
  return rec;
}
// Whether an entry point of this build takes the named input (older builds
// take fewer: kept working for benchmarking against them).
const takes = (i: FutInstance, entry: string, input: string): boolean =>
  !!i.fut.manifest.entry_points[entry]?.inputs.some((x: { name: string }) => x.name === input);

// A []f32 input (freed by the caller).
const f32Array = (i: FutInstance, data: Float32Array): FutharkArray => i.fut.types["[]f32"].from_data(data, data.length);

// Runs render with the flight state as its drive record and the spells in
// flight as an array.
async function render(i: FutInstance, w: number, vw: number, h: number, t: number, yaw: number, pitch: number, dist: number, d: Drive, spellData?: Float32Array): Promise<FutharkArray> {
  if (!takes(i, "render", "dr")) return i.fut.entry.render(w, vw, h, t, yaw, pitch, dist, paramsRecord(i));
  const drive = buildRecord(i, "drive", d);
  const sp = takes(i, "render", "spells") ? f32Array(i, spellData ?? new Float32Array(0)) : null;
  try {
    return await (sp
      ? i.fut.entry.render(w, vw, h, t, yaw, pitch, dist, paramsRecord(i), drive, sp)
      : i.fut.entry.render(w, vw, h, t, yaw, pitch, dist, paramsRecord(i), drive));
  } finally {
    drive.free();
    sp?.free();
  }
}

// The collision probe, from the same state, also at the given points.
async function probeEntry(i: FutInstance, t: number, d: Drive, pts: Float32Array): Promise<FutharkArray> {
  const drive = buildRecord(i, "drive", d);
  const p = takes(i, "probe", "pts") ? f32Array(i, pts) : null;
  try {
    return await (p ? i.fut.entry.probe(t, paramsRecord(i), drive, p) : i.fut.entry.probe(t, paramsRecord(i), drive));
  } finally {
    drive.free();
    p?.free();
  }
}

document.getElementById("drawer-toggle")!.onclick = () => document.body.classList.toggle("drawer-open");

const canvas = document.getElementById("view") as HTMLCanvasElement;
const ctx = canvas.getContext("webgpu")!;
const cam = loadCamera();
attachCamera(cam, canvas);

// Free flight (F toggles): fly in from behind him. Clicking the view takes
// the keys back from the drawer's controls.
const flight = new FreeFlight();
canvas.addEventListener("pointerdown", () => (document.activeElement as HTMLElement | null)?.blur?.());
flight.onToggle = (on) => {
  if (!on) return;
  cam.yaw = Math.PI;
  cam.pitch = 0.3;
  cam.interacted = true;
};

// Spells, cast from the orb along the camera's view: 1 2 3 choose, Space
// casts (or X A B and RB on a controller). How many may be out at once is
// a compile-time parameter, asked of each build.
const spells = new Spells();
const spellLimits = new WeakMap<FutInstance, number>();
let casting = false; // Space held

// The controls panel, showing the keys of whichever device was used last.
const help = new Help(document.getElementById("help")!, document.getElementById("help-open")!);
let device: Device = "keyboard";

// The collision probe: its few floats (for the rock and him, then each
// spell) are copied into a mappable buffer with the frame, and read back
// whenever the GPU gets to them, without waiting. Meanwhile further probes
// are skipped.
const SPHERES = 6; // see collision_spheres in fut/collide.fut
const probes = new WeakMap<GPUDevice, { buf: GPUBuffer; busy: boolean; ids: number[] }>();
function probeReader(device: GPUDevice, bytes: number) {
  let r = probes.get(device);
  if (!r || r.buf.size !== bytes) {
    if (r && !r.busy) r.buf.destroy();
    r = { buf: device.createBuffer({ size: bytes, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ }), busy: false, ids: [] };
    probes.set(device, r);
  }
  return r;
}

// The orb, and the camera's view, in the world (for casting).
function castFrom(): { origin: [number, number, number]; dir: [number, number, number] } {
  const { pos, heading } = flight.where(time);
  const c = Math.cos(heading), s = Math.sin(heading);
  const turn = (x: number, y: number, z: number): [number, number, number] => [x * c + z * s, y, z * c - x * s];
  const o = turn(0.55, 2.1, 0.5); // the orb, in his frame (orb_pos in fut/scene.fut)
  const cp = Math.cos(cam.pitch);
  return {
    origin: [pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]],
    dir: turn(-cp * Math.sin(cam.yaw), -Math.sin(cam.pitch), -cp * Math.cos(cam.yaw)),
  };
}

let renderScale = Number(sessionStorage.getItem("wizard-scale") ?? 1) || 1;
for (const b of document.querySelectorAll<HTMLButtonElement>("#scale button")) {
  const s = Number(b.dataset.scale);
  b.setAttribute("aria-pressed", String(s === renderScale));
  b.onclick = () => {
    renderScale = s;
    try { sessionStorage.setItem("wizard-scale", String(s)); } catch { /* ignore */ }
    for (const o of document.querySelectorAll("#scale button")) o.setAttribute("aria-pressed", String(o === b));
  };
}

let inst: FutInstance | null = null;
let prev: FutharkArray | null = null; // last frame's output, freed after the next submit
let inFlight: Promise<void> | null = null;
let paused = false;
let padConnected = false;

let time = 0;
let fps = 0;
let last = performance.now();

window.addEventListener("keydown", (e) => {
  if (isTyping(e)) return;
  device = "keyboard";
  if (e.code === "KeyP") paused = !paused;
  else if (e.code === "Space") {
    casting = true;
    e.preventDefault();
  } else if (/^Digit[123]$/.test(e.code)) spells.select(Number(e.code.slice(5)) - 1);
});
window.addEventListener("keyup", (e) => {
  if (e.code === "Space") casting = false;
});
window.addEventListener("blur", () => (casting = false));

function configure(device: GPUDevice): void {
  ctx.unconfigure();
  ctx.configure({
    device,
    format: "bgra8unorm", // matches Futhark's packed BGRA8 u32 pixels
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_DST,
    alphaMode: "opaque",
  });
}

async function frame(i: FutInstance, dt: number): Promise<void> {
  const dpr = window.devicePixelRatio || 1;
  const vw = Math.max(1, Math.floor(canvas.clientWidth * dpr * renderScale));
  const h = Math.max(1, Math.floor(canvas.clientHeight * dpr * renderScale));
  const w = Math.ceil(vw / 64) * 64; // bytesPerRow must be a multiple of 256
  if (canvas.width !== vw || canvas.height !== h) {
    canvas.width = vw;
    canvas.height = h;
  }
  if (!paused) time += dt;
  idle(cam, dt);
  const pad = readPad();
  padConnected = !!pad;
  if (pad?.toggle) flight.toggle();
  if (pad?.active) device = "pad";
  if (pad?.spell != null) spells.select(pad.spell);
  spells.limit = spellLimits.get(i) ?? spells.limit;
  if (!paused && (casting || pad?.cast)) {
    const { origin, dir } = castFrom();
    spells.cast(origin, dir);
  }
  spells.update(paused ? 0 : dt, casting || !!pad?.cast);
  help.update(device, spells.selected);
  // right stick: look right turns the view right (the camera's yaw is its
  // azimuth round him, so it decreases), up raises the view
  if (pad) nudgeCamera(cam, -2.5 * pad.look[0] * dt, -1.5 * pad.look[1] * dt);
  // The camera orbits in his frame, which turns with him; in free flight keep
  // it still in the world instead, so W (along the camera) does not chase its
  // own tail as he turns to face the way he goes. (camera_ray in
  // fut/wizard.fut looks along -(sin yaw, cos yaw) in his frame.)
  const heading = flight.heading;
  const stick = pad && { fwd: pad.move[1], side: pad.move[0], up: pad.up, boost: pad.boost };
  flight.update(paused ? 0 : dt, time, heading + cam.yaw + Math.PI, stick);
  if (flight.active) cam.yaw -= flight.heading - heading;

  // casting, in his frame: the camera looks along -(cos p sin y, sin p, cos p cos y)
  const cp = Math.cos(cam.pitch);
  const d: Drive = {
    ...flight.drive(),
    staff: spells.staff, sx: -cp * Math.sin(cam.yaw), sy: -Math.sin(cam.pitch), sz: -cp * Math.cos(cam.yaw),
    flash: spells.sinceCast, spell: spells.selected,
  };
  const points = takes(i, "probe", "pts") ? spells.probePoints() : { ids: [], pts: new Float32Array(0) };
  const probeBytes = (SPHERES + points.pts.length / 3) * 16;
  const reader = probeReader(i.device, probeBytes);
  const wanted = flight.active || spells.count > 0;
  const probe = wanted && !reader.busy && i.fut.entry.probe ? await probeEntry(i, time, d, points.pts) : null;
  const out = await render(i, w, vw, h, time, cam.yaw, cam.pitch, cam.dist, d, spells.pack());
  const enc = i.device.createCommandEncoder();
  if (probe) enc.copyBufferToBuffer(rawBuffer(i, probe, "f32_1d"), 0, reader.buf, 0, probeBytes);
  enc.copyBufferToTexture(
    { buffer: rawBuffer(i, out), bytesPerRow: w * 4, rowsPerImage: h },
    { texture: ctx.getCurrentTexture() },
    [vw, h],
  );
  i.device.queue.submit([enc.finish()]);
  // Futhark may now recycle the previous buffer: its writes are queued after our copy.
  prev?.free();
  prev = out;
  if (probe) {
    probe.free(); // (likewise)
    reader.busy = true;
    reader.ids = points.ids;
    reader.buf
      .mapAsync(GPUMapMode.READ)
      .then(() => {
        const data = new Float32Array(reader.buf.getMappedRange().slice(0));
        reader.buf.unmap();
        flight.collide(data.subarray(0, SPHERES * 4));
        spells.applyProbe(reader.ids, data.subarray(SPHERES * 4));
      })
      .catch(() => {}) // the device went away
      .finally(() => (reader.busy = false));
  }
}

function tick(now: number): void {
  requestAnimationFrame(tick);
  if (!inst || inFlight) return;
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  fps = fps * 0.9 + (dt > 0 ? 1 / dt : 0) * 0.1;
  const current = inst;
  inFlight = frame(current, dt)
    .then(() => setStatus(`${current.version.slice(0, 8)}${current.f32 ? " · f32" : ""}${current.portable ? " · portable" : ""} · ${canvas.width}×${canvas.height} · ${fps.toFixed(1)} fps${paused ? " · paused" : ""}${flight.active ? " · free flight" : ""}${padConnected ? " · controller" : ""}`))
    .catch((e) => showError(`render failed: ${e?.stack ?? e}`))
    .finally(() => (inFlight = null));
}

// Recently used builds stay loaded (each with its own device), so switching
// back to one is instant: builds are content-addressed, so an id always
// means the same program. Mostly for benchmarking variants back to back.
const KEEP_LOADED = 12;
const loaded = new Map<string, FutInstance>(); // in least-recently-used order

async function swapTo(version: string): Promise<void> {
  if (inst?.version === version) return; // e.g. a restarted server re-announcing it
  let next = loaded.get(version);
  if (next) loaded.delete(version);
  else {
    const started = performance.now();
    next = await loadFuthark(version);
    console.info(`loaded ${version} in ${Math.round(performance.now() - started)} ms`);
    next.device.lost.then((info) => {
      if (info.reason !== "destroyed") showError(`GPU device lost: ${info.message}`);
    });
    if (next.shaders.warnings) console.info(`${version}: ${next.shaders.warnings} WGSL warnings (see console)`);
    if (next.fut.entry.spell_limit) spellLimits.set(next, Number(await next.fut.entry.spell_limit(0)));
  }
  // Stop the loop on the old instance and let its last frame finish.
  const old = inst;
  inst = null;
  await inFlight;
  prev?.free();
  prev = null;
  if (old && old !== next) loaded.set(old.version, old);
  while (loaded.size > KEEP_LOADED) {
    const [id, evicted] = loaded.entries().next().value!;
    loaded.delete(id);
    records.get(evicted)?.rec.free();
    unloadFuthark(evicted);
  }
  configure(next.device);
  next.shaders.onError = (report) => showError(`WGSL compilation error:\n\n${report}`);
  inst = next;
  clearError();
}

let loading = Promise.resolve();
function reload(version: string): Promise<void> {
  loading = loading.then(() =>
    swapTo(version).catch((e) => showError(`failed to load build ${version}:\n${e?.message ?? e}`)),
  );
  return loading;
}

if (!navigator.gpu) showError("WebGPU is not available in this browser.");

// Debug/test hook: render one frame and read it back to the CPU. The render
// loop is held off meanwhile, since entry points must not run concurrently.
(window as unknown as { wizard: object }).wizard = {
  flight,
  cam,
  spells,
  async readFrame(w: number, vw: number, h: number, t: number, yaw: number, pitch: number, dist: number) {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const out = await render(i, w, vw, h, t, yaw, pitch, dist, flight.drive());
      const pixels = await out.values();
      out.free();
      return { f32: i.f32, portable: i.portable, pixels: Array.from(pixels) };
    } finally {
      inst = i;
    }
  },
  // Run the collision probe for a flight state, read back (for tests).
  async probe(d: Drive) {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const out = await probeEntry(i, time, d, new Float32Array(0));
      const v = Array.from(await out.values());
      out.free();
      return v;
    } finally {
      inst = i;
    }
  },
  // Load another build into the running page (for the benchmark daemon);
  // resolves to the loaded version, or rejects with the load error.
  async swap(version: string) {
    await reload(version);
    if (inst?.version !== version) throw new Error(document.getElementById("error")?.textContent ?? "load failed");
    return version;
  },
  // Average GPU time per frame in ms, without presenting or vsync.
  async bench(frames: number, vw: number, h: number) {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const w = Math.ceil(vw / 64) * 64;
      const run = async (t: number) => (await render(i, w, vw, h, t, cam.yaw, cam.pitch, cam.dist, ON_PATH)).free();
      await run(0); // warm-up: pipeline creation
      await i.fut.context_sync();
      const start = performance.now();
      for (let f = 0; f < frames; f++) await run(f / 30);
      await i.fut.context_sync();
      return (performance.now() - start) / frames;
    } finally {
      inst = i;
    }
  },
};

const events = new EventSource("/events");
events.addEventListener("building", () => setStatus("compiling…"));
events.addEventListener("error", (e) => {
  if (e instanceof MessageEvent) showError(JSON.parse(e.data).message);
});
// ?build=N pins a specific build (for benchmarking variants); otherwise follow the server.
const pinned = new URLSearchParams(location.search).get("build");
if (pinned) reload(pinned);
else events.addEventListener("fut-reload", (e) => reload(JSON.parse((e as MessageEvent).data).version));
events.addEventListener("web-reload", () => location.reload());

requestAnimationFrame(tick);
