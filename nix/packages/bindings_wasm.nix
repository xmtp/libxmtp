{ emscripten
, stdenv
, lib
, pkg-config
, darwin
, wasm-bindgen-cli_0_2_100
, wasm-pack
, binaryen
, zstd
, lld
, mkShell
, xmtp
, toolchain
}@pkgs:
let
  inherit (stdenv) hostPlatform;
  # Pinned Rust Version

  workspaceFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = (xmtp.filesets{ inherit lib; craneLib = toolchain; }).workspace;
  };

  commonArgs = {
    src = toolchain.cleanCargoSource ./../..;
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
    buildInputs = [ zstd ] ++ lib.optionals hostPlatform.isDarwin
      [
        darwin.apple_sdk.frameworks.Security
        darwin.apple_sdk.frameworks.SystemConfiguration
      ];
    doCheck = false;
    cargoExtraArgs = "--workspace --exclude xmtpv3 --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_grpc";
    RUSTFLAGS = [ "--cfg" "tracing_unstable" "-C" "target-feature=+bulk-memory,+mutable-globals" ];
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
  };

  # enables caching all build time crates
  cargoArtifacts = toolchain.buildDepsOnly (commonArgs // {
    doCheck = false;
  });

  bin = toolchain.buildPackage
    (commonArgs // {
      inherit cargoArtifacts;
      src = workspaceFileset;
      inherit (toolchain.crateNameFromCargoToml {
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
    buildInputs = commonArgs.buildInputs ++ [ toolchain ];
  };
in
{
  inherit bin devShell;
}
