// WebGPU harness: Futhark renders the frame into a storage buffer on its own
// device; we copy that buffer straight into the canvas texture on the GPU.

import { attachCamera, idle, loadCamera, nudgeCamera, viewDir } from "./camera.ts";
import { readPad } from "./gamepad.ts";
import { type FutharkArray, type FutInstance, loadFuthark, rawBuffer, unloadFuthark } from "./futhark-loader.ts";
import { Entries, takes } from "./entries.ts";
import { ProbeReader } from "./probe.ts";
import { clearError, setStatus, showError } from "./overlay.ts";
import { createDrawer, type Params } from "./params-drawer.ts";
import { type Drive, FreeFlight, isTyping, ON_PATH } from "./flight.ts";
import { type Device, Help } from "./help.ts";
import { Spells } from "./spells.ts";
import { Combat } from "./combat.ts";
import { Mode } from "./mode.ts";
import { Music } from "./music.ts";
import { AudioHub } from "./audio.ts";
import { Sfx } from "./sfx.ts";
import { IS_STATIC } from "./static.ts";
import { GlRenderer } from "./webgl.ts";
import { LEVELS, type Level, Quality } from "./quality.ts";

// The drawer's uniforms, and the entry points that take them.
const params: Params = await createDrawer(document.getElementById("params")!);

// What he flies on: the top bar's choice, kept in step with the drawer's.
const platform = document.getElementById("platform") as HTMLSelectElement;
platform.value = String(params.values.platform ?? 0);
platform.onchange = () => params.set("platform", Number(platform.value));
params.listen((id, v) => id === "platform" && (platform.value = String(v)));
const entries = new Entries(params);

document.getElementById("drawer-toggle")!.onclick = () => document.body.classList.toggle("drawer-open");

const canvas = document.getElementById("view") as HTMLCanvasElement;
// Without WebGPU, the WebGL2 fallback draws the scene from a fragment shader
// instead (see webgl.ts). It can also be chosen with the WebGPU / WebGL
// buttons (remembered), or forced with ?webgl. A canvas takes one kind of
// context for good, so switching reloads the page (the camera, flight and
// uniforms are kept in storage).
const BACKEND = "wizard-backend";
const storedBackend = (() => {
  try {
    return localStorage.getItem(BACKEND);
  } catch {
    return null;
  }
})();
// Whether WebGPU works here: a browser can have navigator.gpu with no
// adapter behind it, or an adapter that never answers, or one that then
// fails when the module creates its device (recorded below when the first
// build fails to load, so the page reloads into WebGL with the reason).
const FAILED = "wizard-webgpu-failed";
const failure = (() => {
  try {
    return sessionStorage.getItem(FAILED);
  } catch {
    return null;
  }
})();
const adapter = failure ? null : await Promise.race([
  navigator.gpu?.requestAdapter().catch(() => null) ?? null,
  new Promise<null>((r) => setTimeout(() => r(null), 3000)),
]);
const hasWebGpu = !!adapter;
const noWebGpu = failure ? `WebGPU failed here: ${failure}` : "WebGPU is not available in this browser";
const useGl = !hasWebGpu || new URLSearchParams(location.search).has("webgl") || storedBackend === "webgl";
for (const b of document.querySelectorAll<HTMLButtonElement>("#backend button")) {
  const gl = b.dataset.backend === "webgl";
  b.setAttribute("aria-pressed", String(gl === useGl));
  if (!gl && !hasWebGpu) {
    // (after a failure it can still be tried again)
    b.disabled = !failure;
    b.title = noWebGpu;
  }
  b.onclick = () => {
    if (gl === useGl && !(failure && !gl)) return;
    try {
      localStorage.setItem(BACKEND, gl ? "webgl" : "webgpu");
      if (!gl) sessionStorage.removeItem(FAILED);
    } catch {
      // storage unavailable: ?webgl still chooses for this page
    }
    const url = new URL(location.href);
    if (gl) url.searchParams.set("webgl", "");
    else url.searchParams.delete("webgl");
    location.replace(url.toString().replace(/webgl=(&|$)/, "webgl$1"));
  };
}
// Whether this browser gives out WebGL2 contexts (on a spare canvas).
const hasWebGl2 = (): boolean => !!document.createElement("canvas").getContext("webgl2");

