//! Utilities for xmtp_mls benchmarks
//! Utilities mostly include pre-generating identities in order to save time when writing/testing
//! benchmarks.
#![allow(clippy::unwrap_used)]

mod identity_gen;
pub use identity_gen::*;
pub mod clients;
pub use clients::*;

use once_cell::sync::OnceCell;
use std::sync::Once;
use thiserror::Error;
use tracing::{Metadata, Subscriber};
use tracing_flame::{FlameLayer, FlushGuard};
use tracing_subscriber::{
    layer::{Context, Filter, Layer, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Re-export of functions in private modules for benchmarks
pub mod re_export {
    pub use crate::hpke::encrypt_welcome;
}

#[derive(Debug, Error)]
pub enum BenchError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub use xmtp_common::bench::logger;
