//! Unstable: node mirror of [`xmtp_mls::groups::change_callbacks`].
//!
//! Registered once, at client creation. See the core module for the delivery
//! contract; the short version is that a callback fires after the commit is
//! durable and the group lock is released, so it may publish the result of its
//! merge from inside the callback.

use napi::{bindgen_prelude::Promise, threadsafe_function::ThreadsafeFunction};
use napi_derive::napi;
use std::sync::Arc;
use xmtp_mls::groups::change_callbacks::{
  AppDataChange as RustAppDataChange, AppDataChangeCallback as RustAppDataChangeCallback,
  UnstableChangeCallbacks as RustUnstableChangeCallbacks,
};

/// A change to a group's opaque `appData`, as observed after it was applied.
#[napi(object)]
pub struct AppDataChange {
  /// The group whose `appData` changed, hex-encoded.
  pub group_id: String,
  /// Value before the change. `undefined` when nothing was set.
  pub old_value: Option<String>,
  /// Value after the change. `undefined` when the field was cleared.
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

/// Unstable: the set of group-change callbacks to register on a client.
///
/// Only `appData` exists today. This is one object rather than a bare callback
/// argument so callbacks for the other mutable fields (name, description,
/// imageUrl, admin lists, permissions, disappearing settings) can be added as
/// further optional constructor arguments — additive for existing callers.
///
/// A `#[napi]` class rather than an object because `ThreadsafeFunction` cannot
/// be an object field (it is not `ToNapiValue`).
#[napi]
#[derive(Clone, Default)]
pub struct UnstableChangeCallbacks {
  app_data: Option<Arc<ThreadsafeFunction<AppDataChange, Promise<()>>>>,
}

#[napi]
impl UnstableChangeCallbacks {
  /// `appData` is called when a processed message changed the group's
  /// `appData`. The returned promise is awaited before message processing
  /// continues, so a semantic merge — including republishing via
  /// `updateAppData` — can finish first. It fires for local changes as well as
  /// remote ones, so the merge must be idempotent.
  #[napi(
    constructor,
    ts_args_type = "appData?: (change: AppDataChange) => Promise<void>"
  )]
  pub fn new(app_data: Option<ThreadsafeFunction<AppDataChange, Promise<()>>>) -> Self {
    Self {
      app_data: app_data.map(Arc::new),
    }
  }
}

impl From<&UnstableChangeCallbacks> for RustUnstableChangeCallbacks {
  fn from(callbacks: &UnstableChangeCallbacks) -> Self {
    Self {
      app_data: callbacks.app_data.clone().map(|cb| {
        Arc::new(AppDataChangeBridge { callback: cb }) as Arc<dyn RustAppDataChangeCallback>
      }),
    }
  }
}

/// Adapts a JS async function to the core callback trait.
struct AppDataChangeBridge {
  callback: Arc<ThreadsafeFunction<AppDataChange, Promise<()>>>,
}

#[xmtp_common::async_trait]
impl RustAppDataChangeCallback for AppDataChangeBridge {
  async fn on_app_data_changed(&self, change: RustAppDataChange) {
    // A host callback that throws must not derail message processing — the
    // commit is already durable by the time we get here, and the only sane
    // recovery is to let the next change re-trigger the merge.
    match self.callback.call_async(Ok(change.into())).await {
      Ok(promise) => {
        if let Err(err) = promise.await {
          tracing::warn!("appData change callback rejected: {err}");
        }
      }
      Err(err) => tracing::warn!("could not invoke appData change callback: {err}"),
    }
  }
}
