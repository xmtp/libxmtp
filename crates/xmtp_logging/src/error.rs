//! Error type for the logging pipeline (install + runtime control).

/// Errors raised while installing the global subscriber or driving the runtime
/// [`crate::LoggingHandle`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A global subscriber was already installed for this process. Global
    /// `tracing` init can only happen once.
    #[error("logging already initialized")]
    AlreadyInitialized,
    /// The OTLP span exporter failed to build (e.g. a malformed endpoint).
    #[cfg(not(target_arch = "wasm32"))]
    #[error("telemetry exporter: {0}")]
    Exporter(#[from] opentelemetry_otlp::ExporterBuildError),
    /// Constructing the rolling-file writer failed (e.g. the log directory could
    /// not be created).
    #[error("file logging: {0}")]
    File(String),
    /// A reloadable layer slot could not be swapped (the subscriber was dropped).
    #[error("layer reload: {0}")]
    Reload(#[from] tracing_subscriber::reload::Error),
}
