// The parameter drawer: built from the schema the dev server serves
// (params.json), as nested accordions, each with a button resetting
// everything in it. The uniforms' values are passed to `render` as its
// params record; compile-time parameters are sent to the server, which
// rebuilds the shader once they settle. Native inputs throughout. At the top,
// "Copy changes" puts every value that differs from its default on the
// clipboard as JSON (to hand over as new defaults). In the static build there
// is no server to rebuild the shader, so the compile-time parameters are
// shown but disabled.

import { IS_STATIC } from "./static.ts";

type Kind = "slider" | "int" | "color" | "checkbox" | "select" | "flags";
export type Value = number | boolean | number[];
interface Param {
  id: string;
  label: string;
  kind: Kind;
  default: Value;
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  compile?: boolean;
}
interface Group {
  name: string;
  params?: Param[];
  groups?: Group[];
}
interface Schema {
  groups: Group[];
}

const STORE = "wizard-params";
const OPEN = "wizard-drawer-open"; // the open accordions, by path

const groupParams = (g: Group): Param[] => [...(g.params ?? []), ...(g.groups ?? []).flatMap(groupParams)];

// Colours are linear RGB; the colour input shows them gamma-encoded.
const toHex = (c: number[]) =>
  "#" + c.map((x) => Math.round(Math.min(1, Math.max(0, x)) ** (1 / 2.2) * 255).toString(16).padStart(2, "0")).join("");
const fromHex = (h: string) => [1, 3, 5].map((i) => (parseInt(h.slice(i, i + 2), 16) / 255) ** 2.2);

function load(): Record<string, Value> {
  try {
    return JSON.parse(localStorage.getItem(STORE) ?? "{}");
  } catch {
    return {};
  }
}
function save(values: Record<string, Value>): void {
  try {
    localStorage.setItem(STORE, JSON.stringify(values));
  } catch {
    // storage unavailable: values last for this page only
  }
}

function loadOpen(): Set<string> | null {
  try {
    const s = localStorage.getItem(OPEN);
    return s ? new Set(JSON.parse(s)) : null;
  } catch {
    return null;
  }
}
function saveOpen(open: Set<string>): void {
  try {
    localStorage.setItem(OPEN, JSON.stringify([...open]));
  } catch {
    // ignore
  }
}

const same = (a: Value, b: Value): boolean =>
  Array.isArray(a) && Array.isArray(b) ? a.every((x, i) => Math.abs(x - b[i]) < 1e-4) : typeof a === "number" && typeof b === "number" ? Math.abs(a - b) < 1e-9 : a === b;

// Rounded for reading: colours to 4 decimals, numbers to 6 significant digits.
const tidy = (v: Value): Value =>
  Array.isArray(v) ? v.map((x) => Math.round(x * 1e4) / 1e4) : typeof v === "number" ? Number(v.toPrecision(6)) : v;

// Puts text on the clipboard: the Clipboard API needs a secure context (not
// plain http over the network), so fall back to a selected textarea, and
// failing that leave it shown for copying by hand.
async function copyText(text: string, host: HTMLElement): Promise<string> {
  try {
    await navigator.clipboard.writeText(text);
    return "copied";
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.readOnly = true;
    area.className = "export";
    host.after(area);
    area.select();
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch {
      // unsupported
    }
    if (ok) {
      area.remove();
      return "copied";
    }
    area.onblur = () => area.remove();
    area.focus();
    return "select and copy below";
  }
}

export interface Params {
  // the uniforms by id (colours as [r, g, b]); `version` changes with them
  readonly values: Record<string, Value>;
  readonly version: number;
  // the compile-time parameters in force, the quality presets (compile-time
  // values by level), and in the static site the build of each level
  readonly compile: Record<string, Value>;
  readonly presets: Record<string, Record<string, Value>>;
  readonly builds?: Record<string, string>;
  // sets a parameter as its control in the drawer would (for controls
  // elsewhere, such as the top bar's), and calls `listen`'s functions on
  // every change, from either
  set(id: string, v: Value): void;
  listen(f: (id: string, v: Value) => void): void;
}

