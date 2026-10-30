// Benchmark daemon: keeps Chromium (and Firefox) open on the harness page and
// times builds on request, hot-swapping each into the running page, so a
// measurement costs seconds rather than a browser start and page load.
//
// usage: CHROME=$(which chromium) FIREFOX=$(which firefox) node dev/bench-daemon.ts
//        dev/bench.sh <build id>,<build id>   (ids from dev/variant(s).sh)
// Builds are measured interleaved (A B A B ...) and each keeps its best
// time, so drift and warm-up don't favour one. Times are ms per 1920x1080
// frame. Where the browser has timestamp queries, each kernel's GPU time
// per frame is measured too (its best over the repetitions); add
// &kernels=1 (dev/bench.sh -k) to list them.

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

async function measure(r: Runner, build: string, frames: number): Promise<Sample | string> {
  try {
    await r.evaluate(`window.wizard.swap(${JSON.stringify(build)})`);
    return await r.evaluate<Sample>(`window.wizard.bench(${frames}, 1920, 1080)`);
  } catch (e) {
    return `error: ${String(e).split("\n").slice(0, 3).join(" ")}`;
  }
}

// The best of several samples: the least time, and per kernel the least.
function best(a: Sample | string | undefined, b: Sample | string): Sample | string {
  if (typeof b === "string" || typeof a === "string") return typeof b === "string" ? b : (a as string);
  if (!a) return b;
  const kernels = { ...a.kernels };
  for (const [k, v] of Object.entries(b.kernels ?? {})) kernels[k] = Math.min(kernels[k] ?? Infinity, v);
  return { ms: Math.min(a.ms, b.ms), kernels };
}

let busy = Promise.resolve();
createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://x");
  if (url.pathname !== "/bench") return void res.writeHead(404).end();
  const builds = (url.searchParams.get("b") ?? "").split(",").filter(Boolean);
  const reps = Number(url.searchParams.get("reps") ?? 3);
  const frames = Number(url.searchParams.get("frames") ?? 12);
  const only = url.searchParams.get("browsers")?.split(",");
  const listKernels = url.searchParams.get("kernels") === "1";
  busy = busy.then(async () => {
    const lines: string[] = [];
    for (const r of runners.filter((r) => !only || only.includes(r.name))) {
      const results = new Map<string, Sample | string>();
      for (let k = 0; k < reps; k++) {
        for (const b of builds) results.set(b, best(results.get(b), await measure(r, b, frames)));
      }
      for (const b of builds) {
        const t = results.get(b)!;
        if (typeof t === "string") {
          lines.push(`${r.name.padEnd(8)} ${b}  ${t}`);
          continue;
        }
        const ks = Object.entries(t.kernels ?? {}).sort((x, y) => y[1] - x[1]);
        const gpu = ks.reduce((s, [, v]) => s + v, 0);
        lines.push(`${r.name.padEnd(8)} ${b}  ${t.ms.toFixed(2)} ms${ks.length ? `  (GPU ${gpu.toFixed(2)} ms)` : ""}`);
        if (listKernels) for (const [k, v] of ks) if (v >= 0.005) lines.push(`${"".padEnd(28)}${v.toFixed(3).padStart(8)} ms  ${k}`);
      }
    }
    res.writeHead(200, { "Content-Type": "text/plain" }).end(lines.join("\n") + "\n");
  });
}).listen(8091, () => console.log(`bench daemon on :8091 (${runners.map((r) => r.name).join(", ")})`));
