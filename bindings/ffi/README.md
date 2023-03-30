# Uniffi-based bindings for xmtpv3

This repo provides Uniffi bindings for native libraries in other platforms.

# Status

- Android is tested. See ../examples

# Requirements

- Install Docker
- Install [cross-rs](https://github.com/cross-rs/cross) for zero setup cross-platform builds
- Run  `./cross_build.sh`

# Notes
- `gen_kotlin.sh` is needed for generating kotlin source code, right now requires uncommenting "# Gen Kotlin# part of Cargo.toml
