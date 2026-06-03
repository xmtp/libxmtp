//! Logging and tracing pipeline construction for libxmtp.
//!
//! Sole owner of the `tracing-subscriber` and `opentelemetry` dependencies; all
//! production logging/tracing layers, the OTLP exporter, and the runtime-control
//! handle live here.
//!
//! Platform differences (native file/telemetry vs. browser console) are kept in
//! per-platform submodules rather than `#[cfg]` scattered through shared code:
//! see `handle::{native,wasm}` and `builder::{native,wasm}`.

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

// OTLP trace export is native-only: `opentelemetry-otlp`/`tonic` do not build on
// wasm, and the browser has no exporter.
#[cfg(not(target_arch = "wasm32"))]
mod telemetry;
#[cfg(not(target_arch = "wasm32"))]
pub use telemetry::{SCOPE, TelemetryGuard, init};

// Test subscriber, behind the `test-utils` feature. `logger_layer` is the native
// layer used by binding test harnesses; the browser only needs `logger`.
#[cfg(feature = "test-utils")]
pub mod test_logging;
#[cfg(feature = "test-utils")]
pub use test_logging::logger;
#[cfg(all(feature = "test-utils", not(target_arch = "wasm32")))]
pub use test_logging::logger_layer;
