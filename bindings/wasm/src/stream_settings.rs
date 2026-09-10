use crate::ErrorWrapper;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use xmtp_mls::subscriptions::settings::StreamSettings as RustStreamSettings;

/// Optional positive limits. Omitted fields use the core defaults. Timers use milliseconds.
#[derive(Default, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase", default)]
pub struct StreamSettings {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum rows in one atomic durable admission.
  pub max_admission_rows: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum rows from one network fetch before admission chunks.
  pub max_fetched_rows: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum concurrent dependency and fixed-target requests.
  pub max_dependency_requests: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum candidates in one local message read.
  pub max_local_read_rows: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum encoded bytes in one atomic durable admission.
  pub max_admission_bytes: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum encoded bytes from one network fetch.
  pub max_fetched_bytes: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Total pending group rows across group topics.
  pub group_pending_rows: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Total pending group-envelope bytes across group topics.
  pub group_pending_bytes: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Reserved pending Welcome rows, separate from group capacity.
  pub welcome_pending_rows: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Reserved pending Welcome bytes, separate from group capacity.
  pub welcome_pending_bytes: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Reserved pending identity rows for dependency progress.
  pub identity_pending_rows: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Reserved pending identity bytes for dependency progress.
  pub identity_pending_bytes: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Per-topic row bound that limits one blocked queue.
  pub max_pending_rows_per_topic: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Per-topic byte bound; overflow must not advance F.
  pub max_pending_bytes_per_topic: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Maximum candidate bytes checked before loading message bodies.
  pub max_local_read_bytes: Option<u64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Wait for live receipt before unary catch-up is eligible.
  pub receiver_fallback_interval_ms: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Poll interval for missed wake events and cross-process changes.
  pub active_database_poll_interval_ms: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Time before an unrenewed default-consumer token expires.
  pub default_consumer_lease_duration_ms: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Healthy-primary wait before rejecting an absent exact identity reference.
  pub identity_reference_wait_ms: Option<u32>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Shared deadline for target capture, receipt, and processing.
  pub barrier_timeout_ms: Option<u32>,
}

impl TryFrom<StreamSettings> for RustStreamSettings {
  type Error = JsError;
  fn try_from(options: StreamSettings) -> Result<Self, JsError> {
    let mut value = Self::default();
    if let Some(setting) = options.max_admission_rows {
      value.max_admission_rows = setting;
    }
    if let Some(setting) = options.max_fetched_rows {
      value.max_fetched_rows = setting;
    }
    if let Some(setting) = options.max_dependency_requests {
      value.max_dependency_requests = setting as usize;
    }
    if let Some(setting) = options.max_local_read_rows {
      value.max_local_read_rows = setting;
    }
    if let Some(setting) = options.max_admission_bytes {
      value.max_admission_bytes = setting;
    }
    if let Some(setting) = options.max_fetched_bytes {
      value.max_fetched_bytes = setting;
    }
    if let Some(setting) = options.group_pending_rows {
      value.group_pending.rows = setting;
    }
    if let Some(setting) = options.group_pending_bytes {
      value.group_pending.bytes = setting;
    }
    if let Some(setting) = options.welcome_pending_rows {
      value.welcome_pending.rows = setting;
    }
    if let Some(setting) = options.welcome_pending_bytes {
      value.welcome_pending.bytes = setting;
    }
    if let Some(setting) = options.identity_pending_rows {
      value.identity_pending.rows = setting;
    }
    if let Some(setting) = options.identity_pending_bytes {
      value.identity_pending.bytes = setting;
    }
    if let Some(setting) = options.max_pending_rows_per_topic {
      value.max_pending_rows_per_topic = setting;
    }
    if let Some(setting) = options.max_pending_bytes_per_topic {
      value.max_pending_bytes_per_topic = setting;
    }
    if let Some(setting) = options.max_local_read_bytes {
      value.max_local_read_bytes = setting;
    }
    if let Some(setting) = options.receiver_fallback_interval_ms {
      value.receiver_fallback_interval = std::time::Duration::from_millis(u64::from(setting));
    }
    if let Some(setting) = options.active_database_poll_interval_ms {
      value.active_database_poll_interval = std::time::Duration::from_millis(u64::from(setting));
    }
    if let Some(setting) = options.default_consumer_lease_duration_ms {
      value.default_consumer_lease_duration = std::time::Duration::from_millis(u64::from(setting));
    }
    if let Some(setting) = options.identity_reference_wait_ms {
      value.identity_reference_wait = std::time::Duration::from_millis(u64::from(setting));
    }
    if let Some(setting) = options.barrier_timeout_ms {
      value.barrier_timeout = std::time::Duration::from_millis(u64::from(setting));
    }
    value.validate().map_err(ErrorWrapper::js)?;
    Ok(value)
  }
}
