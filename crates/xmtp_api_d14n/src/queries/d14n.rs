//! Compatibility layer for d14n and previous xmtp_api crate
mod client;
// XIP-83 bidi binding (native-only — full-duplex HTTP/2).
#[cfg(not(target_arch = "wasm32"))]
mod connection;
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;
xmtp_common::if_test! {
    mod test_client;
}
pub use client::*;
#[cfg(not(target_arch = "wasm32"))]
pub use connection::*;

#[cfg(test)]
mod test;
