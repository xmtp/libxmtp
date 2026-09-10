use crate::ErrorWrapper;
use napi::bindgen_prelude::{BigInt, Result};
use napi_derive::napi;
use xmtp_mls::subscriptions::settings::{
  InvalidStreamSettings, StreamSettings as RustStreamSettings,
};

/// Optional positive limits. Omitted fields use the core defaults. Timers use milliseconds.
#[napi(object)]
#[derive(Default)]
pub struct StreamSettings {
  /// Maximum rows in one atomic durable admission.
  pub max_admission_rows: Option<f64>,
  /// Maximum rows from one network fetch before admission chunks.
  pub max_fetched_rows: Option<f64>,
  /// Maximum concurrent dependency and fixed-target requests.
  pub max_dependency_requests: Option<f64>,
  /// Maximum candidates in one local message read.
  pub max_local_read_rows: Option<f64>,
  /// Maximum encoded bytes in one atomic durable admission.
  pub max_admission_bytes: Option<BigInt>,
  /// Maximum encoded bytes from one network fetch.
  pub max_fetched_bytes: Option<BigInt>,
  /// Total pending group rows across group topics.
  pub group_pending_rows: Option<BigInt>,
  /// Total pending group-envelope bytes across group topics.
  pub group_pending_bytes: Option<BigInt>,
  /// Reserved pending Welcome rows, separate from group capacity.
  pub welcome_pending_rows: Option<BigInt>,
  /// Reserved pending Welcome bytes, separate from group capacity.
  pub welcome_pending_bytes: Option<BigInt>,
  /// Reserved pending identity rows for dependency progress.
  pub identity_pending_rows: Option<BigInt>,
  /// Reserved pending identity bytes for dependency progress.
  pub identity_pending_bytes: Option<BigInt>,
  /// Per-topic row bound that limits one blocked queue.
  pub max_pending_rows_per_topic: Option<BigInt>,
  /// Per-topic byte bound; overflow must not advance F.
  pub max_pending_bytes_per_topic: Option<BigInt>,
  /// Maximum candidate bytes checked before loading message bodies.
  pub max_local_read_bytes: Option<BigInt>,
  /// Wait for live receipt before unary catch-up is eligible.
  pub receiver_fallback_interval_ms: Option<f64>,
  /// Poll interval for missed wake events and cross-process changes.
  pub active_database_poll_interval_ms: Option<f64>,
  /// Time before an unrenewed default-consumer token expires.
  pub default_consumer_lease_duration_ms: Option<f64>,
  /// Healthy-primary wait before rejecting an absent exact identity reference.
  pub identity_reference_wait_ms: Option<f64>,
  /// Shared deadline for target capture, receipt, and processing.
  pub barrier_timeout_ms: Option<f64>,
}

impl TryFrom<StreamSettings> for RustStreamSettings {
  type Error = napi::Error;
  fn try_from(options: StreamSettings) -> Result<Self> {
    let mut value = Self::default();
    if let Some(setting) = options.max_admission_rows {
      value.max_admission_rows = positive_number(setting, "max_admission_rows")?;
    }
    if let Some(setting) = options.max_fetched_rows {
      value.max_fetched_rows = positive_number(setting, "max_fetched_rows")?;
    }
    if let Some(setting) = options.max_dependency_requests {
      value.max_dependency_requests = positive_number(setting, "max_dependency_requests")? as usize;
    }
    if let Some(setting) = options.max_local_read_rows {
      value.max_local_read_rows = positive_number(setting, "max_local_read_rows")?;
    }
    if let Some(setting) = options.max_admission_bytes {
      value.max_admission_bytes = positive_integer(setting, "max_admission_bytes")?;
    }
    if let Some(setting) = options.max_fetched_bytes {
      value.max_fetched_bytes = positive_integer(setting, "max_fetched_bytes")?;
    }
    if let Some(setting) = options.group_pending_rows {
      value.group_pending.rows = positive_integer(setting, "group_pending_rows")?;
    }
    if let Some(setting) = options.group_pending_bytes {
      value.group_pending.bytes = positive_integer(setting, "group_pending_bytes")?;
    }
    if let Some(setting) = options.welcome_pending_rows {
      value.welcome_pending.rows = positive_integer(setting, "welcome_pending_rows")?;
    }
    if let Some(setting) = options.welcome_pending_bytes {
      value.welcome_pending.bytes = positive_integer(setting, "welcome_pending_bytes")?;
    }
    if let Some(setting) = options.identity_pending_rows {
      value.identity_pending.rows = positive_integer(setting, "identity_pending_rows")?;
    }
    if let Some(setting) = options.identity_pending_bytes {
      value.identity_pending.bytes = positive_integer(setting, "identity_pending_bytes")?;
    }
    if let Some(setting) = options.max_pending_rows_per_topic {
      value.max_pending_rows_per_topic = positive_integer(setting, "max_pending_rows_per_topic")?;
    }
    if let Some(setting) = options.max_pending_bytes_per_topic {
      value.max_pending_bytes_per_topic = positive_integer(setting, "max_pending_bytes_per_topic")?;
    }
    if let Some(setting) = options.max_local_read_bytes {
      value.max_local_read_bytes = positive_integer(setting, "max_local_read_bytes")?;
    }
    if let Some(setting) = options.receiver_fallback_interval_ms {
      value.receiver_fallback_interval = std::time::Duration::from_millis(u64::from(
        positive_number(setting, "receiver_fallback_interval_ms")?,
      ));
    }
    if let Some(setting) = options.active_database_poll_interval_ms {
      value.active_database_poll_interval = std::time::Duration::from_millis(u64::from(
        positive_number(setting, "active_database_poll_interval_ms")?,
      ));
    }
    if let Some(setting) = options.default_consumer_lease_duration_ms {
      value.default_consumer_lease_duration = std::time::Duration::from_millis(u64::from(
        positive_number(setting, "default_consumer_lease_duration_ms")?,
      ));
    }
    if let Some(setting) = options.identity_reference_wait_ms {
      value.identity_reference_wait = std::time::Duration::from_millis(u64::from(positive_number(
        setting,
        "identity_reference_wait_ms",
      )?));
    }
    if let Some(setting) = options.barrier_timeout_ms {
      value.barrier_timeout = std::time::Duration::from_millis(u64::from(positive_number(
        setting,
        "barrier_timeout_ms",
      )?));
    }
    value.validate().map_err(ErrorWrapper::from)?;
    Ok(value)
  }
}

// N-API unsigned conversion wraps negative numbers. Check the JavaScript number first.
fn positive_number(value: f64, name: &'static str) -> Result<u32> {
  if !value.is_finite() || value.fract() != 0.0 || value < 1.0 || value > f64::from(u32::MAX) {
    return Err(ErrorWrapper::from(InvalidStreamSettings(name)).into());
  }
  Ok(value as u32)
}

fn positive_integer(value: BigInt, name: &'static str) -> Result<u64> {
  let (negative, value, lossless) = value.get_u64();
  if negative || !lossless {
    return Err(ErrorWrapper::from(InvalidStreamSettings(name)).into());
  }
  Ok(value)
}
