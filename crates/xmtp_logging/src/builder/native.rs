//! Native builder surface: the rolling-file / telemetry knobs and the `install`
//! that wires up the registry with reloadable file + telemetry slots.

use super::XmtpLoggingBuilder;
use crate::config::{FileConfig, TelemetryConfig};
use crate::error::Error;
use crate::handle::LoggingHandle;

impl XmtpLoggingBuilder {
    /// Configure (or clear) OTLP telemetry export.
    pub fn with_telemetry(mut self, t: Option<TelemetryConfig>) -> Self {
        self.cfg.telemetry = t;
        self
    }

    /// Configure (or clear) rolling-file logging.
    pub fn with_file(mut self, f: Option<FileConfig>) -> Self {
        self.cfg.file = f;
        self
    }

    /// Install the global subscriber and return the runtime-control handle.
    ///
    /// Errors with [`Error::AlreadyInitialized`] if a global subscriber is
    /// already installed for this process.
    pub fn install(self) -> Result<LoggingHandle, Error> {
        use crate::filter::filter_directive;
        use crate::handle::{BoxLayer, Guards, build_telemetry_layer, empty_file_layer};
        use crate::layers::fmt::stdout_layer;
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::{Registry, reload};

        let cfg = self.cfg;

        // Build the fallible telemetry exporter before the irreversible `try_init`,
        // so a bad endpoint errors here and leaves `install` retryable. Only built
        // when an endpoint is set, to avoid an exporter spamming localhost.
        let mut guards = Guards::default();
        let otel_initial: Option<BoxLayer> = match cfg.telemetry {
            Some(t) if t.endpoint.is_some() => {
                let (trace_layer, appender, guard) = build_telemetry_layer(t)?;
                guards.telemetry = Some(guard);
                // Both the trace exporter and the logs appender ride the single
                // telemetry slot; a Vec<BoxLayer> is itself a Layer<Registry>.
                Some(vec![trace_layer, appender].boxed())
            }
            _ => None,
        };

        // Build the file writer before `try_init` (the irreversible step) so a bad
        // log path errors while `install` is still retryable. Swapped into the file
        // slot post-init via the infallible `apply_file_writer`.
        let file_initial: Option<(_, _, _)> = match cfg.file {
            Some(f) => {
                let (non_blocking, guard) =
                    crate::layers::file::file_writer(&f).map_err(|e| Error::File(e.to_string()))?;
                Some((non_blocking, guard, f.level))
            }
            None => None,
        };

        // Slots are pinned to `S = Registry` and collected into one `Vec<BoxLayer>`
        // (added in a single `.with`) rather than chained, which would re-parameterize
        // each layer's subscriber type and break the storable reload handles.

        // Slot 1: reloadable global level filter (driven by `set_level`).
        let (filter_layer, filter_handle) =
            reload::Layer::new(filter_directive(cfg.level.as_str()));

        // Slot 2: stdout, or the native layer (which carries its own reloadable
        // filter handles on mobile; none on the server/stdout path).
        let (primary_layer, native_filters): (BoxLayer, Vec<_>) = if cfg.native {
            crate::layers::native::native_layer(cfg.native_level.unwrap_or(cfg.level))
        } else {
            (stdout_layer::<Registry>(cfg.json), Vec::new())
        };

        // Slot 3: the always-present file layer, seeded empty so its `FilterId` is
        // allocated at build time; the writer + filter are swapped in via `enable_file`.
        let (file_layer, file_handle) = reload::Layer::new(empty_file_layer());

        // Slot 4: reloadable telemetry layer (seeded with the pre-built exporter).
        let (otel_layer, otel_handle) = reload::Layer::new(otel_initial);

        let layers: Vec<BoxLayer> = vec![
            filter_layer.boxed(),
            primary_layer,
            file_layer.boxed(),
            otel_layer.boxed(),
        ];

        // Only fallible step left is the global-default install itself.
        tracing_subscriber::registry()
            .with(layers)
            .try_init()
            .map_err(|_| Error::AlreadyInitialized)?;

        let handle = LoggingHandle::new(
            filter_handle,
            native_filters,
            file_handle,
            otel_handle,
            guards,
        );

        // Apply the pre-built file writer post-init: the layer already exists, so
        // this only swaps the writer + filter into place (infallible — the fallible
        // construction happened before `try_init` above).
        if let Some((non_blocking, guard, level)) = file_initial {
            handle.apply_file_writer(non_blocking, guard, level)?;
        }

        Ok(handle)
    }
}
