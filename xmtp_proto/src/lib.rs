#![allow(clippy::all)]
include!("gen/mod.rs");
#[cfg(feature = "xmtp-message_api-v1")]
pub mod api_client;
