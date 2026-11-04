// The parameter drawer: built from params.json's schema, as nested
// collapsing groups, each with a button resetting everything in it. The
// uniforms' values go to the shader each frame; the compile-time ones (the
// schema's "compile": true) are sent to the dev server, which rebuilds the
// shader with them once they settle. At the top, "Copy changes" puts every
// value that differs from its default on the clipboard as JSON (to hand
// over as new defaults). Without a dev server (a static build) the
// compile-time parameters are shown but cannot be changed.
//
// Colours are linear RGB.

use crate::types::{ParamValues, Value};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Deserialize, Clone, Debug)]
pub struct Param {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub compile: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Schema {
    pub groups: Vec<Group>,
    // quality presets: compile-time values by level (low, med, high)
    #[serde(default)]
    pub quality: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Schema {
    pub fn params(&self) -> Vec<&Param> {
        fn walk<'a>(gs: &'a [Group], out: &mut Vec<&'a Param>) {
            for g in gs {
                out.extend(g.params.iter());
                walk(&g.groups, out);
            }
        }
        let mut out = vec![];
        walk(&self.groups, &mut out);
        out
    }
}

pub const STORE: &str = "wizard-params";
const OPEN: &str = "wizard-params-open";

pub struct Params {
    pub schema: Schema,
    // the uniforms, by id
    values: HashMap<String, Value>,
    // the compile-time values in force (the shader's), and those changed
    // here but not yet sent
    pub compile: HashMap<String, Value>,
    pending: HashMap<String, Value>,
    pending_since: f64,
    // whether compile-time changes can go anywhere (a dev server)
    pub can_rebuild: bool,
    // groups the user has opened or closed, by path
    open: HashMap<String, bool>,
    pub copied: Option<(String, f64)>,
}

impl Params {
    pub fn new(schema: Schema, storage: Option<&dyn eframe::Storage>) -> Params {
        let mut values: HashMap<String, Value> = schema.params().iter().filter(|p| !p.compile).map(|p| (p.id.clone(), p.default.clone())).collect();
        let compile = schema.params().iter().filter(|p| p.compile).map(|p| (p.id.clone(), p.default.clone())).collect();
        let saved: HashMap<String, Value> = storage.and_then(|s| eframe::get_value(s, STORE)).unwrap_or_default();
        for (k, v) in saved {
            if values.contains_key(&k) {
                values.insert(k, v);
            }
        }
        let open = storage.and_then(|s| eframe::get_value(s, OPEN)).unwrap_or_default();
        Params { schema, values, compile, pending: HashMap::new(), pending_since: 0.0, can_rebuild: false, open, copied: None }
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORE, &self.values);
        eframe::set_value(storage, OPEN, &self.open);
    }

    // The uniforms in force.
    pub fn values(&self) -> ParamValues {
        ParamValues(self.values.clone())
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.values.get(id)
    }

    // Sets a uniform, as its control would (for controls elsewhere, such
    // as the top bar's).
    pub fn set(&mut self, id: &str, v: Value) {
        if self.values.contains_key(id) {
            self.values.insert(id.to_string(), v);
        }
    }

    // The compile-time changes to send, once they have settled (half a
    // second after the last), at time `now` in seconds.
    pub fn take_compile_changes(&mut self, now: f64) -> Option<HashMap<String, Value>> {
        if self.pending.is_empty() || now - self.pending_since < 0.5 {
            return None;
        }
        Some(std::mem::take(&mut self.pending))
    }

