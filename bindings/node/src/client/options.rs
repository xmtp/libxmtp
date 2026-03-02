use napi_derive::napi;
use xmtp_mls::builder::DeviceSyncMode as XmtpSyncWorkerMode;

#[napi(string_enum)]
#[derive(Debug)]
pub enum LogLevel {
  Off,
  Error,
  Warn,
  Info,
  Debug,
  Trace,
}

#[napi(string_enum)]
#[derive(Debug)]
pub enum SyncWorkerMode {
  Enabled,
  Disabled,
}

#[napi(string_enum)]
#[derive(Debug, Default)]
pub enum ClientMode {
  #[default]
  Default,
  Notification,
}

impl From<SyncWorkerMode> for XmtpSyncWorkerMode {
  fn from(value: SyncWorkerMode) -> Self {
    match value {
      SyncWorkerMode::Enabled => Self::Enabled,
      SyncWorkerMode::Disabled => Self::Disabled,
    }
  }
}

impl std::fmt::Display for LogLevel {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    use LogLevel::*;
    let s = match self {
      Off => "off",
      Error => "error",
      Warn => "warn",
      Info => "info",
      Debug => "debug",
      Trace => "trace",
    };
    write!(f, "{}", s)
  }
}

/// Specify options for the logger
#[napi(object)]
#[derive(Default)]
pub struct LogOptions {
  /// enable structured JSON logging to stdout.Useful for third-party log viewers
  /// an option so that it does not require being specified in js object.
  pub structured: Option<bool>,
  /// Filter logs by level
  pub level: Option<LogLevel>,
}
