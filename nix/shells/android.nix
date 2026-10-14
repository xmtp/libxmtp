# Local Android work: the Rust core packages and environment of the `rust`
# shell (not its CI and SDK tools), plus the Android SDK, NDK, emulator,
# Kotlin, ktlint, JDK 21 (Gradle's JDK; the build targets Java 17), and
# PostgreSQL for `psql`. CI Android jobs use the slim `android-ci` shells.
{
  mkShell,
  stdenv,
  postgresql_18,
  grpc-health-probe,
  kotlin,
  ktlint,
  jdk21,
  lib,
  gnused,
  xmtp,
}:
let
  inherit (xmtp) androidEnv shellCommon;
in
mkShell (
  shellCommon.rustShellEnv
  // {
    meta.description = "Android Development environment for Android SDK and Emulator";

    shellHook = shellCommon.rustShellHook;

    XMTP_DEV_SHELL = "android";

    ANDROID_HOME = androidEnv.devPaths.home;
    ANDROID_SDK_ROOT = androidEnv.devPaths.home;
    ANDROID_NDK_HOME = androidEnv.devPaths.ndkHome;
    ANDROID_NDK_ROOT = androidEnv.devPaths.ndkHome;
    NDK_HOME = androidEnv.devPaths.ndkHome;

    inherit (shellCommon.rustBase) nativeBuildInputs;
    buildInputs = shellCommon.rustCorePackages ++ [
      # Gradle's JDK: Kotlin propagated JDK 21 to the front of PATH and
      # JAVA_HOME in this shell, and CI and release Gradle builds ran on it.
      jdk21
      (shellCommon.mkTools "android" (
        [
          kotlin
          ktlint
          androidEnv.devComposition.androidsdk
          gnused
          # `psql` for the Messenger metadata fixtures.
          postgresql_18
          grpc-health-probe
        ]
        ++ lib.optionals androidEnv.hasEmulator [ androidEnv.emulator ]
      ))
    ];
  }
  // lib.optionalAttrs androidEnv.hasEmulator {
    EMULATOR = "${androidEnv.emulator}";
  }
)