// Neither renderer works: say why, once, and stop (no reloading).
let unsupported = false;
let webGpuError: string | null = null;
function noRendererAfter(message: string): void {
  webGpuError = message.split("\n")[0];
  noRenderer();
}
function noRenderer(): void {
  unsupported = true;
  setStatus("no WebGPU or WebGL2");
  showError(
    `This browser offers neither WebGPU nor WebGL2, so there is nothing to draw with.\n\n` +
      `WebGPU: ${webGpuError ?? failure ?? (navigator.gpu ? "no adapter" : "not available")}\n` +
      `WebGL2: no context\n\n` +
      `Both need hardware acceleration: see chrome://gpu (Chromium) or about:support (Firefox). ` +
      `Chromium only offers WebGL without a usable GPU with --enable-unsafe-swiftshader, ` +
      `and WebGPU may need --enable-unsafe-webgpu and --enable-features=Vulkan on Linux.`,
  );
}

let glr: GlRenderer | null = null;
if (useGl) {
  try {
    glr = new GlRenderer(canvas);
  } catch {
    noRenderer();
  }
}
const ctx = (useGl ? null : canvas.getContext("webgpu"))!;
const cam = loadCamera();
attachCamera(cam, canvas);

// The mode (F cycles: see mode.ts): fixed on the flight path, free flight,
// or free flight with the enemies (outside it the renderer gets none,
// whatever the drawer says). Free flight starts from behind him. Clicking
// the view takes the keys back from the drawer's controls.
const flight = new FreeFlight();
const mode = new Mode(document.getElementById("mode-note")!, document.getElementById("modes")!);
if (useGl) mode.forbid("fight", "Fights need WebGPU: the WebGL fallback cannot draw spells");
const enemyCount = () => (mode.fight ? Math.min(6, Number(params.values.enemies ?? 0)) : 0);
mode.onChange = () => {
  flight.setActive(mode.free);
  if (mode.fight) combat.reset();
};
flight.setActive(mode.free);
window.addEventListener("keydown", (e) => {
  if (e.code === "KeyF" && !e.ctrlKey && !e.metaKey && !e.altKey && !isTyping(e)) mode.cycle();
});
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

// The collision probe, for the rock and him (SPHERES of them, see
// collision_spheres in fut/collide.fut) and then each spell, 4 floats each.
const SPHERES = 6;
const probes = new ProbeReader();

// Music (off until the ♪ button or M): chunks of samples from the build's
// `sound` entry point, read back the same way as the probe.
const audio = new AudioHub();
const music = new Music(document.getElementById("music") as HTMLButtonElement, audio);
// Sound effects (see sfx.ts), made by the build like the music.
const sfx = new Sfx(document.getElementById("sfx") as HTMLButtonElement, audio);
const musicPeriods = new WeakMap<FutInstance, number>();
const sounds = new ProbeReader();
if (useGl) music.disable("Music needs WebGPU: its samples come from a compute kernel");
if (useGl) sfx.disable("Sound effects need WebGPU: their samples come from a compute kernel");
// (in the WebGL fallback: a note in place of the vitals, bottom left)
const glNote = document.getElementById("gl-note")!;

// The fight with the enemies, his mana and shield (see combat.ts).
const combat = new Combat(spells, sfx);

// The orb, and the camera's view, in the world (for casting).
function castFrom(): { origin: [number, number, number]; dir: [number, number, number] } {
  const { pos, heading } = flight.where(time);
  const c = Math.cos(heading), s = Math.sin(heading);
  const turn = ([x, y, z]: [number, number, number]): [number, number, number] => [x * c + z * s, y, z * c - x * s];
  const o = turn([0.55, 2.1, 0.5]); // the orb, in his frame (orb_pos in fut/rig.fut)
  return { origin: [pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]], dir: turn(viewDir(cam)) };
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
  else if (e.code === "KeyM") music.toggle();
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

