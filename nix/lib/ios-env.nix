# Shared iOS cross-compilation environment configuration.
# Used by both nix/ios.nix (dev shell) and nix/package/ios.nix (build derivation).
#
# Key insight: /usr/bin/clang is an xcode-select shim that reads DEVELOPER_DIR.
# Nix's stdenv overrides DEVELOPER_DIR to its own apple-sdk, causing the shim
# to dispatch to Nix's cc-wrapper (which injects -mmacos-version-min, breaking
# iOS builds). We bypass this entirely by using the full Xcode toolchain clang path.
{ lib }:
let
  iosTargets = [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "aarch64-apple-ios"
    "aarch64-apple-ios-sim"
  ];

  # Xcode paths
  developerDir = "/Applications/Xcode.app/Contents/Developer";
  iosSdk = "${developerDir}/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk";
  iosSimSdk = "${developerDir}/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk";
  macSdk = "${developerDir}/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk";

  # Direct Xcode toolchain clang — bypasses the /usr/bin/clang shim entirely.
  xcodeClang = "${developerDir}/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang";
  xcodeClangxx = "${developerDir}/Toolchains/XcodeDefault.xctoolchain/usr/bin/clang++";

  # Map target triple to the correct Xcode SDK path.
  # cc-rs uses SDKROOT directly when set, bypassing xcrun entirely.
  # This is necessary because Nix provides its own xcrun (from xcbuild)
  # that doesn't support iOS SDKs.
  sdkrootForTarget = target: {
    "aarch64-apple-ios" = iosSdk;
    "aarch64-apple-ios-sim" = iosSimSdk;
    "x86_64-apple-darwin" = macSdk;
    "aarch64-apple-darwin" = macSdk;
  }.${target};

  isIosTarget = target: builtins.elem target [ "aarch64-apple-ios" "aarch64-apple-ios-sim" ];

  # Cargo/cc-rs environment variables for iOS cross-compilation.
  # Can be used as derivation attrs or exported in shell hooks.
  envVars = {
    DEVELOPER_DIR = developerDir;
    IPHONEOS_DEPLOYMENT_TARGET = "14";
    CC_aarch64_apple_ios = xcodeClang;
    CXX_aarch64_apple_ios = xcodeClangxx;
    CC_aarch64_apple_ios_sim = xcodeClang;
    CXX_aarch64_apple_ios_sim = xcodeClangxx;
    CARGO_TARGET_AARCH64_APPLE_IOS_LINKER = xcodeClang;
    CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER = xcodeClang;
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios = "--target=arm64-apple-ios --sysroot=${iosSdk}";
    BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim = "--target=arm64-apple-ios-simulator --sysroot=${iosSimSdk}";
  };

  # Shell snippet that overrides Nix's build environment for a specific target.
  # Must run as shell code because Nix's apple-sdk setup hook sets DEVELOPER_DIR
  # and SDKROOT AFTER env var assignment. For iOS targets, also bypasses Nix's
  # cc-wrapper which injects conflicting macOS flags.
  #
  # In buildDepsOnly: inline in buildPhaseCargoCommand (preBuild is stripped by crane).
  # In buildPackage: use as preBuild hook.
  envSetup = target: ''
    export DEVELOPER_DIR="${developerDir}"
    export SDKROOT="${sdkrootForTarget target}"
    export PATH="${developerDir}/usr/bin:$PATH"
  '' + lib.optionalString (isIosTarget target) ''
    export CC="${xcodeClang}"
    export CXX="${xcodeClangxx}"
  '';

in {
  inherit
    iosTargets
    developerDir
    iosSdk
    iosSimSdk
    macSdk
    xcodeClang
    xcodeClangxx
    sdkrootForTarget
    isIosTarget
    envVars
    envSetup;
}
