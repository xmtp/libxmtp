# iOS Shell (`nix develop .#ios`)

For building iOS bindings. **macOS only.**

**Source:** `nix/shells/ios.nix` + `nix/lib/ios-env.nix`

## Environment Variables

Set statically by `mkShell`:

| Variable | Value | Purpose |
|----------|-------|---------|
| `XMTP_DEV_SHELL` | `ios` (set in shellHook) | Identifies this shell |

Set dynamically in `shellHook` (resolved from Xcode at shell entry):

| Variable | Value | Purpose |
|----------|-------|---------|
| `DEVELOPER_DIR` | Xcode path | Active Xcode installation |
| `IPHONEOS_DEPLOYMENT_TARGET` | `14` | Minimum iOS version |
| `CC_aarch64_apple_ios` | Xcode clang | iOS device C compiler |
| `CXX_aarch64_apple_ios` | Xcode clang++ | iOS device C++ compiler |
| `CC_aarch64_apple_ios_sim` | Xcode clang | iOS simulator C compiler |
| `CXX_aarch64_apple_ios_sim` | Xcode clang++ | iOS simulator C++ compiler |
| `CARGO_TARGET_AARCH64_APPLE_IOS_LINKER` | Xcode clang | iOS device linker |
| `CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER` | Xcode clang | iOS simulator linker |
| `BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios` | sysroot args | Bindgen iOS device args |
| `BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim` | sysroot args | Bindgen iOS sim args |

Note: `SDKROOT` is explicitly **unset** so xcrun discovers the right SDK per target at build time.

## Rust Targets

- `x86_64-apple-darwin` — macOS Intel (universal binary)
- `aarch64-apple-darwin` — macOS Apple Silicon (universal binary)
- `aarch64-apple-ios` — iOS device (arm64)
- `aarch64-apple-ios-sim` — iOS simulator on Apple Silicon

## Rust Components

`clippy-preview`, `rustfmt-preview`

## Tools Included

- `rust-ios-toolchain` — Rust with all iOS/macOS targets
- `zstd` — Compression library
- `openssl` — OpenSSL
- `sqlite` — SQLite database
- `swiftformat` — Swift code formatter
- `swiftlint` — Swift linter
- Darwin only: `darwin.cctools` — provides `lipo` for universal binaries

## Hardening

`hardeningDisable = ["zerocallusedregs"]` — Nix's default hardening uses a calling convention that Xcode's clang doesn't support, causing "unknown flag" errors during iOS cross-compilation.

## Dynamic Xcode Resolution

Xcode path is resolved dynamically at shell entry via `/usr/bin/xcode-select`. This ensures CI runners using `setup-xcode` (which installs to versioned paths like `/Applications/Xcode_26.1.1.app`) get the correct toolchain automatically.

**Key insight:** `/usr/bin/clang` is an xcode-select shim that reads `DEVELOPER_DIR`. Nix's stdenv overrides `DEVELOPER_DIR` to its own apple-sdk, causing the shim to dispatch to Nix's cc-wrapper (which injects `-mmacos-version-min`, breaking iOS builds). The iOS shell bypasses this by setting CC/CXX to the full Xcode toolchain clang path.

## Xcode Version Check

On shell entry, warns if Xcode < 16 (required for Swift 6.1 Package Traits).

## Platform Restriction

This shell only builds on Darwin (macOS). Attempting to enter on Linux will fail with missing Darwin-specific dependencies.
