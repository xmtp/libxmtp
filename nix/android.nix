{ mkShell
, darwin
, androidenv
, stdenv
, pkg-config
, kotlin
, jdk17
, cargo-ndk
, sqlite
, openssl
, lib
, fenix
}:
let
  frameworks = if stdenv.isDarwin then darwin.apple_sdk.frameworks else null;
  inherit (androidComposition) androidsdk;

  android = {
    platforms = [ "34" ];
    platformTools = "33.0.3";
    buildTools = [ "30.0.3" ];
  };

  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # Pinned Rust Version
  rust-toolchain = fenix.combine [
    fenix.stable.cargo
    fenix.stable.rustc
    (lib.forEach
      androidTargets
      (target: fenix.targets."${target}".stable.rust-std))
  ];

  sdkArgs = {
    platformVersions = android.platforms;
    platformToolsVersion = android.platformTools;
    buildToolsVersions = android.buildTools;
    includeNDK = true;
  };

  # https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/android.section.md
  androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
  androidComposition = androidenv.composeAndroidPackages sdkArgs;

in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  ANDROID_HOME = androidHome;
  ANDROID_SDK_ROOT = androidHome; # ANDROID_SDK_ROOT is deprecated, but some tools may still use it;
  ANDROID_NDK_ROOT = "${androidHome}/ndk-bundle";

  # Packages available to flake while building the environment
  nativeBuildInputs = [ pkg-config ];

  buildInputs = [
    rust-toolchain
    kotlin
    androidsdk
    jdk17
    cargo-ndk

    # System Libraries
    sqlite
    openssl
  ] ++ lib.optionals stdenv.isDarwin [
    # optional packages if on darwin, in order to check if build passes locally
    frameworks.CoreServices
    frameworks.Carbon
    frameworks.ApplicationServices
    frameworks.AppKit
    darwin.cctools
  ];
}

