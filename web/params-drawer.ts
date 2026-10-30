// The parameter drawer: built from the schema the dev server serves
// (params.json). Uniforms are packed into a Float32Array that is passed to
// `render` every frame; compile-time parameters are sent to the server,
// which rebuilds the shader once they settle. Native inputs throughout.

type Kind = "slider" | "int" | "color" | "checkbox" | "select";
type Value = number | boolean | number[];
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
interface Schema {
  groups: { name: string; params: Param[] }[];
}

const STORE = "wizard-params";
const width = (p: Param) => (p.kind === "color" ? 3 : 1);

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

export interface Params {
  // the uniforms, in schema order; `version` changes whenever they do
  readonly values: Float32Array;
  readonly version: number;
}

export async function createDrawer(host: HTMLElement): Promise<Params> {
  const { schema, compile } = (await (await fetch("/params")).json()) as { schema: Schema; compile: Record<string, Value> };
  const uniforms = schema.groups.flatMap((g) => g.params).filter((p) => !p.compile);
  const offset = new Map<string, number>();
  let n = 0;
  for (const p of uniforms) {
    offset.set(p.id, n);
    n += width(p);
  }

  const stored = load();
  const current: Record<string, Value> = {};
  for (const p of uniforms) current[p.id] = p.id in stored ? stored[p.id] : p.default;
  const state = { values: new Float32Array(n), version: 0 };
  const pack = (p: Param) => {
    const v = current[p.id];
    const o = offset.get(p.id)!;
    if (Array.isArray(v)) state.values.set(v, o);
    else state.values[o] = typeof v === "boolean" ? Number(v) : v;
  };
  uniforms.forEach(pack);

  const pending: Record<string, Value> = {};
  let send: number | undefined;
  const set = (p: Param, v: Value) => {
    if (p.compile) {
      // the server waits for changes to settle before rebuilding; batch here too
      pending[p.id] = v;
      clearTimeout(send);
      send = window.setTimeout(() => {
        void fetch("/config", { method: "POST", body: JSON.stringify(pending) });
        for (const k in pending) delete pending[k];
      }, 150);
    } else {
      current[p.id] = v;
      pack(p);
      state.version++;
      save(current);
    }
  };

  host.replaceChildren();
  for (const g of schema.groups) {
    const box = document.createElement("details");
    box.open = !g.params.some((p) => p.compile);
    const summary = document.createElement("summary");
    summary.textContent = g.name;
    box.append(summary);
    for (const p of g.params) box.append(row(p, p.compile ? compile[p.id] : current[p.id], (v) => set(p, v)));
    host.append(box);
  }
  return state;
}

// One labelled control, with a reset-to-default button.
function row(p: Param, initial: Value, onChange: (v: Value) => void): HTMLElement {
  const label = document.createElement("label");
  label.className = "param";
  const name = document.createElement("span");
  name.textContent = p.label;
  const out = document.createElement("output");
  let input: HTMLInputElement | HTMLSelectElement;
  let show: (v: Value) => void;

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

  const reset = document.createElement("button");
  reset.type = "button";
  reset.textContent = "↺";
  reset.title = `Reset to ${Array.isArray(p.default) ? toHex(p.default) : p.default}`;
  reset.onclick = (e) => {
    e.preventDefault();
    show(p.default);
    onChange(p.default);
  };
  label.append(name, input, out, reset);
  return label;
}