// The canvas's size for this frame (in pixels, at the render scale).
function frameSize(): { vw: number; h: number } {
  const dpr = window.devicePixelRatio || 1;
  const vw = Math.max(1, Math.floor(canvas.clientWidth * dpr * renderScale));
  const h = Math.max(1, Math.floor(canvas.clientHeight * dpr * renderScale));
  if (canvas.width !== vw || canvas.height !== h) {
    canvas.width = vw;
    canvas.height = h;
  }
  return { vw, h };
}

// Advances everything by dt (input, camera, flight, spells) and returns this
// frame's drive record. `canCast`: whether spells can be cast (not in the
// WebGL fallback, which cannot draw them).
function step(dt: number, canCast: boolean): Drive {
  entries.override({ enemies: enemyCount() });
  if (!paused) time += dt;
  idle(cam, dt);
  const pad = readPad();
  padConnected = !!pad;
  if (pad?.toggle) mode.cycle();
  // (A tries again once he is defeated; View / Back resets the fight)
  if (pad?.reset || (pad?.confirm && combat.enemies.down >= 0)) combat.reset();
  if (pad?.active) device = "pad";
  if (pad?.spell != null) spells.select(pad.spell);
  combat.update({
    dt: paused ? 0 : dt, time, him: flight.where(time).pos, aim: castFrom(), drawn: canCast, able: canCast && !paused,
    n: enemyCount(), seed: Number(params.values.enemy_seed ?? 0),
    casting: casting || !!pad?.cast, shielding: !!pad?.shield,
  });
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

  // casting, in his frame: along the camera's view
  const [sx, sy, sz] = viewDir(cam);
  return {
    ...flight.drive(), staff: spells.staff, sx, sy, sz, flash: spells.sinceCast, spell: spells.selected,
    ...combat.drive(),
  };
}

async function frame(i: FutInstance, dt: number): Promise<void> {
  const { vw, h } = frameSize();
  const w = Math.ceil(vw / 64) * 64; // bytesPerRow must be a multiple of 256
  spells.limit = spellLimits.get(i) ?? spells.limit;
  const d = step(dt, true);
  const points = takes(i, "probe", "pts") ? spells.probePoints() : { ids: [], pts: new Float32Array(0) };
  const probeBytes = (SPHERES + points.pts.length / 3) * 16;
  const wanted = (flight.active || spells.count > 0) && !!i.fut.entry.probe;
  const probe = wanted && probes.ready(i.device, probeBytes) ? await entries.probe(i, time, d, points.pts) : null;
  // (the music's readback gets a submission of its own, ahead of the
  // render's, so it arrives without waiting for the frame)
  if (sfx.wants(i)) await sfx.make(i);
  const soundBytes = music.maxFrames * 2 * 4;
  const req = musicPeriods.has(i) && sounds.ready(i.device, soundBytes) ? music.want(musicPeriods.get(i)!) : null;
  if (req) {
    const sound: FutharkArray = await i.fut.entry.sound(req.t0, music.rate, req.frames);
    const used = req.frames * 2 * 4;
    const enc = i.device.createCommandEncoder();
    sounds.copy(i, enc, sound, soundBytes, used);
    i.device.queue.submit([enc.finish()]);
    sound.free(); // (Futhark's later writes to it are queued after the copy)
    sounds.read(i.device, soundBytes, (data) => music.play(data, req), used);
  }
  const out = await entries.render(i, w, vw, h, time, cam.yaw, cam.pitch, cam.dist, d, spells.pack());
  const enc = i.device.createCommandEncoder();
  if (probe) probes.copy(i, enc, probe, probeBytes);
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
    probes.read(i.device, probeBytes, (data) => {
      flight.collide(data.subarray(0, SPHERES * 4));
      spells.applyProbe(points.ids, data.subarray(SPHERES * 4));
    });
  }
}

// The WebGL fallback's frame: the same state, drawn by its fragment shader.
function frameGl(r: GlRenderer, dt: number): void {
  const { vw, h } = frameSize();
  const d = step(dt, false);
  r.draw(entries.values(), d, { vw, h, tm: time, yaw: cam.yaw, pitch: cam.pitch, dst: cam.dist });
}

