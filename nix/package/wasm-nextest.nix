# Derivation that runs cargo nextest with llvm-cov on the workspace
{
  xmtp,
  lib,
  chromedriver,
  google-chrome,
  chromium,
  stdenv,
  wasm-bindgen-cli,
  cargo-nextest,
  nodejs_24,
  d14n ? false,
  ...
}:
let
  inherit (lib.fileset) unions fileFilter;
  inherit (xmtp) craneLib base;
  inherit (craneLib.fileset) commonCargoSources;
  root = ./../..;
  rust-toolchain = p: xmtp.mkToolchain p [ "wasm32-unknown-unknown" ] [ "llvm-tools-preview" ];
  rust = craneLib.overrideToolchain rust-toolchain;

  src = lib.fileset.toSource {
    inherit root;
    fileset = unions [
      xmtp.filesets.libraries
      # All bindings and apps cargo sources so the full workspace resolves
      # with --locked. crane replaces source with dummies for buildDepsOnly.
      (commonCargoSources (root + /bindings/wasm))
      # db snapshots
      (fileFilter (file: file.hasExt "xmtp") (root + /crates/xmtp_mls/tests/assets))
      (fileFilter (file: file.hasExt "json") (root + /crates))
    ];
  };

  commonArgs = base.commonArgs // {
    nativeBuildInputs = base.commonArgs.nativeBuildInputs ++ [
      cargo-nextest
      wasm-bindgen-cli
      nodejs_24
    ];
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    preConfigure = ''
      export HOME=$TMPDIR
    '';
    inherit (xmtp.shellCommon.wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
    CARGO_PROFILE = "wasm-test";
  };

  d14nTestArgs = if d14n then "--features d14n" else "";
  wasmPackages = "-p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_d14n -p xmtp_content_types";

  cargoArtifacts = rust.buildDepsOnly (
    commonArgs
    // {
      buildPhaseCargoCommand = "cargo nextest run --locked --cargo-profile $CARGO_PROFILE --no-run ${wasmPackages}";
    }
  );
in
rust.cargoNextest (
  commonArgs
  // {
    inherit src cargoArtifacts;
    inherit (xmtp.shellCommon.wasmEnv)
      CHROMEDRIVER
      RSTEST_TIMEOUT
      WASM_BINDGEN_TEST_TIMEOUT
      WASM_BINDGEN_TEST_WEBDRIVER_JSON
      ;
    doCheck = true;
    WASM_BINDGEN_TEST_NO_ORIGIN_ISOLATION = "1";
    # chromedriver requires home to be editable/set, otherwise it SIGKILLS and fails tests.
    preBuild = "export HOME=$TMPDIR";
    buildInputs =
      base.commonArgs.buildInputs
      ++ [
        chromedriver
      ]
      ++ lib.optionals stdenv.isDarwin [ google-chrome ]
      ++ lib.optionals stdenv.isLinux [ chromium ];

    pname = if d14n then "wasm-d14n" else "wasm-v3";
    partitions = 1;
    partitionType = "count";
    cargoNextestPartitionsExtraArgs = "--no-tests=pass";
    XMTP_TEST_LOGGING = "false";
    RUST_LOG = "off";
    cargoExtraArgs = "${d14nTestArgs} ${wasmPackages}";
    cargoNextestExtraArgs = if d14n then "--profile ci-d14n" else "--profile ci";
    # most tests query docker
    __noChroot = true;
  }
)
