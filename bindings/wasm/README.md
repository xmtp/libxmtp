
# Libxmtp

Libxmtp is a platform agnostic implementation of the core cryptographic functionality to be used in XMTP sdk's

# Dev Setup

Some crates, such as `libxmtp-core` and `bindings/wasm/crate`, are excluded in the `Cargo.toml`, and will not have automatic `rust-analyzer` support in VSCode. You can open these crates directly as projects in order to get that support.

## QuickStart

Run `npm run build` to build the rust library and Node.js bindings.
