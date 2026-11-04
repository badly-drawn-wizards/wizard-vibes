// What the shader needs from the build and the parameters: the uniform
// block's layout (from <build>/wizard.native.json, written by dev/native.ts)
// and its values, the parameters' defaults from params.json, the camera's
// and the frame's arguments, and on the flight path (the drive record's
// `free` unset, as in the browser's fixed mode).

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Deserialize)]
struct Field {
    #[serde(rename = "type")]
    ty: String,
    from: From,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum From {
    Param { id: String, index: Option<usize> },
    Drive {},
    Arg { id: String },
}

#[derive(Deserialize)]
struct Layout {
    fields: Vec<Field>,
    workgroup: u32,
}

// The camera, as the page starts (web/camera.ts).
#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

pub const PAGE_CAMERA: Camera = Camera { yaw: 0.5, pitch: 0.12, dist: 4.4 };
// (as the benchmark daemon sets it: dev/bench-daemon.ts)
pub const BENCH_CAMERA: Camera = Camera { yaw: 0.5, pitch: 0.2, dist: 4.2 };

pub struct Scene {
    pub wgsl: String,
    pub workgroup: u32,
    pub build: PathBuf,
    fields: Vec<Field>,
    params: HashMap<String, Value>,
}

// Values the page gives the renderer in place of the drawer's: outside
// fight mode there are no enemies (web/combat.ts).
const OVERRIDES: &[(&str, f64)] = &[("enemies", 0.0)];

impl Scene {
    // The build in `dir`, or the most recently made one under build/fut.
    pub fn load(root: &Path, dir: Option<PathBuf>) -> Result<Scene, String> {
        let build = match dir {
            Some(d) => d,
            None => newest_build(&root.join("build/fut"))?,
        };
        let json = build.join("wizard.native.json");
        if !json.exists() {
            // (made on demand: the dev server's builds do not include it)
            let ok = Command::new("node")
                .arg("dev/native.ts")
                .arg(&build)
                .current_dir(root)
                .status()
                .map_err(|e| format!("running node dev/native.ts: {e}"))?
                .success();
            if !ok {
                return Err(format!("dev/native.ts failed for {}", build.display()));
            }
        }
        let layout: Layout = serde_json::from_str(&read(&json)?).map_err(|e| format!("{}: {e}", json.display()))?;
        let wgsl = read(&build.join("wizard.native.wgsl"))?;
        let schema: Value = serde_json::from_str(&read(&root.join("params.json"))?).map_err(|e| format!("params.json: {e}"))?;
        let mut params = HashMap::new();
        collect_defaults(&schema["groups"], &mut params);
        for (id, v) in OVERRIDES {
            params.insert(id.to_string(), Value::from(*v));
        }
        Ok(Scene { wgsl, workgroup: layout.workgroup, build, fields: layout.fields, params })
    }

    // Bytes in the uniform block (a multiple of 16).
    pub fn uniform_size(&self) -> u64 {
        ((self.fields.len() * 4 + 15) / 16 * 16) as u64
    }

    // The uniform block for a frame: image `vw` x `h` (rows `w` long), at
    // time `tm` seconds, seen from `cam`.
    pub fn uniforms(&self, w: u32, vw: u32, h: u32, tm: f32, cam: Camera) -> Vec<u8> {
        let mut out = vec![0u8; self.uniform_size() as usize];
        for (i, f) in self.fields.iter().enumerate() {
            let v: f64 = match &f.from {
                From::Param { id, index } => {
                    let v = self.params.get(id).unwrap_or_else(|| panic!("no default for parameter {id}"));
                    match (index, v) {
                        (Some(k), Value::Array(a)) => a[*k].as_f64().unwrap(),
                        (_, Value::Bool(b)) => *b as i32 as f64,
                        (_, v) => v.as_f64().unwrap(),
                    }
                }
                // on the flight path: everything zero, `free` unset
                From::Drive { .. } => 0.0,
                From::Arg { id } => match id.as_str() {
                    "w" => w as f64,
                    "vw" => vw as f64,
                    "h" => h as f64,
                    "tm" => tm as f64,
                    "yaw" => cam.yaw as f64,
                    "pitch" => cam.pitch as f64,
                    "dst" => cam.dist as f64,
                    other => panic!("unknown argument {other}"),
                },
            };
            let bytes = match f.ty.as_str() {
                "i32" => (v.round() as i32).to_le_bytes(),
                _ => (v as f32).to_le_bytes(),
            };
            out[4 * i..4 * i + 4].copy_from_slice(&bytes);
        }
        out
    }
}

fn read(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn collect_defaults(groups: &Value, out: &mut HashMap<String, Value>) {
    for g in groups.as_array().into_iter().flatten() {
        for p in g["params"].as_array().into_iter().flatten() {
            out.insert(p["id"].as_str().unwrap().to_string(), p["default"].clone());
        }
        collect_defaults(&g["groups"], out);
    }
}

fn newest_build(dir: &Path) -> Result<PathBuf, String> {
    let mut builds: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e} (run the dev server or dev/build-fut.ts first)", dir.display()))?
        .flatten()
        // (finished builds: the dev server writes them to a .tmp directory first)
        .filter(|e| !e.file_name().to_string_lossy().contains(".tmp") && e.path().join("wizard.c").exists())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    builds.sort();
    builds.pop().map(|(_, p)| p).ok_or_else(|| format!("no builds in {}", dir.display()))
}
