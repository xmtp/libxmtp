//! Common types shared among all XMTP Crates

mod macros;

#[cfg(any(test, feature = "test-utils"))]
mod test;
#[cfg(any(test, feature = "test-utils"))]
pub use test::*;

#[doc(inline)]
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

pub mod fmt;
pub mod snippet;
pub mod time;
pub mod types;

pub mod r#const;
pub use r#const::*;

#[cfg(feature = "logging")]
pub mod logging;
#[cfg(feature = "logging")]
pub use logging::*;

use rand::distributions::Alphanumeric;
use rand::distributions::DistString;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
pub fn rng() -> ChaCha20Rng {
    ChaCha20Rng::from_entropy()
}

pub fn rand_string<const N: usize>() -> String {
    Alphanumeric.sample_string(&mut rng(), N)
}

pub fn rand_array<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    rng().fill_bytes(&mut buffer);
    buffer
}

pub fn rand_vec<const N: usize>() -> Vec<u8> {
    rand_array::<N>().to_vec()
}
