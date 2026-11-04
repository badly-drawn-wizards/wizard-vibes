// Builds the shader crate (native/shader) for the app:
//
// 1. with the compile-time values in force (config.local.json's, or a
//    quality preset's: WIZARD_DEFAULTS and WIZARD_PRESET, for the crate's
//    params!(), which reads params.json; see wizard-shader-macros);
// 2. compiles it to SPIR-V with rust-gpu (all its entry points in one
//    module), which the native app runs as it is;
// 3. fixes that up for naga (see fixup.rs) and has naga turn it into WGSL,
//    which the browser runs (and which checks it).
//
// Writes into OUT (default build/shader): shader.spv, shader.wgsl (and
// entry-NAME.wgsl, each entry point's alone, which the browser uses) and
// shader.json (the uniform block's fields, for the app to fill; the
// workgroup size), each written to a temporary name and renamed into place,
// shader.json last (so whoever watches it sees a whole build). Also copies
// the SPIR-V to native/shader.spv for the old client's --rust-gpu.
// Needs rust-gpu's nightly: run in `nix develop .#app` (or .#gpu).
//
// usage: cargo run --release -- [--out DIR] [--preset low|med|high] [--defaults]

mod fixup;

use wizard_params as params;

use serde_json::{Map, Value, json};
use spirv_builder::{SpirvBuilder, SpirvMetadata};
use std::path::{Path, PathBuf};

const WORKGROUP: u32 = 256;

fn die(msg: &str) -> ! {
    eprintln!("shader-builder: {msg}");
    std::process::exit(1)
}

// Writes via a temporary file, so a reader never sees half of it.
fn write_atomic(path: &Path, data: &[u8]) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, path)).unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
}

fn main() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = here.join("../..").canonicalize().unwrap();
    let mut out = root.join("build/shader");
    let mut preset: Option<String> = None;
    let mut local = true;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = PathBuf::from(args.next().unwrap_or_else(|| die("--out DIR"))),
            "--preset" => preset = Some(args.next().unwrap_or_else(|| die("--preset LEVEL"))),
            "--defaults" => local = false,
            _ => die(&format!("unknown argument {a}")),
        }
    }

    let schema = params::Schema::load(&root).unwrap_or_else(|e| die(&e));
    let overrides: Map<String, Value> = params::preset(&schema, preset.as_deref()).unwrap_or_else(|e| die(&e));
    let values = params::compile_values(&root, &schema, local, &overrides);
    // (for params!(), in the rust-gpu build this starts; safe: no threads yet)
    unsafe {
        std::env::set_var("WIZARD_PRESET", preset.as_deref().unwrap_or(""));
        std::env::set_var("WIZARD_DEFAULTS", if local { "" } else { "1" });
    }

    let t = std::time::Instant::now();
    let result = SpirvBuilder::new(root.join("native/shader"), "spirv-unknown-vulkan1.2")
        .release(true)
        .spirv_metadata(SpirvMetadata::None)
        .build()
        .unwrap_or_else(|e| die(&format!("shader build failed: {e}")));
    let spv = std::fs::read(result.module.unwrap_single()).unwrap();
    eprintln!("rust-gpu: {} bytes in {:.1} s", spv.len(), t.elapsed().as_secs_f32());

    let t = std::time::Instant::now();
    let words: Vec<u32> = bytemuck::pod_collect_to_vec(&spv);
    let fixed = fixup::fixup(&words).unwrap_or_else(|e| die(&e));
    let module = naga::front::spv::parse_u8_slice(bytemuck::cast_slice(&fixed), &naga::front::spv::Options::default())
        .unwrap_or_else(|e| die(&format!("naga could not read the SPIR-V: {e}")));
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
        .validate(&module)
        .unwrap_or_else(|e| die(&format!("naga rejects the SPIR-V: {}", e.emit_to_string(""))));
    let wgsl = naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
        .unwrap_or_else(|e| die(&format!("naga could not write WGSL: {e}")));
    eprintln!("naga: {} bytes of WGSL in {:.1} s", wgsl.len(), t.elapsed().as_secs_f32());

    std::fs::create_dir_all(&out).unwrap_or_else(|e| die(&format!("{}: {e}", out.display())));
    // each entry point's own WGSL: the module cut down to what it reaches (a
    // pipeline made from the whole module hands the driver all of it, and
    // some drivers (Adreno's, under Firefox) fall over on that)
    let t = std::time::Instant::now();
    let mut entry_files = std::collections::HashMap::new();
    for e in &module.entry_points {
        let mut m = module.clone();
        m.entry_points.retain(|x| x.name == e.name);
        naga::compact::compact(&mut m, naga::compact::KeepUnused::No);
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
            .validate(&m)
            .unwrap_or_else(|err| die(&format!("naga rejects {} alone: {}", e.name, err.emit_to_string(""))));
        let text = naga::back::wgsl::write_string(&m, &info, naga::back::wgsl::WriterFlags::empty())
            .unwrap_or_else(|err| die(&format!("naga could not write {} as WGSL: {err}", e.name)));
        let file = format!("entry-{}.wgsl", e.name);
        write_atomic(&out.join(&file), text.as_bytes());
        entry_files.insert(e.name.clone(), (file, text.len()));
    }
    let sizes: Vec<String> = module.entry_points.iter().map(|e| format!("{} {} kB", e.name, entry_files[&e.name].1 / 1000)).collect();
    eprintln!("naga: each entry point's WGSL in {:.1} s ({})", t.elapsed().as_secs_f32(), sizes.join(", "));
    let fields: Vec<Value> = params::block(&schema).into_iter().map(|f| json!({ "name": f.name, "type": f.ty, "from": f.from })).collect();
    // each entry point's workgroup size and the bindings it uses: (group,
    // binding, "uniform" | "storage" | "storage-read"), for the app's
    // bind group layouts (explicit: SPIR-V run as it is is not reflected)
    let entries: Vec<Value> = module
        .entry_points
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let ep = info.get_entry_point(i);
            let mut bindings = vec![];
            for (h, var) in module.global_variables.iter() {
                if ep[h].is_empty() {
                    continue;
                }
                let Some(rb) = &var.binding else { continue };
                let kind = match var.space {
                    naga::AddressSpace::Uniform => "uniform",
                    naga::AddressSpace::Storage { access } if !access.contains(naga::StorageAccess::STORE) => "storage-read",
                    naga::AddressSpace::Storage { .. } => "storage",
                    _ => continue,
                };
                bindings.push(json!({ "group": rb.group, "binding": rb.binding, "kind": kind }));
            }
            json!({ "name": e.name, "workgroup": e.workgroup_size, "bindings": bindings, "wgsl": entry_files[&e.name].0 })
        })
        .collect();
    // (the SPIR-V's hash: what tells builds apart, as their other metadata
    // is mostly the same)
    let hash = spv.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let meta = json!({ "hash": format!("{hash:016x}"), "fields": fields, "workgroup": WORKGROUP, "entries": entries, "preset": preset, "compile": values });
    write_atomic(&out.join("shader.spv"), &spv);
    write_atomic(&out.join("shader.wgsl"), wgsl.as_bytes());
    write_atomic(&out.join("shader.json"), serde_json::to_string_pretty(&meta).unwrap().as_bytes());
    std::fs::write(root.join("native/shader.spv"), &spv).unwrap();
    eprintln!("wrote {}", out.display());
}
