//! Compatibility layer for d14n and previous xmtp_api crate
mod client;
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;
xmtp_common::if_test! {
    mod test_client;
}
pub use client::*;

#[cfg(test)]
mod test;
