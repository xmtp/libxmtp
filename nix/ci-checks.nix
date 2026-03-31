# Nextest derivations for CI test runs.
# Exposed under a custom `nextest` top-level output so that `om ci run`
# (via devour-flake) won't try to build them — nextest requires Docker
# services which aren't available in sandboxed builds.
# The test-workspace.yml workflow starts Docker first, then builds these
# directly via `nix build .#nextest.<system>.v3`.
{ lib, withSystem, ... }:
{
  flake.nextest =
    lib.genAttrs
      [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ]
      (
        system:
        withSystem system (
          { pkgs, ... }:
          {
            v3 = pkgs.callPackage ./package/nextest.nix { };
            d14n = pkgs.callPackage ./package/nextest.nix { d14n = true; };
            wasm-v3 = pkgs.callPackage ./package/wasm-nextest.nix { };
            wasm-d14n = pkgs.callPackage ./package/wasm-nextest.nix { d14n = true; };
          }
        )
      );
}
