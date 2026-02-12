{
  emscripten,
  lib,
  fenix,
  wasm-pack,
  binaryen,
  zstd,
  zlib,
  mkShell,
  sqlite,
  llvmPackages,
  wasm-bindgen-cli,
  xmtp,
  chromedriver,
  google-chrome,
  corepack,
  pkg-config,
  cargo-nextest,
}:
let
  inherit (xmtp) craneLib;
  # Pinned Rust Version
  rust-toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
  ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  libraryFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.libraries;
  };

  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.forCrate ./../../bindings/wasm;
  };

  commonArgs = {
    meta.description = "WebAssembly Bindings";
    src = libraryFileset;
    strictDeps = true;
    # EM_CACHE = "$TMPDIR/.emscripten_cache";
    # we need to set tmpdir for emscripten cache
    preConfigure = ''
      export HOME=$TMPDIR
    '';
    preBuild = ''
      export HOME=$TMPDIR
      # export EM_CACHE=$TMPDIR
      # export EMCC_DEBUG=2
    '';
    nativeBuildInputs = [
      zstd
      zlib
      pkg-config
      wasm-pack
      emscripten
      llvmPackages.lld
      binaryen
      wasm-bindgen-cli
    ];
    buildInputs = [ sqlite ];
    doCheck = false;
    cargoExtraArgs = "--workspace --exclude xmtpv3 --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_grpc --exclude benches --exclude xmtp-db-tools";
    hardeningDisable = [
      "zerocallusedregs"
      "stackprotector"
    ];
  };

  commonEnv = {
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    NIX_DEBUG = 1;
    # why CC manually (zstd): https://github.com/gyscos/zstd-rs/issues/339
    CC_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/clang";
    AR_wasm32_unknown_unknown = "${llvmPackages.bintools-unwrapped}/bin/llvm-ar";
    CFLAGS_wasm32_unknown_unknown = "-I ${llvmPackages.clang-unwrapped.lib}/lib/clang/21/include";
    # SQLITE_WASM_RS_UPDATE_PREBUILD = 1;
  };

  # enables caching all build time crates
  cargoArtifacts = rust.buildDepsOnly (commonEnv // commonArgs);

  bin = rust.buildPackage (
    (commonEnv // commonArgs)
    // {
      inherit cargoArtifacts;
      src = bindingsFileset;
      inherit
        (rust.crateNameFromCargoToml {
          cargoToml = ./../../bindings/wasm/Cargo.toml;
        })
        pname
        ;
      inherit
        (rust.crateNameFromCargoToml {
          cargoToml = ./../../Cargo.toml;
        })
        version
        ;
      buildPhaseCargoCommand = ''
        mkdir -p $out/dist
        cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)

        HOME=$(mktemp -d fake-homeXXXX) wasm-pack --verbose build --target web --out-dir $out/dist --no-pack --release ./bindings/wasm -- --message-format json-render-diagnostics > "$cargoBuildLog"
      '';

    }
  );

  # this allows re-using build artifacts
  # nextest-libs = nextest "-E 'kind(lib)'";
  # nextest-d14n = nextest "--features d14n -E 'kind(lib)'";
  # nextest-integration = nextest "-E 'package(bindings_wasm)'";

  devShell = mkShell (
    commonEnv
    // {
      inputsFrom = [ commonArgs ];
      buildInputs = [
        rust-toolchain
        google-chrome
        chromedriver
        corepack
        cargo-nextest
      ];

      SQLITE = "${sqlite.dev}";
      SQLITE_OUT = "${sqlite.out}";
      CHROMEDRIVER = "${lib.getBin chromedriver}/bin/chromedriver";
      WASM_BINDGEN_TEST_TIMEOUT = 1024;
      WASM_BINDGEN_TEST_ONLY_WEB = 1;
      RSTEST_TIMEOUT = 90;
      CARGO_PROFILE_TEST_DEBUG = 0;
      WASM_BINDGEN_TEST_WEBDRIVER_JSON = ./../../webdriver.json;
      CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
      XMTP_NIX_ENV = 1;
    }
  );
in
{
  inherit bin devShell;
}
