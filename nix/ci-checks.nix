# CI-only derivations for things `om ci run` (via devour-flake) should
# not pick up automatically:
#
#   - nextest: requires Docker services that aren't available in sandboxed builds.
#     test-workspace.yml starts Docker first, then builds them directly via
#     `nix build .#nextest.<system>.v3`.
#   - cargo-clippy: per-crate clippy gates (currently just `xdbg`) that the
#     existing workspace-wide lint job skips because their crates aren't in
#     `default-members`. They build fine in the standard sandbox; they live
#     here so the per-PR workflows (e.g. test-xdbg.yml) can target them
#     explicitly without enrolling them in `om ci run`.
{ lib, withSystem, ... }:
let
  systems = [
    "aarch64-darwin"
    "x86_64-linux"
    "aarch64-linux"
  ];
in
{
  flake.nextest = lib.genAttrs systems (
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

  flake.cargo-clippy = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, ... }:
      {
        xdbg = pkgs.callPackage ./package/xdbg-check.nix { };
      }
    )
  );
}
