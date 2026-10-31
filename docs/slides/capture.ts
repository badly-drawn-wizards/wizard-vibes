// Regenerates every image the slides use, reproducibly:
//   - renders, by the CPU reference renderer (`futhark multicore`, entry
//     point render_rgb): each shot names its camera, and any changes to the
//     uniforms' defaults or to the source (exact string replacements, which
//     must apply) it needs; shots with the same changes share a compile;
//   - 2D distance-field figures, computed here.
// Images go to docs/slides/img/ as PNG (via ffmpeg).
//
// usage: nix develop -c node docs/slides/capture.ts [name...]   (all if none)

import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { generate } from "../../dev/params.ts";

const OUT = "docs/slides/img";
const WORK = "build/capture";

interface Shot {
  name: string;
  size: [number, number];
  time: number;
  // orbit camera: yaw, pitch, distance, around target
  cam: [number, number, number];
  target?: [number, number, number];
  // uniforms' defaults to override (Futhark literals, e.g. "1.0f32")
  params?: Record<string, string>;
  // source changes: [file in fut/, exact text, replacement]
  edits?: [string, string, string][];
  supersample?: number; // render k times larger, then scale down
  crop?: [number, number, number, number]; // w, h, x, y (after scaling)
}

// (the camera may go below the target's height for low views)
const LOW: [string, string, string] = ["wizard.fut", "let ro = {x = ro.x, y = f32.max ro.y 0.1, z = ro.z}", "let ro = ro"];
// the staff held out and casting fire (the drive record on the path)
const casting = (spell: number): [string, string, string] => [
  "rig.fut",
  "staff = 0, sx = 0, sy = 0, sz = 1, flash = 10, spell = 1}",
  `staff = 1, sx = -0.5, sy = -0.1, sz = 0.8, flash = 0.05, spell = ${spell}}`,
];
const orb = (spell: number): [string, string, string] => ["rig.fut", "flash = 10, spell = 1}", `flash = 10, spell = ${spell}}`];
const terrainEps = (eps: string): [string, string, string] => [
  "scene.fut",
  "if d < 0.0015 * t then (t, i + 1, true, mat",
  `if d < ${eps} * t then (t, i + 1, true, mat`,
];

const DEFAULT: [number, number, number] = [0.5, 0.15, 4.4];
const shots: Shot[] = [
  { name: "hero", size: [1280, 720], time: 3, cam: DEFAULT },
  { name: "cast", size: [960, 540], time: 5, cam: [2.0, 0.2, 3], target: [0.3, 1.5, 0.3], edits: [casting(0)] },
  { name: "orb-lightning", size: [480, 480], time: 3, cam: [0.9, 0.15, 0.75], target: [0.53, 2.15, 0.47], edits: [orb(1)] },
  { name: "orb-fire", size: [480, 480], time: 3, cam: [0.9, 0.15, 0.75], target: [0.53, 2.15, 0.47], edits: [orb(0)] },
  { name: "orb-wind", size: [480, 480], time: 3, cam: [0.9, 0.15, 0.75], target: [0.53, 2.15, 0.47], edits: [orb(2)] },
  { name: "head", size: [640, 640], time: 3, cam: [-0.45, 0.1, 1.0], target: [-0.02, 1.7, 0.1] },
  { name: "normals", size: [640, 640], time: 3, cam: [0.4, 0.1, 2.8], target: [0, 1.2, 0], params: { view: "1i32" } },
  { name: "albedo", size: [640, 640], time: 3, cam: [0.4, 0.1, 2.8], target: [0, 1.2, 0], params: { view: "2i32" } },
  { name: "ao", size: [640, 640], time: 3, cam: [0.4, 0.1, 2.8], target: [0, 1.2, 0], params: { view: "3i32" } },
  { name: "shaded", size: [640, 640], time: 3, cam: [0.4, 0.1, 2.8], target: [0, 1.2, 0] },
  { name: "cottage", size: [960, 540], time: 20, cam: [2.2, 0.15, 6], target: [9.82, -10.4, 9.19], edits: [LOW], params: { structure_density: "1.0f32" } },
  { name: "terrain", size: [960, 540], time: 20, cam: [2.4, 0.02, 9], target: [0, -6, 0], edits: [LOW] },
  { name: "terrain-normals", size: [960, 540], time: 20, cam: [2.4, 0.02, 9], target: [0, -6, 0], edits: [LOW], params: { view: "1i32" } },
  { name: "eps-tight", size: [640, 360], time: 20, cam: [2.4, 0.02, 9], target: [0, -6, 0], edits: [LOW, terrainEps("0.0004")] },
  { name: "eps-loose", size: [640, 360], time: 20, cam: [2.4, 0.02, 9], target: [0, -6, 0], edits: [LOW] },
  { name: "aliased", size: [480, 270], time: 3, cam: DEFAULT, crop: [96, 64, 236, 22] },
  { name: "supersampled", size: [480, 270], time: 3, cam: DEFAULT, supersample: 4, crop: [96, 64, 236, 22] },
];

