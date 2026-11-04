// The shader crate's macros: params!() and melody!().
//
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

// melody!(NAME, STEPS, "BARS"): a tune, written out, as a function
// `fn NAME(i: i32) -> i32` giving the code of its step i (wrapping round
// the tune): 0 a rest, 15 hold the note before, else a degree (1..14, as
// the song reads them). The tune is bars of STEPS steps (at most 8), split
// by `|`; in each, a step is a number (a degree), `-` (hold) or `.` (a
// rest). Each bar is packed into a u32, a step to four bits (the first in
// the lowest), and the function picks its bar's word, so the tune costs a
// GPU a few compares and a shift, and needs no array in memory.
#[proc_macro]
pub fn melody(input: TokenStream) -> TokenStream {
    use proc_macro::TokenTree;
    let toks: Vec<TokenTree> = input.into_iter().filter(|t| !matches!(t, TokenTree::Punct(p) if p.as_char() == ',')).collect();
    let (name, steps, text) = match toks.as_slice() {
        [TokenTree::Ident(n), TokenTree::Literal(s), TokenTree::Literal(t)] => (n.to_string(), s.to_string(), t.to_string()),
        _ => return error("melody!(NAME, STEPS, \"BARS\")"),
    };
    let Ok(steps) = steps.parse::<usize>() else { return error("melody!: STEPS, a number of steps a bar") };
    if !(1..=8).contains(&steps) {
        return error("melody!: at most 8 steps a bar (4 bits each, in a u32)");
    }
    let Some(text) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else { return error("melody!: the bars, as a string") };
    let mut words = vec![];
    for (b, bar) in text.split('|').enumerate() {
        let mut w = 0u32;
        let codes: Vec<&str> = bar.split_whitespace().collect();
        if codes.len() != steps {
            return error(&format!("melody!: bar {} has {} steps, not {steps}", b + 1, codes.len()));
        }
        for (i, c) in codes.iter().enumerate() {
            let v = match *c {
                "-" => 15,
                "." => 0,
                n => match n.parse::<u32>() {
                    Ok(v) if (1..=14).contains(&v) => v,
                    _ => return error(&format!("melody!: bar {}: {n:?} is not a degree (1-14), - or .", b + 1)),
                },
            };
            w |= v << (4 * i);
        }
        words.push(w);
    }
    let mut pick = String::new();
    for (b, w) in words.iter().enumerate() {
        if b + 1 == words.len() {
            pick += &format!("{{ {w:#010x} }}");
        } else {
            pick += &format!("if b == {b} {{ {w:#010x} }} else ");
        }
    }
    let code = format!(
        "#[inline]\nfn {name}(i: i32) -> i32 {{\n    let b = crate::vec::imod(crate::vec::idiv(i, {steps}), {bars});\n    let w: u32 = {pick};\n    ((w >> ((4 * crate::vec::imod(i, {steps})) as u32)) & 15) as i32\n}}\n",
        bars = words.len()
    );
    match code.parse() {
        Ok(t) => t,
        Err(e) => error(&format!("melody!: {e}")),
    }
}
