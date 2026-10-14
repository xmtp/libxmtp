# Android Shells

## `android` (local Android work)

The Rust core packages and environment (`rustCorePackages`,
`rustShellEnv`) plus the Android SDK (platforms 34 and 35, build tools), the
NDK, the emulator and its system image (where the host has one), JDK 17, Kotlin
(which puts its JDK 21 first, so Gradle runs on 21), ktlint, PostgreSQL for `psql`, and
`grpc-health-probe`. It sets `ANDROID_HOME`, `ANDROID_SDK_ROOT`,
`ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, `NDK_HOME`, and `EMULATOR`.
`XMTP_DEV_SHELL=android`. Source: `nix/shells/android.nix`.

## `android-ci` and `android-ci-emulator` (CI, Linux only)

Slim consumer shells for the Android CI jobs and the Android release, which
compile no Rust: the Android SDK and NDK (Gradle strips the JNI libraries with
it, and `sdks/android/dev/check-native-packages.py` reads them), JDK 21 (the
JDK Gradle has run on),
PostgreSQL for `psql`, `grpc-health-probe`, Python 3.11, and `just`. No Rust
toolchain, no Kotlin or ktlint. `android-ci-emulator` adds
the emulator, its system image, and `run-test-emulator`. Source:
`nix/shells/android-ci.nix`.

The emulator launcher (`run-test-emulator`) comes from
`nix/lib/android-env.nix` (`mkEmulator`).
