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
  sources = pkgs.callPackage ../lib/sdk-sources.nix { };
  sdkSource = sources.sdk rust;
  mkProvenance = pkgs.callPackage ../lib/sdk-provenance.nix { };
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
  nativeBuild = rust.buildPackage (
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
  native = mkProvenance {
    compilation = nativeBuild;
    source = sources.provenanceSource;
    target = hostTarget;
  };
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
      src = sources.bindgen rust;
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
          src = sources.sdk iosRust;
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
  nativeLibrary = "${nativeBuild}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"}";
  mkRender =
    language: artifact: pure:
    stdenvNoCC.mkDerivation {
      pname = "xmtp-sdk-render-${language}";
      version = xmtp.mkVersion rust;
      src = sources.generationSource (if language == "swift" then "swift" else "all");
      nativeBuildInputs = [
        bindgen
        rustToolchain
      ]
      ++ lib.optional (artifact != nativeLibrary) wasm-bindgen-cli;
      buildPhase = ''
        # The unpacked source is writable. TypeScript generation changes copies.
        # The Nix build has no JavaScript formatter dependencies.
        xmtp-sdk-bindgen generate --lib ${artifact} \
          --language ${if pure then "typescript-wasm" else language} \
          ${lib.optionalString pure "--pure-only"} --no-format --out "$out" \
          --config apps/xmtp_sdk_bindgen/uniffi-global.toml
        ${lib.optionalString (artifact != nativeLibrary) ''
          xmtp-sdk-bindgen stage-wasm --lib ${artifact} --out "$out"
        ''}
        ${lib.optionalString (language == "typescript-napi") ''
          cp ${nativeLibrary} "$out/"
        ''}
      '';
      installPhase = "true";
    };
  renders = {
    swift = mkRender "swift" nativeLibrary false;
    kotlin = mkRender "kotlin" nativeLibrary false;
    typescript-napi = mkRender "typescript-napi" nativeLibrary false;
    typescript-wasm = mkRender "typescript-wasm" "${wasm}/lib/xmtp_sdk.wasm" false;
    typescript-pure = mkRender "typescript-pure" "${pureWasm}/lib/xmtp_sdk.wasm" true;
  };
  binaries = {
    native = "${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"}";
    bindgen = "${bindgen}/bin/xmtp-sdk-bindgen";
    wasm = "${wasm}/lib/xmtp_sdk.wasm";
    pure = "${pureWasm}/lib/xmtp_sdk.wasm";
  };
  mkGenerated =
    name: languages: roles: runtimeNames:
    stdenvNoCC.mkDerivation {
      pname = "xmtp-sdk-generated${name}";
      version = xmtp.mkVersion rust;
      src = sources.provenanceSource;
      nativeBuildInputs = [ pkgs.python3 ];
      dontFixup = true;
      buildPhase = ''
        mkdir -p "$out"
        ${lib.concatMapStringsSep "\n" (language: ''
          mkdir -p "$out/${language}"
          cp -R ${renders.${language}}/. "$out/${language}/"
        '') languages}
        chmod -R u+w "$out"
        python3 crates/xmtp_sdk/dev/record-generated.py "$out" \
          ${lib.concatMapStringsSep " " (role: "--${role} ${binaries.${role}}") roles}
        ${lib.optionalString (runtimeNames != [ ]) ''
          mkdir -p "$out/runtimes"
          ${lib.concatMapStringsSep "\n" (runtime: ''
            ln -s ${ubrn.${runtime}} "$out/runtimes/${runtime}"
          '') runtimeNames}
        ''}
      '';
      installPhase = "true";
      passthru = {
        provenanceSource = sources.provenanceSource;
        rendering =
          if builtins.length languages == 1 then
            renders.${builtins.head languages}
          else
            pkgs.linkFarm "xmtp-sdk-render${name}" (
              map (language: {
                name = language;
                path = renders.${language};
              }) languages
            );
      };
    };
  generatedSwift = mkGenerated "-swift" [ "swift" ] [ "native" "bindgen" ] [ ];
  generatedKotlin = mkGenerated "-kotlin" [ "kotlin" ] [ "native" "bindgen" ] [ ];
  generatedNode = mkGenerated "-node" [ "typescript-napi" ] [ "native" "bindgen" ] [ "core" "node" ];
  generatedBrowser = mkGenerated "-browser" [
    "typescript-wasm"
    "typescript-pure"
  ] [ "bindgen" "wasm" "pure" ] [ "core" "wasm" ];
  # Re-record all raw roots together to retain the aggregate's full contract.
  generated =
    mkGenerated "" (builtins.attrNames renders)
      [ "native" "wasm" "pure" "bindgen" ]
      [ "core" "node" "wasm" ];
in
{
  libs = native;
  inherit
    wasm
    pureWasm
    bindgen
    generated
    generatedSwift
    generatedKotlin
    generatedNode
    generatedBrowser
    iosTargets
    nativeBuild
    ;
  generationSource = sources.generationSource;
  inherit (sources)
    provenanceSource
    ;
  runtimes = ubrn;
}
