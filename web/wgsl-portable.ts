// Makes Futhark's WGSL acceptable to implementations without the
// `unrestricted_pointer_parameters` language feature (Firefox/naga):
//
// 1. Drop functions not reachable from an entry point. Futhark prepends a
//    ~270-function prelude to every shader; kernels use a handful, and the
//    rest include f16 and storage-pointer helpers naga rejects.
// 2. Specialise the remaining helpers that take storage/uniform/workgroup
//    pointers. Such pointers can only point at module-scope variables, so
//    `write_i8((&buf), i, v)` becomes `write_i8__buf(i, v)`, a copy of the
//    helper with `buf` substituted for the pointer.

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

// `&x`, `(&x)` or `((&x))` -> `x`; anything else -> null.
function addressedGlobal(arg: string): string | null {
  let a = arg.trim();
  while (a.startsWith("(") && matching(a, 0) === a.length - 1) a = a.slice(1, -1).trim();
  return /^&\s*(\w+)$/.exec(a)?.[1] ?? null;
}

function calledNames(body: string): string[] {
  return [...body.matchAll(/\b(\w+)\s*\(/g)].map((m) => m[1]);
}

export function makePortable(input: string): string {
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
      const name = `${callee.name}__${globals.join("__")}`;
      if (!specialised.has(name)) {
        let specBody = callee.body;
        callee.ptrParams.forEach((pi, k) => {
          const p = paramName(callee.params[pi]);
          const g = globals[k]!;
          specBody = specBody
            .replace(new RegExp(`\\(\\s*\\*\\s*${p}\\s*\\)`, "g"), g)
            .replace(new RegExp(`\\*\\s*${p}\\b`, "g"), g)
            .replace(new RegExp(`\\b${p}\\b`, "g"), `(&${g})`);
        });
        const spec: Fn = {
          ...callee,
          name,
          params: callee.params.filter((_, i) => !callee.ptrParams.includes(i)),
          ptrParams: [],
          body: "",
        };
        specialised.set(name, spec);
        spec.body = rewriteCalls(specBody); // after registering, for recursion through helpers
      }
      const rest = args.filter((_, i) => !callee.ptrParams.includes(i)).map(rewriteCalls); // nested calls
      out += body.slice(pos, m.index!) + `${name}(${rest.join(",")})`;
      pos = close + 1;
    }
    return out + body.slice(pos);
  };

  const all = new Map(fns);
  for (const fn of fns.values()) fn.body = rewriteCalls(fn.body);
  for (const [k, v] of specialised) all.set(k, v);

  const reachable = new Set<string>();
  const work = [...all.values()].filter((f) => f.entry).map((f) => f.name);
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
