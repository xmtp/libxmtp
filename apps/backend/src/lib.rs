//! Stateless XMTP service backed by PostgreSQL.

pub mod config;
pub mod db;
pub mod error;
pub mod server;
pub mod service;
mod stream;
pub mod telemetry;
#[cfg(test)]
mod test_support;
mod validation;

pub use service::Backend;
pub use xmtp_proto::xmtp::backend::v1 as api;
