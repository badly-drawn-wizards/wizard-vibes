{
  description = "Futhark WebGPU ray-marching wizard";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # WebGPU backend is only on this branch (diku-dk/futhark PR #2140).
    futhark.url = "github:diku-dk/futhark/webgpu";
    # rust-gpu's pinned nightly, for the native client's shader port
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, futhark, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
      # naga's CLI (wgpu's shader translator), for the WebGL fallback: it
      # turns the render kernel's WGSL into GLSL ES 3.00. Not in nixpkgs.
      naga-cli = pkgs: pkgs.rustPlatform.buildRustPackage rec {
        pname = "naga-cli";
        version = "30.0.1";
        src = pkgs.fetchCrate { inherit pname version; hash = "sha256-k745Wvnt9aprpbOyYORW1rFUIisTIlEAv5AC3EFFavQ="; };
        cargoHash = "sha256-MwqQnYtfztY/3GFvGRmuRCOrmY7es8l331RoEKFI1A8=";
        doCheck = false;
      };
    in {
      packages = forAll (system: pkgs: { naga-cli = naga-cli pkgs; });
      devShells = forAll (system: pkgs: let
        # what the native client (native/) loads at run time: Vulkan and the
        # window system's libraries
        nativeLibs = with pkgs; [ vulkan-loader wayland libxkbcommon libGL libx11 libxcursor libxi libxrandr ];
      in {
        # The native client: wgpu on Vulkan, in Rust (see native/README.md).
        native = pkgs.mkShell {
          packages = [ pkgs.cargo pkgs.rustc pkgs.pkg-config pkgs.nodejs_24 ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath nativeLibs;
        };
        # rust-gpu: the nightly its codegen backend is built against (see
        # rustc_codegen_spirv's build.rs), for native/shader-builder.
        gpu = let
          rust = (rust-overlay.lib.mkRustBin { } pkgs).nightly."2026-08-15".default.override {
            extensions = [ "rust-src" "rustc-dev" "llvm-tools" ];
          };
        in pkgs.mkShell {
          packages = [ rust pkgs.pkg-config pkgs.cmake pkgs.python3 pkgs.nodejs_24 ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (nativeLibs ++ [ rust ]);
        };
        default = pkgs.mkShell {
          packages = [
            futhark.packages.${system}.futhark
            pkgs.emscripten
            pkgs.nodejs_24
            pkgs.ffmpeg # dev/gif.ts
            (naga-cli pkgs) # dev/webgl.ts
          ];
          shellHook = ''
            # The emscripten cache in the nix store is read-only; ports (emdawnwebgpu) need a writable one.
            export EM_CACHE="$PWD/.emcache"
            if [ ! -d "$EM_CACHE" ]; then
              cp -r ${pkgs.emscripten}/share/emscripten/cache "$EM_CACHE" 2>/dev/null || mkdir -p "$EM_CACHE"
              chmod -R u+w "$EM_CACHE"
            fi
          '';
        };
      });
    };
}
