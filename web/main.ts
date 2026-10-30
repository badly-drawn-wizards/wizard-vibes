// WebGPU harness: Futhark renders the frame into a storage buffer on its own
// device; we copy that buffer straight into the canvas texture on the GPU.

import { attachCamera, idle, loadCamera } from "./camera.ts";
import { type FutharkArray, type FutInstance, loadFuthark, rawBuffer, unloadFuthark } from "./futhark-loader.ts";
import { clearError, setStatus, showError } from "./overlay.ts";

const canvas = document.getElementById("view") as HTMLCanvasElement;
const ctx = canvas.getContext("webgpu")!;
const cam = loadCamera();
attachCamera(cam, canvas);

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
let time = 0;
let fps = 0;
let last = performance.now();

window.addEventListener("keydown", (e) => {
  if (e.code === "Space") paused = !paused;
});

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

  const out: FutharkArray = await i.fut.entry.render(w, vw, h, time, cam.yaw, cam.pitch, cam.dist);
  const enc = i.device.createCommandEncoder();
  enc.copyBufferToTexture(
    { buffer: rawBuffer(i, out), bytesPerRow: w * 4, rowsPerImage: h },
    { texture: ctx.getCurrentTexture() },
    [vw, h],
  );
  i.device.queue.submit([enc.finish()]);
  // Futhark may now recycle the previous buffer: its writes are queued after our copy.
  prev?.free();
  prev = out;
}

function tick(now: number): void {
  requestAnimationFrame(tick);
  if (!inst || inFlight) return;
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  fps = fps * 0.9 + (dt > 0 ? 1 / dt : 0) * 0.1;
  const current = inst;
  inFlight = frame(current, dt)
    .then(() => setStatus(`v${current.version}${current.f32 ? " · f32" : ""}${current.portable ? " · portable" : ""} · ${canvas.width}×${canvas.height} · ${fps.toFixed(1)} fps${paused ? " · paused" : ""}`))
    .catch((e) => showError(`render failed: ${e?.stack ?? e}`))
    .finally(() => (inFlight = null));
}

async function swapTo(version: number): Promise<void> {
  const started = performance.now();
  const next = await loadFuthark(version);
  console.info(`loaded v${version} in ${Math.round(performance.now() - started)} ms`);
  // Stop the loop on the old instance and let its last frame finish.
  const old = inst;
  inst = null;
  await inFlight;
  if (old) {
    prev?.free();
    prev = null;
    unloadFuthark(old);
  }
  next.device.lost.then((info) => {
    if (info.reason !== "destroyed") showError(`GPU device lost: ${info.message}`);
  });
  configure(next.device);
  next.shaders.onError = (report) => showError(`WGSL compilation error:\n\n${report}`);
  if (next.shaders.warnings) console.info(`v${version}: ${next.shaders.warnings} WGSL warnings (see console)`);
  inst = next;
  clearError();
}

let loading = Promise.resolve();
function reload(version: number): void {
  loading = loading.then(() =>
    swapTo(version).catch((e) => showError(`failed to load build v${version}:\n${e?.message ?? e}`)),
  );
}

if (!navigator.gpu) showError("WebGPU is not available in this browser.");

// Debug/test hook: render one frame and read it back to the CPU. The render
// loop is held off meanwhile, since entry points must not run concurrently.
(window as unknown as { wizard: object }).wizard = {
  async readFrame(w: number, vw: number, h: number, t: number, yaw: number, pitch: number, dist: number) {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const out = await i.fut.entry.render(w, vw, h, t, yaw, pitch, dist);
      const pixels: Uint32Array = await out.values();
      out.free();
      return { f32: i.f32, portable: i.portable, pixels: Array.from(pixels) };
    } finally {
      inst = i;
    }
  },
  // Average GPU time per frame in ms, without presenting or vsync.
  async bench(frames: number, vw: number, h: number) {
    const i = inst;
    if (!i) throw new Error("no Futhark instance loaded");
    inst = null;
    await inFlight;
    try {
      const w = Math.ceil(vw / 64) * 64;
      const run = async (t: number) => (await i.fut.entry.render(w, vw, h, t, cam.yaw, cam.pitch, cam.dist)).free();
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
events.addEventListener("fut-reload", (e) => reload(JSON.parse((e as MessageEvent).data).version));
events.addEventListener("web-reload", () => location.reload());

requestAnimationFrame(tick);
