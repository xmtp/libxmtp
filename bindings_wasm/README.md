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
- `yarn check:macos`: Run cargo check for macOS (requires Homebrew and `llvm` to
  be installed)

# Publishing

To release a new version of the bindings, update the version in `package.json`
with the appropriate semver value and add an entry to the CHANGELOG.md file.
Once merged, manually trigger the `Release WASM Bindings` workflow to build and
publish the bindings.
