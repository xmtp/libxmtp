> :warning: :warning: :warning: **Under Construction**: Parts of this code are in WIP and should not be used in production without guidance from the XMTP team

# Libxmtp

Libxmtp is a monorepo with multiple crates that encapsulate parts of XMTP messaging functionality, cryptography or bindings to other languages.

## Requirements

- To build `xmtp-proto` Buf must be installed on your machine. Visit the [Buf documentation](https://buf.build/docs/installation) for more info

## Structure

Top-level

- crates/ - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
- crates/xmtp-keystore - first crate, implements the Keystore API in Rust
- crates/xmtp-proto - Generated code for handling XMTP protocol buffers
- crates/xmtp-networking - API client for XMTP's GRPC API, using code from `crates/xmtp-proto`
- bindings/wasm - depends on libxmtp to generate a WASM library and bindings
- bindings/xmtp_rust_swift - Swift bindings

## Rust Keystore QuickStart

- cd `crates/xmtp-keystore`
- `cargo test`

## WASM QuickStart

- cd `bindings/wasm`
- Run `npm run build` to build the rust crate and Node.js bindings.
- Run `npm run test` to build the xmtp-keystore crate, the wasm bindings crate and run against Node.js tests

## Tests

This should compile the xmtp-keystore crate and the wasm bindings, then run tests in JS

- cd `bindings/wasm`
- Run `npm test`
