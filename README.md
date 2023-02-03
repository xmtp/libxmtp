
# Libxmtp

Libxmtp is a platform agnostic implementation of the core cryptographic functionality to be used in XMTP sdk's

## Structure

Top-level
- libxmtp - the pure Rust implementation of XMTP APIs, agnostic to any per-language or per-platform binding
- bindings/wasm - depends on libxmtp to generate a WASM library and bindings

## WASM QuickStart

- cd `bindings/wasm`
- Run `npm run build` to build the rust crate and Node.js bindings.
