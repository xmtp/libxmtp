//! Common types shared among all XMTP Crates

mod macros;

#[cfg(any(test, feature = "test-utils"))]
mod test;
#[cfg(any(test, feature = "test-utils"))]
pub use test::*;

#[allow(unused)]
#[macro_use]
extern crate xmtp_macro;

#[doc(hidden)]
#[cfg(any(test, feature = "test-utils"))]
pub use xmtp_macro::test;

#[cfg(feature = "bench")]
pub mod bench;

pub mod retry;
pub use retry::*;

pub mod wasm;
pub use wasm::*;

pub mod stream_handles;
pub use stream_handles::*;

pub mod time;

pub mod fmt;

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

#[cfg(test)]
pub(crate) mod tests {
    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    fn _setup() {
        crate::test::logger();
    }
}
