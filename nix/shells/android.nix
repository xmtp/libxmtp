{
  mkShell,
  darwin,
  stdenv,
  git,
  kotlin,
  ktlint,
  jdk17,
  openssl,
  lib,
  gnused,
  xmtp,
  zlib,
}:
let
  inherit (xmtp) androidEnv base shellCommon;

  # Rust toolchain with Android cross-compilation targets
  rust-android-toolchain = xmtp.mkNativeToolchain androidEnv.androidTargets [
    "clippy-preview"
    "rustfmt-preview"
  ];
in
mkShell (
  {
    meta.description = "Android Development environment for Android SDK and Emulator";

    XMTP_DEV_SHELL = "android";
    OPENSSL_DIR = shellCommon.rustBase.env.OPENSSL_DIR;
    ANDROID_HOME = androidEnv.devPaths.home;
    ANDROID_SDK_ROOT = androidEnv.devPaths.home;
    ANDROID_NDK_HOME = androidEnv.devPaths.ndkHome;
    ANDROID_NDK_ROOT = androidEnv.devPaths.ndkHome;
    NDK_HOME = androidEnv.devPaths.ndkHome;
    LD_LIBRARY_PATH = lib.makeLibraryPath [
      openssl
      zlib
    ];

    # rustBase, not commonArgs: carries the nix git needed for eval-time
    # builtins.fetchGit under the shell's LD_LIBRARY_PATH.
    inherit (shellCommon.rustBase) nativeBuildInputs;

    buildInputs =
      base.commonArgs.buildInputs
      ++ [
        rust-android-toolchain
        kotlin
        ktlint
        androidEnv.devComposition.androidsdk
        jdk17
        gnused
        # in-shell nix eval spawns git (fetchGit); system git crashes against the nix-store libs in LD_LIBRARY_PATH
        git
      ]
      ++ lib.optionals androidEnv.hasEmulator [
        androidEnv.emulator
      ]
      ++ lib.optionals stdenv.isDarwin [
        darwin.cctools
      ];
  }
  // lib.optionalAttrs androidEnv.hasEmulator {
    EMULATOR = "${androidEnv.emulator}";
  }
)
