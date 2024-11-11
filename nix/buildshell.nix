# This is a nix development shell that can be used to build android packages imperatively
# Swift package management is not currently supported (its a bit more complex)
# Android is nice because the NDK contains all the linkers and compilations tools
# for each platform thats being compiled for. `cargo-ndk` then makes it even easier.
{ lib
, androidenv
, stdenv
, mkShell
, pkg-config
, fenix
, darwin
, kotlin
, jdk17
, cargo-ndk
, sqlite
, openssl
, libiconv
, ...
}:

let
  inherit (stdenv) isDarwin;
  inherit (androidComposition) androidsdk;
  frameworks = if isDarwin then darwin.apple_sdk.frameworks else null;

  # Pinned Rust Version
  rust-toolchain = fenix.fromToolchainFile {
    file = ./../rust-toolchain;
    sha256 = "sha256-yMuSb5eQPO/bHv+Bcf/US8LVMbf/G/0MSfiPwBhiPpk=";
  };

  android = {
    platforms = [ "34" ];
    platformTools = "33.0.3";
    buildTools = [ "30.0.3" ];
  };

  # https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/android.section.md
  androidHome = "${androidComposition.androidsdk}/libexec/android-sdk";
  androidComposition = androidenv.composeAndroidPackages sdkArgs;

  sdkArgs = {
    platformVersions = android.platforms;
    platformToolsVersion = android.platformTools;
    buildToolsVersions = android.buildTools;
    includeNDK = true;
  };
  # Packages available to flake while building the environment
  nativeBuildInputs = [ pkg-config ];
  # Define the packages available to the build environment
  # https://search.nixos.org/packages
  buildInputs = [
    rust-toolchain
    kotlin
    androidsdk
    jdk17
    cargo-ndk

    # System Libraries
    sqlite
    openssl
  ] ++ lib.optionals isDarwin [
    # optional packages if on darwin, in order to check if build passes locally
    libiconv
    frameworks.CoreServices
    frameworks.Carbon
    frameworks.ApplicationServices
    frameworks.AppKit
    darwin.cctools
  ];
in
mkShell {
  OPENSSL_DIR = "${openssl.dev}";
  ANDROID_HOME = androidHome;
  ANDROID_SDK_ROOT = androidHome; # ANDROID_SDK_ROOT is deprecated, but some tools may still use it;
  ANDROID_NDK_ROOT = "${androidHome}/ndk-bundle";

  inherit buildInputs nativeBuildInputs;
}
