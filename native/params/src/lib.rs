// params.json: every tweakable parameter, and how the drawer lays it out.
// From it come the shader crate's `generated` module (the compile-time
// parameters and layer flags as constants, the uniforms as records and as
// the flat block the app fills: wizard-shader-macros' params!() writes it
// as the crate compiles), and the block's layout for the app (shader.json,
// which shader-builder writes), so the app fills it whatever the build's
// parameters.

use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Deserialize, Clone)]
pub struct Param {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub default: Value,
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub compile: bool,
}

#[derive(Deserialize)]
struct Group {
    #[serde(default)]
    params: Vec<Param>,
    #[serde(default)]
    groups: Vec<Group>,
}

#[derive(Deserialize)]
pub struct Schema {
    groups: Vec<Group>,
    #[serde(default)]
    pub quality: Map<String, Value>,
}

fn walk(groups: &[Group], out: &mut Vec<Param>) {
    for g in groups {
        out.extend(g.params.iter().cloned());
        walk(&g.groups, out);
    }
}

impl Schema {
    pub fn load(root: &Path) -> Result<Schema, String> {
        let text = std::fs::read_to_string(root.join("params.json")).map_err(|e| format!("params.json: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("params.json: {e}"))
    }

    pub fn params(&self) -> Vec<Param> {
        let mut out = vec![];
        walk(&self.groups, &mut out);
        out
    }
}

// The compile-time values a quality preset (params.json's "quality") sets,
// or none.
pub fn preset(s: &Schema, level: Option<&str>) -> Result<Map<String, Value>, String> {
    match level {
        None => Ok(Map::new()),
        Some(level) => match s.quality.get(level) {
            Some(Value::Object(m)) => Ok(m.clone()),
            _ => Err(format!("no quality preset {level} in params.json")),
        },
    }
}

// The drive record's fields (fut/rig.fut's drive), in Futhark's order, and
// which are booleans.
pub const DRIVE: &[&str] = &[
    "arena_x", "arena_y", "arena_z", "ax", "ay", "az", "beat", "cam_x", "cam_y", "cam_z", "climb", "flash", "foe0", "foe1", "foe2", "foe3", "foe4", "foe5", "foe6", "free", "fx0", "fx1", "fx2", "fx3", "fx4", "fx5", "fx6", "fy0", "fy1", "fy2", "fy3", "fy4", "fy5", "fy6", "fz0", "fz1", "fz2", "fz3", "fz4", "fz5", "fz6", "groove", "heading", "kill0", "kill1", "kill2", "kill3", "kill4", "kill5", "kill6", "picture", "shield",
    "shield_hit", "shoulder", "spell", "staff", "sx", "sy", "sz", "thumb", "turn", "x", "y", "z",
];
const DRIVE_BOOL: &[&str] = &["free"];

// The compile-time values in force: the defaults, overlaid with the drawer's
// saved changes (config.local.json, unless `local` is false), then with
// `overrides` (e.g. a quality preset).
pub fn compile_values(root: &Path, s: &Schema, local: bool, overrides: &Map<String, Value>) -> Map<String, Value> {
    let mut saved: Map<String, Value> = Map::new();
    if local {
        if let Ok(text) = std::fs::read_to_string(root.join("config.local.json")) {
            if let Ok(Value::Object(m)) = serde_json::from_str(&text) {
                saved = m;
            }
        }
    }
    saved.extend(overrides.clone());
    let mut out = Map::new();
    for p in s.params().into_iter().filter(|p| p.compile) {
        let v = saved.get(&p.id).cloned().unwrap_or(p.default.clone());
        out.insert(p.id, v);
    }
    out
}

// A flags option's constant: "layers" + "Figure" -> layers_figure.
// (each run of other characters becomes one '_')
fn flag_name(p: &Param, option: &str) -> String {
    let mut s = String::new();
    for c in option.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            s.push(c);
        } else if !s.ends_with('_') || s.is_empty() {
            s.push('_');
        }
    }
    format!("{}_{}", p.id, s)
}

fn rust_type(p: &Param) -> &'static str {
    match p.kind.as_str() {
        "color" => "Vec3",
        "checkbox" => "bool",
        "slider" => "f32",
        _ => "i32",
    }
}

// A float as JavaScript prints it, with ".0" on whole numbers.
fn float(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e21 { format!("{x:.1}") } else { format!("{x}") }
}

fn rust_lit(p: &Param, v: &Value) -> String {
    match p.kind.as_str() {
        "color" => {
            let c: Vec<String> = v.as_array().unwrap().iter().map(|x| float(x.as_f64().unwrap())).collect();
            format!("Vec3::new({})", c.join(", "))
        }
        "checkbox" => v.as_bool().unwrap_or(false).to_string(),
        "slider" => float(v.as_f64().unwrap()),
        _ => (v.as_f64().unwrap().round() as i64).to_string(),
    }
}

