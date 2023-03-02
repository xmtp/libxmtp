# WARNING: DO NOT USE FOR PRODUCTION XMTP CLIENTS

This code is preliminary and meant for benchmarking. See latest progress `benchmark` branch.

# Libxmtp

Libxmtp is a platform agnostic implementation of the core cryptographic functionality to be used in XMTP sdk's

## Structure

Top-level
- crates/ - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
 - crates/xmtp-keystore - first crate, implements the Keystore API in Rust
- bindings/wasm - depends on libxmtp to generate a WASM library and bindings

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
