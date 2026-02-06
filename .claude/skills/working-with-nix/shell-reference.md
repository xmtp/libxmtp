# Shell Reference

Detailed tool inventories for each libxmtp Nix development shell.

## Default Shell (`nix develop`)

The general-purpose Rust development shell with comprehensive tooling.

### Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `XMTP_NIX_ENV` | `yes` | Indicates Nix environment is active |
| `OPENSSL_DIR` | `${openssl.dev}` | OpenSSL headers location |
| `OPENSSL_LIB_DIR` | `${openssl}/lib` | OpenSSL library location |
| `OPENSSL_NO_VENDOR` | `1` | Use system OpenSSL, don't build |
| `LD_LIBRARY_PATH` | OpenSSL, zlib paths | Runtime library search path |
| `CC_wasm32_unknown_unknown` | clang path | WASM C compiler |
| `AR_wasm32_unknown_unknown` | llvm-ar path | WASM archiver |

### Rust Targets

- `wasm32-unknown-unknown`
- `x86_64-unknown-linux-gnu`

### Rust Components

- `rust-src`
- `clippy-preview`
- `rust-docs`
- `rustfmt-preview`
- `llvm-tools-preview`

### Tools Included

**Build & Test:**
- `cargo-nextest` - Better test runner
- `cargo-deny` - Dependency auditing
- `cargo-machete` - Unused dependency detection
- `diesel-cli` - Database migrations

**Profiling:**
- `gnuplot` - Plotting for benchmarks
- `flamegraph` - Flame graph generation
- `cargo-flamegraph` - Integrated flame graphs
- `inferno` - Alternative flame graph tool
- `cargo-llvm-cov` (Linux only) - Code coverage

**Debugging:**
- `lldb` - LLVM debugger
- `rr` (Linux only) - Record/replay debugger
- `vscode-lldb` - VS Code debugging extension

**WASM:**
- `wasm-bindgen-cli` - WASM bindings generator
- `wasm-pack` - WASM packaging tool
- `binaryen` - WASM optimization
- `wasm-tools` - WASM inspection
- `emscripten` - Emscripten toolchain

**Protocol & API:**
- `buf` - Protocol buffer tooling
- `protobuf` - Protocol buffers
- `protolint` - Protocol buffer linting

**Blockchain:**
- `foundry-bin` - Ethereum development tools

**Misc:**
- `jdk21` - Java Development Kit
- `kotlin` - Kotlin compiler
- `graphite-cli` - Stacked PRs
- `toxiproxy` - Network fault injection
- `corepack` - Node.js package manager wrapper
- `taplo` - TOML formatting
- `shellcheck` - Shell script linting
- `lcov` - Code coverage reporting
- `omnix` - Nix tooling

---

## Android Shell (`nix develop .#android`)

For building Android bindings and running Android tests.

### Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `ANDROID_HOME` | SDK path | Android SDK location |
| `ANDROID_SDK_ROOT` | SDK path | Deprecated alias |
| `NDK_HOME` | NDK path | Android NDK location |
| `ANDROID_NDK_ROOT` | NDK bundle path | NDK bundle location |
| `EMULATOR` | Emulator path | Android emulator |
| `OPENSSL_DIR` | `${openssl.dev}` | OpenSSL headers |

### Rust Targets

- `aarch64-linux-android`
- `armv7-linux-androideabi`
- `x86_64-linux-android`
- `i686-linux-android`

### Rust Components

- `clippy-preview`
- `rustfmt-preview`

### Android Configuration

- **Platforms:** 33, 34
- **Platform Tools:** 35.0.2
- **Build Tools:** 30.0.3
- **Emulator:** 34.1.19
- **System Images:** google_apis_playstore, default
- **ABI Versions:** x86_64

### Tools Included

- `kotlin` - Kotlin compiler
- `ktlint` - Kotlin linting
- `jdk17` - Java 17 (required for Gradle)
- `cargo-ndk` - Rust Android builds
- `androidsdk` - Full Android SDK
- `androidEmulator` - Pre-configured emulator
- `gnused` - GNU sed (for release scripts)
- `perl` - Perl (for OpenSSL)

