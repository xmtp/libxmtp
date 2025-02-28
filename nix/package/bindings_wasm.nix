{ openssl
, sqlite
, emscriptenStdenv
, stdenv
  # , fetchFromGitHub
, filesets
, lib
, pkg-config
, darwin
, fenix
, wasm-bindgen-cli_0_2_100
, wasm-pack
  # , fetchCrate
  # , rustPlatform
, craneLib
}:
let
  inherit (stdenv) hostPlatform;
  # Pinned Rust Version
  rust-toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    fenix.targets.wasm32-unknown-unknown.stable.rust-std
  ];
  crane = craneLib.overrideToolchain (p: rust-toolchain);

  crateFileset = crate: lib.fileset.toSource
    {
      root = ./../..;
      fileset = filesets.forCrate crate;
    };

  workspaceFileset = lib.fileset.toSource {
    root = ./../..;
    fileset = filesets.workspace;
  };

  commonArgs = {
    src = workspaceFileset;
    strictDeps = true;
    stdenv = emscriptenStdenv;
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

    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ openssl sqlite wasm-bindgen-cli_0_2_100 wasm-pack ] ++ lib.optionals hostPlatform.isDarwin
      [
        darwin.apple_sdk.frameworks.Security
        darwin.apple_sdk.frameworks.SystemConfiguration
      ];
    doCheck = false;
    cargoExtraArgs = "--workspace --exclude xmtpv3 --exclude bindings_node --exclude xmtp_cli --exclude xdbg --exclude mls_validation_service --exclude xmtp_api_grpc --exclude xmtp_v2";
    RUSTFLAGS = [ "--cfg" "tracing_unstable" ];
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
  };

  # enables caching all build time crates
  cargoArtifacts = crane.buildDepsOnly (commonArgs // {
    doCheck = false;
  });

  bin = craneLib.buildPackage
    (commonArgs // {
      inherit cargoArtifacts;
      buildPhaseCargoCommand = ''
        mkdir -p $out/dist

        wasm-pack build --target web --out-dir $out/dist --no-pack --release $src
      '';
    });
in
{
  inherit bin;
  # inherit bin devShell;
}
