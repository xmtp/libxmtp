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
, perl
, xmtp
}:
let
  # Shared Android environment configuration
  androidEnv = import ./lib/android-env.nix { inherit lib androidenv; };

  # Use dev composition (includes emulator)
  androidComposition = androidEnv.composeDevPackages;
  androidPaths = androidEnv.mkAndroidPaths androidComposition;

  # Rust toolchain with Android cross-compilation targets
  rust-android-toolchain = xmtp.mkToolchain androidEnv.androidTargets [ "clippy-preview" "rustfmt-preview" ];

  # Emulator for local testing
  androidEmulator = androidenv.emulateApp {
    name = "libxmtp-emulator-34";
    platformVersion = "34";
    abiVersion = "x86_64";
    systemImageType = "default";
  };
in
mkShell {
  meta.description = "Android Development environment for Android SDK and Emulator";

  XMTP_DEV_SHELL = "android";
  OPENSSL_DIR = "${openssl.dev}";
  ANDROID_HOME = androidPaths.home;
  ANDROID_SDK_ROOT = androidPaths.home;
  ANDROID_NDK_HOME = androidPaths.ndkHome;
  ANDROID_NDK_ROOT = androidPaths.ndkHome;
  NDK_HOME = androidPaths.ndkHome;
  EMULATOR = "${androidEmulator}";

  nativeBuildInputs = [ pkg-config ];

  buildInputs = [
    rust-android-toolchain
    kotlin
    ktlint
    androidComposition.androidsdk
    jdk17
    cargo-ndk
    androidEmulator
    gnused
    perl
    sqlite
    openssl
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.cctools
  ];
}
