# Focused native, wasm, and dependency-isolation checks for xmtp_mls_validation.
{
  xmtp,
  lib,
  wasm-bindgen-cli,
  nodejs_24,
  python3,
  taplo,
}:
let
  inherit (xmtp) craneLib base;
  root = ./../..;
  rust-toolchain = p: xmtp.mkToolchain p [ "wasm32-unknown-unknown" ] [ ];
  rust = craneLib.overrideToolchain rust-toolchain;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      xmtp.filesets.workspace
      (root + /dev/check-validation)
    ];
  };

  commonArgs = base.commonArgs // {
    inherit (xmtp.shellCommon.wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
  };

  cargoArtifacts = base.mkCargoArtifacts rust false (
    (removeAttrs commonArgs [ "src" ])
    // {
      buildPhaseCargoCommand = ''
        cargo test --locked --no-run -p xmtp_mls_validation --features test-utils
        cargo test --locked --no-run -p xmtp_proto \
          all_payloads_have_stable_canonical_bytes_and_distinct_outer_hashes
        cargo test --locked --no-run -p xmtp_id
        cargo test --locked --no-run --profile wasm-test --target wasm32-unknown-unknown \
          -p xmtp_mls_validation --features test-utils
        cargo test --locked --no-run --profile wasm-test --target wasm32-unknown-unknown \
          -p xmtp_proto all_payloads_have_stable_canonical_bytes_and_distinct_outer_hashes
        cargo test --locked --no-run --profile wasm-test --target wasm32-unknown-unknown \
          -p xmtp_id
      '';
    }
  );
in
rust.buildPackage (
  commonArgs
  // {
    inherit src cargoArtifacts;
    pname = "xmtp-mls-validation-check";
    version = xmtp.mkVersion rust;
    nativeBuildInputs = commonArgs.nativeBuildInputs ++ [
      wasm-bindgen-cli
      nodejs_24
      python3
      taplo
    ];
    buildPhaseCargoCommand = "dev/check-validation all";
    doCheck = false;
    doInstallCargoArtifacts = false;
    doNotPostBuildInstallCargoBinaries = true;
    installPhaseCommand = "touch $out";
  }
)
