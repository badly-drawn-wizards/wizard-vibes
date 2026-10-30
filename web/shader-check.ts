// Wraps a device's createShaderModule to (optionally) rewrite Futhark's WGSL
// (strip f16, make it portable to naga) and to report compiler diagnostics
// from getCompilationInfo(), with the offending source line and a caret.

import { stripF16 } from "./wgsl-f32.ts";
import { makePortable } from "./wgsl-portable.ts";

export interface ShaderOptions {
  f32: boolean; // strip f16 (see wgsl-f32.ts)
  portable: boolean; // no unrestricted_pointer_parameters (see wgsl-portable.ts)
}

export interface ShaderLog {
  errors: string[]; // formatted error reports, in arrival order
  warnings: number;
  // Resolves once every module created so far has its compilation info.
  settled(): Promise<void>;
  // Called for errors that arrive after loading finished.
  onError?: (report: string) => void;
}

const MAX_LINE = 120;

function kernelName(code: string): string {
  return /@compute[\s\S]*?\bfn\s+(\w+)/.exec(code)?.[1] ?? "shader";
}

// Name of the last function declared before the message position, i.e. the
// one it points into (Futhark labels every module "Futhark program").
// Computed from line/column: browsers disagree on `offset`.
function enclosingFn(code: string, lineNum: number, linePos: number): string | null {
  const lines = code.split("\n");
  const before = lines.slice(0, lineNum - 1).join("\n") + "\n" + (lines[lineNum - 1] ?? "").slice(0, linePos - 1);
  let name: string | null = null;
  for (const m of before.matchAll(/\bfn\s+(\w+)/g)) name = m[1];
  return name;
}

function formatMessage(label: string, code: string, msg: GPUCompilationMessage): string {
  const fn = msg.lineNum > 0 ? enclosingFn(code, msg.lineNum, msg.linePos) : null;
  const where = `${label}${fn ? ` (fn ${fn})` : ""}:${msg.lineNum}:${msg.linePos}`;
  const head = `${where}: ${msg.type}: ${msg.message}`;
  const line = code.split("\n")[msg.lineNum - 1];
  if (line === undefined || msg.lineNum === 0) return head;
  // Futhark emits long lines; show a window around the error position.
  const col = Math.max(0, msg.linePos - 1);
  const start = line.length > MAX_LINE ? Math.max(0, Math.min(col - 40, line.length - MAX_LINE)) : 0;
  const shown = line.slice(start, start + MAX_LINE);
  const width = Math.max(1, Math.min(msg.length || 1, shown.length - (col - start)));
  const caret = " ".repeat(col - start) + "^".repeat(width);
  const prefix = start > 0 ? "…" : "";
  return `${head}\n  ${prefix}${shown}\n  ${" ".repeat(prefix.length)}${caret}`;
}

export function instrumentShaders(device: GPUDevice, opts: ShaderOptions): ShaderLog {
  const pending: Promise<void>[] = [];
  const log: ShaderLog = {
    errors: [],
    warnings: 0,
    settled: () => Promise.all(pending).then(() => {}),
  };
  const create = device.createShaderModule.bind(device);

  device.createShaderModule = (desc: GPUShaderModuleDescriptor) => {
    let code = opts.f32 ? stripF16(desc.code) : desc.code;
    if (opts.portable) code = makePortable(code);
    const module = create({ ...desc, code });
    const label = desc.label || kernelName(code);
    pending.push(
      module.getCompilationInfo().then((info) => {
        for (const msg of info.messages) {
          if (msg.type === "error") {
            const report = formatMessage(label, code, msg);
            console.error(report);
            log.errors.push(report);
            log.onError?.(report);
          } else if (msg.type === "warning") {
            log.warnings++;
            console.warn(formatMessage(label, code, msg));
          }
        }
      }),
    );
    return module;
  };
  return log;
}
