# Derivation that runs cargo nextest with llvm-cov on the workspace. The check
# phase runs through `services-test` of `services` (nix/services, with the
# backend binary that the tests talk to), on free loopback ports. Crane's
# cargoNextest sets its own check command, so this is a plain
# mkCargoDerivation with Crane's nextest settings restated.
{
  xmtp,
  lib,
  cargo-llvm-cov,
  cargo-nextest,
  python3,
  cacert,
  nukeReferences,
  writeShellScript,
  services,
  ...
}:
let
  inherit (lib.fileset) fileFilter;
  inherit (xmtp) craneLib;
  root = ./../..;
  rust = craneLib.overrideToolchain xmtp.hostToolchain;

  # The default members and their dev-dependencies. Other apps stay stubs.
  src = xmtp.filesets.mkClosureSource rust {
    roots = xmtp.filesets.defaultMembers;
    dev = true;
    extra = [
      (root + /.config/nextest.toml)
      # db snapshots
      (fileFilter (file: file.hasExt "xmtp") (root + /crates/xmtp_mls/tests/assets))
      (fileFilter (file: file.hasExt "json") (root + /crates))
      # trybuild compares compiler errors with checked-in output.
      (fileFilter (file: file.hasExt "stderr") (root + /crates/xmtp_macro/tests/ui))
    ];
  };

  commonArgs = xmtp.base.commonArgs // {
    SQLX_OFFLINE = "true";
    nativeBuildInputs = xmtp.base.commonArgs.nativeBuildInputs ++ [
      cargo-llvm-cov
    ];
  };

  # The coverage report must name no store path: a reference keeps the result
  # from substituting when that path is missing from the cache. The regex
  # drops the records of store sources, such as Crane's dummy build script,
  # whose mangled function names also hold the store hash. `allowedReferences`
  # fails the build if a reference remains, and `test -s` fails it when the
  # report is missing or empty. The backend's own suite runs in backend-tests.
  run = writeShellScript "nextest-run" ''
    set -euo pipefail
    cargo llvm-cov nextest --lcov --output-path "$out/coverage" \
      --ignore-filename-regex '^/nix/store/' \
      ''${CARGO_PROFILE:+--cargo-profile "$CARGO_PROFILE"} \
      --profile ci -E 'not package(xmtp_backend)' --no-tests=pass
  '';

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust (
    (removeAttrs commonArgs [ "src" ])
    // {
      buildPhaseCargoCommand = "cargo llvm-cov --locked --profile $CARGO_PROFILE --no-report";
    }
  );

in
rust.mkCargoDerivation (
  commonArgs
  // {
    inherit src cargoArtifacts;
    cargoVendorDir = xmtp.base.mkCargoVendorDir rust;
    # Crane would read these from the dummy source during evaluation.
    pname = "cargo-package";
    version = xmtp.mkVersion rust;
    pnameSuffix = "-nextest";
    # The migration lock test uses a separate SQLite process.
    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [
      cargo-nextest
      python3
      nukeReferences
    ];
    # A reqwest client loads the system trust store when it is built (rustls
    # platform verifier); a Linux sandbox has none, so cacert sets
    # SSL_CERT_FILE.
    nativeCheckInputs = [ cacert ];
    # Nextest redraws a progress bar by default, which garbles build logs.
    NEXTEST_SHOW_PROGRESS = "counter";
    buildPhaseCargoCommand = "mkdir -p $out";
    doCheck = true;
    checkPhaseCargoCommand = ''
      ${services.checkCommand run}
      ${import ../lib/junit.nix "target/nextest/ci/junit.xml"}
    '';
    postInstall = "test -s $out/coverage && test -s $out/junit.xml";
    allowedReferences = [ ];
    doInstallCargoArtifacts = false;
  }
  # The build runs in the sandbox, with the services' sandbox settings. On
  # Darwin, the toxiproxy tests also need configd: toxiproxy_rust builds a
  # reqwest 0.11 client, which reads the system proxy settings and panics when
  # the sandbox denies that lookup.
  // services.sandbox
  // {
    sandboxProfile = services.sandbox.sandboxProfile + ''
      (allow mach-lookup (global-name "com.apple.SystemConfiguration.configd"))
    '';
  }
)
