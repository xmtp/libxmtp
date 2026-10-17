//! Stateless XMTP service backed by PostgreSQL.

pub mod auth;
pub mod db;
pub mod error;
mod handles;
mod published;
mod push;
pub mod server;
pub mod service;
mod stream;
pub mod telemetry;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support;
mod validation;

pub use service::Backend;
pub use xmtp_backend_config as config;
pub use xmtp_proto::xmtp::backend::v1 as api;
