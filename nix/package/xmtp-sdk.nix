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
    pname: features:
    rust.buildPackage (
      common
      // wasmArgs
      // {
        inherit pname;
        src = sdkSource;
        cargoArtifacts = wasmDeps;
        buildPhaseCargoCommand =
          wasmArgs.buildPhaseCargoCommand + lib.optionalString (features != "") " --features ${features}";
        installPhaseCommand = ''
          mkdir -p $out/lib
          cp target/wasm32-unknown-unknown/release/xmtp_sdk.wasm $out/lib/
        '';
      }
    );
  wasm = mkWasm "xmtp-sdk-wasm" "";
  pureWasm = mkWasm "xmtp-sdk-pure-wasm" "pure-only";
  # Browser platform test fixtures. `conformance` adds test exports and enables
  # no third-party feature, so they reuse `wasmDeps`. WASM aborts on a panic in
  # every profile, so release fixtures keep the panic proofs' meaning.
  panicFixtureWasm = mkWasm "xmtp-sdk-wasm-fixture-panic-lib" "bridge-panic-test,conformance";
  codecFixtureWasm = mkWasm "xmtp-sdk-pure-fixture-codec-lib" "pure-only,conformance";
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
            cargo build --release --locked ${xmtp.iosEnv.nativeCargoArgs} -p xmtp_sdk --lib --target ${target}
          '';
        in
        xmtp.base.assertNativeEnv {
          macos = true;
          ios = true;
          drv = iosRust.buildPackage (
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
              # cctools ranlib records the build UID when the strip hook
              # rebuilds the index. LLVM ranlib uses deterministic headers.
              preFixup = ''
                export RANLIB=${pkgs.buildPackages.llvmPackages.llvm}/bin/llvm-ranlib
              '';
              installPhaseCommand = ''
                mkdir -p $out/lib
                cp target/${target}/release/libxmtp_sdk.a $out/lib/
                cp target/${target}/release/libxmtp_sdk.dylib $out/lib/
              '';
            }
          );
        }
      )
    );
  wrapIos =
    targets:
    lib.mapAttrs (
      target: compilation:
      (import ../lib/sdk-provenance.nix { inherit stdenvNoCC; }) {
        inherit compilation target;
        inherit (sources) provenance;
      }
    ) targets;
  iosTargets = wrapIos (mkIosTargets xmtp.iosEnv.xcode.key);
  iosTargetsLocal = wrapIos (mkIosTargets "local");
  ubrn = xmtp.ubrn;
  # Renders bind the host library product (`xmtp-sdk-libs`), the same bytes
  # that packages and receipts name.
  nativeLibrary = "${native}/lib/libxmtp_sdk.${if pkgs.stdenv.isDarwin then "dylib" else "so"}";
  renderTargets = {
    swift = "swift";
    kotlin = "kotlin";
    typescript-napi = "node";
    typescript-react-native = "react-native";
    typescript-wasm = "browser-worker";
    typescript-pure = "browser-pure";
  };
  # The runtime and configuration a render reads: its `sdkProvenance.render` key.
  renderLanguage =
    language:
    if language == "typescript-react-native" then
      "react-native"
    else if lib.hasPrefix "typescript-" language then
      "typescript"
    else
      language;
  # The @ubjs runtimes that each TypeScript tree imports.
  runtimeLinks = {
    typescript-react-native = [ "core" ];
    typescript-napi = [
      "core"
      "node"
    ];
    typescript-wasm = [
      "core"
      "wasm"
    ];
    typescript-pure = [
      "core"
      "wasm"
    ];
  };
  # One renderer writes each tree and its receipt. It takes the identities that
  # Nix computes at evaluation, and TypeScript is formatted with one nixpkgs
  # oxfmt, so the cargo route (`sdk-artifacts.py`) writes the same bytes. A
  # TypeScript tree links its @ubjs runtimes in `node_modules`, so the output
  # is their GC root and loads without a link step.
  mkRender =
    {
      pname,
      language,
      artifact,
      features ? "",
    }:
    stdenvNoCC.mkDerivation {
      inherit pname;
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
          --source ${sources.provenance.source} \
          --generator ${sources.provenance.render.${renderLanguage language}} \
          ${lib.optionalString (features != "") "--features ${features}"} --out "$out"
        ${lib.concatMapStrings (name: ''
          mkdir -p "$out/node_modules/@ubjs"
          ln -s ${ubrn.${name}}/lib/node_modules/@ubjs/${name} "$out/node_modules/@ubjs/${name}"
        '') (runtimeLinks.${language} or [ ])}
      '';
      installPhase = "true";
    };
  mkProductRender =
    language: artifact: features:
    mkRender {
      pname = "xmtp-sdk-render-${language}";
      inherit language artifact features;
    };
  renders = {
    swift = mkProductRender "swift" nativeLibrary "";
    kotlin = mkProductRender "kotlin" nativeLibrary "";
    typescript-napi = mkProductRender "typescript-napi" nativeLibrary "";
    typescript-react-native = mkProductRender "typescript-react-native" nativeLibrary "";
    typescript-wasm = mkProductRender "typescript-wasm" "${wasm}/lib/xmtp_sdk.wasm" "";
    typescript-pure = mkProductRender "typescript-pure" "${pureWasm}/lib/xmtp_sdk.wasm" "pure-only";
  };
  panicFixture = mkRender {
    pname = "xmtp-sdk-wasm-fixture-panic";
    language = "typescript-wasm";
    artifact = "${panicFixtureWasm}/lib/xmtp_sdk.wasm";
    features = "bridge-panic-test,conformance";
  };
  codecFixture = mkRender {
    pname = "xmtp-sdk-pure-fixture-codec";
    language = "typescript-pure";
    artifact = "${codecFixtureWasm}/lib/xmtp_sdk.wasm";
    features = "pure-only,conformance";
  };
  # Each tree is a copy of its render: the receipt and the @ubjs links come
  # with it.
  mkGenerated =
    name: languages:
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
  generatedSwift = mkGenerated "-swift" [ "swift" ];
  generatedKotlin = mkGenerated "-kotlin" [ "kotlin" ];
  generatedNode = mkGenerated "-node" [ "typescript-napi" ];
  generatedReactNative = mkGenerated "-react-native" [ "typescript-react-native" ];
  generatedBrowser = mkGenerated "-browser" [
    "typescript-wasm"
    "typescript-pure"
  ];
  generated = mkGenerated "" (lib.remove "typescript-react-native" (builtins.attrNames renders));
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
    generatedReactNative
    generatedBrowser
    panicFixture
    codecFixture
    iosTargets
    iosTargetsLocal
    nativeBuild
    ;
  inherit (sources) provenance;
  runtimes = ubrn;
}
