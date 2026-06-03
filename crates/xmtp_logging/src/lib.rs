//! Logging and tracing pipeline construction for libxmtp.
//!
//! Sole owner of the `tracing-subscriber` and `opentelemetry` dependencies; all
//! production logging/tracing layers, the OTLP exporter, and the runtime-control
//! handle live here.

mod config;
mod filter;
pub use config::*;
pub use filter::filter_directive;

#[cfg(not(target_arch = "wasm32"))]
mod telemetry;
#[cfg(not(target_arch = "wasm32"))]
pub use telemetry::{init, TelemetryGuard, SCOPE};
