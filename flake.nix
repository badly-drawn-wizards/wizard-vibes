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
      # rust-gpu's nightly (the one its codegen backend is built against: see
      # rustc_codegen_spirv's build.rs), with the wasm target. Everything
      # builds with it: the app, the dev server and the shader.
      rustOf = pkgs: (rust-overlay.lib.mkRustBin { } pkgs).nightly."2026-08-15".default.override {
        extensions = [ "rust-src" "rustc-dev" "llvm-tools" ];
        targets = [ "wasm32-unknown-unknown" ];
      };
      # what the app loads at run time: Vulkan, the window system, sound
      # (ALSA) and game controllers (udev)
      runtimeOf = pkgs: with pkgs; [ vulkan-loader wayland libxkbcommon libGL libx11 libxcursor libxi libxrandr alsa-lib udev ];
    in {
      # The app (`nix run`, the default: `wizard`) and the dev server
      # (`wizard-dev`, run from the repository, in the dev shell). The app
      # comes with its shaders (built here too, with rust-gpu, at the
      # defaults and for each quality level), in share/wizard, where it is
      # told to look (WIZARD_ROOT); params.json is compiled in.
      packages = forAll (system: pkgs: let
        lib = pkgs.lib;
        rust = rustOf pkgs;
        rustPlatform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };
        version = "0.1.0";
        # (the sources: none of the builds, nor the Futhark reference)
        src = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let rel = lib.removePrefix (toString ./. + "/") (toString path);
            in !(lib.hasPrefix "build" rel || lib.hasPrefix "dist" rel || lib.hasPrefix "fut" rel || baseNameOf path == "target");
        };
        # (the linker .cargo/config.toml asks for, on Linux; and the
        # libraries the app links against)
        tools = [ pkgs.pkg-config ] ++ lib.optionals pkgs.stdenv.isLinux [ pkgs.clang pkgs.mold ];
        libs = lib.optionals pkgs.stdenv.isLinux (with pkgs; [ alsa-lib udev ]);
        # The shaders: the builder (native/shader-builder: rust-gpu's codegen
        # backend in it) built, then run, cargo building the shader crate
        # offline from the vendored crates of both.
        shaders = rustPlatform.buildRustPackage {
          pname = "wizard-shaders";
          inherit version src;
          cargoRoot = "native/shader-builder";
          buildAndTestSubdir = "native/shader-builder";
          cargoDeps = pkgs.symlinkJoin {
            name = "cargo-vendor-dir";
            paths = [
              (rustPlatform.importCargoLock { lockFile = ./native/shader-builder/Cargo.lock; })
              (rustPlatform.importCargoLock { lockFile = ./native/shader/Cargo.lock; })
              # (rust-gpu builds the standard library from its sources: its
              # crates too)
              (rustPlatform.importCargoLock { lockFile = "${rust}/lib/rustlib/src/rust/library/Cargo.lock"; })
            ];
          };
          nativeBuildInputs = tools;
          doCheck = false;
          # (cargo-auditable, in front of rustc, gets in the way of rust-gpu's
          # codegen backend)
          auditable = false;
          WIZARD_DEFAULTS = "1";
          postBuild = ''
            builder=$(realpath "$(find . -path '*/release/wizard-shader-builder' -type f | head -1)")
            # (rust-gpu's codegen backend, built with it (beside it, or in its
            # build script's output), found on the library path)
            backend=$(realpath "$(find . -name 'librustc_codegen_spirv*.so' -o -name 'librustc_codegen_spirv*.dylib' | head -1)")
            echo "shader builder: $builder; codegen backend: $backend"
            export LD_LIBRARY_PATH="$(dirname "$builder"):$(dirname "$backend")''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            export DYLD_LIBRARY_PATH="$LD_LIBRARY_PATH"
            for level in low med high; do "$builder" --defaults --out "build/shader-$level" --preset "$level"; done
            "$builder" --defaults --out build/shader
          '';
          installPhase = ''
            mkdir -p $out
            cp -r build $out/build
          '';
        };
        wizard = rustPlatform.buildRustPackage {
          pname = "wizard";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "wizard" "--bin" "wizard" ];
          nativeBuildInputs = tools ++ [ pkgs.makeWrapper ];
          buildInputs = libs;
          doCheck = false;
          WIZARD_DEFAULTS = "1";
          postInstall = ''
            mkdir -p $out/share/wizard
            cp -r ${shaders}/build $out/share/wizard/build
            wrapProgram $out/bin/wizard --set-default WIZARD_ROOT $out/share/wizard \
              ${lib.optionalString pkgs.stdenv.isLinux "--prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath (runtimeOf pkgs)}"}
          '';
          meta = { description = "The ray-marched wizard, in a window"; mainProgram = "wizard"; };
        };
        wizard-dev = rustPlatform.buildRustPackage {
          pname = "wizard-dev";
          inherit version src;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "wizard-dev" ];
          nativeBuildInputs = tools;
          doCheck = false;
          meta = { description = "The wizard's dev server (run from the repository, in the dev shell)"; mainProgram = "wizard-dev"; };
        };
      in {
        inherit wizard wizard-dev;
        wizard-shaders = shaders;
        default = wizard;
      });

      devShells = forAll (system: pkgs: let
        rust = rustOf pkgs;
        runtime = runtimeOf pkgs;
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
