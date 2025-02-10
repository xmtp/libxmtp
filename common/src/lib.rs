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

pub mod wasm;
pub use wasm::*;

pub mod stream_handles;
pub use stream_handles::*;

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
