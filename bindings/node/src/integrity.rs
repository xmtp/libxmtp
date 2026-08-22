use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;

#[napi(string_enum)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityCheckLevel {
  Quick,
  Full,
}

impl From<IntegrityCheckLevel> for xmtp_db::prelude::IntegrityCheckLevel {
  fn from(level: IntegrityCheckLevel) -> Self {
    match level {
      IntegrityCheckLevel::Quick => Self::Quick,
      IntegrityCheckLevel::Full => Self::Full,
    }
  }
}

#[napi(object)]
pub struct IntegrityCheckOutcome {
  /// "ok" | "corrupt" | "unreadable" | "saltMissing" | "locked" | "failed"
  pub outcome: String,
  /// Row-level findings (corrupt) or the error/reason string (other
  /// non-ok outcomes). Empty when ok.
  pub findings: Vec<String>,
}

impl From<xmtp_db::prelude::IntegrityCheckResult> for IntegrityCheckOutcome {
  fn from(r: xmtp_db::prelude::IntegrityCheckResult) -> Self {
    use xmtp_db::prelude::IntegrityCheckResult::*;
    let (outcome, findings) = match r {
      Ok => ("ok", vec![]),
      Corrupt { findings } => ("corrupt", findings),
      Unreadable { reason } => ("unreadable", vec![reason]),
      SaltMissing => ("saltMissing", vec![]),
      Locked => ("locked", vec![]),
      Failed { error } => ("failed", vec![error]),
    };
    IntegrityCheckOutcome {
      outcome: outcome.into(),
      findings,
    }
  }
}

/// Read-only integrity check of a database file by path, without a client.
/// Runs off the JS event loop. For encrypted databases pass the same
/// 32-byte encryption key used to create the client.
#[napi]
pub async fn check_database_integrity(
  db_path: String,
  encryption_key: Option<Uint8Array>,
  level: Option<IntegrityCheckLevel>,
) -> Result<IntegrityCheckOutcome> {
  // Same 32-byte validation and error message as
  // `client::create_client::build_store`'s key conversion.
  let key = encryption_key
    .map(|k| -> Result<xmtp_db::EncryptionKey> {
      k.deref()
        .try_into()
        .map_err(|_| Error::from_reason("Malformed 32 byte encryption key"))
    })
    .transpose()?;
  let level = level.map(Into::into).unwrap_or_default();
  let result = tokio::task::spawn_blocking(move || {
    xmtp_db::prelude::check_database_integrity(&db_path, key.as_ref(), level)
  })
  .await
  .map_err(|e| Error::from_reason(e.to_string()))?;
  Ok(result.into())
}
