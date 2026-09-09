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

  wasmPackages = "-p xmtp_mls -p xmtp_cryptography -p xmtp_common -p xmtp_api -p xmtp_id -p xmtp_db -p xmtp_api_backend -p xmtp_content_types";

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
    (removeAttrs commonArgs [ "src" ])
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

    pname = "wasm";
    doInstallCargoArtifacts = false;
    partitions = 1;
    partitionType = "count";
    cargoNextestPartitionsExtraArgs = "--no-tests=pass";
    XMTP_TEST_LOGGING = "false";
    RUST_LOG = "off";
    cargoExtraArgs = "${wasmPackages}";
    cargoNextestExtraArgs = "--profile ci";
    # most tests query docker
    __noChroot = true;
  }
)
