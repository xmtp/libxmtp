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
      (root + /crates/xmtp_sdk)
      (root + /apps/xmtp_sdk_bindgen)
    ];
  };
  common = xmtp.base.commonArgs // {
    version = xmtp.mkVersion rust;
    doNotPostBuildInstallCargoBinaries = true;
  };
  native = rust.buildPackage (
    common
    // {
      pname = "xmtp-sdk-libs";
      src = sdkSource;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust false null;
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
  wasm = rust.buildPackage (
    common
    // {
      pname = "xmtp-sdk-wasm";
      src = sdkSource;
      CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
      inherit (xmtp.shellCommon.wasmEnv)
        CC_wasm32_unknown_unknown
        AR_wasm32_unknown_unknown
        CFLAGS_wasm32_unknown_unknown
        ;
      nativeBuildInputs = common.nativeBuildInputs ++ [ wasm-bindgen-cli ];
      buildPhaseCargoCommand = "cargo build --release --locked -p xmtp_sdk --lib --target wasm32-unknown-unknown";
      installPhaseCommand = ''
        mkdir -p $out/lib
        cp target/wasm32-unknown-unknown/release/xmtp_sdk.wasm $out/lib/
      '';
    }
  );
  bindgen = rust.buildPackage (
    common
    // {
      pname = "xmtp-sdk-bindgen";
      src = sdkSource;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust false null;
      buildPhaseCargoCommand = "cargo build --release --locked -p xmtp-sdk-bindgen";
      installPhaseCommand = ''
          mkdir -p $out/bin
        cp target/${hostTarget}/release/xmtp-sdk-bindgen $out/bin/
      '';
    }
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
    ];
    buildPhase = ''
      cd "$src"
      mkdir -p $out
      for language in swift kotlin typescript-napi; do
        xmtp-sdk-bindgen generate \
          --lib ${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"} \
          --language "$language" --out "$out/$language" \
          --config apps/xmtp_sdk_bindgen/uniffi-global.toml
      done
      xmtp-sdk-bindgen generate --lib ${wasm}/lib/xmtp_sdk.wasm \
        --language typescript-wasm --out "$out/typescript-wasm" \
        --config apps/xmtp_sdk_bindgen/uniffi-global.toml
      xmtp-sdk-bindgen stage-wasm --lib ${wasm}/lib/xmtp_sdk.wasm \
        --out "$out/typescript-wasm"
      cp ${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"} $out/typescript-napi/
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
  inherit wasm bindgen generated;
  runtimes = ubrn;
}
