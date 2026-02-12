{ mkShell
, darwin
, stdenv
, kotlin
, ktlint
, jdk17
, cargo-ndk
, openssl
, lib
, gnused
, xmtp
, zlib
}:
let
  inherit (xmtp) androidEnv mobile shellCommon;

  # Rust toolchain with Android cross-compilation targets
  rust-android-toolchain = xmtp.mkToolchain androidEnv.androidTargets [ "clippy-preview" "rustfmt-preview" ];
in
mkShell {
  meta.description = "Android Development environment for Android SDK and Emulator";

  XMTP_DEV_SHELL = "android";
  OPENSSL_DIR = shellCommon.rustBase.env.OPENSSL_DIR;
  ANDROID_HOME = androidEnv.devPaths.home;
  ANDROID_SDK_ROOT = androidEnv.devPaths.home;
  ANDROID_NDK_HOME = androidEnv.devPaths.ndkHome;
  ANDROID_NDK_ROOT = androidEnv.devPaths.ndkHome;
  NDK_HOME = androidEnv.devPaths.ndkHome;
  EMULATOR = "${androidEnv.emulator}";
  LD_LIBRARY_PATH = lib.makeLibraryPath [ openssl zlib ];

  inherit (mobile.commonArgs) nativeBuildInputs;

  buildInputs = mobile.commonArgs.buildInputs ++ [
    rust-android-toolchain
    kotlin
    ktlint
    androidEnv.devComposition.androidsdk
    jdk17
    cargo-ndk
    androidEnv.emulator
    gnused
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.cctools
  ];
}
