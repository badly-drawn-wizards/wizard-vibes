// Benchmark daemon: keeps Chromium (and Firefox) open on the harness page and
// times builds on request, hot-swapping each into the running page, so a
// measurement costs seconds rather than a browser start and page load.
//
// usage: CHROME=$(which chromium) FIREFOX=$(which firefox) node dev/bench-daemon.ts
//        dev/bench.sh <build id>,<build id>   (ids from dev/variant(s).sh)
// After warming up (the laptop runs ~1 ms faster cold and drifts up for a
// minute, then holds at its ~70 W power limit; fans make no difference),
// builds are measured in mirrored rounds (A B .. B A) and each is compared
// with the first by its mean difference over the rounds, with a 95%
// confidence interval. Measured (an A/A test): a round's difference has an
// sd of ~0.25 ms, so detecting 0.5 ms takes ~3 rounds, 0.25 ms ~9, 0.1 ms
// ~55 (95% confidence, 80% power). Times are GPU ms per 1920x1080 frame
// (timestamp queries; the frame time where the browser has none); add
// &t0=T starts the frames at flight time T (e.g. over another biome);
// &kernels=1 (dev/bench.sh -k) to list each kernel's. &raw=1 returns every
// sample with the GPU and CPU temperatures, clock and power.

import { readdirSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { chromium, type Page } from "playwright-core";
import puppeteer from "puppeteer-core";
import { CHROME_ARGS } from "./chrome-args.ts";

const PAGE = "http://localhost:8080/?build=none&profile";
const CAMERA = JSON.stringify({ yaw: 0.5, pitch: 0.2, dist: 4.2 });

type Runner = { name: string; evaluate: <T>(fn: string) => Promise<T> };

async function chromeRunner(): Promise<Runner> {
  const b = await chromium.launch({ executablePath: process.env.CHROME, args: CHROME_ARGS });
  const p: Page = await b.newPage();
  await p.addInitScript(`sessionStorage.setItem("wizard-camera", ${JSON.stringify(CAMERA)})`);
  await p.goto(PAGE);
  return { name: "chrome", evaluate: (fn) => p.evaluate(fn) };
}

async function firefoxRunner(): Promise<Runner> {
  const b = await puppeteer.launch({
    browser: "firefox",
    executablePath: process.env.FIREFOX,
    headless: true,
    extraPrefsFirefox: { "dom.webgpu.enabled": true, "gfx.webgpu.ignore-blocklist": true },
  });
  const p = await b.newPage();
  await p.evaluateOnNewDocument(`sessionStorage.setItem("wizard-camera", ${JSON.stringify(CAMERA)})`);
  await p.goto(PAGE);
  return { name: "firefox", evaluate: (fn) => p.evaluate(fn) as never };
}

const runners: Runner[] = [await chromeRunner()];
if (process.env.FIREFOX) runners.push(await firefoxRunner());

type Sample = { ms: number; kernels?: Record<string, number> };

// The machine's state, from hwmon: GPU edge and CPU (Tctl) temperatures in
// degrees C, GPU clock in MHz and power in W (null where not exposed).
type Thermals = { edge: number | null; tctl: number | null; sclk: number | null; watts: number | null };
const hwmon = (name: string): string | null => {
  try {
    for (const h of readdirSync("/sys/class/hwmon")) {
      const dir = `/sys/class/hwmon/${h}`;
      if (readFileSync(`${dir}/name`, "utf8").trim() === name) return dir;
    }
  } catch {
    // not Linux, or no hwmon
  }
  return null;
};
const GPU = hwmon("amdgpu");
const CPU = hwmon("k10temp") ?? hwmon("coretemp");
const read = (dir: string | null, file: string, scale: number): number | null => {
  try {
    return dir ? Number(readFileSync(`${dir}/${file}`, "utf8")) / scale : null;
  } catch {
    return null;
  }
};
function thermals(): Thermals {
  return { edge: read(GPU, "temp1_input", 1000), tctl: read(CPU, "temp1_input", 1000), sclk: read(GPU, "freq1_input", 1e6), watts: read(GPU, "power1_average", 1e6) };
}

const mean = (xs: number[]) => xs.reduce((a, x) => a + x, 0) / Math.max(1, xs.length);
// GPU time per frame (all kernels), or the frame time where not measured.
const gpuOf = (t: Sample) => (t.kernels && Object.keys(t.kernels).length ? Object.values(t.kernels).reduce((a, x) => a + x, 0) : t.ms);

// Renders until frame times stop drifting up (two successive 3-sample means
// within 0.5%), or for `seconds` at most.
async function warmUp(r: Runner, build: string, seconds: number): Promise<void> {
  const end = Date.now() + seconds * 1000;
  let last = Infinity;
  while (Date.now() < end) {
    const xs: number[] = [];
    for (let i = 0; i < 3; i++) {
      const t = await measure(r, build, 24);
      if (typeof t !== "string") xs.push(gpuOf(t));
    }
    const m = mean(xs);
    if (Math.abs(m - last) < 0.005 * m) return;
    last = m;
  }
}

// Waits (up to 10 minutes) until both temperatures are at most `limit`.
async function coolTo(limit: number): Promise<number> {
  const start = Date.now();
  for (;;) {
    const t = thermals();
    const hot = Math.max(t.edge ?? 0, t.tctl ?? 0);
    if (hot <= limit || Date.now() - start > 600_000) return Date.now() - start;
    await new Promise((r) => setTimeout(r, 2000));
  }
}

async function measure(r: Runner, build: string, frames: number, t0 = 0): Promise<Sample | string> {
  try {
    await r.evaluate(`window.wizard.swap(${JSON.stringify(build)})`);
    return await r.evaluate<Sample>(`window.wizard.bench(${frames}, 1920, 1080, ${t0})`);
  } catch (e) {
    return `error: ${String(e).split("\n").slice(0, 3).join(" ")}`;
  }
}


let busy = Promise.resolve();
createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://x");
  if (url.pathname !== "/bench") return void res.writeHead(404).end();
  const builds = (url.searchParams.get("b") ?? "").split(",").filter(Boolean);
  const reps = Number(url.searchParams.get("reps") ?? 3);
  const frames = Number(url.searchParams.get("frames") ?? 12);
  const t0 = Number(url.searchParams.get("t0") ?? 0); // flight time of the first frame
  const only = url.searchParams.get("browsers")?.split(",");
  const listKernels = url.searchParams.get("kernels") === "1";
  // raw=1: every sample as JSON lines, with the machine's state; cool=T:
  // wait until the GPU and CPU are at most T degrees C first
  const raw = url.searchParams.get("raw") === "1";
  const cool = Number(url.searchParams.get("cool") ?? 0);
  const warm = Number(url.searchParams.get("warm") ?? 20); // seconds at most
  busy = busy.then(async () => {
    const lines: string[] = [];
    if (cool > 0) lines.push(`# cooled to ${cool} C in ${Math.round((await coolTo(cool)) / 1000)} s`);
    if (raw) {
      // rounds of A B .. B A (each build twice, mirrored), so a steady drift
      // (heating) weighs on every build alike
      for (const r of runners.filter((r) => !only || only.includes(r.name))) {
        for (let k = 0; k < reps; k++) {
          for (const b of [...builds, ...[...builds].reverse()]) {
            const before = thermals();
            const t = await measure(r, b, frames, t0);
            const gpu = typeof t === "string" ? null : Object.values(t.kernels ?? {}).reduce((s, v) => s + v, 0);
            lines.push(JSON.stringify({ browser: r.name, build: b, round: k, ms: typeof t === "string" ? null : t.ms, gpu, error: typeof t === "string" ? t : undefined, ...before }));
          }
        }
      }
      return void res.writeHead(200, { "Content-Type": "text/plain" }).end(lines.join("\n") + "\n");
    }
    for (const r of runners.filter((r) => !only || only.includes(r.name))) {
      // Warm up until frame times settle: the laptop starts ~1 ms faster
      // cold and drifts up for a minute before holding at its power limit.
      // (warm=0 skips this.)
      if (warm > 0) await warmUp(r, builds[0], warm);
      // rounds of A B .. B A, each build twice, mirrored, so a steady drift
      // weighs on every build alike; each build's round mean is compared
      // with the first build's in the same round
      const samples = new Map<string, Sample[]>(builds.map((b) => [b, []]));
      const perRound: Map<string, number>[] = [];
      const errors = new Map<string, string>();
      for (let k = 0; k < reps; k++) {
        const round = new Map<string, number[]>();
        for (const b of [...builds, ...[...builds].reverse()]) {
          const t = await measure(r, b, frames, t0);
          if (typeof t === "string") {
            errors.set(b, t);
            continue;
          }
          samples.get(b)!.push(t);
          if (!round.has(b)) round.set(b, []);
          round.get(b)!.push(gpuOf(t));
        }
        perRound.push(new Map([...round].map(([b, v]) => [b, v.reduce((a, x) => a + x, 0) / v.length])));
      }
      for (const b of builds) {
        if (errors.has(b)) {
          lines.push(`${r.name.padEnd(8)} ${b}  ${errors.get(b)}`);
          continue;
        }
        const ss = samples.get(b)!;
        const gpu = mean(ss.map(gpuOf));
        let line = `${r.name.padEnd(8)} ${b}  ${gpu.toFixed(2)} ms GPU (${mean(ss.map((x) => x.ms)).toFixed(2)} ms frame)`;
        if (b !== builds[0]) {
          const d = perRound.filter((m) => m.has(b) && m.has(builds[0])).map((m) => m.get(b)! - m.get(builds[0])!);
          const sd = Math.sqrt(d.reduce((a, x) => a + (x - mean(d)) ** 2, 0) / Math.max(1, d.length - 1));
          const ci = (1.96 * sd) / Math.sqrt(d.length);
          const sig = Math.abs(mean(d)) > ci ? (mean(d) < 0 ? "  faster" : "  slower") : "  (no significant difference)";
          line += `  ${mean(d) >= 0 ? "+" : ""}${mean(d).toFixed(2)} ± ${ci.toFixed(2)} ms vs first${sig}`;
        }
        lines.push(line);
        if (listKernels) {
          const names = [...new Set(ss.flatMap((x) => Object.keys(x.kernels ?? {})))];
          const ks = names.map((n) => [n, mean(ss.map((x) => x.kernels?.[n] ?? 0))] as const).sort((x, y) => y[1] - x[1]);
          for (const [k, v] of ks) if (v >= 0.005) lines.push(`${"".padEnd(28)}${v.toFixed(3).padStart(8)} ms  ${k}`);
        }
      }
    }
    res.writeHead(200, { "Content-Type": "text/plain" }).end(lines.join("\n") + "\n");
  });
}).listen(8091, () => console.log(`bench daemon on :8091 (${runners.map((r) => r.name).join(", ")})`));
