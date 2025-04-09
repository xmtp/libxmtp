//! Utilities for xmtp_mls benchmarks
//! Utilities mostly include pre-generating identities in order to save time when writing/testing
//! benchmarks.
#![allow(clippy::unwrap_used)]

mod identity_gen;
pub use identity_gen::*;
pub mod clients;
pub use clients::*;

use thiserror::Error;
/// Re-export of functions in private modules for benchmarks
pub mod re_export {
    pub use crate::groups::mls_ext::{wrap_welcome, WrapperAlgorithm};
}

#[derive(Debug, Error)]
pub enum BenchError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
