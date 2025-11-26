{ emscripten
, stdenv
, lib
, fenix
, wasm-pack
, binaryen
, zstd
, craneLib
, mkShell
, sqlite
, llvmPackages
, wasm-bindgen-cli
, xmtp
, geckodriver
, firefox
, corepack
}:
let
  # Pinned Rust Version
  rust-toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
  ];
  rust = craneLib.overrideToolchain (p: rust-toolchain);

  workspaceFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets { inherit lib craneLib; }).workspace;
  };

  commonArgs = {
    src = rust.cleanCargoSource ./../..;
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

    nativeBuildInputs = [ wasm-pack emscripten llvmPackages.lld binaryen wasm-bindgen-cli ];
    buildInputs = [ zstd sqlite ];
    doCheck = false;
    cargoExtraArgs = "--workspace --exclude xmtpv3 --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_grpc --exclude benches";
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    hardeningDisable = [ "zerocallusedregs" "stackprotector" ];
    NIX_DEBUG = 1;
    # SQLITE_WASM_RS_UPDATE_PREBUILD = 1;
  } // lib.optionalAttrs stdenv.isDarwin {
    CC_wasm32_unknown_unknown = "${llvmPackages.libcxxClang}/bin/clang";
    AR_wasm32_unknown_unknown = "${llvmPackages.bintools-unwrapped}/bin/llvm-ar";
  };

  # enables caching all build time crates
  cargoArtifacts = rust.buildDepsOnly (commonArgs // {
    doCheck = false;
  });

  bin = rust.buildPackage
    (commonArgs // {
      inherit cargoArtifacts;
      src = workspaceFileset;
      inherit (rust.crateNameFromCargoToml {
        cargoToml = ./../../bindings_wasm/Cargo.toml;
      }) pname;
      inherit (rust.crateNameFromCargoToml {
        cargoToml = ./../../Cargo.toml;
      }) version;
      buildPhaseCargoCommand = ''
        mkdir -p $out/dist
        cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)

        HOME=$(mktemp -d fake-homeXXXX) wasm-pack --verbose build --target web --out-dir $out/dist --no-pack --release ./bindings_wasm -- --message-format json-render-diagnostics > "$cargoBuildLog"
      '';
    });
  devShell = mkShell
    {
      inherit (commonArgs) nativeBuildInputs;
      buildInputs = commonArgs.buildInputs ++ [ rust-toolchain firefox geckodriver corepack ];
      CC_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/clang";
      AR_wasm32_unknown_unknown = "${llvmPackages.bintools-unwrapped}/bin/llvm-ar";
      SQLITE = "${sqlite.dev}";
      SQLITE_OUT = "${sqlite.out}";
      GECKODRIVER = "${lib.getBin geckodriver}/bin/geckodriver";
      CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
      WASM_BINDGEN_SPLIT_LINKED_MODULES = 1;
      WASM_BINDGEN_TEST_TIMEOUT = 256;
      WASM_BINDGEN_TEST_ONLY_WEB = 1;
      RSTEST_TIMEOUT = 90;
      CARGO_PROFILE_TEST_DEBUG = 0;
      WASM_BINDGEN_TEST_WEBDRIVER_JSON = ./../../webdriver.json;
    } // commonArgs;
in
{
  inherit bin devShell;
}




