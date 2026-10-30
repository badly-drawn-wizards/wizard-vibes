// Headless check: opens the page in Chromium with WebGPU enabled, waits,
// saves a screenshot and prints console output.
//
// usage: CHROME=$(which chromium) node dev/screenshot.ts [url] [out.png] [waitMs]

import { chromium } from "playwright-core";
import { CHROME_ARGS } from "./chrome-args.ts";

const [url = "http://localhost:8080/", out = "build/screenshot.png", wait = "6000"] = process.argv.slice(2);

const browser = await chromium.launch({
  executablePath: process.env.CHROME ?? "chromium",
  headless: process.env.HEADED ? false : true,
  args: CHROME_ARGS,
});
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
page.on("console", (m) => console.log(`[console.${m.type()}] ${m.text()}`));
page.on("pageerror", (e) => console.log(`[pageerror] ${e.stack ?? e}`));
if (url === "probe") {
  // navigator.gpu needs a secure context; localhost counts.
  await page.route("http://localhost:1/", (r) => r.fulfill({ body: "<!doctype html>", contentType: "text/html" }));
  await page.goto("http://localhost:1/");
  console.log(await page.evaluate(async () => {
    if (!navigator.gpu) return "no navigator.gpu";
    const a = await navigator.gpu.requestAdapter();
    if (!a) return "no adapter";
    return JSON.stringify({ features: [...a.features], info: { vendor: a.info.vendor, arch: a.info.architecture, desc: a.info.description } });
  }));
} else {
  await page.goto(url);
  await page.waitForTimeout(Number(wait));
  console.log("status:", await page.locator("#status").textContent());
  const err = page.locator("#error");
  if (await err.isVisible()) console.log("overlay:", await err.textContent());
  await page.screenshot({ path: out });
  console.log(`saved ${out}`);
}
await browser.close();
