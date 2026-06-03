//! Native runtime-control handle: reloadable level/file/telemetry slots plus the
//! worker guards that keep the file writer and OTel exporter alive.

use parking_lot::Mutex;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::Layer;
use tracing_subscriber::reload;
use tracing_subscriber::{EnvFilter, Registry};

use crate::config::{FileConfig, Level, TelemetryConfig};
use crate::error::Error;
use crate::filter::filter_directive;
use crate::telemetry::{self, TelemetryGuard};

/// A boxed, type-erased layer over the global [`Registry`]. Used for the
/// reloadable file and telemetry slots.
pub(crate) type BoxLayer = Box<dyn Layer<Registry> + Send + Sync>;

/// Build the rolling-file logging layer and its worker guard from `cfg`. This is
/// the fallible part of enabling file logging (opening the file / spawning the
/// writer thread); pulling it out lets `install` validate the config *before*
/// the irreversible global-subscriber init, and lets `enable_file` reuse it.
pub(crate) fn build_file_layer(cfg: &FileConfig) -> Result<(BoxLayer, WorkerGuard), Error> {
    let (non_blocking, guard) =
        crate::layers::file::file_writer(cfg).map_err(|e| Error::File(e.to_string()))?;

    let layer: BoxLayer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking)
        .boxed();

    Ok((layer, guard))
}

/// Build the OTLP telemetry layer and its tracer-provider guard from `cfg`. The
/// fallible part of enabling telemetry (constructing the exporter); shared by
/// `install` (pre-init validation) and `enable_telemetry`.
pub(crate) fn build_telemetry_layer(
    cfg: TelemetryConfig,
) -> Result<(BoxLayer, TelemetryGuard), Error> {
    let (layer, guard) = telemetry::init::<Registry>(cfg.endpoint, cfg.resource_attributes)?;
    Ok((layer.boxed(), guard))
}

/// Worker guards that must stay alive for the lifetime of the process: the
/// file-writer worker thread and the OTel tracer provider. Dropping either
/// flushes/stops it.
#[derive(Default)]
pub(crate) struct Guards {
    pub(crate) file_worker: Option<WorkerGuard>,
    pub(crate) telemetry: Option<TelemetryGuard>,
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
    file: reload::Handle<Option<BoxLayer>, Registry>,
    telemetry: reload::Handle<Option<BoxLayer>, Registry>,
    guards: Mutex<Guards>,
}

impl LoggingHandle {
    /// Build the native handle from its reload handles plus any guards for
    /// file/telemetry layers that were seeded at install time. Constructed by
    /// `install`; not public API.
    pub(crate) fn new(
        filter: reload::Handle<EnvFilter, Registry>,
        file: reload::Handle<Option<BoxLayer>, Registry>,
        telemetry: reload::Handle<Option<BoxLayer>, Registry>,
        guards: Guards,
    ) -> Self {
        Self {
            filter,
            file,
            telemetry,
            guards: Mutex::new(guards),
        }
    }

    /// Change the active log level for all libxmtp targets at runtime.
    pub fn set_level(&self, level: Level) -> Result<(), Error> {
        self.filter.reload(filter_directive(level.as_str()))?;
        Ok(())
    }

    /// Turn on rolling-file logging at runtime. Builds the non-blocking file
    /// writer described by `cfg`, installs a JSON fmt layer writing to it, and
    /// keeps the worker guard alive. Replaces any previously-enabled file layer.
    pub fn enable_file(&self, cfg: FileConfig) -> Result<(), Error> {
        let (layer, guard) = build_file_layer(&cfg)?;
        self.file.reload(Some(layer))?;
        self.guards.lock().file_worker = Some(guard);
        Ok(())
    }

    /// Turn off rolling-file logging at runtime. Removes the file layer and drops
    /// the worker guard (which flushes any buffered lines).
    pub fn disable_file(&self) -> Result<(), Error> {
        self.file.reload(None)?;
        self.guards.lock().file_worker = None;
        Ok(())
    }

    /// Turn on OTLP trace export at runtime. Builds the exporter + tracing layer
    /// from `cfg`, installs it in the telemetry slot, and keeps the tracer
    /// provider guard alive. Replaces any previously-enabled telemetry layer.
    pub fn enable_telemetry(&self, cfg: TelemetryConfig) -> Result<(), Error> {
        let (layer, guard) = build_telemetry_layer(cfg)?;
        self.telemetry.reload(Some(layer))?;
        self.guards.lock().telemetry = Some(guard);
        Ok(())
    }

    /// Flush pending telemetry spans (best-effort). File writer lines flush as the
    /// worker drains and on drop; this primarily forces the OTel exporter to push
    /// queued spans, e.g. before process exit.
    pub fn flush(&self) {
        if let Some(t) = self.guards.lock().telemetry.as_ref() {
            t.shutdown();
        }
    }
}
