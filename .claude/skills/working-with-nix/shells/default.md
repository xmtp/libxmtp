# Default Shell (`nix develop`)

Full local development shell — superset of all other Rust-based shells. Includes combined Rust toolchain with all cross-compilation targets, Android SDK/emulator, iOS env setup (Darwin), debugging/profiling tools, and convenience packages.

**Source:** `nix/shells/local.nix`

## Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `XMTP_NIX_ENV` | `yes` | Nix environment is active |
| `XMTP_DEV_SHELL` | `local` | Identifies this shell |
| `OPENSSL_DIR` | `${openssl.dev}` | OpenSSL headers location |
| `OPENSSL_LIB_DIR` | `${openssl}/lib` | OpenSSL library location |
| `OPENSSL_NO_VENDOR` | `1` | Use system OpenSSL, don't build |
| `STACK_OVERFLOW_CHECK` | `0` | Disable stack overflow checks |
| `LD_LIBRARY_PATH` | OpenSSL, zlib paths | Runtime library search path |
| `CC_wasm32_unknown_unknown` | clang path | WASM C compiler |
| `AR_wasm32_unknown_unknown` | llvm-ar path | WASM archiver |
| `CFLAGS_wasm32_unknown_unknown` | clang include path | WASM C compiler flags |
| `ANDROID_HOME` | SDK path | Android SDK location |
| `ANDROID_SDK_ROOT` | SDK path | Android SDK (deprecated alias) |
| `ANDROID_NDK_HOME` | NDK path | Android NDK location |
| `ANDROID_NDK_ROOT` | NDK path | Android NDK (alias) |
| `NDK_HOME` | NDK path | Android NDK (alias) |
| `EMULATOR` | Script path | `run-test-emulator` script |

### Darwin-only shell hook (set dynamically)

| Variable | Value | Purpose |
|----------|-------|---------|
| `DEVELOPER_DIR` | Xcode path | Active Xcode installation |
| `IPHONEOS_DEPLOYMENT_TARGET` | `14` | Minimum iOS version |
| `CC_aarch64_apple_ios` | Xcode clang | iOS device C compiler |
| `CC_aarch64_apple_ios_sim` | Xcode clang | iOS simulator C compiler |
| `CARGO_TARGET_AARCH64_APPLE_IOS_LINKER` | Xcode clang | iOS device linker |
| `CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER` | Xcode clang | iOS simulator linker |

Note: `SDKROOT` is explicitly **unset** so xcrun discovers the right SDK per target. The `swift` command is wrapped to sanitize NIX_CFLAGS that conflict with Swift Package Manager.

## Rust Targets

- `wasm32-unknown-unknown`
- `x86_64-unknown-linux-gnu`
- `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`, `i686-linux-android`
- Darwin only: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-darwin`, `aarch64-apple-darwin`

## Rust Components

`rust-src`, `clippy-preview`, `rust-docs`, `rustfmt-preview`, `llvm-tools-preview`

## Tools Included

**Build & Test (cargoTools):**
`cargo-nextest`, `cargo-deny`, `cargo-machete`, `cargo-hakari`

**CI (cargoCiTools, Linux only):**
`lcov`, `cargo-llvm-cov`

**WASM (wasmTools):**
`wasm-bindgen-cli`, `wasm-pack`, `binaryen`, `emscripten`, `wasm-tools`

**Protobuf (protoTools):**
`buf`, `protobuf`, `protolint`

**Lint (lintTools):**
`taplo`, `shellcheck`, `nixfmt`

**Debug & Profiling (debugTools):**
`lldb`, `vscode-lldb`, `gnuplot`, `flamegraph`, `cargo-flamegraph`, `inferno`, `rr` (Linux only)

**Misc (miscDevTools):**
`jq`, `curl`, `graphite-cli`, `toxiproxy`, `omnix`

**Direct dependencies:**
`foundry-bin`, `sqlcipher`, `corepack`, `cargo-ndk`, `gnused`, `mktemp`, `diesel-cli`, `jdk21`, `jdk17`, `kotlin`, `ktlint`

**Darwin only:**
`darwin.cctools`, `swiftformat`, `swiftlint`, `kotlin-language-server`

## Xcode Version Check (Darwin)

On shell entry, warns if Xcode < 16 (required for Swift 6.1 Package Traits). Xcode path is resolved dynamically via `/usr/bin/xcode-select` to support CI runners with versioned Xcode paths.
