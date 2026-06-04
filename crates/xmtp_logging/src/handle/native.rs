//! Native runtime-control handle: reloadable level/file/telemetry slots plus the
//! worker guards that keep the file writer and OTel exporter alive.

use parking_lot::Mutex;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Filtered;
use tracing_subscriber::fmt::format::{Format, Json, JsonFields};
use tracing_subscriber::reload;
use tracing_subscriber::{EnvFilter, Registry};

use crate::config::{FileConfig, Level, TelemetryConfig};
use crate::error::Error;
use crate::filter::filter_directive;
use crate::layers::file::EmptyOrFileWriter;
use crate::telemetry::{self, TelemetryGuard};

/// A boxed, type-erased layer over the global [`Registry`]. Used for the
/// reloadable telemetry slot.
pub(crate) type BoxLayer = Box<dyn Layer<Registry> + Send + Sync>;

/// The concrete, always-present rolling-file fmt layer. Spelled out so the reload
/// handle has a storable type; toggled in place via [`reload::Handle::modify`]
/// rather than added/removed, to keep its per-layer `FilterId` stable.
pub(crate) type FileLayer = Filtered<
    tracing_subscriber::fmt::Layer<Registry, JsonFields, Format<Json>, EmptyOrFileWriter>,
    EnvFilter,
    Registry,
>;

/// The initial (off) file layer seeded at `install()` time: an empty-writer JSON
/// fmt layer with an `off` filter. `enable_file` swaps in the real writer + filter.
pub(crate) fn empty_file_layer() -> FileLayer {
    tracing_subscriber::fmt::layer()
        .json()
        .with_writer(EmptyOrFileWriter::Empty)
        .with_filter(EnvFilter::new("off"))
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
    /// Reloadable filters on the native (logcat/oslog) layers. Empty on non-mobile
    /// builds; one per native layer otherwise.
    native_filters: Vec<reload::Handle<EnvFilter, Registry>>,
    file: reload::Handle<FileLayer, Registry>,
    telemetry: reload::Handle<Option<BoxLayer>, Registry>,
    guards: Mutex<Guards>,
}

impl LoggingHandle {
    /// Build the native handle from its reload handles plus any guards for
    /// file/telemetry layers that were seeded at install time. Constructed by
    /// `install`; not public API.
    pub(crate) fn new(
        filter: reload::Handle<EnvFilter, Registry>,
        native_filters: Vec<reload::Handle<EnvFilter, Registry>>,
        file: reload::Handle<FileLayer, Registry>,
        telemetry: reload::Handle<Option<BoxLayer>, Registry>,
        guards: Guards,
    ) -> Self {
        Self {
            filter,
            native_filters,
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

    /// Change the native (logcat/oslog) layer's log level at runtime. No-op on
    /// non-mobile builds, where the native filter is not reloadable.
    pub fn set_native_level(&self, level: Level) -> Result<(), Error> {
        for handle in &self.native_filters {
            handle.reload(crate::filter::filter_directive(level.as_str()))?;
        }
        Ok(())
    }

    /// Turn on rolling-file logging at runtime. Swaps the file writer and level
    /// filter into the always-present file layer in place, keeping the guard alive.
    pub fn enable_file(&self, cfg: FileConfig) -> Result<(), Error> {
        let (non_blocking, guard) =
            crate::layers::file::file_writer(&cfg).map_err(|e| Error::File(e.to_string()))?;
        self.file.modify(|layer| {
            *layer.inner_mut().writer_mut() = EmptyOrFileWriter::File(non_blocking);
            *layer.filter_mut() = filter_directive(cfg.level.as_str());
        })?;
        self.guards.lock().file_worker = Some(guard);
        Ok(())
    }

    /// Turn off rolling-file logging at runtime. Swaps the writer back to empty and
    /// the filter to `off`, then drops the guard (flushing buffered lines).
    pub fn disable_file(&self) -> Result<(), Error> {
        self.file.modify(|layer| {
            *layer.inner_mut().writer_mut() = EmptyOrFileWriter::Empty;
            *layer.filter_mut() = EnvFilter::new("off");
        })?;
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

    /// Flush pending telemetry spans (best-effort) **without** stopping the
    /// exporter, so logging continues normally afterwards. File writer lines flush
    /// as the worker drains and on drop; this primarily forces the OTel exporter
    /// to push queued spans, e.g. at a checkpoint or before process exit. The
    /// exporter is fully shut down (terminal) when the handle is dropped.
    pub fn flush(&self) {
        if let Some(t) = self.guards.lock().telemetry.as_ref() {
            t.force_flush();
        }
    }
}
