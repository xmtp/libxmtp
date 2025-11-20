//! Common types shared among all XMTP Crates
// required to be able to use xmtp_macros::async_trait in this crate
extern crate self as xmtp_common;

mod macros;

#[cfg(any(test, feature = "test-utils"))]
mod test;
#[cfg(any(test, feature = "test-utils"))]
pub use test::*;

#[doc(inline)]
#[cfg(any(test, feature = "test-utils"))]
pub use xmtp_macro::test;

#[doc(inline)]
pub use xmtp_macro::async_trait;

#[cfg(feature = "bench")]
pub mod bench;

pub mod retry;
pub use retry::*;

pub mod wasm;
pub use wasm::*;

pub mod stream_handles;
pub use stream_handles::*;

pub mod fmt;
pub mod hex;
pub mod snippet;
pub mod time;
pub mod types;

pub mod r#const;
pub use r#const::*;

pub use xmtp_cryptography::hash::*;
pub use xmtp_cryptography::rand::*;

#[cfg(feature = "logging")]
pub mod logging;
#[cfg(feature = "logging")]
pub use logging::*;
