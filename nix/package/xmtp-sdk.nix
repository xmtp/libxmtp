{
  lib,
  stdenvNoCC,
  xmtp,
  pkgs,
  wasm-bindgen-cli,
}:
let
  rustToolchain = xmtp.hostToolchain pkgs;
  rust = xmtp.craneLib.overrideToolchain xmtp.hostToolchain;
  hostTarget = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  sources = pkgs.callPackage ../lib/sdk-sources.nix { };
  sdkSource = sources.sdk rust;
  common = xmtp.base.commonArgs // {
    cargoVendorDir = xmtp.base.mkCargoVendorDir rust;
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
  # The one host library. On Linux it is the zigbuild glibc-floor build, so
  # renders, the host Node package, and the JNA test host share one compile.
  native = pkgs.callPackage ./xmtp-sdk-native.nix { };
  nativeBuild = native.compilation;
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
  wasmDeps = xmtp.base.mkCargoArtifacts rust wasmArgs;
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
  # The SDK library and the bindgen keep separate dependency layers. Cargo
  # unifies features across `-p xmtp_sdk -p xmtp-sdk-bindgen` (the bindgen is
  # outside hakari), so a shared layer gives libc, syn, serde_json, and others
  # features that a final `-p xmtp_sdk` build lacks, and it recompiles them.
  bindgenCommand = "cargo build --release --locked -p xmtp-sdk-bindgen";
  bindgen = rust.buildPackage (
    common
    // {
      pname = "xmtp-sdk-bindgen";
      src = sources.bindgen rust;
      cargoArtifacts = xmtp.base.mkCargoArtifacts rust { buildPhaseCargoCommand = bindgenCommand; };
      buildPhaseCargoCommand = bindgenCommand;
      installPhaseCommand = ''
          mkdir -p $out/bin
        cp target/${hostTarget}/release/xmtp-sdk-bindgen $out/bin/
      '';
    }
  );
  iosRust = (xmtp.craneLib.overrideScope (_: _: { stdenv = pkgs.stdenvNoCC; })).overrideToolchain (
    p: xmtp.mkToolchain p ([ hostTarget ] ++ xmtp.sdkPlatforms.apple) [ ]
  );
  # The slices and their dependency layer compile C code with Xcode clang, so
  # their key holds the Xcode (`xcode`): `xmtp.iosEnv.xcode.key` for the CI and
  # release outputs, which fail on another Xcode, or `local`.
  mkIosTargets =
    xcode:
    lib.optionalAttrs pkgs.stdenv.isDarwin (
      lib.genAttrs xmtp.sdkPlatforms.apple (
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
            cargoVendorDir = xmtp.base.mkCargoVendorDir iosRust;
            CARGO_BUILD_TARGET = target;
            XMTP_XCODE = xcode;
            __noChroot = true;
            cargoArtifacts = xmtp.base.mkCargoArtifacts iosRust (
              nativeArgs
              // {
                CARGO_BUILD_TARGET = target;
                XMTP_XCODE = xcode;
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
  iosTargets = mkIosTargets xmtp.iosEnv.xcode.key;
  iosTargetsLocal = mkIosTargets "local";
  ubrn = xmtp.ubrn;
  # Renders bind the host library product (`xmtp-sdk-libs`), the same bytes
  # that packages and receipts name.
  nativeLibrary = "${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"}";
  renderTargets = {
    swift = "swift";
    kotlin = "kotlin";
    typescript-napi = "node";
    typescript-wasm = "browser-worker";
    typescript-pure = "browser-pure";
  };
  # One renderer writes each tree and its receipt. It takes the identities that
  # Nix computes at evaluation, and TypeScript is formatted with one nixpkgs
  # oxfmt, so the cargo route (`sdk-artifacts.py`) writes the same bytes.
  mkRender =
    language: artifact: pure:
    stdenvNoCC.mkDerivation {
      pname = "xmtp-sdk-render-${language}";
      version = xmtp.mkVersion rust;
      src = sources.generationSource language;
      nativeBuildInputs = [
        bindgen
        rustToolchain
      ]
      ++ lib.optional (artifact != nativeLibrary) wasm-bindgen-cli;
      buildPhase = ''
        xmtp-sdk-bindgen render --lib ${artifact} --target ${renderTargets.${language}} \
          ${lib.optionalString (lib.hasPrefix "typescript-" language) "--oxfmt ${pkgs.oxfmt}/bin/oxfmt"} \
          --source ${sources.provenance.source} --generator ${sources.provenance.generator} \
          ${lib.optionalString pure "--features pure-only"} --out "$out"
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
  # Each tree is a copy of its render, with its receipt, plus links to the
  # @ubjs runtimes that staging copies.
  mkGenerated =
    name: languages: runtimeNames:
    stdenvNoCC.mkDerivation {
      pname = "xmtp-sdk-generated${name}";
      version = xmtp.mkVersion rust;
      dontUnpack = true;
      dontFixup = true;
      buildPhase = ''
        mkdir -p "$out"
        ${lib.concatMapStringsSep "\n" (language: ''
          cp -R ${renders.${language}} "$out/${language}"
        '') languages}
        ${lib.optionalString (runtimeNames != [ ]) ''
          mkdir -p "$out/runtimes"
          ${lib.concatMapStringsSep "\n" (runtime: ''
            ln -s ${ubrn.${runtime}} "$out/runtimes/${runtime}"
          '') runtimeNames}
        ''}
      '';
      installPhase = "true";
      passthru = {
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
  generatedSwift = mkGenerated "-swift" [ "swift" ] [ ];
  generatedKotlin = mkGenerated "-kotlin" [ "kotlin" ] [ ];
  generatedNode = mkGenerated "-node" [ "typescript-napi" ] [ "core" "node" ];
  generatedBrowser =
    mkGenerated "-browser"
      [
        "typescript-wasm"
        "typescript-pure"
      ]
      [ "core" "wasm" ];
  generated = mkGenerated "" (builtins.attrNames renders) [
    "core"
    "node"
    "wasm"
  ];
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
    iosTargetsLocal
    nativeBuild
    ;
  inherit (sources) provenance;
  runtimes = ubrn;
}
