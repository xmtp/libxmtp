# LibXMTP

LibXMTP is a shared library encapsulating core functionality of the XMTP messaging protocol such as the cryptography, networking, and language bindings.

[![Test](https://github.com/xmtp/libxmtp/actions/workflows/test.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/test.yml)
[![Lint](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml/badge.svg)](https://github.com/xmtp/libxmtp/actions/workflows/lint.yml)

**⚠️ Experimental:** Early development stage, expect frequent changes and unresolved issues.

## Requirements

- Install [Rustup](https://rustup.rs/)

## Development

Install other dependencies and start background services:

```sh
dev/up
```

Run tests:

```sh
dev/test
```

## Structure

Shared:

- `xmtp_proto` - Generated code for handling XMTP protocol buffers
- `xmtp_networking` - API client for XMTP's GRPC API, using code from `xmtp_proto`
- `uniffi_bindgen_generator` - A binary crate used internally for generating [uniffi bindings](https://mozilla.github.io/uniffi-rs/tutorial/foreign_language_bindings.html#multi-crate-workspaces)

v3:

- `xmtp` - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
- `xmtp_cryptography` - cryptographic operations for v3
- `bindings_ffi` - FFI bindings for Android and iOS
- `bindings_wasm` (unused) - wasm bindings
- `bindings_js` (unused) - JS bindings

v2:

- `xmtp_crypto` - cryptographic operations for v2
- `bindings_swift` - Swift bindings for XMTP v2 - exposes networking and cryptographic operations
- `xmtp_dh` - A Uniffi binding for the Rust-based Diffie-Hellman operation for Android
- `xmtp_keystore` (unused) - implements the v2 Keystore API in Rust
