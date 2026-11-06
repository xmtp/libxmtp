# The shipped Apple package uses the same SDK artifact and generator as conformance.
{
  lib,
  stdenv,
  pkgs,
  xmtp,
  sdk,
  # true: the -local outputs, for a Mac whose Xcode is not `xmtp.iosEnv.xcode`.
  # They skip the Xcode check, and caches do not hold them.
  local ? false,
  ...
}:
let
  version = "8.0.0";
  host = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  xcode = if local then "local" else xmtp.iosEnv.xcode.key;
  slices = if local then sdk.iosTargetsLocal else sdk.iosTargets;
  # The host slice uses the Nix toolchain, not Xcode, so it has no Xcode key.
  native = target: if target == host then sdk.libs else slices.${target};
  swiftBindings = stdenv.mkDerivation {
    pname = "xmtp-sdk-swift";
    inherit version;
    dontUnpack = true;
    buildPhase = ''
      mkdir -p "$out/swift"
      cp -R ${sdk.generatedSwift}/swift/. "$out/swift/"
      chmod -R u+w "$out/swift"
      mkdir -p "$out/swift/include"
      cp "$out/swift/xmtp_sdkFFI.h" "$out/swift/include/"
      cp "$out/swift/xmtp_sdkFFI.modulemap" "$out/swift/include/module.modulemap"
    '';
    installPhase = "true";
    passthru = {
      rendering = sdk.generatedSwift.rendering;
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
      # xcodebuild assembles the framework, so its key holds the Xcode too.
      XMTP_XCODE = xcode;
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
  allTargets = [ host ] ++ xmtp.sdkPlatforms.apple;
  # The fast framework adds the simulator slice of the build host's CPU.
  hostSimulators = builtins.filter (
    target:
    lib.hasPrefix "${pkgs.stdenv.hostPlatform.parsed.cpu.name}-" target && lib.hasSuffix "-sim" target
  ) xmtp.sdkPlatforms.apple;
  fastTargets =
    assert lib.assertMsg (
      builtins.length hostSimulators == 1
    ) "nix/sdk-platforms.json needs one Apple simulator target for ${host}";
    [ host ] ++ hostSimulators;
  releaseFramework = framework allTargets;
in
{
  inherit swiftBindings;
  devFast = framework fastTargets;
  release = stdenv.mkDerivation {
    pname = "xmtp-sdk-ios-release";
    inherit version;
    # Only copies. The key marks it as an iOS output of this Xcode, so the
    # rebuild command in sdks/ios/AGENTS.md finds the `local` one.
    XMTP_XCODE = xcode;
    dontUnpack = true;
    installPhase = ''
      mkdir -p "$out/XmtpSdkFFI/Sources/XmtpSdk"
      cp -r ${releaseFramework}/XmtpSdkFFI.xcframework "$out/XmtpSdkFFI/"
      cp ${swiftBindings}/swift/xmtp_sdk.swift "$out/XmtpSdkFFI/Sources/XmtpSdk/"
      cp -r ${swiftBindings}/swift/runtime "$out/XmtpSdkFFI/Sources/XmtpSdk/"
      cp ${../../sdks/ios/Sources/XmtpSdk/AppleLogSink.swift} "$out/XmtpSdkFFI/Sources/XmtpSdk/AppleLogSink.swift"
      cp ${../../LICENSE} "$out/XmtpSdkFFI/LICENSE"
    '';
  };
}
