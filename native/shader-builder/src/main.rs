// Builds the shader crate (native/shader) to SPIR-V with rust-gpu, and
// copies the module to native/shader.spv for the client (--rust-gpu).
// First regenerates its parameters (generated.rs) from params.json.
// Needs rust-gpu's nightly: run in `nix develop .#gpu`.
//
// usage: cargo run --release [-- OUT.spv]

use spirv_builder::{SpirvBuilder, SpirvMetadata};
use std::path::Path;
use std::process::Command;

fn main() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ok = Command::new("node").args(["dev/native.ts", "--rust"]).current_dir(here.join("../..")).status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        eprintln!("node dev/native.ts --rust failed");
        std::process::exit(1);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.first().map(Into::into).unwrap_or_else(|| here.join("../shader.spv"));
    let result = SpirvBuilder::new(here.join("../shader"), "spirv-unknown-vulkan1.2")
        .release(true)
        .spirv_metadata(SpirvMetadata::None)
        .build()
        .unwrap_or_else(|e| {
            eprintln!("shader build failed: {e}");
            std::process::exit(1)
        });
    let spv = result.module.unwrap_single();
    std::fs::copy(spv, &out).unwrap();
    eprintln!("wrote {} ({} bytes)", out.display(), std::fs::metadata(&out).unwrap().len());
}
