# Slim Android consumer shells for CI. The Android jobs compile no Rust: they
# take the Android libraries from the products job or substitute them, so the
# shells hold no Rust toolchain. They keep the NDK, which Gradle uses to strip
# the JNI libraries and the package check uses to read them. They leave out
# Kotlin and ktlint (Gradle brings its own) but keep their JDK 21.
#
#   android-ci           SDK platforms and build tools (build jobs, release)
#   android-ci-emulator  plus the emulator and its system image
{
  mkShell,
  lib,
  jdk21,
  postgresql_18,
  grpc-health-probe,
  python311,
  just,
  xmtp,
  emulator ? false,
}:
let
  inherit (xmtp) androidEnv;
  composition = if emulator then androidEnv.ciEmulatorComposition else androidEnv.ciComposition;
  paths = androidEnv.mkAndroidPaths composition;
in
mkShell (
  {
    meta.description = "Android CI consumer environment${lib.optionalString emulator " with the emulator"}";

    XMTP_DEV_SHELL = if emulator then "android-ci-emulator" else "android-ci";
    ANDROID_HOME = paths.home;
    ANDROID_SDK_ROOT = paths.home;
    ANDROID_NDK_HOME = paths.ndkHome;
    ANDROID_NDK_ROOT = paths.ndkHome;

    buildInputs = [
      composition.androidsdk
      # The JDK that Gradle has run on in CI and release: the `android`
      # shell's Kotlin puts JDK 21 first in PATH and JAVA_HOME.
      jdk21
      # `psql` for the Messenger metadata fixtures.
      postgresql_18
      grpc-health-probe
      (python311.withPackages (packages: [ packages.pyyaml ]))
      just
    ]
    ++ lib.optionals emulator [ androidEnv.ciEmulator ];
  }
  // lib.optionalAttrs emulator {
    EMULATOR = "${androidEnv.ciEmulator}";
  }
)
