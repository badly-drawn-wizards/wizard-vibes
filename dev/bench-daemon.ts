// Benchmark daemon: keeps Chromium (and Firefox) open on the harness page and
// times builds on request, hot-swapping each into the running page, so a
// measurement costs seconds rather than a browser start and page load.
//
// usage: CHROME=$(which chromium) FIREFOX=$(which firefox) node dev/bench-daemon.ts
//        dev/bench.sh <build id>,<build id>   (ids from dev/variant.sh)
// Builds are measured interleaved (A B A B ...) and each keeps its best
// time, so drift and warm-up don't favour one. Times are 1920x1080 GPU ms.

import { createServer } from "node:http";
import { chromium, type Page } from "playwright-core";
import puppeteer from "puppeteer-core";
import { CHROME_ARGS } from "./chrome-args.ts";

const PAGE = "http://localhost:8080/?build=none";
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

async function measure(r: Runner, build: string, frames: number): Promise<number | string> {
  try {
    await r.evaluate(`window.wizard.swap(${JSON.stringify(build)})`);
    return await r.evaluate<number>(`window.wizard.bench(${frames}, 1920, 1080)`);
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
  const only = url.searchParams.get("browsers")?.split(",");
  busy = busy.then(async () => {
    const lines: string[] = [];
    for (const r of runners.filter((r) => !only || only.includes(r.name))) {
      const best = new Map<string, number | string>();
      for (let k = 0; k < reps; k++) {
        for (const b of builds) {
          const t = await measure(r, b, frames);
          const prev = best.get(b);
          if (typeof t === "string") best.set(b, t);
          else if (typeof prev !== "string") best.set(b, Math.min(prev ?? Infinity, t));
        }
      }
      for (const b of builds) {
        const t = best.get(b)!;
        lines.push(`${r.name.padEnd(8)} ${b}  ${typeof t === "number" ? t.toFixed(2) + " ms" : t}`);
      }
    }
    res.writeHead(200, { "Content-Type": "text/plain" }).end(lines.join("\n") + "\n");
  });
}).listen(8091, () => console.log(`bench daemon on :8091 (${runners.map((r) => r.name).join(", ")})`));
