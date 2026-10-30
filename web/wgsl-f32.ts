// Futhark's WebGPU backend always requests the `shader-f16` device feature
// and prefixes every shader with `enable f16;` plus f16 helper functions,
// even when the program never uses f16. On adapters without f16 the loader
// drops the feature request and shader-check.ts strips those declarations
// before shader compilation.

export const F16_TYPE = /\bf16\b|\bvec[234]h\b|\bmat[234]x[234]h\b/;

export function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
}

// Splits WGSL into top-level declarations: each ends at a `;` or a closing
// `}` at brace depth 0. Attributes stay attached to the declaration they precede.
export function topLevelItems(src: string): string[] {
  const items: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) {
        // `struct S { ... };` – swallow an optional trailing semicolon.
        const end = src[i + 1] === ";" ? i + 2 : i + 1;
        items.push(src.slice(start, end));
        start = end;
      }
    } else if (c === ";" && depth === 0) {
      items.push(src.slice(start, i + 1));
      start = i + 1;
    }
  }
  items.push(src.slice(start));
  return items;
}

function declaredName(item: string): string | null {
  return /\b(?:fn|alias|struct|const|override|var(?:<[^>]*>)?)\s+([A-Za-z_]\w*)/.exec(item)?.[1] ?? null;
}

// Futhark also declares unused `var index_certs_N : f16;` placeholders in
// kernels that index arrays with bounds checks; those are simply removed.
const UNUSED_F16_VAR = /\bvar\s+(\w+)\s*:\s*f16\s*;/g;

function dropUnusedF16Vars(code: string): string {
  const counts = new Map<string, number>();
  for (const m of code.matchAll(/\b\w+\b/g)) counts.set(m[0], (counts.get(m[0]) ?? 0) + 1);
  return code.replace(UNUSED_F16_VAR, (decl, name) => (counts.get(name) === 1 ? "" : decl));
}

export function stripF16(code: string): string {
  if (!/\benable\s+f16\s*;/.test(code)) return code;
  const items = topLevelItems(dropUnusedF16Vars(stripComments(code)).replace(/\benable\s+f16\s*;/, ""));
  const dropped = new Set<string>();
  const kept: string[] = [];
  for (const item of items) {
    if (F16_TYPE.test(item)) {
      const name = declaredName(item);
      if (!name) throw new Error(`cannot strip f16 from WGSL declaration:\n${item.trim().slice(0, 200)}`);
      // never silently drop a kernel: that surfaces much later, as a missing entry point
      if (/@compute\b/.test(item)) throw new Error(`kernel '${name}' uses f16, so it needs the shader-f16 feature`);
      dropped.add(name);
    } else {
      kept.push(item);
    }
  }
  const out = kept.join("");
  for (const name of dropped) {
    if (new RegExp(`\\b${name}\\b`).test(out)) {
      throw new Error(`shader uses f16 (via '${name}'), so it needs the shader-f16 feature`);
    }
  }
  return out;
}
