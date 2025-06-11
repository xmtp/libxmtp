{ emscripten
, stdenv
, filesets
, lib
, pkg-config
, fenix
, wasm-bindgen-cli_0_2_100
, wasm-pack
, binaryen
, zstd
, craneLib
, lld
, mkShell
, sqlite
, llvmPackages
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
    fileset = filesets.workspace;
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
      export EM_CACHE=$TMPDIR
      export EMCC_DEBUG=2
    '';

    nativeBuildInputs = [ pkg-config wasm-pack emscripten lld binaryen wasm-bindgen-cli_0_2_100 ];
    buildInputs = [ zstd sqlite ];
    doCheck = false;
    cargoExtraArgs = "--workspace --exclude xmtpv3 --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_grpc";
    RUSTFLAGS = [ "--cfg" "tracing_unstable" "--cfg" "getrandom_backend=\"wasm_js\"" "-C" "target-feature=+bulk-memory,+mutable-globals" ];
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
  } // lib.optionalAttrs stdenv.isDarwin {
    CC_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/clang";
    AR_wasm32_unknown_unknown = "${llvmPackages.clang-unwrapped}/bin/llvm-ar";

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
      }) pname version;
      buildPhaseCargoCommand = ''
        mkdir -p $out/dist
        cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)

        HOME=$(mktemp -d fake-homeXXXX) wasm-pack --verbose build --target web --out-dir $out/dist --no-pack --release ./bindings_wasm -- --message-format json-render-diagnostics > "$cargoBuildLog"
      '';
    });
  devShell = mkShell {
    inherit (commonArgs) nativeBuildInputs RUSTFLAGS;
    buildInputs = commonArgs.buildInputs ++ [ rust-toolchain ];
  };
in
{
  inherit bin devShell;
}