// ---------------------------------------------------------------------
// Renders

function sh(cmd: string, args: string[], input?: string | Buffer): Promise<Buffer> {
  return new Promise((done, fail) => {
    const p = spawn(cmd, args);
    const out: Buffer[] = [];
    let err = "";
    p.stdout.on("data", (d) => out.push(d));
    p.stderr.on("data", (d) => (err += d));
    p.on("close", (code) => (code === 0 ? done(Buffer.concat(out)) : fail(new Error(`${cmd} ${args.join(" ")}: ${err.slice(-2000)}`))));
    if (input !== undefined) p.stdin.end(input);
    else p.stdin.end();
  });
}

// The program for a set of changes: copied, regenerated, edited, compiled.
const programs = new Map<string, Promise<string>>();
function program(params: Record<string, string>, edits: [string, string, string][]): Promise<string> {
  const key = createHash("sha256").update(JSON.stringify([params, edits])).digest("hex").slice(0, 12);
  if (!programs.has(key)) {
    programs.set(key, (async () => {
      const dir = join(WORK, key);
      mkdirSync(dir, { recursive: true });
      for (const f of readdirSync("fut")) if (f.endsWith(".fut")) cpSync(join("fut", f), join(dir, f));
      generate(dir, false); // the defaults, not this machine's config.local.json
      let pf = readFileSync(join(dir, "params.fut"), "utf8");
      for (const [id, lit] of Object.entries(params)) {
        const re = new RegExp(`(\\b${id} = )[^\\n]+`);
        if (!re.test(pf.split("def default_params")[1] ?? "")) throw new Error(`no default for ${id}`);
        const [head, tail] = pf.split("def default_params");
        pf = head + "def default_params" + tail.replace(re, `$1${lit}`);
      }
      writeFileSync(join(dir, "params.fut"), pf);
      for (const [file, from, to] of edits) {
        const src = readFileSync(join(dir, file), "utf8");
        if (!src.includes(from)) throw new Error(`edit does not apply to ${file}: ${from}`);
        writeFileSync(join(dir, file), src.replace(from, to));
      }
      const exe = join(dir, "reference");
      if (!existsSync(exe)) {
        console.error(`compiling ${key} (${Object.keys(params).join(", ") || "-"}; ${edits.length} edits)`);
        await sh("futhark", ["multicore", "-o", exe, join(dir, "reference.fut")]);
      }
      return exe;
    })());
  }
  return programs.get(key)!;
}

async function renderShot(s: Shot): Promise<void> {
  const exe = await program(s.params ?? {}, s.edits ?? []);
  const k = s.supersample ?? 1;
  const [w, h] = [s.size[0] * k, s.size[1] * k];
  const [tx, ty, tz] = s.target ?? [0, 1.15, 0];
  const [yaw, pitch, dist] = s.cam;
  const input = `${w}i32 ${h}i32 ${s.time}f32 ${yaw}f32 ${pitch}f32 ${dist}f32 ${tx}f32 ${ty}f32 ${tz}f32`;
  const out = await sh(exe, ["-e", "render_rgb", "-b"], input);
  const rgb = out.subarray(7 + 24, 7 + 24 + w * h * 3);
  const filters = [k > 1 ? `scale=${s.size[0]}:${s.size[1]}:flags=area` : "", s.crop ? `crop=${s.crop.join(":")}` : ""].filter(Boolean);
  await savePng(rgb, w, h, s.name, filters);
  console.error(`  ${s.name}`);
}

async function savePng(rgb: Uint8Array, w: number, h: number, name: string, filters: string[] = []): Promise<void> {
  await sh("ffmpeg", ["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", `${w}x${h}`, "-i", "-",
    ...(filters.length ? ["-vf", filters.join(",")] : []), join(OUT, `${name}.png`)], Buffer.from(rgb));
}

// ---------------------------------------------------------------------
// 2D distance fields

