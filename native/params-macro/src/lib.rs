// params!(): the shader crate's generated module (see wizard-params), from
// params.json as the crate compiles, with the compile-time values in force:
// config.local.json's (unless WIZARD_DEFAULTS is set and not empty), under
// the quality preset WIZARD_PRESET names, if any. shader-builder sets
// those for each build it makes.
//
// rustc does not know what a macro reads, so the expansion says it: it
// includes the JSON files it read (include_bytes!) and asks for the
// environment variables (option_env!), so a change to either rebuilds the
// crate.

use proc_macro::TokenStream;
use std::path::PathBuf;

fn error(msg: &str) -> TokenStream {
    format!("compile_error!({msg:?});").parse().unwrap()
}

#[proc_macro]
pub fn params(_: TokenStream) -> TokenStream {
    // (the crate expanding it is native/shader; the repository is two up)
    let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") else { return error("params!(): no CARGO_MANIFEST_DIR") };
    let Ok(root) = PathBuf::from(manifest).join("../..").canonicalize() else { return error("params!(): no repository root") };
    let preset = std::env::var("WIZARD_PRESET").ok().filter(|v| !v.is_empty());
    let local = std::env::var("WIZARD_DEFAULTS").map_or(true, |v| v.is_empty());
    let schema = match wizard_params::Schema::load(&root) {
        Ok(s) => s,
        Err(e) => return error(&e),
    };
    let overrides = match wizard_params::preset(&schema, preset.as_deref()) {
        Ok(m) => m,
        Err(e) => return error(&e),
    };
    let values = wizard_params::compile_values(&root, &schema, local, &overrides);
    let mut code = wizard_params::generated_rs(&schema, &values);
    let mut read = vec![root.join("params.json")];
    if local {
        // (made if missing, empty, so that there is a file to watch: the
        // drawer's first saved change then rebuilds too)
        let config = root.join("config.local.json");
        if !config.exists() {
            let _ = std::fs::write(&config, "{}\n");
        }
        read.push(config);
    }
    code += "\n// (what this was made from, for rustc to watch)\n";
    for f in read {
        code += &format!("const _: &[u8] = include_bytes!({:?});\n", f.display().to_string());
    }
    code += "const _: Option<&str> = option_env!(\"WIZARD_PRESET\");\nconst _: Option<&str> = option_env!(\"WIZARD_DEFAULTS\");\n";
    match code.parse() {
        Ok(t) => t,
        Err(e) => error(&format!("params!(): {e}")),
    }
}
