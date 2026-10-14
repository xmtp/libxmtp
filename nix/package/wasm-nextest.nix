# The WASM tests as one nextest archive and its run partitions.
#
# The archive compiles the test binaries once. Each partition runs one slice
# of the tests from that archive, so CI runs the slices in parallel jobs, with
# the test services (`services`, nix/services) started inside the build. The
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
  flock,
  nukeReferences,
  writeShellScript,
  services,
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

  # WASM has no `std::env`, so the tests bake the backend, proxy, and chain
  # URLs in at compile time (xmtp_configuration `api.rs`, `scw.rs`). Every
  # partition therefore starts the services on these ports. The block
  # 20000-20099 is below every ephemeral port range and above every worktree
  # slot port below 55432 (the highest is 13,464), so it collides with no
  # worktree stack. Each port keeps the residue of its worktree base port.
  ports = {
    XMTP_BACKEND_DB_PORT = 20032;
    XMTP_BACKEND_PORT = 20050;
    XMTP_BACKEND_METRICS_PORT = 20064;
    XMTP_ANVIL_PORT = 20045;
    XMTP_TOXIPROXY_API_PORT = 20074;
    XMTP_TOXIPROXY_PORT = 20010;
    XMTP_S3_PORT = 20067;
  };
  # The services export the same URLs for these ports.
  urls = {
    XMTP_BACKEND_URL = "http://127.0.0.1:${toString ports.XMTP_BACKEND_PORT}";
    XMTP_BACKEND_TOXIC_URL = "http://127.0.0.1:${toString ports.XMTP_TOXIPROXY_PORT}";
    ANVIL_URL = "http://127.0.0.1:${toString ports.XMTP_ANVIL_PORT}";
  };

  cargoArtifacts = xmtp.base.mkCargoArtifacts rust false (
    (removeAttrs commonArgs [ "src" ])
    // {
      buildPhaseCargoCommand = "cargo nextest run --locked --cargo-profile $CARGO_PROFILE --no-run ${wasmPackages}";
    }
  );

  testArgs =
    commonArgs
    // urls
    // {
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
    let
      run = writeShellScript "wasm-nextest-part-${toString n}" ''
        set -euo pipefail
        cargo nextest run --archive-file ${archive}/archive.tar.zst --workspace-remap . \
          --profile ci --partition slice:${toString n}/${toString partitions} --no-tests=pass
      '';
    in
    rust.mkCargoDerivation (
      testArgs
      // lib.mapAttrs (_: toString) ports
      // {
        pnameSuffix = "-nextest-part-${toString n}";
        # Everything that the run needs is in the archive.
        cargoArtifacts = null;
        cargoVendorDir = null;
        doInstallCargoArtifacts = false;
        CARGO_PROFILE = "";
        doCheck = true;
        buildPhaseCargoCommand = "mkdir -p $out";
        # The partitions of one machine share the port block, so a run holds
        # an exclusive lock on one file in /tmp while its services run; a
        # second run waits. Each build can run as another build user, and
        # Linux fs.protected_regular refuses an O_CREAT open of another user's
        # file in /tmp, which `flock <path>` always does. So the file is
        # created once (O_EXCL, world-readable), opened read-only, and locked
        # by descriptor. macOS can refuse a bind of a block port for a while
        # after the previous run's services stopped (back-to-back Darwin
        # partitions waited 27 s for the PostgreSQL port, with nothing
        # listening), so a busy port gets up to 90 s.
        checkPhaseCargoCommand = ''
          lock=/tmp/xmtp-wasm-nextest.lock
          (umask 000 && set -C && : >"$lock") 2>/dev/null || true
          exec 9<"$lock"
          if ! flock -n 9; then
            echo "wasm-nextest: waiting for $lock"
            flock -x 9
          fi
          # The services must not inherit the lock: a leaked child would hold it.
          ${lib.removeSuffix "\n" (services.checkCommand run)} 9<&-
          ${import ../lib/junit.nix "target/nextest/ci/junit.xml"}
        '';
        inherit (xmtp.shellCommon.wasmEnv)
          CHROMEDRIVER
          WASM_BINDGEN_TEST_TIMEOUT
          WASM_BINDGEN_TEST_WEBDRIVER_JSON
          ;
        WASM_BINDGEN_TEST_NO_ORIGIN_ISOLATION = "1";
        XMTP_SERVICES_PORT_WAIT = "90";
        # Chrome and Core Foundation need a writable home during browser tests.
        preBuild = ''
          export HOME=$TMPDIR
          export CFFIXED_USER_HOME=$TMPDIR
        '';
        nativeBuildInputs = testArgs.nativeBuildInputs ++ [
          flock
          nukeReferences
        ];
        buildInputs =
          base.commonArgs.buildInputs
          ++ [
            chromedriver
          ]
          ++ lib.optionals stdenv.isDarwin [ google-chrome ]
          ++ lib.optionals stdenv.isLinux [ chromium ];
      }
      # Chrome inside the Nix sandbox is not proven, so a partition still
      # runs outside it; the services' sandbox settings apply once it does.
      // services.sandbox
      // {
        __noChroot = true;
      }
    );
in
{
  inherit archive;
  partitions = lib.genList (i: mkPartition (i + 1)) partitions;
}
