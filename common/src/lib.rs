//! Common types shared among all XMTP Crates

mod macros;

#[cfg(feature = "test-utils")]
mod test;
#[cfg(feature = "test-utils")]
pub use test::*;

#[cfg(feature = "bench")]
pub mod bench;

pub mod retry;
pub use retry::*;

/// Global Marker trait for WebAssembly
#[cfg(target_arch = "wasm32")]
pub trait Wasm {}
#[cfg(target_arch = "wasm32")]
impl<T> Wasm for T {}

pub mod time;

use rand::{
    distributions::{Alphanumeric, DistString},
    RngCore,
};
use xmtp_cryptography::utils as crypto_utils;

pub fn rand_string<const N: usize>() -> String {
    Alphanumeric.sample_string(&mut crypto_utils::rng(), N)
}

pub fn rand_array<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    crypto_utils::rng().fill_bytes(&mut buffer);
    buffer
}

/// Yield back control to the async runtime
#[cfg(not(target_arch = "wasm32"))]
pub async fn yield_() {
    tokio::task::yield_now().await
}

/// Yield back control to the async runtime
#[cfg(target_arch = "wasm32")]
pub async fn yield_() {
    time::sleep(crate::time::Duration::from_millis(1)).await;
}
