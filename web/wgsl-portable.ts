// Makes Futhark's WGSL acceptable to implementations without the
// `unrestricted_pointer_parameters` language feature (Firefox/naga):
//
// 1. Drop functions not reachable from an entry point. Futhark prepends a
//    ~270-function prelude to every shader; kernels use a handful, and the
//    rest include f16 and storage-pointer helpers naga rejects.
// 2. Specialise the remaining helpers that take storage/uniform/workgroup
//    pointers. Such pointers can only point at module-scope variables, so
//    `write_i8((&buf), i, v)` becomes `write_i8__buf(i, v)`, a copy of the
//    helper with `buf` substituted for the pointer; and a pointer to an
//    element, `atomic_read((&buf[k]), 0)`, becomes `atomic_read__buf_at(k,
//    0)`, the index passed in its place.
//
// `only`: the entry points to keep (all of them if not given).

import { F16_TYPE, stripComments, topLevelItems } from "./wgsl-f32.ts";

interface Fn {
  name: string;
  params: string[]; // raw parameter declarations
  ptrParams: number[]; // indices of storage/uniform/workgroup pointer params
  body: string; // text from the opening brace to the end
  header: string; // attributes and everything before the parameter list
  tail: string; // return type etc. between ')' and '{'
  entry: boolean;
}

const FN_HEAD = /^(\s*(?:@[\s\S]*?)?)\bfn\s+(\w+)\s*\(/;
const RESTRICTED_PTR = /\bptr\s*<\s*(storage|uniform|workgroup)\b/;

// Index of the bracket matching the one at `open`.
function matching(src: string, open: number): number {
  const pairs: Record<string, string> = { "(": ")", "{": "}", "[": "]" };
  const close = pairs[src[open]];
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    if (src[i] === src[open]) depth++;
    else if (src[i] === close && --depth === 0) return i;
  }
  return -1;
}

// Splits on commas at bracket depth 0. Angle brackets only count in
// declarations (`ptr<storage, array<i32>, read_write>`); in call arguments
// they are comparison and shift operators.
function splitArgs(src: string, generics = false): string[] {
  const open = generics ? "([{<" : "([{";
  const close = generics ? ")]}>" : ")]}";
  const out: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (open.includes(c)) depth++;
    else if (close.includes(c)) depth--;
    else if (c === "," && depth === 0) {
      out.push(src.slice(start, i));
      start = i + 1;
    }
  }
  if (src.slice(start).trim()) out.push(src.slice(start));
  return out;
}

function parseFn(item: string): Fn | null {
  const m = FN_HEAD.exec(item);
  if (!m) return null;
  const open = m[0].length - 1;
  const close = matching(item, open);
  const brace = item.indexOf("{", close);
  const params = splitArgs(item.slice(open + 1, close), true);
  return {
    name: m[2],
    params,
    ptrParams: params.flatMap((p, i) => (RESTRICTED_PTR.test(p) ? [i] : [])),
    body: item.slice(brace),
    header: m[1],
    tail: item.slice(close + 1, brace),
    entry: /@(compute|vertex|fragment)\b/.test(m[1]),
  };
}

function paramName(decl: string): string {
  return /^\s*(?:@[^\s]+\s+)*(\w+)/.exec(decl)![1];
}

