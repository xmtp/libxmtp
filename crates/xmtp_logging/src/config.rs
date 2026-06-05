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
    /// Level filter for the file layer, ANDed with the global filter (can narrow
    /// the file below the global level, not widen it).
    pub level: Level,
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
    /// Level for the server-compact native fmt layer (`native = true`). `None`
    /// (the default) follows the global `level`. `Some(l)` narrows that layer to
    /// `l` independently of the global `level` (narrows only, never widens).
    pub native_level: Option<Level>,
    /// Level for the plain/JSON stdout layer (`native = false`). `None` (the
    /// default) follows the global `level`. `Some(l)` narrows stdout to `l` — e.g.
    /// `Some(Warn)` to quiet stdout so a log shipper does not duplicate the
    /// OTLP-exported stream while OTLP still receives `level`.
    pub stdout_level: Option<Level>,
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

    #[test]
    fn file_config_carries_level() {
        let cfg = FileConfig {
            dir: "/tmp".into(),
            rotation: Rotation::Daily,
            max_files: 3,
            process_type: ProcessType::Main,
            level: Level::Trace,
        };
        assert_eq!(cfg.level, Level::Trace);
    }
}
