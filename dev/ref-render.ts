// CPU reference render: compiles fut/wizard.fut with `futhark multicore`,
// calls the `render_rgb` entry point and writes build/ref.png.
//
// usage: node dev/ref-render.ts [width height time yaw pitch dist]

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";

const [w = "640", h = "360", t = "1.0", yaw = "0.35", pitch = "0.12", dist = "5.2"] = process.argv.slice(2);

mkdirSync("build/ref", { recursive: true });
execFileSync("futhark", ["multicore", "-o", "build/ref/wizard", "fut/wizard.fut"], { stdio: "inherit" });

const input = `${w}i32 ${h}i32 ${t}f32 ${yaw}f32 ${pitch}f32 ${dist}f32`;
const out = execFileSync("build/ref/wizard", ["-e", "render_rgb", "-b"], { input, maxBuffer: 1 << 30 });

// Futhark binary data format: 'b', version, rank, 4-char type, i64 dims, payload.
if (out[0] !== 0x62 || out[2] !== 3 || out.toString("latin1", 3, 7) !== "  u8") {
  throw new Error("unexpected futhark binary output");
}
const dims = [0, 1, 2].map((i) => Number(out.readBigInt64LE(7 + 8 * i)));
const [rows, cols] = dims;
const pixels = out.subarray(7 + 24);

writeFileSync("build/ref.png", encodePng(cols, rows, pixels));
console.log(`wrote build/ref.png (${cols}x${rows})`);

function encodePng(width: number, height: number, rgb: Uint8Array): Buffer {
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (width * 3 + 1)] = 0; // filter: none
    raw.set(rgb.subarray(y * width * 3, (y + 1) * width * 3), y * (width * 3 + 1) + 1);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, 2, 0, 0, 0], 8); // 8-bit RGB
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function chunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function crc32(buf: Buffer): number {
  let c = ~0;
  for (const b of buf) {
    c ^= b;
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}
