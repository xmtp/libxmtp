//! Unstable: FFI mirror of [`xmtp_mls::groups::change_callbacks`].
//!
//! Registered once, at [`crate::mls::create_client`]. See the core module for
//! the delivery contract; the short version is that a callback fires after the
//! commit is durable and the group lock is released, so it may publish the
//! result of its merge from inside the callback.

use std::sync::Arc;
use xmtp_mls::groups::change_callbacks::{
    AppDataChange, AppDataChangeCallback, UnstableChangeCallbacks,
};

/// A change to a group's opaque `app_data`, as observed after it was applied.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiAppDataChange {
    /// The group whose `app_data` changed.
    pub group_id: Vec<u8>,
    /// Value before the change. `None` when nothing was set.
    pub old_value: Option<String>,
    /// Value after the change. `None` when the field was cleared.
    pub new_value: Option<String>,
}

impl From<AppDataChange> for FfiAppDataChange {
    fn from(change: AppDataChange) -> Self {
        Self {
            group_id: change.group_id,
            old_value: change.old_value,
            new_value: change.new_value,
        }
    }
}

/// Notified when a processed message changed a group's `app_data`.
///
/// Async so an implementation can complete a semantic merge — including
/// republishing the merged value via `update_app_data` — before returning.
/// Fires for local commits as well as remote ones, so the merge must be
/// idempotent.
#[uniffi::export(with_foreign)]
#[xmtp_common::async_trait]
pub trait FfiAppDataChangeCallback: Send + Sync + 'static {
    async fn on_app_data_changed(&self, change: FfiAppDataChange);
}

/// Unstable: the set of group-change callbacks to register on a client.
///
/// Only `app_data` exists today. This is a record rather than a bare callback
/// argument so callbacks for the other mutable fields (name, description,
/// image url, admin lists, permissions, disappearing settings) can be added as
/// fields later — same pattern as [`crate::mls::FfiUpdateAppDataOptions`].
///
/// WARNING: uniffi Records get NO default field values unless the field
/// carries `#[uniffi(default = ...)]`. Any field added later MUST carry a
/// uniffi default (and a serde/napi default on the wasm/node mirror), or the
/// generated Swift/Kotlin constructors change and the addition breaks compiled
/// apps.
#[derive(uniffi::Record, Clone, Default)]
pub struct FfiUnstableChangeCallbacks {
    #[uniffi(default = None)]
    pub app_data: Option<Arc<dyn FfiAppDataChangeCallback>>,
}

impl From<FfiUnstableChangeCallbacks> for UnstableChangeCallbacks {
    fn from(callbacks: FfiUnstableChangeCallbacks) -> Self {
        Self {
            app_data: callbacks
                .app_data
                .map(|cb| Arc::new(FfiAppDataChangeCallbackBridge::new(cb)) as _),
            ..Default::default()
        }
    }
}

/// Adapts the foreign-implemented [`FfiAppDataChangeCallback`] to the core
/// trait, mirroring `FfiAuthCallbackBridge`.
pub(crate) struct FfiAppDataChangeCallbackBridge {
    callback: Arc<dyn FfiAppDataChangeCallback>,
}

impl FfiAppDataChangeCallbackBridge {
    pub fn new(callback: Arc<dyn FfiAppDataChangeCallback>) -> Self {
        Self { callback }
    }
}

#[xmtp_common::async_trait]
impl AppDataChangeCallback for FfiAppDataChangeCallbackBridge {
    async fn on_app_data_changed(&self, change: AppDataChange) {
        self.callback.on_app_data_changed(change.into()).await;
    }
}
