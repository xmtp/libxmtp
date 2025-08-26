# LibXMTP - Claude Assistant Context

This file provides context for Claude Code to understand the libxmtp project structure and development workflows.

## Project Overview

LibXMTP is a shared library encapsulating the core functionality of the XMTP messaging protocol, implementing cryptography, networking, and language bindings. The project is built primarily in Rust with bindings for various platforms.

## Project Structure

- **Core Libraries**: Rust workspace with multiple crates
- **Language Bindings**: FFI (Android/iOS), WASM, and Node.js bindings
- **Examples**: CLI client and Android example app
- **Database**: SQLite with Diesel ORM and migrations
- **Protocol**: MLS (Messaging Layer Security) implementation

## Key Directories

- `xmtp_mls/` - Core MLS v3 implementation
- `xmtp_cryptography/` - Cryptographic operations
- `xmtp_api_grpc/` - gRPC API client
- `xmtp_db/` - Database layer and migrations
- `bindings_ffi/` - FFI bindings for mobile
- `bindings_wasm/` - WebAssembly bindings
- `bindings_node/` - Node.js bindings
- `examples/cli/` - Command-line example client
- `dev/` - Development scripts and tools

## Development Commands

### Environment Setup
```bash
dev/up                    # Install dependencies and start services
```

### Testing
```bash
cargo test                # Run Rust tests
RUST_LOG=off cargo test   # Run tests with minimal logging
dev/test/wasm            # Run WASM tests headless
dev/test/browser-sdk     # Run browser SDK tests
```

### Code Quality
```bash
dev/lint                 # Run all linting (shellcheck, markdown, rust)
dev/fmt                  # Format code (markdown and rust)
cargo fmt               # Format Rust code only
dev/lint-rust           # Run Rust clippy linter against all targets
```

### Build & Services
```bash
dev/docker/up           # Start Docker services (XMTP node)
dev/docker/down         # Stop Docker services
```

### Platform-Specific
```bash
dev/check-wasm          # Check WASM bindings
dev/check-android       # Check Android bindings
dev/check-swift         # Check Swift bindings
```

## Testing Tips

### Log Output Control
- `CONTEXTUAL=1 cargo test` - Async-aware structured logging
- `STRUCTURED=1 cargo test` - JSON structured logs
- `RUST_LOG=xmtp_mls=debug,xmtp_api=off cargo test` - Filter by crate

### Test Utilities
- Many developers use `cargo nextest` for better test isolation
- Use `TestLogReplace` for human-readable test output
- Build `TesterBuilder` with `.with_name()` for named test instances

## Required Tools

- Rust (via rustup)
- Docker Desktop
- Foundry (for blockchain tests)
- Platform-specific: Java/Kotlin, Swift, Node.js, WASM tools

## Nix Development Environment

The project provides Nix flake-based development shells with all required dependencies. This is the recommended approach for development as it provides consistent, reproducible environments across different platforms.

### Prerequisites
```bash
# Install Nix with flakes enabled
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

### Available Development Shells
```bash
nix develop                    # Default shell for general Rust development
nix develop .#android         # Android development shell with NDK
nix develop .#ios             # iOS development shell (macOS only)
nix develop .#js              # JavaScript/Node.js development shell
nix develop .#wasmBuild       # WebAssembly build environment
```

### Using the Default Shell
The default development shell includes:
- Rust toolchain (version pinned to 1.89.0)
- Cargo and related tools
- Docker and Docker Compose
- Foundry blockchain tools
- Development scripts and utilities

```bash
# Enter the development shell
nix develop

# Once in the shell, use normal development commands
cargo test
dev/up
dev/lint
```

### Shell Features
- **Cachix Integration**: Pre-built binaries available via `xmtp.cachix.org`
- **Pinned Dependencies**: Consistent tool versions across all environments
- **Cross-compilation**: Android and iOS targets available in respective shells
- **Development Scripts**: All `dev/` scripts work within the Nix environment

## Build Profiles

- `dev` - Default development profile with minimal debug info
- `dbg` - Full debug profile with assertions
- `release` - Optimized release build
- `bench` - Benchmark profile with debug symbols

## Database

Uses Diesel ORM with SQLite backend. Migrations are in `xmtp_db/migrations/`.