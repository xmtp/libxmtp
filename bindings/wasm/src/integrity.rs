use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;
use xmtp_db::database::init_sqlite;

use crate::ErrorWrapper;
use crate::client::Client;

#[wasm_bindgen_numbered_enum]
#[derive(Default)]
pub enum IntegrityCheckLevel {
  #[default]
  Quick = 0,
  Full = 1,
}

impl From<IntegrityCheckLevel> for xmtp_db::prelude::IntegrityCheckLevel {
  fn from(level: IntegrityCheckLevel) -> Self {
    match level {
      IntegrityCheckLevel::Quick => Self::Quick,
      IntegrityCheckLevel::Full => Self::Full,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

#[wasm_bindgen]
impl Client {
  /// Read-only integrity check of this client's database.
  ///
  /// Wasm is single-threaded, so this runs the (sync) core check directly
  /// on the current task instead of spawning a blocking thread.
  #[wasm_bindgen(js_name = dbIntegrityCheck)]
  pub async fn db_integrity_check(
    &self,
    level: Option<IntegrityCheckLevel>,
  ) -> Result<IntegrityCheckOutcome, JsError> {
    let level = level.unwrap_or_default();
    let result = self
      .inner_client()
      .db_integrity_check(level.into())
      .map_err(ErrorWrapper::js)?;
    Ok(result.into())
  }
}

/// Read-only integrity check of a database file by path, without a client.
/// Wasm databases are unencrypted, so there is no encryption key parameter.
#[wasm_bindgen(js_name = checkDatabaseIntegrity)]
pub async fn check_database_integrity(
  #[wasm_bindgen(js_name = dbPath)] db_path: String,
  level: Option<IntegrityCheckLevel>,
) -> Result<IntegrityCheckOutcome, JsError> {
  init_sqlite().await;
  let level = level.unwrap_or_default();
  let result = xmtp_db::prelude::check_database_integrity(&db_path, level.into()).await;
  Ok(result.into())
}
