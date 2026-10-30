// Dev server: serves the harness, rebuilds the Futhark program when fut/
// changes and tells the page to hot-swap it over server-sent events.

import { watch } from "node:fs";
import { readFile } from "node:fs/promises";
import { createServer, type ServerResponse } from "node:http";
import { extname, join, normalize } from "node:path";
import * as esbuild from "esbuild";
import { buildFuthark, FUT_ROOT } from "./build-fut.ts";

const PORT = Number(process.env.PORT ?? 8080);

const clients = new Set<ServerResponse>();
let current: number | null = null; // latest good Futhark build
let lastError: string | null = null;

function send(res: ServerResponse, type: string, data: unknown): void {
  res.write(`event: ${type}\ndata: ${JSON.stringify(data)}\n\n`);
}
function broadcast(type: string, data: unknown = {}): void {
  for (const c of clients) send(c, type, data);
}

// ---- Futhark builds: serialised, with at most one queued rebuild.
let building = false;
let queued = false;
async function rebuild(): Promise<void> {
  if (building) {
    queued = true;
    return;
  }
  building = true;
  broadcast("building");
  console.log("[fut] building…");
  const r = await buildFuthark();
  if (r.ok) {
    current = r.version;
    lastError = null;
    console.log(`[fut] v${r.version} built in ${r.ms} ms`);
    broadcast("fut-reload", { version: r.version });
  } else {
    lastError = r.message;
    console.error(`[fut] build failed:\n${r.message}`);
    broadcast("error", { message: r.message });
  }
  building = false;
  if (queued) {
    queued = false;
    void rebuild();
  }
}

let debounce: NodeJS.Timeout | undefined;
watch("fut", { recursive: true }, (_event, file) => {
  if (!file || !/\.(fut|js)$/.test(file)) return;
  clearTimeout(debounce);
  debounce = setTimeout(() => void rebuild(), 150);
});

// ---- Harness bundle.
const web = await esbuild.context({
  entryPoints: ["web/main.ts"],
  bundle: true,
  format: "esm",
  target: "es2022",
  sourcemap: true,
  outdir: "build/web",
  logLevel: "warning",
  plugins: [
    {
      name: "notify",
      setup(b) {
        let first = true;
        b.onEnd((r) => {
          if (r.errors.length) broadcast("error", { message: r.errors.map((e) => e.text).join("\n") });
          else if (!first) broadcast("web-reload");
          first = false;
        });
      },
    },
  ],
});
await web.watch();
watch("web", (_e, file) => {
  if (file?.endsWith(".html")) broadcast("web-reload");
});

// ---- HTTP.
const MIME: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".map": "application/json",
  ".wasm": "application/wasm",
  ".json": "application/json",
};

function resolvePath(url: string): string | null {
  const path = normalize(decodeURIComponent(url.split("?")[0]));
  if (path.includes("..")) return null;
  if (path === "/" || path === "/index.html") return "web/index.html";
  if (path.startsWith("/web/")) return join("build", path); // bundled TS
  if (path.startsWith("/fut/")) return join(FUT_ROOT, path.slice(5));
  return null;
}

createServer(async (req, res) => {
  if (req.url === "/events") {
    res.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-store", Connection: "keep-alive" });
    clients.add(res);
    req.on("close", () => clients.delete(res));
    if (current !== null) send(res, "fut-reload", { version: current });
    if (lastError) send(res, "error", { message: lastError });
    if (building) send(res, "building", {});
    return;
  }
  const file = resolvePath(req.url ?? "/");
  try {
    if (!file) throw new Error("not found");
    const body = await readFile(file);
    res.writeHead(200, { "Content-Type": MIME[extname(file)] ?? "application/octet-stream", "Cache-Control": "no-store" });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
}).listen(PORT, () => console.log(`serving http://localhost:${PORT}`));

void rebuild();
