use napi_derive::napi;
use xmtp_configuration::{GrpcUrlsXnet, XmtpEnv as CoreXmtpEnv};
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

#[napi(string_enum)]
#[derive(Debug, Clone, Copy)]
pub enum XmtpEnv {
  Local,
  Dev,
  Production,
  TestnetStaging,
  TestnetDev,
  Testnet,
  Mainnet,
  MigrationLocal,
  MigrationStaging,
  MigrationProduction,
  MigrationXnet,
}

impl XmtpEnv {
  pub fn is_migration(&self) -> bool {
    matches!(
      self,
      Self::MigrationLocal
        | Self::MigrationStaging
        | Self::MigrationProduction
        | Self::MigrationXnet
    )
  }

  pub fn is_migration_xnet(&self) -> bool {
    matches!(self, Self::MigrationXnet)
  }

  /// Returns `(v3_host, gateway_host)` for `MigrationXnet`, else `None`.
  pub fn xnet_hosts(&self) -> Option<(&'static str, &'static str)> {
    if self.is_migration_xnet() {
      Some((GrpcUrlsXnet::NODE, GrpcUrlsXnet::GATEWAY))
    } else {
      None
    }
  }
}

impl From<XmtpEnv> for CoreXmtpEnv {
  fn from(env: XmtpEnv) -> Self {
    match env {
      XmtpEnv::Local => Self::Local,
      XmtpEnv::Dev => Self::Dev,
      XmtpEnv::Production => Self::Production,
      XmtpEnv::TestnetStaging => Self::TestnetStaging,
      XmtpEnv::TestnetDev => Self::TestnetDev,
      XmtpEnv::Testnet => Self::Testnet,
      XmtpEnv::Mainnet => Self::Mainnet,
      XmtpEnv::MigrationLocal => Self::Local,
      XmtpEnv::MigrationStaging => Self::TestnetStaging,
      XmtpEnv::MigrationProduction => Self::Production,
      // MigrationXnet uses runtime-selected hosts (see xnet_hosts); any
      // env-derived URL is bypassed, so Production is a neutral placeholder.
      XmtpEnv::MigrationXnet => Self::Production,
    }
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
