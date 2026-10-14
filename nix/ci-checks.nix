# CI-only derivations, outside `packages`, that CI jobs build by name:
#
#   - nextest, wasm-nextest-archive, wasm-nextest-part-<n>: the runs need
#     Docker services that aren't available in sandboxed builds.
#     test-workspace.yml starts Docker first, then builds them directly via
#     `nix build .#nextest.<system>`.
#   - cargo-clippy: per-crate clippy gates (currently just `xdbg`) that the
#     existing workspace-wide lint job skips because their crates aren't in
#     `default-members`. They build fine in the standard sandbox; they live
#     here so the per-PR workflows (e.g. test-xdbg.yml) can target them
#     explicitly.
#   - backend-tests: the backend test suite with disposable PostgreSQL and
#     VersityGW services. It runs in the standard sandbox; test-backend.yml
#     builds it.
{ lib, withSystem, ... }:
let
  systems = [
    "aarch64-darwin"
    "x86_64-linux"
    "aarch64-linux"
  ];
  # The WASM tests compile once into an archive and run in this many slices.
  # .github/workflows/test-wasm.yml and dev/wasm.just list the same slices.
  wasmPartitions = 4;
  wasmNextest =
    system:
    withSystem system (
      { pkgs, self', ... }:
      pkgs.callPackage ./package/wasm-nextest.nix {
        xmtp-backend = self'.packages.xmtp-backend;
        partitions = wasmPartitions;
      }
    );
in
{
  flake.nextest = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, ... }:
      pkgs.callPackage ./package/nextest.nix { }
    )
  );

  flake.wasm-nextest-archive = lib.genAttrs systems (system: (wasmNextest system).archive);

  # `wasm-nextest-part-<n>.<system>`, one output per slice.
  imports = [
    {
      flake = lib.listToAttrs (
        lib.genList (i: {
          name = "wasm-nextest-part-${toString (i + 1)}";
          value = lib.genAttrs systems (system: builtins.elemAt (wasmNextest system).partitions i);
        }) wasmPartitions
      );
    }
  ];

  flake.cargo-clippy = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, ... }:
      {
        xdbg = pkgs.callPackage ./package/xdbg-check.nix { };
        keepalive-probe = pkgs.callPackage ./package/keepalive-probe-check.nix { };
      }
    )
  );

  flake.validation = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, ... }:
      pkgs.callPackage ./package/validation-check.nix { }
    )
  );

  flake.backend-tests = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, ... }:
      pkgs.callPackage ./package/backend-tests.nix { }
    )
  );
}
