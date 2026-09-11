# Android Shell (`nix develop .#android`)

For building Android bindings and running Android tests.

**Source:** `nix/shells/android.nix` + `nix/lib/android-env.nix`

## Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `XMTP_DEV_SHELL` | `android` | Identifies this shell |
| `OPENSSL_DIR` | `${openssl.dev}` | OpenSSL headers |
| `ANDROID_HOME` | SDK path | Android SDK location |
| `ANDROID_SDK_ROOT` | SDK path | Android SDK (deprecated alias) |
| `ANDROID_NDK_HOME` | NDK path | Android NDK location |
| `ANDROID_NDK_ROOT` | NDK path | Android NDK (alias) |
| `NDK_HOME` | NDK path | Android NDK (alias) |
| `EMULATOR` | Script path | `run-test-emulator` script |
| `LD_LIBRARY_PATH` | OpenSSL, zlib paths | Runtime library search path |

## Rust Targets

- `aarch64-linux-android`
- `armv7-linux-androideabi`
- `x86_64-linux-android`
- `i686-linux-android`

## Rust Components

`clippy-preview`, `rustfmt-preview`

## Android SDK Configuration

| Setting | Value |
|---------|-------|
| Platforms | 34, 35 |
| Platform Tools | 35.0.2 |
| Build Tools | 34.0.0, 35.0.0 |
| Emulator | 35.3.11 |
| System Images | `default` |
| ABI | `arm64-v8a` (aarch64 host) or `x86_64` (x86_64 host) |

## Tools Included

- `kotlin` — Kotlin compiler
- `ktlint` — Kotlin linting
- `jdk17` — Java 17 (required for Gradle)
- `cargo-ndk` — Rust Android builds
- `androidsdk` — Full Android SDK
- `gnused` — GNU sed (for release scripts)
- Darwin only: `darwin.cctools`

## Emulator

The `EMULATOR` env var points to the `run-test-emulator` custom script (not nixpkgs' `emulateApp`). This script:

- Scans ports **5560-5584** to avoid conflicts with Docker services on 5555-5558
- Creates a temporary AVD with 4GB RAM and 8GB partition
- Waits for boot completion before returning
- Sets `ANDROID_SERIAL` for subsequent `adb` commands

Usage:
```bash
run-test-emulator  # Starts emulator, blocks until ready
```
