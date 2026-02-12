# Shared Android cross-compilation environment configuration.
# Used by both nix/android.nix (dev shell) and nix/package/android.nix (build derivation).
{ lib, androidenv, stdenv }:
let
  # Android targets for Rust cross-compilation
  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # Host architecture -> matching Android target (for fast single-target builds)
  hostArch = stdenv.hostPlatform.parsed.cpu.name;
  hostAndroidTarget = {
    "aarch64" = "aarch64-linux-android";
    "x86_64" = "x86_64-linux-android";
  }.${hostArch} or (throw "Unsupported host architecture for Android: ${hostArch}");

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

  # Emulator configuration — used by both composeDevPackages and run-test-emulator
  # Version >= 35.3.11 required: earlier versions lack arch metadata in nixpkgs'
  # repo.json, so aarch64-darwin gets an x86_64 binary that can't run arm64 guests.
  # With 35.3.11+, nixpkgs selects the correct native binary per architecture.
  emulatorConfig = {
    platformVersion = "34";
    systemImageType = "default";
    abiVersion = if hostArch == "aarch64" then "arm64-v8a" else "x86_64";
    emulatorVersion = "35.3.11";
  };

  # Compose Android packages for dev shell (includes emulator)
  composeDevPackages = androidenv.composeAndroidPackages {
    platformVersions = sdkConfig.platforms;
    platformToolsVersion = sdkConfig.platformTools;
    buildToolsVersions = sdkConfig.buildTools;
    includeNDK = true;
    inherit (emulatorConfig) emulatorVersion;
    includeEmulator = true;
    includeSystemImages = true;
    systemImageTypes = [ emulatorConfig.systemImageType ];
    abiVersions = [ emulatorConfig.abiVersion ];
  };

  # Helper to extract paths from an android composition
  mkAndroidPaths = composition: rec {
    home = "${composition.androidsdk}/libexec/android-sdk";
    # NDK version extraction from the ndk-bundle attribute name
    ndkVersion = builtins.head (lib.lists.reverseList (builtins.split "-" "${composition.ndk-bundle}"));
    ndkHome = "${home}/ndk/${ndkVersion}";
  };

in {
  inherit androidTargets hostAndroidTarget sdkConfig emulatorConfig composeBuildPackages composeDevPackages mkAndroidPaths;
}
