// Dev server: serves the harness, rebuilds the Futhark program when fut/
// changes and tells the page to hot-swap it over server-sent events.

import { watch } from "node:fs";
import { readFile } from "node:fs/promises";
import { createServer, type ServerResponse } from "node:http";
import { extname, join, normalize } from "node:path";
import * as esbuild from "esbuild";
import { buildFuthark, FUT_ROOT } from "./build-fut.ts";
import { compileValues, generate, loadSchema, saveCompileValues, SCHEMA, type Value } from "./params.ts";

const PORT = Number(process.env.PORT ?? 8080);

const clients = new Set<ServerResponse>();
let current: string | null = null; // id of the latest good Futhark build
let lastError: string | null = null;

function send(res: ServerResponse, type: string, data: unknown): void {
  res.write(`event: ${type}\ndata: ${JSON.stringify(data)}\n\n`);
}
function broadcast(type: string, data: unknown = {}): void {
  for (const c of clients) send(c, type, data);
}

// ---- Futhark builds: serialised, with at most one queued rebuild.
// One build at a time. A change while one runs cancels it (its result
// would be stale) and starts again; content-addressed builds make any
// already-built state instant.
let building: AbortController | null = null;
let queued = false;
async function rebuild(): Promise<void> {
  if (building) {
    queued = true;
    building.abort();
    return;
  }
  const ctl = new AbortController();
  building = ctl;
  broadcast("building");
  console.log("[fut] building…");
  const r = await buildFuthark("fut", ctl.signal);
  if (!r.ok && r.cancelled) console.log("[fut] cancelled (superseded)");
  else if (r.ok) {
    current = r.version;
    lastError = null;
    console.log(`[fut] ${r.version} ${r.cached ? "cached" : `built in ${r.ms} ms`}`);
    broadcast("fut-reload", { version: r.version });
  } else {
    lastError = r.message;
    console.error(`[fut] build failed:\n${r.message}`);
    broadcast("error", { message: r.message });
  }
  building = null;
  if (queued) {
    queued = false;
    void rebuild();
  }
}

// Compile-time parameters from the drawer: saved, then applied once they
// have stopped changing for a moment, so dragging a slider does not start a
// build per step. (Regenerating config.fut makes the watcher below rebuild.)
const SETTLE_MS = 700;
let settle: NodeJS.Timeout | undefined;
function setCompileParams(changes: Record<string, Value>): void {
  saveCompileValues(loadSchema(), changes);
  clearTimeout(settle);
  settle = setTimeout(() => generate(), SETTLE_MS);
}

// The schema itself changed: regenerate, and reload the page's drawer.
watch(".", (_e, file) => {
  if (file !== SCHEMA) return;
  try {
    generate();
    broadcast("web-reload");
  } catch (e) {
    broadcast("error", { message: `params.json: ${e}` });
  }
});

let debounce: NodeJS.Timeout | undefined;
// Watch the directory, not the files: editors that save by rename (vim,
// sed -i) replace the inode, which silently ends a per-file watch.
watch("fut", (_event, file) => {
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
  define: { STATIC: "false" }, // (see web/static.ts)
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
  if (req.url === "/params") {
    const schema = loadSchema();
    res.writeHead(200, { "Content-Type": "application/json", "Cache-Control": "no-store" });
    return void res.end(JSON.stringify({ schema, compile: compileValues(schema) }));
  }
  if (req.url === "/config" && req.method === "POST") {
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => {
      try {
        setCompileParams(JSON.parse(body));
        res.writeHead(204).end();
      } catch (e) {
        res.writeHead(400).end(String(e));
      }
    });
    return;
  }
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
}).listen(PORT, process.env.HOST ?? "0.0.0.0", () => console.log(`serving http://${process.env.HOST ?? "0.0.0.0"}:${PORT}`));

void rebuild();
