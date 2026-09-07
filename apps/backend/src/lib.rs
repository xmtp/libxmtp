//! Stateless XMTP service backed by PostgreSQL.

pub mod config;
pub mod db;
pub mod error;
pub mod server;
pub mod service;
mod validation;

pub use service::Backend;
pub use xmtp_proto::xmtp::backend::v1 as api;
