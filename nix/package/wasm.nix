{
  emscripten,
  lib,
  wasm-pack,
  binaryen,
  mkShell,
  sqlite,
  llvmPackages,
  wasm-bindgen-cli,
  xmtp,
  chromedriver,
  google-chrome,
  chromium,
  corepack,
  cargo-nextest,
  stdenv,
  test ? false,
}:
let
  inherit (xmtp) craneLib base;
  # Pinned Rust Version (must use mkToolchain to match the rest of the project)
  rust-toolchain =
    xmtp.mkNativeToolchain
      [ "wasm32-unknown-unknown" ]
      [ "clippy-preview" "rustfmt-preview" ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  features = if test then "--features test-utils" else "";
  bindingsFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = xmtp.filesets.forCrate ./../../bindings/wasm;
  };

  commonArgs = base.commonArgs // {
    meta.description = "WebAssembly Bindings";
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
    nativeBuildInputs = base.commonArgs.nativeBuildInputs ++ [
      wasm-pack
      emscripten
      llvmPackages.lld
      binaryen
      wasm-bindgen-cli
    ];
    buildInputs = [ sqlite ];
    hardeningDisable = [
      "zerocallusedregs"
      "stackprotector"
    ];
  };

  commonEnv = {
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    inherit (xmtp.shellCommon.wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
    # why CC manually (zstd): https://github.com/gyscos/zstd-rs/issues/339
  };

  # enables caching all build time crates
  cargoArtifacts = rust.buildDepsOnly (
    (base.commonArgs // commonEnv)
    // {
      buildPhaseCargoCommand = "cargo build --package bindings_wasm ${features} --profile $CARGO_PROFILE --locked";
    }
  );

  bin = rust.buildPackage (
    (commonArgs // commonEnv)
    // {
      inherit cargoArtifacts;
      src = bindingsFileset;
      inherit
        (rust.crateNameFromCargoToml {
          cargoToml = ./../../bindings/wasm/Cargo.toml;
        })
        pname
        ;
      version = xmtp.mkVersion rust;
      buildPhaseCargoCommand = ''
        mkdir -p $out/dist
        cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)

        HOME=$(mktemp -d fake-homeXXXX) wasm-pack \
          --verbose build --target web --out-dir $out/dist \
          --no-pack --release ./bindings/wasm -- \
          ${features} --message-format json-render-diagnostics > "$cargoBuildLog"
      '';
    }
  );

  devShell = mkShell (
    commonEnv
    // {
      inputsFrom = [ commonArgs ];
      buildInputs = [
        rust-toolchain
        cargo-nextest
        chromedriver
        corepack
      ]
      # chromium unsupported on darwin
      # google-chrome unsupported on aarch64-linux
      # Firefox compiles from scratch on everything but x86_64 (unreliable build)
      ++ lib.optionals stdenv.isDarwin [ google-chrome ]
      ++ lib.optionals stdenv.isLinux [ chromium ];
      inherit (xmtp.shellCommon.wasmEnv)
        RSTEST_TIMEOUT
        WASM_BINDGEN_TEST_TIMEOUT
        WASM_BINDGEN_TEST_WEBDRIVER_JSON
        CHROMEDRIVER
        ;

      SQLITE = "${sqlite.dev}";
      SQLITE_OUT = "${sqlite.out}";
      CARGO_PROFILE_TEST_DEBUG = 0;
      CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
      XMTP_NIX_ENV = 1;
    }
  );
in
{
  inherit devShell bin;
}
