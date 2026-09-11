# Rust Shell (`nix develop .#rust`)

Focused Rust development shell for crates/ and bindings/ work. Supports `dev/lint`, `dev/lint-rust`, `cargo test`, `cargo nextest`, and WASM checks. Does NOT include debugging/profiling tools or convenience packages — see the default shell for those.

**Source:** `nix/shells/rust.nix`

## Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `XMTP_NIX_ENV` | `yes` | Nix environment is active |
| `OPENSSL_DIR` | `${openssl.dev}` | OpenSSL headers location |
| `OPENSSL_LIB_DIR` | `${openssl}/lib` | OpenSSL library location |
| `OPENSSL_NO_VENDOR` | `1` | Use system OpenSSL, don't build |
| `STACK_OVERFLOW_CHECK` | `0` | Disable stack overflow checks |
| `LD_LIBRARY_PATH` | OpenSSL, zlib paths | Runtime library search path |
| `CC_wasm32_unknown_unknown` | clang path | WASM C compiler |
| `AR_wasm32_unknown_unknown` | llvm-ar path | WASM archiver |
| `CFLAGS_wasm32_unknown_unknown` | clang include path | WASM C compiler flags |

Note: `XMTP_DEV_SHELL` is **not set** in this shell. Use `XMTP_NIX_ENV` to detect it.

## Rust Targets

- `wasm32-unknown-unknown`
- `x86_64-unknown-linux-gnu`

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

**Direct dependencies:**
`foundry-bin`, `sqlcipher`, `corepack`

**Darwin only:**
`darwin.cctools`
