use crate::XmtpError;
use std::collections::HashMap;
use std::sync::OnceLock;

static LOGGING: OnceLock<xmtp_logging::LoggingHandle> = OnceLock::new();

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for xmtp_logging::Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Off => Self::Off,
            LogLevel::Error => Self::Error,
            LogLevel::Warn => Self::Warn,
            LogLevel::Info => Self::Info,
            LogLevel::Debug => Self::Debug,
            LogLevel::Trace => Self::Trace,
        }
    }
}

impl From<xmtp_logging::Level> for LogLevel {
    fn from(value: xmtp_logging::Level) -> Self {
        match value {
            xmtp_logging::Level::Off => Self::Off,
            xmtp_logging::Level::Error => Self::Error,
            xmtp_logging::Level::Warn => Self::Warn,
            xmtp_logging::Level::Info => Self::Info,
            xmtp_logging::Level::Debug => Self::Debug,
            xmtp_logging::Level::Trace => Self::Trace,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LoggingOptions {
    #[uniffi(default = None)]
    pub level: Option<LogLevel>,
    #[uniffi(default = false)]
    pub structured: bool,
    #[uniffi(default = false)]
    pub performance: bool,
    #[uniffi(default = None)]
    pub otel: Option<OtelOptions>,
    #[uniffi(default)]
    pub resource_attributes: HashMap<String, String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct OtelOptions {
    pub endpoint: String,
    pub service_name: String,
    #[uniffi(default = 1.0)]
    pub sample_ratio: f64,
}

#[xmtp_macro::sdk_export]
pub fn init_logging(options: LoggingOptions) -> Result<(), XmtpError> {
    let level = options.level.unwrap_or(LogLevel::Info);
    if let Some(handle) = LOGGING.get() {
        return handle.set_level(level.into()).map_err(XmtpError::unknown);
    }
    let builder = xmtp_logging::XmtpLogging::builder()
        .level(level.into())
        .json(options.structured)
        .with_native(!options.structured)
        .with_performance(options.performance);
    #[cfg(not(target_arch = "wasm32"))]
    let builder = builder.with_telemetry(options.otel.map(|otel| xmtp_logging::TelemetryConfig {
        endpoint: Some(otel.endpoint),
        service_name: Some(otel.service_name),
        sample_ratio: otel.sample_ratio,
        logs: true,
        resource_attributes: options.resource_attributes.into_iter().collect(),
    }));
    let handle = builder.install().map_err(XmtpError::unknown)?;
    LOGGING
        .set(handle)
        .map_err(|_| XmtpError::invalid("logging was initialized concurrently"))
}

#[xmtp_macro::sdk_export]
pub fn flush_telemetry() {
    if let Some(handle) = LOGGING.get() {
        handle.flush();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{LOGGING, LogLevel, XmtpError};
    use std::sync::Arc;
    use xmtp_logging::{BoundedSink, LogSinkTarget};

    #[derive(Clone, Debug, uniffi::Record)]
    pub struct LogRecord {
        pub level: LogLevel,
        pub target: String,
        pub message: String,
        pub fields: std::collections::HashMap<String, String>,
        pub timestamp_ns: i64,
        pub dropped_records: u64,
    }

    impl From<xmtp_logging::LogRecord> for LogRecord {
        fn from(value: xmtp_logging::LogRecord) -> Self {
            Self {
                level: value.level.into(),
                target: value.target,
                message: value.message,
                fields: value.fields.into_iter().collect(),
                timestamp_ns: value.timestamp_ns,
                dropped_records: value.dropped_records,
            }
        }
    }

    #[xmtp_macro::callback_error]
    #[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
    pub enum LogSinkError {
        #[error("log sink failed: {reason}")]
        Failed { reason: String },
        #[error("log sink is busy")]
        Busy,
    }

    impl From<uniffi::UnexpectedUniFFICallbackError> for LogSinkError {
        fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
            Self::Failed {
                reason: error.to_string(),
            }
        }
    }

    // Foreign traits need `with_foreign`, which `sdk_export` cannot emit.
    #[uniffi::export(with_foreign)]
    pub trait LogSink: Send + Sync + 'static {
        fn log(&self, record: LogRecord) -> Result<(), LogSinkError>;
    }

    struct SinkBridge(Arc<dyn LogSink>);
    impl LogSinkTarget for SinkBridge {
        fn on_record(
            &self,
            record: xmtp_logging::LogRecord,
        ) -> Result<(), xmtp_logging::SinkError> {
            self.0
                .log(record.into())
                .map_err(|error| Box::new(error) as xmtp_logging::SinkError)
        }
    }

    fn handle() -> Result<&'static xmtp_logging::LoggingHandle, XmtpError> {
        LOGGING
            .get()
            .ok_or_else(|| XmtpError::invalid("init_logging must be called first"))
    }

    #[xmtp_macro::sdk_export]
    pub fn set_log_sink(sink: Option<Arc<dyn LogSink>>) -> Result<(), XmtpError> {
        handle()?.set_sink(sink.map(|sink| Arc::new(SinkBridge(sink)) as Arc<dyn LogSinkTarget>));
        Ok(())
    }

    #[xmtp_macro::sdk_export]
    pub fn set_log_sink_queued(sink: Arc<dyn LogSink>) -> Result<(), XmtpError> {
        let queue = BoundedSink::new(Arc::new(SinkBridge(sink))).map_err(XmtpError::unknown)?;
        handle()?.set_sink(Some(Arc::new(queue) as Arc<dyn LogSinkTarget>));
        Ok(())
    }

    #[xmtp_macro::sdk_export]
    pub fn clear_log_sink() -> Result<(), XmtpError> {
        handle()?.set_sink(None);
        Ok(())
    }

    #[derive(Clone, Copy, Debug, uniffi::Enum)]
    pub enum LogRotation {
        Minutely,
        Hourly,
        Daily,
        Never,
    }
    impl From<LogRotation> for xmtp_logging::Rotation {
        fn from(value: LogRotation) -> Self {
            match value {
                LogRotation::Minutely => Self::Minutely,
                LogRotation::Hourly => Self::Hourly,
                LogRotation::Daily => Self::Daily,
                LogRotation::Never => Self::Never,
            }
        }
    }

    #[derive(Clone, Copy, Debug, uniffi::Enum)]
    pub enum LogProcessType {
        Main,
        Extension,
    }
    impl From<LogProcessType> for xmtp_logging::ProcessType {
        fn from(value: LogProcessType) -> Self {
            match value {
                LogProcessType::Main => Self::Main,
                LogProcessType::Extension => Self::NotificationExtension,
            }
        }
    }

    #[xmtp_macro::sdk_export]
    pub fn enter_debug_writer(
        directory: String,
        rotation: LogRotation,
        max_files: u32,
        level: LogLevel,
        process_type: LogProcessType,
    ) -> Result<(), XmtpError> {
        handle()?
            .enable_file(xmtp_logging::FileConfig {
                dir: directory,
                rotation: rotation.into(),
                max_files,
                process_type: process_type.into(),
                level: level.into(),
            })
            .map_err(XmtpError::unknown)
    }

    #[xmtp_macro::sdk_export]
    pub fn exit_debug_writer() -> Result<(), XmtpError> {
        handle()?.disable_file().map_err(XmtpError::unknown)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        struct Throwing;
        impl LogSink for Throwing {
            fn log(&self, _: LogRecord) -> Result<(), LogSinkError> {
                Err(LogSinkError::Failed {
                    reason: "test failure".into(),
                })
            }
        }

        #[xmtp_common::test]
        fn sink_throw_does_not_panic() {
            let bridge = SinkBridge(Arc::new(Throwing));
            let record = xmtp_logging::LogRecord {
                level: xmtp_logging::Level::Error,
                target: "xmtp_sdk".into(),
                message: "test".into(),
                fields: Default::default(),
                timestamp_ns: 0,
                dropped_records: 0,
            };
            assert!(bridge.on_record(record).is_err());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{LogProcessType, LogRecord, LogRotation, LogSink, LogSinkError};