// The uniform block's members, in order: (name, type, where from).
pub struct Field {
    pub name: String,
    pub ty: &'static str,
    pub from: Value,
}

// The arguments in the block after the uniforms and the drive record.
const ARGS: &[(&str, &str)] =
    &[("vw", "i32"), ("h", "i32"), ("tm", "f32"), ("yaw", "f32"), ("pitch", "f32"), ("dst", "f32"), ("w", "i32"), ("n_spells", "i32"), ("m_t0", "f32"), ("m_rate", "f32"), ("m_n", "i32"), ("m_chunk", "i32"), ("m_song", "i32"), ("o_t", "f32"), ("o_kind", "i32")];

fn uniforms(s: &Schema) -> Vec<Param> {
    let mut u: Vec<Param> = s.params().into_iter().filter(|p| !p.compile).collect();
    u.sort_by(|a, b| a.id.cmp(&b.id));
    u
}

pub fn block(s: &Schema) -> Vec<Field> {
    let mut out = vec![];
    for p in uniforms(s) {
        if p.kind == "color" {
            for k in 0..3 {
                out.push(Field { name: format!("p_{}_{k}", p.id), ty: "f32", from: serde_json::json!({ "kind": "param", "id": p.id, "index": k }) });
            }
        } else {
            out.push(Field { name: format!("p_{}", p.id), ty: if p.kind == "slider" { "f32" } else { "i32" }, from: serde_json::json!({ "kind": "param", "id": p.id }) });
        }
    }
    for id in DRIVE {
        out.push(Field { name: format!("d_{id}"), ty: if DRIVE_BOOL.contains(id) { "i32" } else { "f32" }, from: serde_json::json!({ "kind": "drive", "id": id }) });
    }
    for (id, ty) in ARGS {
        out.push(Field { name: format!("a_{id}"), ty, from: serde_json::json!({ "kind": "arg", "id": id }) });
    }
    out
}

// The shader crate's generated module, as Rust source.
pub fn generated_rs(s: &Schema, values: &Map<String, Value>) -> String {
    let all = s.params();
    let us = uniforms(s);
    let mut o = String::from("use crate::prelude::*;\n");
    for p in all.iter().filter(|p| p.kind == "flags") {
        let opts = p.options.as_deref().unwrap_or(&[]);
        o += &format!("\n// {}\n", p.label);
        for (i, opt) in opts.iter().enumerate() {
            o += &format!("pub const {}: i32 = {};\n", flag_name(p, opt), 1 << i);
        }
        o += &format!("pub const {}_all: i32 = {};\n", p.id, (1 << opts.len()) - 1);
    }
    o += "\n// Compile-time parameters (with config.local.json's values)\n";
    for p in all.iter().filter(|p| p.compile) {
        o += &format!("pub const {}: {} = {}; // {}\n", p.id, rust_type(p), rust_lit(p, &values[&p.id]), p.label);
    }
    o += "\n// The uniforms (fut/params.fut's params record)\n#[derive(Clone, Copy)]\npub struct Params {\n";
    for p in &us {
        o += &format!("    pub {}: {},\n", p.id, rust_type(p));
    }
    o += "}\n\n// The flight state (fut/rig.fut's drive record)\n#[derive(Clone, Copy)]\npub struct Drive {\n";
    for id in DRIVE {
        o += &format!("    pub {id}: {},\n", if DRIVE_BOOL.contains(id) { "bool" } else { "f32" });
    }
    o += "}\n\n// The uniform block, as the client fills it (shader.json's fields)\n#[repr(C)]\npub struct Uniforms {\n";
    for f in block(s) {
        o += &format!("    pub {}: {},\n", f.name, f.ty);
    }
    o += "}\n\npub fn unpack(u: &Uniforms) -> (Params, Drive) {\n    (\n        Params {\n";
    for p in &us {
        let v = match p.kind.as_str() {
            "color" => format!("Vec3::new(u.p_{0}_0, u.p_{0}_1, u.p_{0}_2)", p.id),
            "checkbox" => format!("u.p_{} != 0", p.id),
            _ => format!("u.p_{}", p.id),
        };
        o += &format!("            {}: {v},\n", p.id);
    }
    o += "        },\n        Drive {\n";
    for id in DRIVE {
        o += &format!("            {id}: {},\n", if DRIVE_BOOL.contains(id) { format!("u.d_{id} != 0") } else { format!("u.d_{id}") });
    }
    o += "        },\n    )\n}\n";
    o
}
