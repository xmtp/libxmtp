# Derivation that runs cargo nextest with llvm-cov on the workspace
{
  xmtp,
  lib,
  cargo-llvm-cov,
  d14n ? false,
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
      (commonCargoSources (root + /apps/mls_validation_service))
      # db snapshots
      (fileFilter (file: file.hasExt "xmtp") (root + /crates/xmtp_mls/tests/assets))
      (fileFilter (file: file.hasExt "json") (root + /crates))
    ];
  };

  commonArgs = xmtp.base.commonArgs // {
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
    pnameSuffix = if d14n then "nextest-d14n" else "nextest-v3";
    partitions = 1;
    partitionType = "count";
    cargoNextestPartitionsExtraArgs = "--no-tests=pass";
    cargoExtraArgs = if d14n then "--features d14n" else "";
    cargoNextestExtraArgs = if d14n then "--profile ci-d14n" else "--profile ci";
    withLlvmCov = true;
    doInstallCargoArtifacts = false;
    # most tests query docker
    __noChroot = true;
  }
)
