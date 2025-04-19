{ emscripten
, stdenv
, filesets
, lib
, pkg-config
, darwin
, fenix
, wasm-bindgen-cli_0_2_100
, wasm-pack
, binaryen
, zstd
, craneLib
, lld
, mkShell
}:
let
  inherit (stdenv) hostPlatform;
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

  # Emscripten 3 causes a weird import issue with wasm,
  # and nixpkgs hasn't updated to emscripten 4 yet
  # emscripten4 = (emscripten.overrideAttrs {
  #   version = "4.0.4";
  #   src = fetchFromGitHub {
  #     owner = "emscripten-core";
  #     repo = "emscripten";
  #     hash = "sha256-4qxx+iQ51KMWr26fbf6NpuWOn788TqS6RX6gJPkCxVI=";
  #     rev = "4.0.4";
  #   };
  #
  #   nodeModules = emscripten.nodeModules.overrideAttrs {
  #     name = "emscripten-node-modules-4.0.4";
  #     version = "4.0.4";
  #     npmDepsHash = "sha256-0000000000000000000000000000000000000000000=";
  #   };
  # }).override { llvmPackages = llvmPackages_20; };

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




