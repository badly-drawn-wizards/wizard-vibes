// Renders one 12 s animation loop on the CPU and encodes build/wizard.gif
// with ffmpeg (two-pass palette for clean gradients).
//
// usage: node dev/gif.ts [width height fps yaw pitch dist]

import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { compileCpu, renderRgb } from "./cpu-render.ts";

const LOOP_SECONDS = 12; // must match loop_seconds in fut/wizard.fut

const [width = 480, height = 270, fps = 20, yaw = 0.5, pitch = 0.35, dist = 4.2] = process.argv.slice(2).map(Number);
const view = { width, height, yaw, pitch, dist };
const frames = LOOP_SECONDS * fps;

compileCpu();
rmSync("build/frames", { recursive: true, force: true });
mkdirSync("build/frames", { recursive: true });

for (let i = 0; i < frames; i++) {
  const rgb = renderRgb(view, (i / frames) * LOOP_SECONDS); // excludes t = 12, which equals t = 0
  const header = Buffer.from(`P6\n${width} ${height}\n255\n`, "latin1");
  writeFileSync(`build/frames/${String(i).padStart(4, "0")}.ppm`, Buffer.concat([header, rgb]));
  process.stdout.write(`\rframe ${i + 1}/${frames}`);
}
process.stdout.write("\n");

execFileSync(
  "ffmpeg",
  [
    "-y", "-loglevel", "error",
    "-framerate", String(fps),
    "-i", "build/frames/%04d.ppm",
    "-filter_complex", "[0]split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=sierra2_4a",
    "-loop", "0",
    "build/wizard.gif",
  ],
  { stdio: "inherit" },
);
console.log(`wrote build/wizard.gif (${frames} frames, ${width}x${height} @ ${fps} fps)`);