export async function createDrawer(host: HTMLElement): Promise<Params> {
  const { schema, compile, presets, builds } = (await (await fetch(IS_STATIC ? "params.json" : "/params")).json()) as {
    schema: Schema;
    compile: Record<string, Value>;
    presets?: Record<string, Record<string, Value>>;
    builds?: Record<string, string>;
  };
  const all = schema.groups.flatMap(groupParams);
  const uniforms = all.filter((p) => !p.compile);
  const stored = load();
  const current: Record<string, Value> = {};
  for (const p of uniforms) current[p.id] = p.id in stored ? stored[p.id] : p.default;
  const setters = new Map<string, (v: Value) => void>();
  const listeners: ((id: string, v: Value) => void)[] = [];
  const state = {
    values: current, version: 0, compile, presets: presets ?? {}, builds,
    set: (id: string, v: Value) => setters.get(id)?.(v),
    listen: (f: (id: string, v: Value) => void) => listeners.push(f),
  };

  const pending: Record<string, Value> = {};
  let send: number | undefined;
  const set = (p: Param, v: Value) => {
    if (p.compile) {
      if (IS_STATIC) return;
      // the server waits for changes to settle before rebuilding; batch here too
      pending[p.id] = v;
      clearTimeout(send);
      send = window.setTimeout(() => {
        void fetch("/config", { method: "POST", body: JSON.stringify(pending) });
        for (const k in pending) delete pending[k];
      }, 150);
    } else {
      current[p.id] = v;
      state.version++;
      save(current);
    }
  };

  host.replaceChildren();
  const compileNow = { ...compile }; // what the drawer last set them to
  const resets: (() => void)[] = [];

  // the toolbar: copy the changes from the defaults, reset everything
  const bar = document.createElement("div");
  bar.className = "drawer-bar";
  const copy = document.createElement("button");
  copy.type = "button";
  copy.textContent = "Copy changes";
  copy.title = "Copy every value that differs from its default, as JSON";
  const note = document.createElement("span");
  copy.onclick = async () => {
    const diff: Record<string, Value> = {};
    for (const p of all) {
      const v = p.compile ? compileNow[p.id] : current[p.id];
      if (v !== undefined && !same(v, p.default)) diff[p.id] = tidy(v);
    }
    const n = Object.keys(diff).length;
    note.textContent = n ? `${await copyText(JSON.stringify(diff, null, 2), bar)} (${n} change${n > 1 ? "s" : ""})` : "no changes";
  };
  const resetAll = document.createElement("button");
  resetAll.type = "button";
  resetAll.textContent = "Reset all";
  resetAll.onclick = () => resets.forEach((r) => r());
  bar.append(copy, resetAll, note);
  host.append(bar);

  // the accordions; top-level ones start open (except the shader's), until
  // the user opens and closes them
  const open = loadOpen() ?? new Set(schema.groups.filter((g) => !groupParams(g).some((p) => p.compile)).map((g) => g.name));
  const build = (g: Group, path: string): { el: HTMLElement; reset: () => void } => {
    const box = document.createElement("details");
    box.open = open.has(path);
    box.ontoggle = () => {
      if (box.open) open.add(path);
      else open.delete(path);
      saveOpen(open);
    };
    const summary = document.createElement("summary");
    const name = document.createElement("span");
    name.textContent = g.name;
    const mine: (() => void)[] = [];
    const reset = () => mine.forEach((r) => r());
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "↺";
    button.title = `Reset ${g.name} to its defaults`;
    button.onclick = (e) => {
      e.preventDefault(); // (don't toggle the accordion)
      reset();
    };
    summary.append(name, button);
    box.append(summary);
    for (const p of g.params ?? []) {
      const r = row(p, p.compile ? compile[p.id] : current[p.id], (v) => {
        if (p.compile) compileNow[p.id] = v;
        set(p, v);
        for (const f of listeners) f(p.id, v);
      });
      setters.set(p.id, r.set);
      if (p.compile && IS_STATIC) {
        for (const c of r.el.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>("input, select, button")) c.disabled = true;
        r.el.title = "Fixed in this build (changing it rebuilds the shader, which needs the dev server)";
      }
      box.append(r.el);
      mine.push(r.reset);
    }
    for (const sub of g.groups ?? []) {
      const s = build(sub, `${path}/${sub.name}`);
      box.append(s.el);
      mine.push(s.reset);
    }
    return { el: box, reset };
  };
  for (const g of schema.groups) {
    const b = build(g, g.name);
    host.append(b.el);
    resets.push(b.reset);
  }
  return state;
}

