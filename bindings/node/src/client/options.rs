use napi::bindgen_prelude::BigInt;
use napi_derive::napi;
use xmtp_configuration::XmtpEnv as CoreXmtpEnv;
use xmtp_mls::builder::DeviceSyncMode as XmtpSyncWorkerMode;
use xmtp_mls::worker::{WorkerConfig as XmtpWorkerConfig, WorkerKind as XmtpWorkerKind};

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

#[napi(string_enum)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind {
  DeviceSync,
  DisappearingMessages,
  KeyPackageCleaner,
  CommitLog,
  TaskRunner,
  PendingSelfRemove,
}

impl From<WorkerKind> for XmtpWorkerKind {
  fn from(k: WorkerKind) -> Self {
    match k {
      WorkerKind::DeviceSync => Self::DeviceSync,
      WorkerKind::DisappearingMessages => Self::DisappearingMessages,
      WorkerKind::KeyPackageCleaner => Self::KeyPackageCleaner,
      WorkerKind::CommitLog => Self::CommitLog,
      WorkerKind::TaskRunner => Self::TaskRunner,
      WorkerKind::PendingSelfRemove => Self::PendingSelfRemove,
    }
  }
}

/// A single per-worker interval override (nanoseconds).
#[napi(object)]
pub struct WorkerIntervalOverride {
  pub kind: WorkerKind,
  pub interval_ns: BigInt,
}

/// A single per-worker jitter override (nanoseconds).
#[napi(object)]
pub struct WorkerJitterOverride {
  pub kind: WorkerKind,
  pub jitter_ns: BigInt,
}

/// Tuning for the background worker scheduler. All fields optional; omitting
/// the whole object preserves default behavior.
#[napi(object)]
#[derive(Default)]
pub struct WorkerConfigOptions {
  /// Global default interval for all workers, in nanoseconds.
  pub default_interval_ns: Option<BigInt>,
  /// Per-worker interval overrides (nanoseconds).
  pub worker_intervals_ns: Option<Vec<WorkerIntervalOverride>>,
  /// Per-worker jitter overrides (nanoseconds).
  pub worker_jitters_ns: Option<Vec<WorkerJitterOverride>>,
  /// Workers to disable. Anything not listed stays enabled.
  pub disabled_workers: Option<Vec<WorkerKind>>,
}

impl From<WorkerConfigOptions> for XmtpWorkerConfig {
  fn from(o: WorkerConfigOptions) -> Self {
    let to_u64 = |b: BigInt| -> u64 { b.get_u64().1 };
    let mut cfg = XmtpWorkerConfig {
      default_interval_ns: o.default_interval_ns.map(to_u64),
      ..Default::default()
    };
    if let Some(overrides) = o.worker_intervals_ns {
      for ov in overrides {
        cfg
          .interval_overrides
          .insert(ov.kind.into(), to_u64(ov.interval_ns));
      }
    }
    if let Some(jitters) = o.worker_jitters_ns {
      for ov in jitters {
        cfg
          .jitter_overrides
          .insert(ov.kind.into(), to_u64(ov.jitter_ns));
      }
    }
    if let Some(disabled) = o.disabled_workers {
      for k in disabled {
        cfg.enabled.insert(k.into(), false);
      }
    }
    cfg
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
  /// Filter logs by level. Also the level exported to OTLP when `otelEndpoint`
  /// is set (the appender ships events at this level).
  pub level: Option<LogLevel>,
  /// Level for the stdout console layer only. Defaults to `level`. Set to `warn`
  /// to quiet stdout below the OTLP export level — e.g. so a log shipper does not
  /// duplicate logs already exported via OTLP, while OTLP still receives `level`.
  pub stdout_level: Option<LogLevel>,
  /// OTLP endpoint (e.g. "http://collector:4317"). When set, spans AND logs are
  /// exported via OTLP to this endpoint. A downstream OpenTelemetry Collector
  /// derives metrics from the spans and forwards the correlated logs.
  pub otel_endpoint: Option<String>,
  /// Resource attributes attached to all exported spans (e.g.
  /// { "service.instance.id": "herald-7", "deployment.environment": "prod" }).
  /// Use these to attribute telemetry to its source.
  pub resource_attributes: Option<std::collections::HashMap<String, String>>,
}
