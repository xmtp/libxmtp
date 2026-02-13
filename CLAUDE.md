# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LibXMTP is a shared library implementing the XMTP messaging protocol using MLS (Messaging Layer Security). It's a Rust workspace with bindings for mobile (Android/iOS via FFI), WebAssembly, and Node.js.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      LANGUAGE BINDINGS                          │
│  bindings/mobile (uniffi) │ bindings/wasm │ bindings/node (napi)│
└──────────────────────────────┬──────────────────────────────────┘
                               │
                ┌──────────────▼──────────────┐
                │      xmtp_mls (Client)      │
                │  Groups, messages, sync     │
                └──────────────┬──────────────┘
        ┌──────────┬───────────┼───────────┬──────────┐
        ▼          ▼           ▼           ▼          ▼
   xmtp_api   xmtp_db     xmtp_id    xmtp_proto  xmtp_cryptography
   (traits)   (storage)   (identity) (protobuf)  (crypto ops)
        │
        ├─► xmtp_api_grpc (gRPC implementation)
        └─► xmtp_api_d14n (decentralized API)
```

**Key patterns:**
- `Client<Context>` - Generic client parameterized by context (allows different API/DB combinations)
- `ClientBuilder` - Fluent builder for client construction with identity, API, and storage config
- `XmtpMlsLocalContext` - Centralizes dependencies (API, storage, identity, locks, events)
- Trait abstractions (`XmtpApi`, `XmtpDb`, `InboxOwner`) enable pluggable implementations
- Platform-specific code via `if_native!`/`if_wasm!` macros

## Development Commands

### Environment Setup

```bash
dev/up                    # Install dependencies and start Docker services (XMTP node)
dev/docker/down           # Stop Docker services
```

### Testing

```bash
cargo test                          # Run all Rust tests
cargo test -p xmtp_mls              # Run tests for a specific crate
cargo test test_name                # Run a single test by name
cargo test -p xmtp_mls test_name    # Run a single test in a specific crate
RUST_LOG=off cargo test             # Run tests with minimal logging
dev/test/wasm                       # Run WASM tests headless
dev/test/coverage                   # Run tests and open coverage report in browser
```

### Code Quality

```bash
dev/lint                 # Run all linting (shellcheck, markdown, rust) - ALWAYS run before committing
dev/fmt                  # Format code (markdown and rust)
dev/lint-rust            # Run Rust clippy against all targets
```

### Platform Checks

```bash
dev/check-wasm          # Check WASM bindings compile
dev/check-android       # Check Android bindings
dev/check-swift         # Check Swift bindings
```

### Android SDK

```bash
nix develop .#android             # Enter Android development shell
./sdks/android/dev/bindings       # Build Android bindings via Nix
./sdks/android/dev/build          # Build the full Android SDK
nix build .#android-libs          # Build all Android targets via Nix
nix build .#android-libs-fast     # Build host-matching target only
```

### Node.js Bindings

```bash
nix build .#node-bindings-fast    # Build host-matching .node binary
nix build .#node-bindings-js      # Generate JS/TS bindings (index.js + index.d.ts)
```

### Benchmarks

```bash
dev/bench                                    # Run all benchmarks
dev/bench add_1_member_to_group              # Run a specific benchmark
cargo bench --features bench -p xmtp_mls --bench group_limit  # Run benchmark category
```

## Writing Tests

- **ALWAYS use `#[xmtp_common::test(unwrap_try = true)]` instead of `#[test]`** - ensures tests run in both native and WASM environments
- **Use `unwrap_try = true`** - automatically unwraps `?` operators in tests, providing better error messages
- Use `rstest` for parameterized tests with `#[case]` attributes
- Use the `tester!` macro for tests that require a wallet
- `cargo nextest` provides better test isolation

```rust
#[rstest]
#[case("input1", "expected1")]
#[case("input2", "expected2")]
fn test_function(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(function_to_test(input), expected);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_simple() {
    // Single test case - can use ? operator freely
}
```

### Log Output Control

- `CONTEXTUAL=1 cargo test` - Async-aware structured logging (supports `TestLogReplace` for readable IDs)
- `STRUCTURED=1 cargo test` - JSON structured logs
- `RUST_LOG=xmtp_mls=debug,xmtp_api=off cargo test` - Filter by crate

## Database

Uses Diesel ORM with encrypted SQLite. Migrations are in `crates/xmtp_db/migrations/`.

## Code Change Requirements

- **Always run `./dev/lint`** before committing Rust changes
- For `bindings_node` changes, also run `yarn && yarn format:check` in `bindings/node`
- Add test coverage for new functionality
