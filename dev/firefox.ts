// Drives stock Firefox over WebDriver BiDi (puppeteer-core): probes WebGPU,
// or opens the page, prints console output and saves a screenshot.
//
// usage: FIREFOX=$(which firefox) node dev/firefox.ts [probe | url] [out.png] [waitMs]

import puppeteer from "puppeteer-core";

const [target = "http://localhost:8080/", out = "build/firefox.png", wait = "8000"] = process.argv.slice(2);

const browser = await puppeteer.launch({
  browser: "firefox",
  executablePath: process.env.FIREFOX ?? "firefox",
  headless: !process.env.HEADED,
  dumpio: !!process.env.DUMPIO, // Firefox stderr, e.g. for crashes
  extraPrefsFirefox: {
    "dom.webgpu.enabled": true,
    "gfx.webgpu.ignore-blocklist": true,
  },
});
const page = await browser.newPage();
await page.setViewport({ width: 1280, height: 720 });
page.on("console", (m) => console.log(`[console.${m.type()}] ${m.text()}`));
page.on("pageerror", (e) => console.log(`[pageerror] ${e instanceof Error ? e.message : e}`));

if (target === "probe") {
  await page.goto("http://localhost:8080/__probe"); // any localhost page is a secure context
  console.log(
    await page.evaluate(async () => {
      if (!navigator.gpu) return "no navigator.gpu";
      const a = await navigator.gpu.requestAdapter();
      if (!a) return "no adapter";
      return JSON.stringify({ ua: navigator.userAgent, features: [...a.features], info: { vendor: a.info?.vendor, arch: a.info?.architecture }, fallback: a.info?.isFallbackAdapter });
    }),
  );
} else {
  await page.goto(target);
  await new Promise((r) => setTimeout(r, Number(wait)));
  console.log("status:", await page.$eval("#status", (e) => e.textContent));
  console.log("overlay:", await page.$eval("#error", (e) => ((e as HTMLElement).hidden ? "none" : e.textContent)));
  await page.screenshot({ path: out as `${string}.png` });
  console.log(`saved ${out}`);
}
await browser.close();
