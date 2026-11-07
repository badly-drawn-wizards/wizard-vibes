{
  description = "The ray-marched wizard: a rust-gpu shader with an egui app, native and in the browser";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # rust-gpu's pinned nightly
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    # The Futhark the shader was ported from (fut/, kept as a reference); its
    # WebGPU backend is only on this branch (diku-dk/futhark PR #2140).
    futhark.url = "github:diku-dk/futhark/webgpu";
  };

  outputs = { self, nixpkgs, rust-overlay, futhark }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in {
      devShells = forAll (system: pkgs: let
        # rust-gpu's nightly (the one its codegen backend is built against: see
        # rustc_codegen_spirv's build.rs), with the wasm target. Everything
        # builds with it: the app, the dev server and the shader.
        rust = (rust-overlay.lib.mkRustBin { } pkgs).nightly."2026-08-15".default.override {
          extensions = [ "rust-src" "rustc-dev" "llvm-tools" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
        # what the app loads at run time: Vulkan, the window system, sound
        # (ALSA) and game controllers (udev)
        runtime = with pkgs; [ vulkan-loader wayland libxkbcommon libGL libx11 libxcursor libxi libxrandr alsa-lib udev ];
      in {
        default = pkgs.mkShell {
          # dx: Dioxus's CLI, for Subsecond's hot-patching; wasm-bindgen's CLI
          # (the client's wasm-bindgen crate is pinned to its version)
          packages = [ rust pkgs.dioxus-cli pkgs.wasm-bindgen-cli pkgs.pkg-config pkgs.alsa-lib pkgs.udev pkgs.cmake pkgs.python3 pkgs.clang pkgs.mold ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtime ++ [ rust ]);
        };
        # For the Futhark reference (fut/): e.g. to compare the port with it.
        futhark = pkgs.mkShell {
          packages = [ futhark.packages.${system}.futhark ];
        };
      });
    };
}
