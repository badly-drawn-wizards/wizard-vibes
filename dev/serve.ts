// Runs the dev server (dev/server.ts) and restarts it whenever its own code
// under dev/ changes, so edits to the server, the build or the parameter
// generator take effect without a manual restart. The page's event stream
// reconnects by itself and is told the current build again.
//
// usage: node dev/serve.ts   (npm run dev)

import { type ChildProcess, spawn } from "node:child_process";
import { watch } from "node:fs";

// Tools that run on their own rather than inside the server.
const NOT_SERVER = new Set(["serve.ts", "bench-daemon.ts", "screenshot.ts", "firefox.ts", "gif.ts", "ref-render.ts"]);

let child: ChildProcess | null = null;
function start(): void {
  child = spawn(process.execPath, ["dev/server.ts"], { stdio: "inherit" });
  child.on("exit", (code, signal) => {
    if (signal === null && code !== 0) console.error(`[serve] server exited (${code}); restarts on the next change`);
    child = null;
  });
}

function restart(): void {
  if (!child) return start();
  child.once("exit", start);
  child.kill("SIGTERM");
}

let debounce: NodeJS.Timeout | undefined;
watch("dev", (_e, file) => {
  if (!file?.endsWith(".ts") || NOT_SERVER.has(file)) return;
  clearTimeout(debounce);
  debounce = setTimeout(() => {
    console.log(`[serve] ${file} changed; restarting the server`);
    restart();
  }, 200);
});

for (const sig of ["SIGINT", "SIGTERM"] as const) process.on(sig, () => (child?.kill(sig), process.exit()));
start();
