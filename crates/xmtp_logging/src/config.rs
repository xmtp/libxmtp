//! Canonical logging configuration types. Each binding's FFI exposes its own
//! record that maps to these.

/// Log level filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        }
    }
}

/// Rolling-file rotation interval (native file logging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    Minutely,
    Hourly,
    Daily,
    Never,
}

/// Process kind, used in the rolling-file filename suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessType {
    Main,
    NotificationExtension,
}

impl ProcessType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessType::Main => "main",
            ProcessType::NotificationExtension => "notif",
        }
    }
}

/// Rolling-file logging configuration (native only).
#[derive(Debug, Clone)]
pub struct FileConfig {
    pub dir: String,
    pub rotation: Rotation,
    pub max_files: u32,
    pub process_type: ProcessType,
}

/// OTLP trace export configuration (native only).
#[derive(Debug, Clone, Default)]
pub struct TelemetryConfig {
    pub endpoint: Option<String>,
    pub resource_attributes: Vec<(String, String)>,
}

/// Full logging pipeline configuration.
#[derive(Debug, Clone, Default)]
pub struct LoggingConfig {
    pub level: Level,
    pub json: bool,
    pub file: Option<FileConfig>,
    pub telemetry: Option<TelemetryConfig>,
    pub native: bool,
    pub performance: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn level_strings() {
        assert_eq!(Level::Info.as_str(), "info");
        assert_eq!(Level::default(), Level::Info);
    }
}
