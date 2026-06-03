//! Runtime-control handle for the installed logging pipeline.
//!
//! The handle is produced by [`crate::XmtpLoggingBuilder::install`]. It holds a
//! [`tracing_subscriber::reload::Handle`] for each reloadable layer slot in the
//! global [`Registry`], letting callers change the log level, enable/disable file
//! logging, and enable telemetry at runtime without re-installing the subscriber.
//!
//! Every reloadable slot is typed as either an [`EnvFilter`] (the level filter)
//! or `Option<Box<dyn Layer<Registry>>>` (file + telemetry). Boxing erases the
//! concrete layer type so the handle avoids the deeply-nested generic types that
//! a fully-typed layer stack would require.

use tracing_subscriber::reload;
use tracing_subscriber::{EnvFilter, Registry};

#[cfg(not(target_arch = "wasm32"))]
use parking_lot::Mutex;

use crate::config::Level;
use crate::error::Error;
use crate::filter::filter_directive;

#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::Layer;

#[cfg(not(target_arch = "wasm32"))]
use crate::config::{FileConfig, TelemetryConfig};
#[cfg(not(target_arch = "wasm32"))]
use crate::telemetry::{self, TelemetryGuard};
#[cfg(not(target_arch = "wasm32"))]
use tracing_appender::non_blocking::WorkerGuard;

/// A boxed, type-erased layer over the global [`Registry`]. Used for the
/// reloadable file and telemetry slots.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type BoxLayer = Box<dyn Layer<Registry> + Send + Sync>;

/// Worker guards that must stay alive for the lifetime of the process: the
/// file-writer worker thread and the OTel tracer provider. Dropping either
/// flushes/stops it.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct Guards {
    file_worker: Option<WorkerGuard>,
    telemetry: Option<TelemetryGuard>,
}

/// Handle to the installed logging pipeline. Holds the reload handles for each
/// runtime-mutable layer slot plus the worker guards that keep the file writer
/// and telemetry exporter alive.
///
/// Created by [`crate::XmtpLoggingBuilder::install`]. Keep it alive for the
/// process lifetime; dropping it flushes the file writer and shuts down the
/// telemetry exporter.
pub struct LoggingHandle {
    filter: reload::Handle<EnvFilter, Registry>,
    #[cfg(not(target_arch = "wasm32"))]
    file: reload::Handle<Option<BoxLayer>, Registry>,
    #[cfg(not(target_arch = "wasm32"))]
    telemetry: reload::Handle<Option<BoxLayer>, Registry>,
    #[cfg(not(target_arch = "wasm32"))]
    guards: Mutex<Guards>,
}

impl LoggingHandle {
    /// Build the native handle from its reload handles. Constructed by
    /// `install`; not public API.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn new(
        filter: reload::Handle<EnvFilter, Registry>,
        file: reload::Handle<Option<BoxLayer>, Registry>,
        telemetry: reload::Handle<Option<BoxLayer>, Registry>,
    ) -> Self {
        Self {
            filter,
            file,
            telemetry,
            guards: Mutex::new(Guards::default()),
        }
    }

    /// Build the wasm handle. Only the level filter is reloadable in the browser.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn new(filter: reload::Handle<EnvFilter, Registry>) -> Self {
        Self { filter }
    }

    /// Change the active log level for all libxmtp targets at runtime.
    pub fn set_level(&self, level: Level) -> Result<(), Error> {
        self.filter.reload(filter_directive(level.as_str()))?;
        Ok(())
    }

    /// Turn on rolling-file logging at runtime. Builds the non-blocking file
    /// writer described by `cfg`, installs a JSON fmt layer writing to it, and
    /// keeps the worker guard alive. Replaces any previously-enabled file layer.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_file(&self, cfg: FileConfig) -> Result<(), Error> {
        let (non_blocking, guard) =
            crate::layers::file::file_writer(&cfg).map_err(|e| Error::File(e.to_string()))?;

        let layer: BoxLayer = tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking)
            .boxed();

        self.file.reload(Some(layer))?;
        self.guards.lock().file_worker = Some(guard);
        Ok(())
    }

    /// Turn off rolling-file logging at runtime. Removes the file layer and drops
    /// the worker guard (which flushes any buffered lines).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn disable_file(&self) -> Result<(), Error> {
        self.file.reload(None)?;
        self.guards.lock().file_worker = None;
        Ok(())
    }

    /// Turn on OTLP trace export at runtime. Builds the exporter + tracing layer
    /// from `cfg`, installs it in the telemetry slot, and keeps the tracer
    /// provider guard alive. Replaces any previously-enabled telemetry layer.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_telemetry(&self, cfg: TelemetryConfig) -> Result<(), Error> {
        let (layer, guard) = telemetry::init::<Registry>(cfg.endpoint, cfg.resource_attributes)?;
        self.telemetry.reload(Some(layer.boxed()))?;
        self.guards.lock().telemetry = Some(guard);
        Ok(())
    }

    /// Flush pending telemetry spans (best-effort). File writer lines flush as the
    /// worker drains and on drop; this primarily forces the OTel exporter to push
    /// queued spans, e.g. before process exit.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn flush(&self) {
        if let Some(t) = self.guards.lock().telemetry.as_ref() {
            t.shutdown();
        }
    }

    /// No-op flush on wasm (no file/telemetry exporters).
    #[cfg(target_arch = "wasm32")]
    pub fn flush(&self) {}
}
