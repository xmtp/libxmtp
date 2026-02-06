# Shared Android cross-compilation environment configuration.
# Used by both nix/android.nix (dev shell) and nix/package/android.nix (build derivation).
{ lib, androidenv }:
let
  # Android targets for Rust cross-compilation
  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # SDK configuration - keep in sync with sdks/android/library/build.gradle
  # Library: compileSdk 35, Example: compileSdk 34
  # Gradle auto-selects buildTools matching compileSdk when not specified.
  sdkConfig = {
    platforms = [ "34" "35" ];
    platformTools = "35.0.2";
    buildTools = [ "34.0.0" "35.0.0" ];
  };

  # Compose Android packages for builds (minimal - no emulator)
  composeBuildPackages = androidenv.composeAndroidPackages {
    platformVersions = sdkConfig.platforms;
    platformToolsVersion = sdkConfig.platformTools;
    buildToolsVersions = sdkConfig.buildTools;
    includeNDK = true;
  };

  # Compose Android packages for dev shell (includes emulator)
  composeDevPackages = androidenv.composeAndroidPackages {
    platformVersions = sdkConfig.platforms;
    platformToolsVersion = sdkConfig.platformTools;
    buildToolsVersions = sdkConfig.buildTools;
    includeNDK = true;
    # Emulator config - optional for dev shell
    emulatorVersion = "34.1.19";
    includeEmulator = true;
    includeSystemImages = true;
    systemImageTypes = [ "default" ];
    abiVersions = [ "x86_64" ];
  };

  # Helper to extract paths from an android composition
  mkAndroidPaths = composition: rec {
    home = "${composition.androidsdk}/libexec/android-sdk";
    # NDK version extraction from the ndk-bundle attribute name
    ndkVersion = builtins.head (lib.lists.reverseList (builtins.split "-" "${composition.ndk-bundle}"));
    ndkHome = "${home}/ndk/${ndkVersion}";
  };

in {
  inherit androidTargets sdkConfig composeBuildPackages composeDevPackages mkAndroidPaths;
}