// `&x`, `(&x)` or `((&x))` -> x; `&x[k]` -> x at index k; else null.
function addressedGlobal(arg: string): { g: string; index: string | null } | null {
  let a = arg.trim();
  while (a.startsWith("(") && matching(a, 0) === a.length - 1) a = a.slice(1, -1).trim();
  const whole = /^&\s*(\w+)$/.exec(a);
  if (whole) return { g: whole[1], index: null };
  const elem = /^&\s*(\w+)\s*\[/.exec(a);
  if (!elem) return null;
  const open = elem[0].length - 1;
  return matching(a, open) === a.length - 1 ? { g: elem[1], index: a.slice(open + 1, -1) } : null;
}

function calledNames(body: string): string[] {
  return [...body.matchAll(/\b(\w+)\s*\(/g)].map((m) => m[1]);
}

export function makePortable(input: string, only?: string[]): string {
  const code = stripComments(input);
  const directives: string[] = [];
  const others: string[] = [];
  const fns = new Map<string, Fn>();
  for (const item of topLevelItems(code)) {
    if (/^\s*(enable|requires|diagnostic)\b/.test(item)) directives.push(item.trim());
    else {
      const fn = parseFn(item);
      if (fn) fns.set(fn.name, fn);
      else if (item.trim()) others.push(item);
    }
  }

  // Specialise pointer-taking callees, then keep only what entry points reach.
  const specialised = new Map<string, Fn>();
  const rewriteCalls = (body: string): string => {
    let out = "";
    let pos = 0;
    for (const m of body.matchAll(/\b(\w+)\s*\(/g)) {
      const callee = fns.get(m[1]);
      if (!callee || callee.ptrParams.length === 0 || m.index! < pos) continue;
      const open = m.index! + m[0].length - 1;
      const close = matching(body, open);
      const args = splitArgs(body.slice(open + 1, close));
      const globals = callee.ptrParams.map((i) => addressedGlobal(args[i] ?? ""));
      if (globals.some((g) => g === null)) continue; // leave it; the compiler will say why
      const name = `${callee.name}__${globals.map((x) => x!.g + (x!.index === null ? "" : "_at")).join("__")}`;
      if (!specialised.has(name)) {
        let specBody = callee.body;
        callee.ptrParams.forEach((pi, k) => {
          const p = paramName(callee.params[pi]);
          const { g, index } = globals[k]!;
          const target = index === null ? g : `${g}[${p}_at]`;
          specBody = specBody
            .replace(new RegExp(`\\(\\s*\\*\\s*${p}\\s*\\)`, "g"), target)
            .replace(new RegExp(`\\*\\s*${p}\\b`, "g"), target)
            .replace(new RegExp(`\\b${p}\\b`, "g"), `(&${target})`);
        });
        // (an element pointer's parameter becomes its index)
        const params = callee.params.flatMap((decl, i) => {
          const k = callee.ptrParams.indexOf(i);
          if (k < 0) return [decl];
          return globals[k]!.index === null ? [] : [`${paramName(decl)}_at: i32`];
        });
        const spec: Fn = { ...callee, name, params, ptrParams: [], body: "" };
        specialised.set(name, spec);
        spec.body = rewriteCalls(specBody); // after registering, for recursion through helpers
      }
      const rest = args.flatMap((a, i) => {
        const k = callee.ptrParams.indexOf(i);
        if (k < 0) return [rewriteCalls(a)]; // (nested calls)
        const index = globals[k]!.index;
        return index === null ? [] : [`i32(${rewriteCalls(index)})`];
      });
      out += body.slice(pos, m.index!) + `${name}(${rest.join(",")})`;
      pos = close + 1;
    }
    return out + body.slice(pos);
  };

  const all = new Map(fns);
  for (const fn of fns.values()) fn.body = rewriteCalls(fn.body);
  for (const [k, v] of specialised) all.set(k, v);

  const reachable = new Set<string>();
  const work = [...all.values()].filter((f) => (only ? only.includes(f.name) : f.entry)).map((f) => f.name);
  while (work.length) {
    const n = work.pop()!;
    if (reachable.has(n)) continue;
    reachable.add(n);
    for (const c of calledNames(all.get(n)!.body)) if (all.has(c) && !reachable.has(c)) work.push(c);
  }

  const kept = [...all.values()]
    .filter((f) => reachable.has(f.name))
    .map((f) => `${f.header}fn ${f.name}(${f.params.join(",")})${f.tail}${f.body}\n`);
  const body = others.join("") + "\n" + kept.join("");
  // Keep `enable f16;` only if something left still needs it.
  const dirs = directives.filter((d) => !/^enable\s+f16\b/.test(d) || F16_TYPE.test(body));
  return dirs.join("\n") + "\n" + body;
}
