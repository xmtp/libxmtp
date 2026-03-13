# Derivation that runs cargo nextest with llvm-cov on the workspace
{
  xmtp,
  lib,
  pkg-config,
  openssl,
  perl,
  sqlite,
  sqlcipher,
  chromedriver,
  google-chrome,
  chromium,
  stdenv,
  wasm-bindgen-cli,
  d14n ? false,
  ...
}:
let
  inherit (lib.fileset) unions fileFilter;
  inherit (xmtp) craneLib;
  inherit (craneLib.fileset) commonCargoSources;
  root = ./../..;
  rust-toolchain = xmtp.mkToolchain [ "wasm32-unknown-unknown" ] [ "llvm-tools-preview" ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.libraries
      (commonCargoSources (root + /bindings/wasm))
      # since validation is part of default members it must be included
      (root + /apps/mls_validation_service/src/main.rs)
      (root + /apps/.gitkeep)
      # db snapshots
      (fileFilter (file: file.hasExt "xmtp") (root + /crates/xmtp_mls/tests/assets))
      (fileFilter (file: file.hasExt "json") (root + /crates))
    ];
  };

  commonArgs = {
    inherit src;
    strictDeps = true;
    nativeBuildInputs = [
      pkg-config
      openssl
      perl
      sqlite
      sqlcipher
      wasm-bindgen-cli
    ];
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    inherit (xmtp.shellCommon.wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
  };

  cargoArtifacts = rust.buildDepsOnly commonArgs // {
    doCheck = false;
  };
in
rust.cargoNextest (
  commonArgs
  // {
    inherit cargoArtifacts;
    inherit (xmtp.shellCommon.wasmEnv)
      CHROMEDRIVER
      RSTEST_TIMEOUT
      WASM_BINDGEN_TEST_TIMEOUT
      WASM_BINDGEN_TEST_ONLY_WEB
      WASM_BINDGEN_TEST_WEBDRIVER_JSON
      ;
    buildInputs = [
      chromedriver
    ]
    ++ lib.optionals stdenv.isDarwin [ google-chrome ]
    ++ lib.optionals stdenv.isLinux [ chromium ];

    pnameSuffix = if d14n then "wasm-nextest-d14n" else "wasm-nextest-v3";
    partitions = 1;
    partitionType = "count";
    cargoNextestPartitionsExtraArgs = "--no-tests=pass";
    XMTP_TEST_LOGGING = "false";
    RUST_LOG = "off";
    cargoExtraArgs = if d14n then "--features d14n" else "";
    cargoNextestExtraArgs = if d14n then "--profile ci-d14n" else "--profile ci";
    # most tests query docker
    __noChroot = true;
  }
)