    fn changes(&self) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        for p in self.schema.params() {
            let v = if p.compile { self.compile.get(&p.id) } else { self.values.get(&p.id) };
            if let Some(v) = v {
                if *v != p.default {
                    out.insert(p.id.clone(), round(v));
                }
            }
        }
        out
    }

    // The drawer's contents.
    pub fn ui(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.horizontal(|ui| {
            if ui.button("Copy changes").on_hover_text("Every value that differs from its default, as JSON").clicked() {
                let text = serde_json::to_string_pretty(&self.changes()).unwrap();
                ui.ctx().copy_text(text);
                self.copied = Some(("copied".into(), now));
            }
            if ui.button("Reset all").clicked() {
                let all: Vec<Param> = self.schema.params().into_iter().cloned().collect();
                for p in &all {
                    self.reset(p, now);
                }
            }
            if let Some((msg, t)) = &self.copied {
                if now - t < 2.0 {
                    ui.weak(msg.as_str());
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let groups = self.schema.groups.clone();
            for (i, g) in groups.iter().enumerate() {
                self.group(ui, g, &format!("{i}"), 0, now);
            }
        });
    }

    fn group(&mut self, ui: &mut egui::Ui, g: &Group, path: &str, depth: usize, now: f64) {
        // top-level groups start open, except the shader's (compile-time)
        let shader_only = !g.params.is_empty() && g.params.iter().all(|p| p.compile);
        let default_open = depth == 0 && !shader_only;
        let id = ui.make_persistent_id(("params-group", path));
        let open = *self.open.get(path).unwrap_or(&default_open);
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open);
        state.set_open(open);
        let (toggle, _, body) = state
            .show_header(ui, |ui| {
                ui.strong(&g.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("↺").on_hover_text(format!("Reset {}", g.name)).clicked() {
                        let mut ps = vec![];
                        collect(g, &mut ps);
                        for p in &ps {
                            self.reset(p, now);
                        }
                    }
                });
            })
            .body(|ui| {
                for p in &g.params {
                    self.control(ui, p, now);
                }
                for (i, sub) in g.groups.iter().enumerate() {
                    self.group(ui, sub, &format!("{path}/{i}"), depth + 1, now);
                }
            });
        if toggle.clicked() {
            self.open.insert(path.to_string(), !open);
        }
        let _ = body;
    }

    fn reset(&mut self, p: &Param, now: f64) {
        if p.compile {
            if self.can_rebuild && self.compile.get(&p.id) != Some(&p.default) {
                self.set_compile(&p.id, p.default.clone(), now);
            }
        } else {
            self.values.insert(p.id.clone(), p.default.clone());
        }
    }

    fn set_compile(&mut self, id: &str, v: Value, now: f64) {
        self.compile.insert(id.to_string(), v.clone());
        self.pending.insert(id.to_string(), v);
        self.pending_since = now;
    }

    // One labelled control, with a reset-to-default button.
    fn control(&mut self, ui: &mut egui::Ui, p: &Param, now: f64) {
        let enabled = !p.compile || self.can_rebuild;
        let mut v = if p.compile { self.compile.get(&p.id) } else { self.values.get(&p.id) }.cloned().unwrap_or(p.default.clone());
        let before = v.clone();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(enabled, |ui| {
                let label = if p.compile { format!("{} *", p.label) } else { p.label.clone() };
                let tip = if p.compile {
                    if enabled { "Compile-time: changing it rebuilds the shader" } else { "Compile-time: fixed without the dev server" }
                } else {
                    ""
                };
                let l = ui.label(label);
                if !tip.is_empty() {
                    l.on_hover_text(tip);
                }
                match p.kind.as_str() {
                    "slider" => {
                        let mut x = num(&v);
                        let (lo, hi) = (p.min.unwrap_or(0.0), p.max.unwrap_or(1.0));
                        let mut s = egui::Slider::new(&mut x, lo..=hi);
                        if let Some(st) = p.step {
                            s = s.step_by(st);
                        }
                        ui.add(s);
                        v = Value::Num(x);
                    }
                    "int" => {
                        let mut x = num(&v).round() as i64;
                        let (lo, hi) = (p.min.unwrap_or(0.0) as i64, p.max.unwrap_or(100.0) as i64);
                        ui.add(egui::Slider::new(&mut x, lo..=hi));
                        v = Value::Num(x as f64);
                    }
                    "checkbox" => {
                        let mut b = matches!(v, Value::Bool(true)) || matches!(v, Value::Num(x) if x != 0.0);
                        ui.checkbox(&mut b, "");
                        v = Value::Bool(b);
                    }
                    "select" => {
                        let opts = p.options.clone().unwrap_or_default();
                        let mut i = num(&v).round() as usize;
                        egui::ComboBox::from_id_salt(("select", &p.id)).selected_text(opts.get(i).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                            for (k, o) in opts.iter().enumerate() {
                                ui.selectable_value(&mut i, k, o);
                            }
                        });
                        v = Value::Num(i as f64);
                    }
                    "flags" => {
                        let opts = p.options.clone().unwrap_or_default();
                        let mut bits = num(&v).round() as i64;
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (k, o) in opts.iter().enumerate() {
                                    let mut on = bits & (1 << k) != 0;
                                    if ui.checkbox(&mut on, o).changed() {
                                        bits ^= 1 << k;
                                    }
                                }
                            });
                        });
                        v = Value::Num(bits as f64);
                    }
                    "color" => {
                        let c = colour(&v);
                        // (the picker works in gamma-encoded sRGB)
                        let mut rgba = egui::Rgba::from_rgb(c[0] as f32, c[1] as f32, c[2] as f32);
                        egui::widgets::color_picker::color_edit_button_rgba(ui, &mut rgba, egui::widgets::color_picker::Alpha::Opaque);
                        v = Value::Colour([rgba.r() as f64, rgba.g() as f64, rgba.b() as f64]);
                    }
                    _ => {
                        ui.weak(format!("({})", p.kind));
                    }
                }
                if v != p.default && ui.small_button("↺").on_hover_text("Reset").clicked() {
                    v = p.default.clone();
                }
            });
        });
        if v != before {
            if p.compile {
                self.set_compile(&p.id, v, now);
            } else {
                self.values.insert(p.id.clone(), v);
            }
        }
    }
}

fn collect(g: &Group, out: &mut Vec<Param>) {
    out.extend(g.params.iter().cloned());
    for s in &g.groups {
        collect(s, out);
    }
}

fn num(v: &Value) -> f64 {
    match v {
        Value::Num(x) => *x,
        Value::Bool(b) => *b as i32 as f64,
        Value::Colour(c) => c[0],
    }
}

fn colour(v: &Value) -> [f64; 3] {
    match v {
        Value::Colour(c) => *c,
        other => [num(other); 3],
    }
}

// Rounded for reading: colours to 4 decimals, numbers to 6 significant digits.
fn round(v: &Value) -> Value {
    let sig = |x: f64| {
        if x == 0.0 {
            0.0
        } else {
            let k = 10f64.powi(5 - x.abs().log10().floor() as i32);
            (x * k).round() / k
        }
    };
    match v {
        Value::Colour(c) => Value::Colour(c.map(|x| (x * 1e4).round() / 1e4)),
        Value::Num(x) => Value::Num(sig(*x)),
        b => b.clone(),
    }
}
