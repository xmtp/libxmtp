//! Unstable: wasm mirror of [`xmtp_mls::groups::change_callbacks`].
//!
//! Registered once, at client creation. See the core module for the delivery
//! contract; the short version is that a callback fires after the commit is
//! durable and the group lock is released, so it may publish the result of its
//! merge from inside the callback.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tsify::Tsify;
use wasm_bindgen::prelude::*;
use xmtp_mls::groups::change_callbacks::{
  AppDataChange as RustAppDataChange, AppDataChangeCallback as RustAppDataChangeCallback,
  UnstableChangeCallbacks as RustUnstableChangeCallbacks,
};

/// A change to a group's opaque `appData`, as observed after it was applied.
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct AppDataChange {
  /// The group whose `appData` changed, hex-encoded.
  pub group_id: String,
  /// Value before the change. Absent when nothing was set.
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub old_value: Option<String>,
  /// Value after the change. Absent when the field was cleared.
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub new_value: Option<String>,
}

impl From<RustAppDataChange> for AppDataChange {
  fn from(change: RustAppDataChange) -> Self {
    Self {
      group_id: hex::encode(change.group_id),
      old_value: change.old_value,
      new_value: change.new_value,
    }
  }
}

#[wasm_bindgen]
extern "C" {
  /// Notified when a processed message changed a group's `appData`.
  ///
  /// `onAppDataChanged` is awaited before message processing continues, so a
  /// semantic merge — including republishing via `updateAppData` — can finish
  /// first. It fires for local changes as well as remote ones, so the merge
  /// must be idempotent.
  pub type AppDataChangeCallback;

  #[wasm_bindgen(catch, method, js_name = onAppDataChanged)]
  pub async fn on_app_data_changed(
    this: &AppDataChangeCallback,
    change: JsValue,
  ) -> Result<JsValue, JsValue>;
}

#[xmtp_common::async_trait]
impl RustAppDataChangeCallback for AppDataChangeCallback {
  async fn on_app_data_changed(&self, change: RustAppDataChange) {
    let change: AppDataChange = change.into();
    let value = match serde_wasm_bindgen::to_value(&change) {
      Ok(value) => value,
      Err(err) => {
        tracing::warn!("could not serialize appData change for callback: {err}");
        return;
      }
    };
    // A host callback that throws must not derail message processing — the
    // commit is already durable by the time we get here, and the only sane
    // recovery is to let the next change re-trigger the merge.
    if let Err(err) = self.on_app_data_changed(value).await {
      tracing::warn!("appData change callback rejected: {err:?}");
    }
  }
}

/// Unstable: the set of group-change callbacks to register on a client.
///
/// Only `appData` exists today. This is one object rather than a bare callback
/// argument so callbacks for the other mutable fields (name, description,
/// imageUrl, admin lists, permissions, disappearing settings) can be added as
/// further optional fields — additive for existing callers.
#[wasm_bindgen]
#[derive(Default)]
pub struct UnstableChangeCallbacks {
  app_data: Option<AppDataChangeCallback>,
}

#[wasm_bindgen]
impl UnstableChangeCallbacks {
  #[wasm_bindgen(constructor)]
  pub fn new() -> Self {
    Self::default()
  }

  #[wasm_bindgen(js_name = "appData")]
  pub fn app_data(&mut self, callback: AppDataChangeCallback) {
    self.app_data = Some(callback);
  }
}

impl From<UnstableChangeCallbacks> for RustUnstableChangeCallbacks {
  fn from(mut callbacks: UnstableChangeCallbacks) -> Self {
    Self {
      app_data: callbacks
        .app_data
        .take()
        .map(|cb| Arc::new(cb) as Arc<dyn RustAppDataChangeCallback>),
    }
  }
}
