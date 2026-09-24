#[cfg(not(target_arch = "wasm32"))]
mod bidi;
mod client;
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
