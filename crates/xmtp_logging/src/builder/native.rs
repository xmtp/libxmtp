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
        use crate::handle::{BoxLayer, Guards, build_file_layer, build_telemetry_layer};
        use crate::layers::fmt::stdout_layer;
        use crate::layers::native::native_layer;
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::{Registry, reload};

        let cfg = self.cfg;

        // Build the fallible dynamic layers (file writer, OTLP exporter) FIRST,
        // before installing the global subscriber. `try_init` calls
        // `set_global_default`, which is irreversible — so if a bad file path or
        // telemetry endpoint were validated only *after* init, a config error
        // would leave the subscriber installed and the next `install()` would
        // fail with `AlreadyInitialized`, with no way to retry. Constructing them
        // up front means any such error returns here, before init, leaving the
        // process free to retry with a fixed config.
        //
        // Telemetry is only built when an endpoint is set; otherwise the exporter
        // would target localhost and spam connection errors when no collector is
        // present.
        let mut guards = Guards::default();
        let file_initial: Option<BoxLayer> = match cfg.file {
            Some(f) => {
                let (layer, guard) = build_file_layer(&f)?;
                guards.file_worker = Some(guard);
                Some(layer)
            }
            None => None,
        };
        let otel_initial: Option<BoxLayer> = match cfg.telemetry {
            Some(t) if t.endpoint.is_some() => {
                let (layer, guard) = build_telemetry_layer(t)?;
                guards.telemetry = Some(guard);
                Some(layer)
            }
            _ => None,
        };

        // Every reloadable slot is pinned to `S = Registry` so the reload handles
        // have a concrete, storable type. A `Box<dyn Layer<Registry>>` only
        // implements `Layer` over exactly `Registry`, so the slots cannot be
        // chained with `.with(a).with(b)` (which would re-parameterize each
        // layer's subscriber to a `Layered<..>` type). Instead all slots are
        // collected into a single `Vec<BoxLayer>`, which itself implements
        // `Layer<Registry>`, and added to the registry in one `.with(..)`.

        // Slot 1: reloadable level filter. As a bare `EnvFilter` layer it filters
        // every sibling layer in the registry — exactly what `set_level` drives.
        let (filter_layer, filter_handle) =
            reload::Layer::new(filter_directive(cfg.level.as_str()));

        // Slot 2 (fixed): stdout/native fmt output — not reloadable.
        let stdout: BoxLayer = if cfg.native {
            native_layer::<Registry>()
        } else {
            stdout_layer::<Registry>(cfg.json)
        };

        // Slot 3: reloadable file layer (seeded with the pre-built layer, if any).
        let (file_layer, file_handle) = reload::Layer::new(file_initial);

        // Slot 4: reloadable telemetry layer (seeded with the pre-built exporter).
        let (otel_layer, otel_handle) = reload::Layer::new(otel_initial);

        let layers: Vec<BoxLayer> = vec![
            filter_layer.boxed(),
            stdout,
            file_layer.boxed(),
            otel_layer.boxed(),
        ];

        // Only fallible step left is the global-default install itself.
        tracing_subscriber::registry()
            .with(layers)
            .try_init()
            .map_err(|_| Error::AlreadyInitialized)?;

        Ok(LoggingHandle::new(
            filter_handle,
            file_handle,
            otel_handle,
            guards,
        ))
    }
}
