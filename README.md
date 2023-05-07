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

Top-level:

- `xmtp` - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
- `xmtp_keystore` - first crate, implements the Keystore API in Rust
- `xmtp_proto` - Generated code for handling XMTP protocol buffers
- `xmtp_networking` - API client for XMTP's GRPC API, using code from `xmtp_proto`
- `bindings_swift` - Swift bindings
