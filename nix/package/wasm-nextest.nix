# The WASM tests as one nextest archive and its run partitions.
#
# The archive compiles the test binaries once. Each partition runs one slice
# of the tests from that archive, so CI runs the slices in parallel jobs. The
# backend binary is a run input only: a backend change reruns the partitions
# and substitutes the archive. Adapted from crane `lib/cargoNextest.nix`,
# whose `partitions` option gives the archive and the partitions one argument
# set, so a run input would also enter the archive key.
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
  removeReferencesTo,
  xmtp-backend,
  partitions,
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

  # The tested crates and the dependencies that Cargo builds for wasm32, with
  # the roots' dev-dependencies. Other members stay stubs. `apps/backend` is a
  # native-only dev-dependency of `xmtp_mls`, so a backend change keeps the
  # archive key (Q12 of the Nix build plan).
  closureArgs = {
    roots = map (name: root + "/crates/${name}") wasmCrates;
    dev = true;
    target = "wasm32-unknown-unknown";
  };
  src =
    assert lib.assertMsg (
      !(builtins.elem (root + /apps/backend) (xmtp.filesets.closure closureArgs))
    ) "wasm-nextest: apps/backend must stay out of the WASM archive source";
    xmtp.filesets.mkClosureSource rust (
      closureArgs
      // {
        extra = [
          (root + /.config/nextest.toml)
          # db snapshots
          (fileFilter (file: file.hasExt "xmtp") (root + /crates/xmtp_mls/tests/assets))
          (fileFilter (file: file.hasExt "json") (root + /crates))
        ];
      }
    );

  # The dependency layer reads these; keep them apart from the test arguments
  # below, so a test setting does not rebuild the layer.
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

  testArgs = commonArgs // {
    inherit src;
    pname = "wasm";
    version = xmtp.mkVersion rust;
    # rstest_macros reads RSTEST_TIMEOUT, and xmtp_macro reads
    # XMTP_TEST_LOGGING, when the test macros expand, so the archive needs them.
    inherit (xmtp.shellCommon.wasmEnv) RSTEST_TIMEOUT;
    XMTP_TEST_LOGGING = "false";
    RUST_LOG = "off";
    NEXTEST_SHOW_PROGRESS = "counter";
    # An output must name no store path, so it substitutes on a hit.
    allowedReferences = [ ];
  };

  archive = rust.mkCargoDerivation (
    testArgs
    // {
      pnameSuffix = "-nextest-archive";
      inherit cargoArtifacts;
      cargoVendorDir = base.mkCargoVendorDir rust;
      doInstallCargoArtifacts = false;
      nativeBuildInputs = testArgs.nativeBuildInputs ++ [ removeReferencesTo ];
      # The archived Cargo metadata and the debug info of the test binaries
      # name the vendor directory. A run reads neither, so drop the
      # reference; otherwise each partition would fetch every vendored crate.
      # Nextest accepts only `target/...` entry paths, so pack `target`.
      buildPhaseCargoCommand = ''
        cargo nextest archive --locked --cargo-profile $CARGO_PROFILE ${wasmPackages} \
          --archive-format tar-zst --archive-file archive.tar.zst
        mkdir archive
        zstd -dc archive.tar.zst | tar -x -C archive
        find archive -type f -exec remove-references-to -t "$cargoVendorDir" '{}' +
        mkdir -p $out
        tar -C archive --format=gnu --sort=name --mtime=@1 --owner=0 --group=0 --numeric-owner \
          -c target | zstd -q -T0 -o $out/archive.tar.zst
      '';
    }
  );

  # Run slice `n` of the archive. Nextest reads the target runner from
  # `.cargo/config.toml` and the `ci` profile from `.config/nextest.toml` in
  # the remapped workspace.
  mkPartition =
    n:
    rust.mkCargoDerivation (
      testArgs
      // {
        pnameSuffix = "-nextest-part-${toString n}";
        # Everything that the run needs is in the archive.
        cargoArtifacts = null;
        cargoVendorDir = null;
        doInstallCargoArtifacts = false;
        CARGO_PROFILE = "";
        doCheck = true;
        buildPhaseCargoCommand = "mkdir -p $out";
        checkPhaseCargoCommand = ''
          cargo nextest run --archive-file ${archive}/archive.tar.zst --workspace-remap . \
            --profile ci --partition slice:${toString n}/${toString partitions} --no-tests=pass
        '';
        # The tests run against the backend that the Docker stack serves from
        # the same source. Naming the binary keys each run on the backend, so
        # a backend change reruns the tests without a recompile.
        XMTP_BACKEND_BIN = "${xmtp-backend}/bin/xmtp-backend";
        inherit (xmtp.shellCommon.wasmEnv)
          CHROMEDRIVER
          WASM_BINDGEN_TEST_TIMEOUT
          WASM_BINDGEN_TEST_WEBDRIVER_JSON
          ;
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
        # most tests query docker
        __noChroot = true;
      }
    );
in
{
  inherit archive;
  partitions = lib.genList (i: mkPartition (i + 1)) partitions;
}
