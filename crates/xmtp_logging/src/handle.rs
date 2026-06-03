//! Runtime-control handle for the installed logging pipeline.
//!
//! The handle is produced by [`crate::XmtpLoggingBuilder::install`]. It holds a
//! [`tracing_subscriber::reload::Handle`] for each reloadable layer slot in the
//! global [`tracing_subscriber::Registry`], letting callers change the log level,
//! enable/disable file logging, and enable telemetry at runtime without
//! re-installing the subscriber.
//!
//! The native and wasm handles differ substantially — native carries reloadable
//! file/telemetry slots plus worker guards, while the browser only has a
//! reloadable level filter — so each lives in its own platform module rather than
//! threading `#[cfg]` through a single shared definition. Both expose the same
//! `LoggingHandle` name and a common `set_level` / `flush` surface.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::LoggingHandle;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{BoxLayer, Guards, build_file_layer, build_telemetry_layer};

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::LoggingHandle;
