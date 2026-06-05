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
//! [`crate::Error::AlreadyInitialized`]).
//!
//! The cross-platform builder surface lives here; the platform-specific bits —
//! the `install()` body that wires up the registry, plus the native-only
//! `with_telemetry` / `with_file` knobs — live in the `native` and `wasm`
//! submodules so neither file is peppered with `#[cfg]`.

use crate::config::{Level, LoggingConfig};

// `install()` and the native-only fluent setters are defined in these platform
// modules as additional `impl XmtpLoggingBuilder` blocks.
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

/// Entry point for the logging builder. Call [`XmtpLogging::builder`].
pub struct XmtpLogging;

impl XmtpLogging {
    /// Start building a logging pipeline with default configuration.
    pub fn builder() -> XmtpLoggingBuilder {
        XmtpLoggingBuilder::default()
    }
}

/// Fluent builder for the logging pipeline. Construct via [`XmtpLogging::builder`]
/// or [`XmtpLoggingBuilder::from_config`], tweak fields, then call `install`
/// (defined per-platform in the `native` / `wasm` submodules).
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

    /// Override the level for the server-compact native fmt layer (only used when
    /// [`Self::with_native`]`(true)`). By default this layer follows the global
    /// `level`; calling this narrows it to `l` independently (narrows only). For
    /// the plain stdout layer (`with_native(false)`) use [`Self::stdout_level`].
    pub fn native_level(mut self, l: Level) -> Self {
        self.cfg.native_level = Some(l);
        self
    }

    /// Override the level for the plain/JSON stdout layer (used when
    /// [`Self::with_native`]`(false)`). By default this layer follows the global
    /// `level`; set this to `Warn` to quiet stdout below the OTLP export level —
    /// e.g. so a log shipper does not duplicate logs already exported via OTLP,
    /// while OTLP still receives `level`.
    pub fn stdout_level(mut self, l: Level) -> Self {
        self.cfg.stdout_level = Some(l);
        self
    }

    /// Use JSON stdout output when `true`, compact otherwise.
    pub fn json(mut self, j: bool) -> Self {
        self.cfg.json = j;
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

    #[test]
    fn native_level_defaults_none_and_is_settable() {
        let b = XmtpLogging::builder();
        assert_eq!(b.cfg.native_level, None);
        let b = b.native_level(Level::Warn);
        assert_eq!(b.cfg.native_level, Some(Level::Warn));
    }

    #[test]
    fn stdout_level_defaults_none_and_is_settable() {
        let b = XmtpLogging::builder();
        assert_eq!(b.cfg.stdout_level, None);
        let b = b.stdout_level(Level::Warn);
        assert_eq!(b.cfg.stdout_level, Some(Level::Warn));
    }

    // A single global-init test: `install()` can only succeed once per process,
    // so all install/runtime-control coverage lives in this one test. It is the
    // only test in this binary that touches the global subscriber.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn install_then_set_level() {
        use crate::error::Error;

        let handle = XmtpLogging::builder()
            .level(Level::Info)
            .install()
            .expect("first install should succeed");

        // Runtime level changes go through the reloadable filter slot.
        handle.set_level(Level::Debug).expect("set_level");
        handle.set_level(Level::Trace).expect("set_level");

        // Non-mobile installs stdout, so the native filter is None → no-op Ok.
        handle
            .set_native_level(Level::Debug)
            .expect("set_native_level no-op ok");

        // The file slot toggles off cleanly even when never enabled.
        handle.disable_file().expect("disable_file");

        // flush is a best-effort no-op when telemetry was never enabled.
        handle.flush();

        // A second global install must fail rather than panic.
        let second = XmtpLogging::builder().install();
        assert!(matches!(second, Err(Error::AlreadyInitialized)));
    }
}
