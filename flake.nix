{
  description = "Futhark WebGPU ray-marching wizard";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # WebGPU backend is only on this branch (diku-dk/futhark PR #2140).
    futhark.url = "github:diku-dk/futhark/webgpu";
  };

  outputs = { self, nixpkgs, futhark }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in {
      devShells = forAll (system: pkgs: {
        default = pkgs.mkShell {
          packages = [
            futhark.packages.${system}.futhark
            pkgs.emscripten
            pkgs.nodejs_24
            pkgs.ffmpeg # dev/gif.ts
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
