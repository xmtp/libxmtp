# Rust Shell (`dev/nix-shell --shell rust`)

The Rust core. CI Rust jobs and the SDK checks use it; `default` layers on it,
and `android` takes its core packages. `wasm` is an alias.

**Source:** `nix/shells/rust.nix`, `nix/lib/shell-common.nix`

## Environment (`shellCommon.rustShellEnv`, equal in rust, default, android)

| Variable | Value |
| --- | --- |
| `XMTP_DEV_SHELL` | `rust` |
| `XMTP_NIX_ENV` | `yes` |
| `OPENSSL_DIR`, `OPENSSL_LIB_DIR`, `OPENSSL_NO_VENDOR=1` | Nix OpenSSL |
| `STACK_OVERFLOW_CHECK` | `0` |
| `LD_LIBRARY_PATH` | OpenSSL, zlib |
| `CC_wasm32_unknown_unknown`, `AR_…`, `CFLAGS_…` | Nix clang for wasm32 C code |

On Darwin the shellHook sets one compiler environment: Xcode clang for the
host and the Apple slices, `MACOSX_DEPLOYMENT_TARGET=11.0`,
`IPHONEOS_DEPLOYMENT_TARGET=14`, and Xcode `usr/bin` after the Nix paths.
`wasm32` builds need `--target wasm32-unknown-unknown` (`dev/wasm.just` passes it).

## Toolchain (`shellCommon.rustToolchain`)

Targets: the host, `wasm32-unknown-unknown`, `x86_64-unknown-linux-gnu`, the
four Android ABIs, and on Darwin the macOS, iOS, and iOS simulator slices.
Components: `rust-src`, `clippy`, `rustfmt`, `llvm-tools`.

## Tools

- Core (`rustCorePackages`): cargo tools (nextest, deny, machete, hakari,
  sqlx-cli, Kache, `grpc-health-probe`, `grpcurl`), git, Python 3.11 with PyYAML, `just`.
- Rust shell: foundry, sqlcipher, Node 26 and pnpm 11, uv, `wasm-bindgen-cli`
  with Chrome and chromedriver, rtk, buf, protobuf, protolint, markdownlint,
  taplo, shellcheck, nixfmt, `lcov`, and `cargo-llvm-cov`.