---

## iOS Shell (`nix develop .#ios`)

For building iOS bindings. **macOS only.**

### Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `OPENSSL_DIR` | `${openssl.dev}` | OpenSSL headers |
| `OPENSSL_LIB_DIR` | `${openssl}/lib` | OpenSSL libraries |
| `OPENSSL_NO_VENDOR` | `1` | Use system OpenSSL |
| `LLVM_PATH` | LLVM stdenv path | LLVM toolchain |

### Rust Targets

- `x86_64-apple-darwin`
- `aarch64-apple-ios`
- `x86_64-apple-ios`
- `aarch64-apple-ios-sim`

### Rust Components

- `clippy-preview`
- `rustfmt-preview`

### Tools Included

- `xcbuild` - Xcode build system
- `darwin.cctools` - Apple compiler tools
- `llvmPackages_19` - LLVM 19 toolchain
- `zstd` - Compression library
- `openssl` - OpenSSL
- `sqlite` - SQLite database

### Platform Restriction

This shell will only build on Darwin (macOS). Attempting to enter on Linux will fail with missing Darwin-specific dependencies.

---

## JavaScript Shell (`nix develop .#js`)

For JavaScript/Node.js development and browser testing.

### Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `PLAYWRIGHT_BROWSERS_PATH` | Browser path | Pre-built browsers |
| `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS` | `true` | Skip host checks |
| `PLAYWRIGHT_VERSION` | Version string | Playwright version |
| `VITE_PROJECT_ID` | Project ID | Vite configuration |

### Tools Included

- `corepack` - Node.js package managers (yarn, pnpm)
- `playwright` - Browser automation
- `playwright-driver.browsers` - Pre-built browsers
- `geckodriver` - Firefox WebDriver
- `buf` - Protocol buffers
- `curl` - HTTP client
- `mktemp` - Temporary file creation

### Use Cases

- Running `yarn` and Node.js scripts
- Browser-based testing with Playwright
- Protocol buffer code generation

---

## WASM Shell (`nix develop .#wasm`)

For WebAssembly builds and testing.

### Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `CARGO_BUILD_TARGET` | `wasm32-unknown-unknown` | Default build target |
| `SQLITE` | SQLite dev path | SQLite headers |
| `SQLITE_OUT` | SQLite out path | SQLite binaries |
| `GECKODRIVER` | Geckodriver path | Firefox WebDriver |
| `WASM_BINDGEN_SPLIT_LINKED_MODULES` | `1` | Module splitting |
| `WASM_BINDGEN_TEST_TIMEOUT` | `256` | Test timeout seconds |
| `WASM_BINDGEN_TEST_ONLY_WEB` | `1` | Web-only tests |
| `RSTEST_TIMEOUT` | `90` | rstest timeout |
| `CARGO_PROFILE_TEST_DEBUG` | `0` | Disable debug in tests |
| `WASM_BINDGEN_TEST_WEBDRIVER_JSON` | Config path | WebDriver config |

### Rust Configuration

Uses `fenix.stable` toolchain with:
- `cargo`
- `rustc`
- `wasm32-unknown-unknown` target

Note: This shell uses stable Rust from fenix, not the project-wide pinned version.

### Tools Included

- `wasm-pack` - WASM packaging
- `wasm-bindgen-cli` - Bindings generator
- `binaryen` - WASM optimization
- `emscripten` - Emscripten toolchain
- `llvmPackages.lld` - LLVM linker
- `firefox` - Firefox browser
- `geckodriver` - Firefox WebDriver
- `corepack` - Node.js package managers

### Building WASM Package

```bash
# In the WASM shell
wasm-pack build --target web bindings/wasm

# Or build as Nix package
nix build .#wasm-bindings
```

---

## Platform Compatibility Matrix

| Shell | aarch64-darwin | x86_64-linux |
|-------|----------------|--------------|
| default | Yes | Yes |
| android | Yes | Yes |
| ios | Yes | No |
| js | Yes | Yes |
| wasm | Yes | Yes |
