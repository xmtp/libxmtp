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
just backend up           # Build validation service + start Docker
just backend down         # Stop Docker services
```

### Testing

```bash
just test                           # Run all tests (v3 + d14n)
just test v3                        # Run v3 tests only
just test d14n                      # Run d14n tests only
just test crate xmtp_mls            # Run tests for a specific crate
just test v3 test_name              # Run a specific test in v3
just test d14n -E 'test(pattern)'   # Run matching tests in d14n
just wasm test                      # Run WASM unit tests
just wasm test-v3 test_name         # Run a specific WASM test
just node test                      # Run Node.js tests
just ios test                       # Run iOS Swift tests
just android test                   # Run Android unit tests
dev/test/coverage                   # Run tests and open coverage report in browser
```

All `just test` and `just wasm test` variants pass extra args through to `cargo nextest run`.

### Code Quality

```bash
just lint                # Run all linting (rust + config + markdown) - ALWAYS run before committing
just lint-rust           # Run Rust clippy, fmt check, hakari
just lint-config         # Lint TOML, Nix, shell scripts
just format              # Format all code (Rust, Nix, TOML, TypeScript, Swift, Kotlin)
```

### Platform Checks

```bash
just check                          # Check workspace compiles
just check crate xmtp_mls           # Check specific crate
just wasm check                     # Check WASM bindings compile
just android check                  # Check Android bindings
just ios check                      # Check iOS bindings
```

### Android SDK

```bash
nix develop .#android             # Enter Android development shell
./sdks/android/dev/bindings       # Build Android bindings via Nix
just android build                # Build Android native bindings
```

### Benchmarks

```bash
dev/bench                                    # Run all benchmarks
dev/bench add_1_member_to_group              # Run a specific benchmark
cargo bench --features bench -p xmtp_mls --bench group_limit  # Run benchmark category
```

## Writing Tests

Use the `writing-rust-tests` skill for comprehensive guidance on test macros, fixtures, WASM compatibility, assertions, and running tests. Key rules:

- **Always use `#[xmtp_common::test(unwrap_try = true)]` instead of `#[test]`**
- Use the `tester!` macro for tests that require a client
- `cargo nextest` provides better test isolation

## Database

Uses Diesel ORM with encrypted SQLite. Migrations are in `crates/xmtp_db/migrations/`.

## Code Change Requirements

- **Always run `just lint`** before committing Rust changes
- For `bindings_node` changes, also run `just node lint`
- Add test coverage for new functionality
