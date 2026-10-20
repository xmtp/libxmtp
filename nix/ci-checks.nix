# CI-only derivations, outside `packages`, that CI jobs build by name:
#
#   - nextest: the workspace tests with the test services (nix/services)
#     inside the build. test-workspace.yml builds `.#nextest.<system>`.
#   - junit-refs: proves that the JUnit step of the test derivations
#     (nix/lib/junit.nix) leaves no store path. test-workspace.yml builds it.
#   - wasm-nextest-archive, wasm-nextest-part-<n>: the WASM test archive and
#     its slices. Each slice starts the test services (nix/services) inside
#     the build on the fixed port block 20000-20099 and holds a machine-wide
#     lock; test-wasm.yml builds them with no Docker stack.
#   - cargo-clippy: per-crate clippy gates (xdbg and keepalive-probe) that
#     the workspace-wide lint job skips because their crates aren't in
#     `default-members`. They build fine in the standard sandbox;
#     test-nix-checks.yml builds them.
#   - backend-tests: the backend test suite with the storage services
#     (nix/services). It runs in the standard sandbox; test-nix-checks.yml
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
        services = self'.legacyPackages.clientServices;
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
        services = self'.legacyPackages.clientServices;
      }
    )
  );

  # The JUnit step of the test derivations on a report that names a real
  # input of this derivation (a planted store path with its context kept). A
  # missing `nuke-refs` leaves the path in the output, and
  # `allowedReferences = [ ]` fails the build with a reference error. A
  # literal `/nix/store/<hash>-x` string would not prove this: Nix registers
  # only paths of the input closure as references.
  flake.junit-refs = lib.genAttrs systems (
    system:
    withSystem system (
      { pkgs, ... }:
      pkgs.runCommand "junit-refs"
        {
          nativeBuildInputs = [ pkgs.nukeReferences ];
          allowedReferences = [ ];
        }
        ''
          mkdir -p "$out" target/nextest/ci
          printf '<testsuites><testsuite><testcase name="t"><flakyFailure message="panicked at %s/src/lib.rs:1:1"/></testcase></testsuite></testsuites>\n' \
            ${builtins.toFile "planted" "x"} >target/nextest/ci/junit.xml
          ${import ./lib/junit.nix "target/nextest/ci/junit.xml"}
          grep -q flakyFailure "$out/junit.xml"
        ''
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
