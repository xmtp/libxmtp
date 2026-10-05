# The shipped Apple package uses the same SDK artifact and generator as conformance.
{
  lib,
  stdenv,
  pkgs,
  xmtp,
  ...
}:
let
  sdk = pkgs.callPackage ./xmtp-sdk.nix { };
  version = "8.0.0";
  host = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  native = target: if target == host then sdk.libs else sdk.iosTargets.${target};
  swiftBindings = stdenv.mkDerivation {
    pname = "xmtp-sdk-swift";
    inherit version;
    src = ../..;
    nativeBuildInputs = [
      sdk.bindgen
      (xmtp.mkNativeToolchain [ ] [ ])
    ];
    buildPhase = ''
      xmtp-sdk-bindgen generate --lib ${sdk.libs}/lib/libxmtp_sdk.dylib \
        --language swift --no-format --out "$out/swift" \
        --config apps/xmtp_sdk_bindgen/uniffi-global.toml
      mkdir -p "$out/swift/include"
      cp "$out/swift/xmtp_sdkFFI.h" "$out/swift/include/"
      cp "$out/swift/xmtp_sdkFFI.modulemap" "$out/swift/include/module.modulemap"
    '';
    installPhase = "true";
  };
  mkIos = targetList: {
    targets = lib.genAttrs targetList native;
    inherit swiftBindings;
    aggregate = stdenv.mkDerivation {
      pname = "xmtp-sdk-apple-libs";
      inherit version;
      dontUnpack = true;
      installPhase = ''
        mkdir -p "$out/swift"
        ${lib.concatMapStringsSep "\n" (target: ''
          mkdir -p "$out/${target}"
          ln -s ${native target}/lib/libxmtp_sdk.a "$out/${target}/libxmtp_sdk.a"
        '') targetList}
        cp -r ${swiftBindings}/swift/. "$out/swift/"
      '';
    };
  };
  framework =
    targetList:
    stdenv.mkDerivation {
      pname = "xmtp-sdk-apple-xcframework";
      inherit version;
      dontUnpack = true;
      dontFixup = true;
      __noChroot = true;
      installPhase = ''
        ${xmtp.iosEnv.envSetup host}
        export PATH="$_XCODE_DEV/Toolchains/XcodeDefault.xctoolchain/usr/bin:/usr/bin:$PATH"
        mkdir -p "$out"
        xcodebuild -create-xcframework \
          ${
            lib.concatMapStringsSep " \\\n        " (
              target: "-library ${native target}/lib/libxmtp_sdk.a -headers ${swiftBindings}/swift/include"
            ) targetList
          } \
          -output "$out/XmtpSdkFFI.xcframework"
        cp -r ${swiftBindings}/swift "$out/swift"
        test -f "$out/XmtpSdkFFI.xcframework/Info.plist"
        for slice in "$out/XmtpSdkFFI.xcframework"/*/; do
          test -f "$slice/Headers/xmtp_sdkFFI.h"
          test -f "$slice/Headers/module.modulemap"
        done
      '';
    };
  allTargets = [
    host
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
  ];
  fastTargets = [
    host
    "aarch64-apple-ios-sim"
  ];
  releaseFramework = framework allTargets;
in
{
  inherit mkIos swiftBindings;
  inherit (mkIos allTargets) targets aggregate;
  devFast = framework fastTargets;
  release = stdenv.mkDerivation {
    pname = "xmtp-sdk-ios-release";
    inherit version;
    dontUnpack = true;
    installPhase = ''
      mkdir -p "$out/XmtpSdkFFI/Sources/XmtpSdk"
      cp -r ${releaseFramework}/XmtpSdkFFI.xcframework "$out/XmtpSdkFFI/"
      cp ${swiftBindings}/swift/xmtp_sdk.swift "$out/XmtpSdkFFI/Sources/XmtpSdk/"
      cp -r ${swiftBindings}/swift/runtime "$out/XmtpSdkFFI/Sources/XmtpSdk/"
      cp ${../../sdks/ios/Sources/XmtpSdk/AppleLogSink.swift} "$out/XmtpSdkFFI/Sources/XmtpSdk/"
      cp ${../../LICENSE} "$out/XmtpSdkFFI/LICENSE"
    '';
  };
}
