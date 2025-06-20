{ mkShell
, darwin
, androidenv
, stdenv
, pkg-config
, kotlin
, ktlint
, jdk17
, cargo-ndk
, sqlite
, openssl
, lib
, gnused
, mkToolchain
}:
let
  frameworks = if stdenv.isDarwin then darwin.apple_sdk.frameworks else null;
  inherit (androidComposition) androidsdk;

  android = {
    platforms = [ "33" "34" ];
    platformTools = "34.0.4";
    buildTools = [ "30.0.3" ];
  };

  androidTargets = [
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
    "i686-linux-android"
  ];

  # Pinned Rust Version
  rust-android-toolchain = mkToolchain androidTargets [ "clippy-preview" "rustfmt-preview" ];
  sdkArgs = {
    platformVersions = android.platforms;
    platformToolsVersion = android.platformTools;
    buildToolsVersions = android.buildTools;
    emulatorVersion = "34.1.9";
    systemImageTypes = [ "google_apis_playstore" "default" ];
    abiVersions = [ "x86_64" ];
    includeNDK = true;
    includeEmulator = true;
    includeSystemImages = true;
  };

  # https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/android.section.md
  androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
  androidComposition = androidenv.composeAndroidPackages sdkArgs;
  androidEmulator = androidenv.emulateApp {
    name = "libxmtp-emulator-34";
    platformVersion = "34";
    abiVersion = "x86_64"; # armeabi-v7a, mips, x86_64
    systemImageType = "default";
  };
in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  ANDROID_HOME = androidHome;
  NDK_HOME = "${androidComposition.androidsdk}/libexec/android-sdk/ndk/${builtins.head (lib.lists.reverseList (builtins.split "-" "${androidComposition.ndk-bundle}"))}";
  ANDROID_SDK_ROOT = androidHome; # ANDROID_SDK_ROOT is deprecated, but some tools may still use it;
  ANDROID_NDK_ROOT = "${androidHome}/ndk-bundle";
  EMULATOR = "${androidEmulator}";

  # Packages available to flake while building the environment
  nativeBuildInputs = [ pkg-config ];

  buildInputs = [
    rust-android-toolchain
    kotlin
    ktlint
    androidsdk
    jdk17
    cargo-ndk
    androidEmulator
    gnused # for ./dev/release-kotlin

    # System Libraries
    sqlite
    openssl
  ] ++ lib.optionals stdenv.isDarwin [
    # optional packages if on darwin, in order to check if build passes locally
    darwin.cctools
  ];
}

