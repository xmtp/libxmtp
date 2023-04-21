# WARNING: DO NOT USE FOR PRODUCTION XMTP CLIENTS

This code is still under development.

## Structure

Top-level
- crates/ - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
 - crates/libxmtp-core - first crate, entrypoint for the Rust API
- bindings/wasm - depends on libxmtp to generate a WASM library and bindings

## Rust QuickStart

- cd `crates/libxmtp-core`
- `cargo test`

## WASM QuickStart

- cd `bindings/wasm`
- Run `npm run build` to build the rust crate and Node.js bindings.
- Run `npm run test` to build the libxmtp-core crate, the wasm bindings crate and run against Node.js tests