function tick(now: number): void {
  requestAnimationFrame(tick);
  if (glr) {
    if (!glr.ready) return;
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    fps = fps * 0.9 + (dt > 0 ? 1 / dt : 0) * 0.1;
    frameGl(glr, dt);
    quality.frame(!paused && !document.hidden);
    glNote.textContent = `${hasWebGpu ? "WebGL" : failure ? "WebGL fallback (WebGPU failed)" : "WebGL fallback (no WebGPU)"}: no spells, collisions, fights or anti-aliasing`;
    glNote.hidden = false;
    setStatus(`${glr.version!.slice(0, 8)} · ${canvas.width}×${canvas.height} · ${fps.toFixed(1)} fps${paused ? " · paused" : ""}${padConnected ? " · controller" : ""}`);
    return;
  }
  if (!inst || inFlight) return;
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  fps = fps * 0.9 + (dt > 0 ? 1 / dt : 0) * 0.1;
  const current = inst;
  inFlight = frame(current, dt)
    .then(() => quality.frame(!paused && !document.hidden))
    .then(() => setStatus(`${current.version.slice(0, 8)}${current.f32 ? " · f32" : ""}${current.portable ? " · portable" : ""} · ${canvas.width}×${canvas.height} · ${fps.toFixed(1)} fps${paused ? " · paused" : ""}${padConnected ? " · controller" : ""}`))
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
    if (next.fut.entry.music_period) musicPeriods.set(next, Number(await next.fut.entry.music_period(0)));
    music.reset();
    // Compile the pipelines Futhark only makes on first use (the shading's
    // among them) with a small frame, while the current build keeps
    // running, so the swap does not stall.
    try {
      await warmUp(next);
    } catch (e) {
      console.warn(`warm-up of ${version} failed: ${e}`);
    }
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
    entries.forget(evicted);
    unloadFuthark(evicted);
  }
  configure(next.device);
  next.shaders.onError = (report) => showError(`WGSL compilation error:\n\n${report}`);
  inst = next;
  clearError();
}

// A frame of a newly loaded build, small and not shown.
async function warmUp(i: FutInstance): Promise<void> {
  (await entries.render(i, 64, 64, 36, time, cam.yaw, cam.pitch, cam.dist, ON_PATH, spells.pack())).free();
  if (i.fut.entry.probe) (await entries.probe(i, time, ON_PATH, new Float32Array(spells.limit * 3))).free();
  await i.fut.context_sync();
}

// Builds load one after another; each resolves the waiters for "the next
// build is running" (see the quality panel under the dev server).
let loading = Promise.resolve();
const swapWaiters: (() => void)[] = [];
function reload(version: string): Promise<void> {
  const load = async () => {
    if (unsupported) return;
    if (!glr) await swapTo(version);
    else {
      // (the current shader keeps drawing while the new one compiles)
      await glr.load(version);
      clearError();
    }
    for (const done of swapWaiters.splice(0)) done();
  };
  loading = loading.then(() =>
    load().catch((e) => {
      const message = String(e?.message ?? e);
      // WebGPU itself failing before any build has run here (not a shader
      // error, which a later build fixes, nor a missing build): fall back
      // to WebGL, saying why, if there is WebGL to fall back to, and only
      // once
      const webGpuError = /webgpu|device|adapter|program exited/i.test(message) && !message.startsWith("WGSL compilation failed");
      if (!glr && !inst && loaded.size === 0 && !failure && webGpuError) {
        if (!hasWebGl2()) {
          console.warn(`WebGPU failed, and there is no WebGL2: ${message}`);
          noRendererAfter(message);
          return;
        }
        console.warn(`WebGPU failed; reloading with WebGL: ${message}`);
        try {
          sessionStorage.setItem(FAILED, message.split("\n")[0].slice(0, 200));
          location.reload();
          return;
        } catch {
          // no storage: show the error instead
        }
      }
      showError(`failed to load build ${version}:\n${message}`);
    }),
  );
  return loading;
}

