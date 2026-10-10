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
  inherit (lib.fileset) fileFilter;
  inherit (xmtp) craneLib base;
  root = ./../..;
  rust-toolchain = p: xmtp.mkToolchain p [ "wasm32-unknown-unknown" ] [ "llvm-tools-preview" ];
  rust = craneLib.overrideToolchain rust-toolchain;

  wasmCrates = [
    "xmtp_mls"
    "xmtp_cryptography"
    "xmtp_common"
    "xmtp_api"
    "xmtp_id"
    "xmtp_db"
    "xmtp_api_backend"
    "xmtp_content_types"
    "xmtp_attachments"
    "xmtp_sdk"
  ];

  # The tested crates and their dev-dependencies. Other members stay stubs.
  src = xmtp.filesets.mkClosureSource rust {
    roots = map (name: root + "/crates/${name}") wasmCrates;
    dev = true;
    extra = [
      (root + /.config/nextest.toml)
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

  wasmPackages = lib.concatMapStringsSep " " (name: "-p ${name}") wasmCrates;

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
    cargoVendorDir = base.mkCargoVendorDir rust;
    inherit (xmtp.shellCommon.wasmEnv)
      CHROMEDRIVER
      RSTEST_TIMEOUT
      WASM_BINDGEN_TEST_TIMEOUT
      WASM_BINDGEN_TEST_WEBDRIVER_JSON
      ;
    doCheck = true;
    WASM_BINDGEN_TEST_NO_ORIGIN_ISOLATION = "1";
    # Chrome and Core Foundation need a writable home during browser tests.
    preBuild = ''
      export HOME=$TMPDIR
      export CFFIXED_USER_HOME=$TMPDIR
    '';
    buildInputs =
      base.commonArgs.buildInputs
      ++ [
        chromedriver
      ]
      ++ lib.optionals stdenv.isDarwin [ google-chrome ]
      ++ lib.optionals stdenv.isLinux [ chromium ];

    pname = "wasm";
    version = xmtp.mkVersion rust;
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
