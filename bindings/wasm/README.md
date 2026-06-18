# WASM bindings for the libXMTP rust library

> [!INFO]
> These bindings are not intended to be used directly, use the associated SDK instead.

## Setup

1. Install the [emscripten toolchain](https://emscripten.org/docs/getting_started/downloads.html): `brew install emscripten`. `emscripten` is used to compile from Rust to WebAssembly.
2. Install LLVM: `brew install llvm`, and then follow the instructions to add it to your PATH. Emscripten depends on LLVM's Clang (as opposed to Apple's Clang).

## Useful commands

- `yarn`: Installs all dependencies (required before building)
- `yarn build`: Build a release version of the WASM bindings for the current
  platform
- `yarn lint`: Run cargo clippy and fmt checks
- `yarn format:check`: Check formatting of integration tests
- `yarn typecheck`: Run typecheck on integration tests
- `yarn test`: Run cargo test with `wasm32-unknown-unknown` target
- `yarn test:integration`: Run integration tests using vitest

### macOS commands

These commands require Homebrew and `llvm` to be installed. See above.

- `yarn check:macos`: Run cargo check
- `yarn lint:macos`: Run cargo clippy and fmt checks
- `yarn build:macos`: Build a release version of the WASM bindings
- `yarn test:integration:macos`: Run integration tests using vitest

## Building with Cargo or other tools that rely on it

When building with Cargo, a `config.toml` file must be present at `.cargo/config.toml` in the project root. At minimum, it needs to specify the default build target:

```toml
[build]
target = "wasm32-unknown-unknown"
```

Without this, Cargo will not default to the correct target and `rust-analyzer` will report errors.

### Apple Silicon (M-series Macs)

If you are developing on a Mac with an Apple Silicon chip, you also need to configure the linker and compiler toolchain explicitly. Add the following to your `.cargo/config.toml`:

```toml
[target.wasm32-unknown-unknown]
linker = "/opt/homebrew/opt/llvm/bin/wasm-ld"

[env]
CC_wasm32_unknown_unknown = "/opt/homebrew/opt/llvm/bin/clang"
AR_wasm32_unknown_unknown = "/opt/homebrew/opt/llvm/bin/llvm-ar"
```

> **Note:** This assumes LLVM was installed via Homebrew (`brew install llvm`). If your installation path differs, adjust the paths accordingly.

# Publishing

To release a new version of the bindings, update the version in `package.json` with the appropriate semver value. Once merged, manually trigger the `Release WASM Bindings` workflow to build and publish the bindings.
