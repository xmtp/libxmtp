# CI-only derivations, outside `packages`, that CI jobs build by name:
#
#   - nextest: the workspace tests with the test services (nix/services)
#     inside the build. test-workspace.yml builds `.#nextest.<system>`.
#   - wasm-nextest-archive, wasm-nextest-part-<n>: the WASM test archive and
#     its slices. Each slice starts the test services (nix/services) inside
#     the build on the fixed port block 20000-20099 and holds a machine-wide
#     lock; test-wasm.yml builds them with no Docker stack.
#   - cargo-clippy: per-crate clippy gates (currently just `xdbg`) that the
#     existing workspace-wide lint job skips because their crates aren't in
#     `default-members`. They build fine in the standard sandbox; they live
#     here so the per-PR workflows (e.g. test-xdbg.yml) can target them
#     explicitly.
#   - backend-tests: the backend test suite with the storage services
#     (nix/services). It runs in the standard sandbox; test-backend.yml
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
        services = self'.legacyPackages.mkServices self'.packages.xmtp-backend;
        partitions = wasmPartitions;
      }
    );
in
{
  flake.nextest = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, self', ... }:
      pkgs.callPackage ./package/nextest.nix {
        services = self'.legacyPackages.mkServices self'.packages.xmtp-backend;
      }
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
      { pkgs, self', ... }:
      pkgs.callPackage ./package/backend-tests.nix {
        services = self'.legacyPackages.storageServices;
      }
    )
  );
}
