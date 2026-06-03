//! Logging and tracing pipeline construction for libxmtp.
//!
//! Sole owner of the `tracing-subscriber` and `opentelemetry` dependencies; all
//! production logging/tracing layers, the OTLP exporter, and the runtime-control
//! handle live here.

mod builder;
mod config;
mod error;
mod filter;
mod handle;
mod layers;
pub use builder::{XmtpLogging, XmtpLoggingBuilder};
pub use config::*;
pub use error::Error;
pub use filter::filter_directive;
pub use handle::LoggingHandle;

#[cfg(not(target_arch = "wasm32"))]
mod telemetry;
#[cfg(not(target_arch = "wasm32"))]
pub use telemetry::{SCOPE, TelemetryGuard, init};
