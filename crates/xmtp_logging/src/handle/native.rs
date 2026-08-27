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
#[cfg(feature = "sentry")]
use crate::sentry::SentryConfig;
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

/// Build the OTLP trace layer, the OTLP logs appender layer, and the guard that
/// owns both providers. Both layers go into the telemetry slot together so they
/// are enabled/disabled atomically.
pub(crate) fn build_telemetry_layer(
    cfg: TelemetryConfig,
) -> Result<(BoxLayer, BoxLayer, TelemetryGuard), Error> {
    let (trace_layer, appender, guard) =
        telemetry::init::<Registry>(cfg.endpoint, cfg.resource_attributes)?;
    Ok((trace_layer.boxed(), appender, guard))
}

/// Worker guards that must stay alive for the lifetime of the process: the
/// file-writer worker thread and the OTel tracer provider. Dropping either
/// flushes/stops it.
#[derive(Default)]
pub(crate) struct Guards {
    pub(crate) file_worker: Option<WorkerGuard>,
    pub(crate) telemetry: Option<TelemetryGuard>,
    #[cfg(feature = "sentry")]
    pub(crate) sentry: Option<sentry::ClientInitGuard>,
    /// What the process hub carried before we took it over, stashed on the
    /// none -> owner transition only. A Rust host embedding libxmtp may have run
    /// its own `sentry::init`, and `disable_sentry` hands that client back rather
    /// than clearing the hub. `Some(None)` records "the host had none", which is
    /// not the same as the `None` meaning "we never took the hub over".
    #[cfg(feature = "sentry")]
    pub(crate) prev_main_client: Option<Option<std::sync::Arc<sentry::Client>>>,
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
    /// Reloadable filter handles for the native layers, driven by
    /// [`Self::set_native_level`]: one on the server/stdout build, one on iOS,
    /// two on Android.
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

    /// Change the native (stdout / logcat / oslog) layer's level at runtime, on
    /// all native targets. Note: reloads with a per-libxmtp-crate filter
    /// (`filter_directive`), so a prior `RUST_LOG` override no longer applies
    /// after the first call.
    pub fn set_native_level(&self, level: Level) -> Result<(), Error> {
        for handle in &self.native_filters {
            handle.reload(crate::filter::filter_directive(level.as_str()))?;
        }
        Ok(())
    }

    /// Turn on rolling-file logging at runtime. Swaps the file writer and level
    /// filter into the always-present file layer in place, keeping the guard alive.
    pub fn enable_file(&self, cfg: FileConfig) -> Result<(), Error> {
        // The fallible part (opening the file / spawning the writer thread) runs
        // first; the infallible slot-swap follows.
        let (non_blocking, guard) =
            crate::layers::file::file_writer(&cfg).map_err(|e| Error::File(e.to_string()))?;
        self.apply_file_writer(non_blocking, guard, cfg.level)?;
        Ok(())
    }