// One labelled control, with a reset-to-default button; `reset` does the same
// (only if the value differs, so a group reset does not trigger needless
// shader rebuilds).
function row(p: Param, initial: Value, onChange: (v: Value) => void): { el: HTMLElement; reset: () => void; set: (v: Value) => void } {
  let value = initial;
  const changed = onChange;
  onChange = (v) => {
    value = v;
    changed(v);
  };
  let reset = () => {};
  let set = (_: Value) => {};
  const el = rowElement(p, initial, onChange, (show) => {
    set = (v) => {
      if (same(value, v)) return;
      show(v);
      onChange(v);
    };
    reset = () => set(p.default);
  });
  return { el, reset: () => reset(), set: (v) => set(v) };
}

function rowElement(p: Param, initial: Value, onChange: (v: Value) => void, expose: (show: (v: Value) => void) => void): HTMLElement {
  const label = document.createElement("label");
  label.className = "param";
  const name = document.createElement("span");
  name.textContent = p.label;
  const out = document.createElement("output");
  let input: HTMLInputElement | HTMLSelectElement;
  let show: (v: Value) => void;

  if (p.kind === "flags") {
    // one checkbox per option; the value is the bitmask of the checked ones
    const box = document.createElement("span");
    box.className = "flags";
    const checks = p.options!.map((o, i) => {
      const l = document.createElement("label");
      const c = document.createElement("input");
      c.type = "checkbox";
      c.onchange = () => onChange(checks.reduce((m, x, j) => m | (x.checked ? 1 << j : 0), 0));
      l.append(c, o);
      box.append(l);
      return c;
    });
    show = (v) => checks.forEach((c, i) => (c.checked = ((v as number) & (1 << i)) !== 0));
    show(initial);
    expose(show);
    const reset = resetButton(p, show, onChange);
    label.classList.add("wide");
    label.append(name, box, reset);
    return label;
  }

  if (p.kind === "select") {
    const sel = document.createElement("select");
    p.options!.forEach((o, i) => sel.add(new Option(o, String(i))));
    input = sel;
    show = (v) => (sel.value = String(v));
    sel.onchange = () => onChange(Number(sel.value));
  } else {
    const el = document.createElement("input");
    input = el;
    if (p.kind === "checkbox") {
      el.type = "checkbox";
      show = (v) => (el.checked = Boolean(v));
      el.onchange = () => onChange(el.checked);
    } else if (p.kind === "color") {
      el.type = "color";
      show = (v) => (el.value = toHex(v as number[]));
      el.oninput = () => onChange(fromHex(el.value));
    } else {
      el.type = "range";
      el.min = String(p.min ?? 0);
      el.max = String(p.max ?? 1);
      el.step = String(p.kind === "int" ? 1 : (p.step ?? 0.01));
      const digits = p.kind === "int" ? 0 : Math.max(0, -Math.floor(Math.log10(p.step ?? 0.01)));
      show = (v) => {
        el.value = String(v);
        out.value = (v as number).toFixed(digits);
      };
      el.oninput = () => {
        out.value = Number(el.value).toFixed(digits);
        onChange(Number(el.value));
      };
    }
  }
  show(initial);
  expose(show);

  label.append(name, input, out, resetButton(p, show, onChange));
  return label;
}

function resetButton(p: Param, show: (v: Value) => void, onChange: (v: Value) => void): HTMLButtonElement {
  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "↺";
  reset.title = `Reset to ${Array.isArray(p.default) ? toHex(p.default) : p.default}`;
  reset.onclick = (e) => {
    e.preventDefault();
    show(p.default);
    onChange(p.default);
  };
  return reset;
}