type F = (x: number, y: number) => number;
const len = (x: number, y: number) => Math.hypot(x, y);
const circle = (cx: number, cy: number, r: number): F => (x, y) => len(x - cx, y - cy) - r;
const box = (cx: number, cy: number, bx: number, by: number): F => (x, y) => {
  const qx = Math.abs(x - cx) - bx, qy = Math.abs(y - cy) - by;
  return len(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0);
};
const smin = (k: number, a: number, b: number) => {
  const h = Math.min(1, Math.max(0, 0.5 + (0.5 * (b - a)) / k));
  return b + (a - b) * h - k * h * (1 - h);
};
const capsule = (ax: number, ay: number, bx: number, by: number, r: number): F => (x, y) => {
  const px = x - ax, py = y - ay, vx = bx - ax, vy = by - ay;
  const h = Math.min(1, Math.max(0, (px * vx + py * vy) / (vx * vx + vy * vy)));
  return len(px - h * vx, py - h * vy) - r;
};

// Colours a field as in the usual SDF illustrations: warm outside, cool
// inside, banded every 0.1, the surface white. (x, y in [-1.6, 1.6] by
// [-0.9, 0.9], y up.)
async function field(name: string, f: F, w = 640, h = 360, extra?: (x: number, y: number) => number): Promise<void> {
  const rgb = new Uint8Array(w * h * 3);
  for (let j = 0; j < h; j++) {
    for (let i = 0; i < w; i++) {
      const x = ((i + 0.5) / w - 0.5) * 3.2, y = (0.5 - (j + 0.5) / h) * 1.8;
      const d = f(x, y);
      const base = d > 0 ? [0.95, 0.62, 0.32] : [0.4, 0.62, 0.95];
      let c = base.map((v) => v * (1 - Math.exp(-6 * Math.abs(d))) * (0.8 + 0.2 * Math.cos(Math.PI * 2 * d / 0.1)));
      const edge = 1 - Math.min(1, Math.max(0, (Math.abs(d) - 0.004) / 0.01));
      c = c.map((v) => v * (1 - edge) + edge);
      if (extra) {
        const e = extra(x, y);
        c = c.map((v, k) => v * (1 - e) + e * [0.79, 0.64, 0.15][k]);
      }
      for (let k = 0; k < 3; k++) rgb[(j * w + i) * 3 + k] = Math.round(Math.min(1, c[k]) ** (1 / 2.2) * 255);
    }
  }
  await savePng(rgb, w, h, name);
  console.error(`  ${name}`);
}

const fields: Record<string, () => Promise<void>> = {
  "field-shapes": () => field("field-shapes", (x, y) => Math.min(circle(-0.55, 0, 0.45)(x, y), box(0.65, 0, 0.35, 0.3)(x, y))),
  "field-min": () => field("field-min", (x, y) => Math.min(circle(-0.35, 0, 0.45)(x, y), circle(0.4, 0.05, 0.35)(x, y))),
  "field-smin": () => field("field-smin", (x, y) => smin(0.35, circle(-0.35, 0, 0.45)(x, y), circle(0.4, 0.05, 0.35)(x, y))),
  "field-subtract": () => field("field-subtract", (x, y) => Math.max(box(0, 0, 0.7, 0.45)(x, y), -circle(0.45, 0.3, 0.4)(x, y))),
  // one circle, repeated by folding space into 0.6-wide cells
  "field-repeat": () => {
    const m = (v: number, p: number) => v - p * Math.round(v / p);
    return field("field-repeat", (x, y) => circle(0, 0, 0.17)(m(x, 0.6), m(y, 0.6)),
      undefined, undefined, (x, y) => (Math.abs(m(x + 0.3, 0.6)) < 0.006 || Math.abs(m(y + 0.3, 0.6)) < 0.006 ? 0.7 : 0));
  },
  // a capsule, bent by rotating each point by an angle growing with height
  "field-bend": () => field("field-bend", (x, y) => {
    const a = 0.9 * (y + 0.8);
    const c = Math.cos(a), s = Math.sin(a);
    return capsule(0, -0.8, 0, 0.7, 0.12)(c * x - s * (y + 0.8), s * x + c * (y + 0.8) - 0.8);
  }),
};

// ---------------------------------------------------------------------

mkdirSync(OUT, { recursive: true });
mkdirSync(WORK, { recursive: true });
const want = new Set(process.argv.slice(2));
const pick = (n: string) => want.size === 0 || want.has(n);
for (const [n, f] of Object.entries(fields)) if (pick(n)) await f();
// a few renders at a time (each is multi-threaded already)
const queue = shots.filter((s) => pick(s.name));
await Promise.all(Array.from({ length: 3 }, async () => {
  for (let s = queue.shift(); s; s = queue.shift()) await renderShot(s);
}));
