//! Fluent builder that constructs and installs the global logging pipeline.
//!
//! ```ignore
//! let handle = XmtpLogging::builder()
//!     .from_config(cfg)
//!     .install()?;
//! handle.set_level(Level::Debug)?;
//! ```
//!
//! [`XmtpLoggingBuilder::install`] installs a *global* `tracing` subscriber, so
//! it can only succeed once per process (subsequent calls return
//! [`Error::AlreadyInitialized`]).

use crate::config::{Level, LoggingConfig};
use crate::error::Error;
use crate::handle::LoggingHandle;

#[cfg(not(target_arch = "wasm32"))]
use crate::config::{FileConfig, TelemetryConfig};

/// Entry point for the logging builder. Call [`XmtpLogging::builder`].
pub struct XmtpLogging;

impl XmtpLogging {
    /// Start building a logging pipeline with default configuration.
    pub fn builder() -> XmtpLoggingBuilder {
        XmtpLoggingBuilder::default()
    }
}

/// Fluent builder for the logging pipeline. Construct via [`XmtpLogging::builder`]
/// or [`XmtpLoggingBuilder::from_config`], tweak fields, then [`Self::install`].
#[derive(Default)]
pub struct XmtpLoggingBuilder {
    pub(crate) cfg: LoggingConfig,
}

impl XmtpLoggingBuilder {
    /// Build from a fully-specified [`LoggingConfig`].
    pub fn from_config(cfg: LoggingConfig) -> Self {
        Self { cfg }
    }

    /// Set the initial log level.
    pub fn level(mut self, l: Level) -> Self {
        self.cfg.level = l;
        self
    }

    /// Use JSON stdout output when `true`, compact otherwise.
    pub fn json(mut self, j: bool) -> Self {
        self.cfg.json = j;
        self
    }

    /// Configure (or clear) OTLP telemetry export.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_telemetry(mut self, t: Option<TelemetryConfig>) -> Self {
        self.cfg.telemetry = t;
        self
    }

    /// Configure (or clear) rolling-file logging.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_file(mut self, f: Option<FileConfig>) -> Self {
        self.cfg.file = f;
        self
    }

    /// Use the platform native logging layer (logcat/os_log/server-compact)
    /// instead of the plain stdout fmt layer.
    pub fn with_native(mut self, n: bool) -> Self {
        self.cfg.native = n;
        self
    }

    /// Enable the browser performance-timeline layer (wasm only; no-op elsewhere).
    pub fn with_performance(mut self, p: bool) -> Self {
        self.cfg.performance = p;
        self
    }

    /// Install the global subscriber and return the runtime-control handle.
    ///
    /// Errors with [`Error::AlreadyInitialized`] if a global subscriber is
    /// already installed for this process.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install(self) -> Result<LoggingHandle, Error> {
        use crate::filter::filter_directive;
        use crate::handle::BoxLayer;
        use crate::layers::fmt::stdout_layer;
        use crate::layers::native::native_layer;
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::{Registry, reload};

        let cfg = self.cfg;

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

        // Slot 3: reloadable file layer (initially off).
        let (file_layer, file_handle) = reload::Layer::new(None::<BoxLayer>);

        // Slot 4: reloadable telemetry layer (initially off).
        let (otel_layer, otel_handle) = reload::Layer::new(None::<BoxLayer>);

        let layers: Vec<BoxLayer> = vec![
            filter_layer.boxed(),
            stdout,
            file_layer.boxed(),
            otel_layer.boxed(),
        ];

        tracing_subscriber::registry()
            .with(layers)
            .try_init()
            .map_err(|_| Error::AlreadyInitialized)?;

        let handle = LoggingHandle::new(filter_handle, file_handle, otel_handle);

        // Apply the initial dynamic config. Telemetry is only enabled when an
        // endpoint is set (otherwise the exporter would target localhost and
        // spam connection errors when no collector is present).
        if let Some(t) = cfg.telemetry
            && t.endpoint.is_some()
        {
            handle.enable_telemetry(t)?;
        }
        if let Some(f) = cfg.file {
            handle.enable_file(f)?;
        }

        Ok(handle)
    }

    /// Install the global subscriber (wasm). Only the level filter is reloadable;
    /// file logging and telemetry are not available in the browser.
    #[cfg(target_arch = "wasm32")]
    pub fn install(self) -> Result<LoggingHandle, Error> {
        use crate::filter::filter_directive;
        use crate::layers::web::{console_layer, perf_layer};
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::reload;

        let cfg = self.cfg;

        // Unlike native, the wasm layers are chained with `.with(..)` instead of
        // collected into a `Vec<Box<dyn Layer>>`. The browser layers are not
        // `Send + Sync`, and `tracing-subscriber` only implements `Layer` for
        // `Box<dyn Layer + Send + Sync>` — so a boxed `Vec` of them has no `Layer`
        // impl. Chaining the concrete (unboxed) layers sidesteps that entirely.
        // Only the level filter is reloadable; `console`/`perf` are fixed, and the
        // optional perf layer rides through as an `Option<impl Layer>` (which is
        // itself a `Layer`). The layer type parameters are left to inference: each
        // `.with(..)` re-parameterizes the subscriber, so the layers must bind to
        // the accumulated `Layered<..>` type rather than to a pinned `Registry`.
        let (filter_layer, filter_handle) =
            reload::Layer::new(filter_directive(cfg.level.as_str()));

        let perf = cfg.performance.then(|| perf_layer());

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(console_layer())
            .with(perf)
            .try_init()
            .map_err(|_| Error::AlreadyInitialized)?;

        Ok(LoggingHandle::new(filter_handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Level;

    #[test]
    fn builder_from_config_sets_fields() {
        let b = XmtpLoggingBuilder::from_config(LoggingConfig {
            level: Level::Debug,
            json: true,
            ..Default::default()
        });
        assert_eq!(b.cfg.level, Level::Debug);
        assert!(b.cfg.json);
    }

    #[test]
    fn builder_methods_mutate_config() {
        let b = XmtpLogging::builder()
            .level(Level::Trace)
            .json(true)
            .with_native(true)
            .with_performance(true);
        assert_eq!(b.cfg.level, Level::Trace);
        assert!(b.cfg.json);
        assert!(b.cfg.native);
        assert!(b.cfg.performance);
    }

    #[test]
    fn builder_default_is_info_compact() {
        let b = XmtpLogging::builder();
        assert_eq!(b.cfg.level, Level::Info);
        assert!(!b.cfg.json);
        assert!(!b.cfg.native);
    }

    // A single global-init test: `install()` can only succeed once per process,
    // so all install/runtime-control coverage lives in this one test. It is the
    // only test in this binary that touches the global subscriber.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn install_then_set_level() {
        let handle = XmtpLogging::builder()
            .level(Level::Info)
            .install()
            .expect("first install should succeed");

        // Runtime level changes go through the reloadable filter slot.
        handle.set_level(Level::Debug).expect("set_level");
        handle.set_level(Level::Trace).expect("set_level");

        // The file slot toggles off cleanly even when never enabled.
        handle.disable_file().expect("disable_file");

        // flush is a best-effort no-op when telemetry was never enabled.
        handle.flush();

        // A second global install must fail rather than panic.
        let second = XmtpLogging::builder().install();
        assert!(matches!(second, Err(Error::AlreadyInitialized)));
    }
}
