# Derivation that runs cargo nextest with llvm-cov on the workspace
{
  xmtp,
  lib,
  cargo-llvm-cov,
  ...
}:
let
  inherit (lib.fileset) unions fileFilter;
  inherit (xmtp) craneLib;
  inherit (craneLib.fileset) commonCargoSources;
  root = ./../..;
  rust-toolchain = p: xmtp.mkToolchain p [ ] [ "llvm-tools-preview" ];
  rust = craneLib.overrideToolchain rust-toolchain;

  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.libraries
      # include xmtpv3 tests
      (commonCargoSources (root + /bindings/mobile))
      # db snapshots
      (fileFilter (file: file.hasExt "xmtp") (root + /crates/xmtp_mls/tests/assets))
      (fileFilter (file: file.hasExt "json") (root + /crates))
    ];
  };

  commonArgs = xmtp.base.commonArgs // {
    SQLX_OFFLINE = "true";
    nativeBuildInputs = xmtp.base.commonArgs.nativeBuildInputs ++ [
      cargo-llvm-cov
    ];
  };

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
    (removeAttrs commonArgs [ "src" ])
    // {
      buildPhaseCargoCommand = "cargo llvm-cov --locked --profile $CARGO_PROFILE --no-report";
    }
  );

in
rust.cargoNextest (
  commonArgs
  // {
    inherit src cargoArtifacts;
    doCheck = true;
    DATABASE_URL = "postgres://xmtp:xmtp@localhost:55432/xmtp_backend";
    pnameSuffix = "nextest";
    partitions = 1;
    partitionType = "count";
    cargoNextestPartitionsExtraArgs = "--no-tests=pass";
    # The dedicated backend workflow owns the PostgreSQL service suite.
    cargoNextestExtraArgs = "--profile ci -E 'not package(xmtp_backend)'";
    withLlvmCov = true;
    doInstallCargoArtifacts = false;
    # most tests query docker
    __noChroot = true;
  }
)
