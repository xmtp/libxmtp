# WASM bindings for the libXMTP rust library

> [!INFO]
> These bindings are not intended to be used directly, use the associated SDK instead.

## Setup

1. Install the [emscripten toolchain](https://emscripten.org/docs/getting_started/downloads.html): `brew install emscripten`. `emscripten` is used to compile from Rust to WebAssembly.
2. Install LLVM: `brew install llvm`, and then follow the instructions to add it to your PATH. Emscripten depends on LLVM's Clang (as opposed to Apple's Clang).

## Useful commands

- `just install`: Install root workspace dependencies before the first build.
- `just wasm build`: Build a release version of the WASM bindings.
- `just wasm lint`: Run cargo clippy, rustfmt, and Prettier checks.
- `just wasm test`: Run cargo tests with the `wasm32-unknown-unknown` target.
- `just wasm test-integration`: Run integration tests with Vitest.

## Publishing

To release a new version of the bindings, update the version in `package.json` with the appropriate semver value. Once merged, manually trigger the `Release WASM Bindings` workflow to build and publish the bindings.
