# WARNING: DO NOT USE FOR PRODUCTION XMTP CLIENTS

This code is preliminary and meant for benchmarking.

# Libxmtp

Libxmtp is a platform agnostic implementation of the core cryptographic functionality to be used in XMTP sdk's

## Structure

Top-level
- crates/ - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
 - crates/xmtp-keystore - first crate, implements the Keystore API in Rust
- bindings/wasm - depends on libxmtp to generate a WASM library and bindings

## WASM QuickStart

- cd `bindings/wasm`
- Run `npm run build` to build the rust crate and Node.js bindings.
- Run `npm run prepublishOnly` to run tests that 1) check XmtpApi instantiation and 2) check that a 12 word mnemonic can be generated

## Tests

This should compile the xmtp-keystore crate and the wasm bindings, then run tests in JS

- cd `bindings/wasm`
- Run `npm test`
