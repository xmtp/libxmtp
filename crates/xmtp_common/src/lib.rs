//! Common types shared among all XMTP Crates
// required to be able to use xmtp_macros::async_trait in this crate
extern crate self as xmtp_common;

mod macros;

mod error_code;
pub use error_code::ErrorCode;

#[doc(inline)]
pub use xmtp_macro::ErrorCode;

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

mod event_logging;
pub use event_logging::*;

pub use xmtp_cryptography::hash::*;
pub use xmtp_cryptography::rand::*;

pub use xmtp_macro::log_event;

#[cfg(feature = "logging")]
pub mod logging;
#[cfg(feature = "logging")]
pub use logging::*;
