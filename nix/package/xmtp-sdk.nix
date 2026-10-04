{
  lib,
  stdenvNoCC,
  xmtp,
  pkgs,
  wasm-bindgen-cli,
}:
let
  rustToolchain = xmtp.mkNativeToolchain [ "wasm32-unknown-unknown" ] [ ];
  rust = xmtp.craneLib.overrideToolchain (p: rustToolchain);
  hostTarget = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  root = ./../..;
  sdkSource = lib.fileset.toSource {
    inherit root;
    # Cargo checks every workspace member against Cargo.lock. Keep the full
    # workspace so the filtered source does not change the lock file.
    fileset = lib.fileset.unions [
      xmtp.filesets.workspace
      (lib.fileset.fileFilter (
        file:
        (
          lib.hasSuffix ".rs" file.name || lib.hasSuffix ".proto" file.name || lib.hasSuffix ".sql" file.name
        )
        || file.name == "Cargo.toml"
      ) (root + /crates))
      (lib.fileset.fileFilter (
        file:
        (
          lib.hasSuffix ".rs" file.name || lib.hasSuffix ".proto" file.name || lib.hasSuffix ".sql" file.name
        )
        || file.name == "Cargo.toml"
      ) (root + /apps))
      (root + /flake.lock)
      (root + /rust-toolchain.toml)
      (root + /crates/xmtp_sdk)
      (root + /apps/xmtp_sdk_bindgen)
      (root + /crates/xmtp_configuration)
    ];
  };
  common = xmtp.base.commonArgs // {
    version = xmtp.mkVersion rust;
    doNotPostBuildInstallCargoBinaries = true;
  };
  nativeArgs = {
    OPENSSL_NO_VENDOR = "0";
    OPENSSL_STATIC = "1";
  }
  // lib.optionalAttrs pkgs.stdenv.isDarwin {
    MACOSX_DEPLOYMENT_TARGET = "11.0";
    # Darwin setup replaces this variable before the Cargo build.
    preBuild = "export MACOSX_DEPLOYMENT_TARGET=11.0";
  };
  native = rust.buildPackage (
    common
    // nativeArgs
    // {
      pname = "xmtp-sdk-libs";
      src = sdkSource;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust false nativeArgs;
      buildPhaseCargoCommand = "cargo build --release --locked -p xmtp_sdk --lib";
      installPhaseCommand = ''
          mkdir -p $out/lib
        cp target/${hostTarget}/release/libxmtp_sdk.a $out/lib/
        cp target/${hostTarget}/release/libxmtp_sdk.${
          if pkgs.stdenv.isDarwin then "dylib" else "so"
        } $out/lib/
      '';
    }
  );
  wasmArgs = {
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    inherit (xmtp.shellCommon.wasmEnv)
      CC_wasm32_unknown_unknown
      AR_wasm32_unknown_unknown
      CFLAGS_wasm32_unknown_unknown
      ;
    buildPhaseCargoCommand = "cargo build --release --locked -p xmtp_sdk --lib --target wasm32-unknown-unknown";
  };
  # Full and pure WASM share the dependency cache. They remain distinct
  # artifacts because their exported Rust interfaces differ.
  wasmDeps = xmtp.base.mkCargoArtifacts rust false wasmArgs;
  mkWasm =
    pure:
    rust.buildPackage (
      common
      // wasmArgs
      // {
        pname = if pure then "xmtp-sdk-pure-wasm" else "xmtp-sdk-wasm";
        src = sdkSource;
        cargoArtifacts = wasmDeps;
        buildPhaseCargoCommand =
          wasmArgs.buildPhaseCargoCommand + lib.optionalString pure " --features pure-only";
        installPhaseCommand = ''
          mkdir -p $out/lib
          cp target/wasm32-unknown-unknown/release/xmtp_sdk.wasm $out/lib/
        '';
      }
    );
  wasm = mkWasm false;
  pureWasm = mkWasm true;
  bindgen = rust.buildPackage (
    common
    // {
      pname = "xmtp-sdk-bindgen";
      src = sdkSource;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust false { };
      buildPhaseCargoCommand = "cargo build --release --locked -p xmtp-sdk-bindgen";
      installPhaseCommand = ''
          mkdir -p $out/bin
        cp target/${hostTarget}/release/xmtp-sdk-bindgen $out/bin/
      '';
    }
  );
  iosRust = (xmtp.craneLib.overrideScope (_: _: { stdenv = pkgs.stdenvNoCC; })).overrideToolchain (
    p: xmtp.mkToolchain p [ hostTarget "aarch64-apple-ios" "aarch64-apple-ios-sim" ] [ ]
  );
  iosTargets = lib.optionalAttrs pkgs.stdenv.isDarwin (
    lib.genAttrs [ "aarch64-apple-ios" "aarch64-apple-ios-sim" ] (
      target:
      let
        envSetup = xmtp.iosEnv.envSetup target;
        command = ''
          ${envSetup}
          cargo build --release --locked -p xmtp_sdk --lib --target ${target}
        '';
      in
      iosRust.buildPackage (
        common
        // nativeArgs
        // {
          pname = "xmtp-sdk-ios-${target}";
          src = sdkSource;
          CARGO_BUILD_TARGET = target;
          __noChroot = true;
          cargoArtifacts = xmtp.base.mkCargoArtifacts iosRust false (
            nativeArgs
            // {
              CARGO_BUILD_TARGET = target;
              __noChroot = true;
              buildPhaseCargoCommand = command;
            }
          );
          buildPhaseCargoCommand = command;
          installPhaseCommand = ''
            mkdir -p $out/lib
            cp target/${target}/release/libxmtp_sdk.a $out/lib/
            cp target/${target}/release/libxmtp_sdk.dylib $out/lib/
          '';
        }
      )
    )
  );
  ubrn = pkgs.callPackage ../lib/packages/ubrn.nix { };
  generated = stdenvNoCC.mkDerivation {
    pname = "xmtp-sdk-generated";
    version = xmtp.mkVersion rust;
    src = sdkSource;
    nativeBuildInputs = [
      bindgen
      rustToolchain
      wasm-bindgen-cli
      pkgs.python3
    ];
    buildPhase = ''
      # Use the unpacked source: generation rewrites copied TypeScript files,
      # while the source files in the Nix store are read-only.
      mkdir -p $out
      # The Nix build has no JavaScript workspace dependencies. Formatting
      # changes layout only, so use the generator's supported no-format mode.
      for language in swift kotlin typescript-napi; do
        xmtp-sdk-bindgen generate \
          --lib ${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"} \
          --language "$language" --no-format --out "$out/$language" \
          --config apps/xmtp_sdk_bindgen/uniffi-global.toml
      done
      xmtp-sdk-bindgen generate --lib ${wasm}/lib/xmtp_sdk.wasm \
        --language typescript-wasm --no-format --out "$out/typescript-wasm" \
        --config apps/xmtp_sdk_bindgen/uniffi-global.toml
      xmtp-sdk-bindgen stage-wasm --lib ${wasm}/lib/xmtp_sdk.wasm \
        --out "$out/typescript-wasm"
      xmtp-sdk-bindgen generate --lib ${pureWasm}/lib/xmtp_sdk.wasm \
        --language typescript-wasm --pure-only --no-format --out "$out/typescript-pure" \
        --config apps/xmtp_sdk_bindgen/uniffi-global.toml
      xmtp-sdk-bindgen stage-wasm --lib ${pureWasm}/lib/xmtp_sdk.wasm \
        --out "$out/typescript-pure"
      cp ${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"} $out/typescript-napi/
      python3 crates/xmtp_sdk/dev/record-generated.py "$out" \
        --native ${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"} \
        --wasm ${wasm}/lib/xmtp_sdk.wasm --pure ${pureWasm}/lib/xmtp_sdk.wasm \
        --bindgen ${bindgen}/bin/xmtp-sdk-bindgen
      mkdir -p $out/runtimes
      ln -s ${ubrn.core} $out/runtimes/core
      ln -s ${ubrn.node} $out/runtimes/node
      ln -s ${ubrn.wasm} $out/runtimes/wasm
    '';
    installPhase = "true";
  };
in
{
  libs = native;
  inherit
    wasm
    pureWasm
    bindgen
    generated
    iosTargets
    ;
  runtimes = ubrn;
}