// Debug/test hook: render one frame and read it back to the CPU. The render
// loop is held off meanwhile, since entry points must not run concurrently.
(window as unknown as { wizard: object }).wizard = {
  flight,
  cam,
  spells,
  enemies: combat.enemies,
  sfx,
  mana: combat.mana,
  shield: combat.shield,
  get time() {
    return time;
  },
  music,
  async readFrame(w: number, vw: number, h: number, t: number, yaw: number, pitch: number, dist: number) {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const out = await entries.render(i, w, vw, h, t, yaw, pitch, dist, flight.drive());
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
      const out = await entries.probe(i, time, d, new Float32Array(0));
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
  // Average time per frame in ms, without presenting or vsync, and (with
  // ?profile) the GPU time per frame of each kernel.
  async bench(frames: number, vw: number, h: number, t0 = 0): Promise<{ ms: number; kernels?: Record<string, number> }> {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const w = Math.ceil(vw / 64) * 64;
      const run = async (t: number) => (await entries.render(i, w, vw, h, t, cam.yaw, cam.pitch, cam.dist, ON_PATH)).free();
      await run(t0); // warm-up: pipeline creation
      await i.fut.context_sync();
      i.profile?.start();
      const start = performance.now();
      for (let f = 0; f < frames; f++) await run(t0 + f / 30);
      await i.fut.context_sync();
      const ms = (performance.now() - start) / frames;
      const kernels = await i.profile?.stop();
      return { ms, kernels: kernels && Object.fromEntries([...kernels].map(([k, v]) => [k, v / frames])) };
    } finally {
      inst = i;
    }
  },
};

// The quality panel (see quality.ts). The static site ships a build per
// level; under the dev server a level is a preset of the compile-time
// parameters, which the server rebuilds for (and announces as usual).
const pinned = new URLSearchParams(location.search).get("build");
const levelOf = (compile: Record<string, unknown>): Level | null =>
  LEVELS.find((l) => params.presets[l] && Object.entries(params.presets[l]).every(([k, v]) => compile[k] === v)) ?? null;
const devLevel = IS_STATIC ? null : levelOf(params.compile);
const qualityOn = !pinned && Object.keys(params.presets).length > 0 && (!IS_STATIC || !!params.builds);
// Under the dev server a level is this tab's own build of the current
// sources with the preset (/variant), so a tab on Auto never changes the
// shared shader settings; when the sources change it rebuilds its variant.
// Following the server's own build (the drawer's shader settings) again
// means picking the level those match, or reloading.
let tabLevel: Level | null = null;
let serverBuild: string | null = null;
async function loadVariant(level: Level): Promise<void> {
  const res = await fetch("/variant", { method: "POST", body: JSON.stringify(params.presets[level]) });
  if (!res.ok) throw new Error(await res.text());
  await reload(((await res.json()) as { version: string }).version);
}
const quality = new Quality(document.getElementById("quality")!, devLevel, async (level) => {
  if (IS_STATIC) return reload(params.builds![level]);
  tabLevel = level === devLevel ? null : level;
  if (tabLevel) await loadVariant(tabLevel);
  else if (serverBuild) await reload(serverBuild);
}, IS_STATIC ? "auto" : devLevel, qualityOn);
if (!qualityOn) document.getElementById("quality")!.hidden = true;

// The static site loads its build for the starting quality; otherwise
// follow the dev server's builds (hot reload).
if (IS_STATIC) {
  const { build } = (await (await fetch("params.json")).json()) as { build: string };
  const level = params.builds ? quality.initial : null;
  quality.level = level;
  reload(level ? params.builds![level] : build);
} else {
  const events = new EventSource("/events");
  events.addEventListener("building", () => setStatus("compiling…"));
  events.addEventListener("error", (e) => {
    if (e instanceof MessageEvent) showError(JSON.parse(e.data).message);
  });
  // ?build=N pins a specific build (for benchmarking variants); otherwise follow the server.
  if (pinned) reload(pinned);
  else {
    events.addEventListener("fut-reload", (e) => {
      serverBuild = JSON.parse((e as MessageEvent).data).version;
      if (tabLevel) void loadVariant(tabLevel);
      else reload(serverBuild!);
    });
  }
  events.addEventListener("web-reload", () => location.reload());
}

requestAnimationFrame(tick);