    /// Swap an already-built file writer into the file slot. The infallible half
    /// of file logging — `install` runs the fallible `file_writer` before the
    /// irreversible init and applies it here, keeping `install` retryable.
    pub(crate) fn apply_file_writer(
        &self,
        non_blocking: tracing_appender::non_blocking::NonBlocking,
        guard: WorkerGuard,
        level: Level,
    ) -> Result<(), Error> {
        self.file.modify(|layer| {
            *layer.inner_mut().writer_mut() = EmptyOrFileWriter::File(non_blocking);
            *layer.filter_mut() = filter_directive(level.as_str());
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

    /// Turn on OTLP trace + log export at runtime. Builds the exporter + tracing layer
    /// from `cfg`, installs it in the telemetry slot, and keeps the tracer
    /// provider guard alive. Replaces any previously-enabled telemetry layer.
    pub fn enable_telemetry(&self, cfg: TelemetryConfig) -> Result<(), Error> {
        // Exclusivity check, build, slot reload and guard store are one critical
        // section; see `enable_sentry` for why splitting them races.
        let previous = {
            let mut guards = self.guards.lock();
            #[cfg(feature = "sentry")]
            if guards.sentry.is_some() {
                return Err(Error::Telemetry(
                    "sentry telemetry active; disable it before enabling OTLP".into(),
                ));
            }
            let (trace_layer, appender, guard) = build_telemetry_layer(cfg)?;
            let combined: BoxLayer = vec![trace_layer, appender].boxed();
            match self.telemetry.reload(Some(combined)) {
                Ok(()) => guards.telemetry.replace(guard),
                // Release the lock before the fresh guard's shutdown-on-drop.
                Err(e) => {
                    drop(guards);
                    return Err(e.into());
                }
            }
        };
        // Dropping a `TelemetryGuard` shuts its exporters down, which blocks; do
        // it outside the lock, as `enable_sentry` does.
        drop(previous);
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
        #[cfg(feature = "sentry")]
        if self.guards.lock().sentry.is_some()
            && let Some(client) = sentry::Hub::main().client()
        {
            client.flush(Some(std::time::Duration::from_secs(2)));
        }
    }

    /// Turn on the Sentry backend at runtime. Occupies the same telemetry slot
    /// as OTLP; the two are mutually exclusive. Replaces a prior Sentry layer.
    #[cfg(feature = "sentry")]
    pub fn enable_sentry(&self, cfg: SentryConfig) -> Result<(), Error> {
        // The whole transition — exclusivity check, host-client stash, build,
        // slot reload, guard store — is one critical section. Checking the other
        // backend's guard under a separate short lock lets two concurrent
        // enables both pass their check and install competing layers, leaving
        // one live backend with no layer in the slot and no guard recorded.
        let previous = {
            let mut guards = self.guards.lock();
            if guards.telemetry.is_some() {
                return Err(Error::Telemetry(
                    "OTLP telemetry active; disable it before enabling sentry".into(),
                ));
            }
            // Read before `build_sentry_layer` overwrites the process hub, and only
            // while we are not already the owner: on a re-enable this would otherwise
            // stash our own client and "restore" it on the way out.
            let host_client = guards
                .sentry
                .is_none()
                .then(|| sentry::Hub::main().client());
            // Fallible, and nothing above it mutated state: an early return here
            // leaves the slot and the guards exactly as they were.
            let (layer, guard) = crate::sentry::build_sentry_layer(cfg)?;
            match self.telemetry.reload(Some(layer)) {
                Ok(()) => {
                    if let Some(host) = host_client {
                        guards.prev_main_client = Some(host);
                    }
                    guards.sentry.replace(guard)
                }
                // Release the lock before the fresh guard's blocking drop.
                Err(e) => {
                    drop(guards);
                    return Err(e.into());
                }
            }
        };
        // Drop the previous guard (if any) outside the guards lock: its Drop can
        // block up to the client shutdown timeout, which would stall concurrent
        // flush/set_level/enable_file callers waiting on the same mutex.
        drop(previous);
        Ok(())
    }

    /// Turn off the Sentry backend: empty the slot, flush, drop the client.
    #[cfg(feature = "sentry")]
    pub fn disable_sentry(&self) -> Result<(), Error> {
        // Ownership check, slot clear and guard hand-off in one critical section,
        // so no concurrent enable can interleave between them. Everything that
        // blocks (the flush, the guard's drop) runs after the lock is released.
        let (prev_guard, prev_main) = {
            let mut guards = self.guards.lock();
            // No-op unless we own the telemetry slot: otherwise this would tear down
            // another owner's layer (e.g. OTLP's) without clearing its guard.
            if guards.sentry.is_none() {
                return Ok(());
            }
            self.telemetry.reload(None)?;
            (guards.sentry.take(), guards.prev_main_client.take())
        };
        // Flush the client we own, taken from our own guard rather than looked up
        // on the process hub: by here the hub is no longer authoritative for it.
        if let Some(guard) = &prev_guard {
            guard.flush(Some(std::time::Duration::from_secs(2)));
        }
        // Undo the propagation `build_sentry_layer` did: the process hub is where
        // every later fork (and `bind_task_hub`'s adoption) looks, so a client left
        // there would keep being handed out after we closed it. Restores whatever
        // the host had there rather than clearing, so an embedding Rust app's own
        // `sentry::init` survives our enable/disable cycle. Owner path only — we
        // returned above if the slot is not ours.
        sentry::Hub::main().bind_client(prev_main.flatten());
        crate::sentry::set_user_stable_id(None);
        // Closes the client; blocks up to its shutdown timeout, hence out here.
        drop(prev_guard);
        Ok(())
    }
}
