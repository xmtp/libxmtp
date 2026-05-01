# CI-only derivations for things `om ci run` (via devour-flake) should
# not pick up automatically:
#
#   - nextest: requires Docker services that aren't available in sandboxed builds.
#     test-workspace.yml starts Docker first, then builds them directly via
#     `nix build .#nextest.<system>.v3`.
#   - xdbg-check: a clippy gate for the apps/xmtp_debug crate. It builds fine
#     in the standard sandbox; it lives here so the per-PR test-xdbg.yml
#     workflow can target it explicitly without enrolling it in `om ci run`.
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

  flake.xdbg-check = lib.genAttrs systems (
    system: withSystem system ({ pkgs, ... }: pkgs.callPackage ./package/xdbg-check.nix { })
  );
}
