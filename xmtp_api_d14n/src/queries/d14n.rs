//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

mod client;
pub use client::*;
