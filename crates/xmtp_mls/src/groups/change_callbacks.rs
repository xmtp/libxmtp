//! Unstable: post-commit notification of group-state changes.
//!
//! Registered once at client construction ([`crate::builder::ClientBuilder`]),
//! not passed per call — the changes worth reacting to arrive from the stream
//! and sync paths, where no SDK method call is on the stack to carry a
//! parameter.
//!
//! # Why a struct of callbacks
//!
//! Only [`UnstableChangeCallbacks::app_data`] is implemented today. The
//! registry is a struct rather than a bare callback argument so callbacks for
//! the other mutable fields (name, description, image url, admin lists,
//! permissions, disappearing settings) land as *additive* fields on a type the
//! SDKs already construct — the same reason [`crate::groups::UpdateAppDataOptions`]
//! and the FFI options records are structs.
//!
//! # Delivery contract
//!
//! Callbacks fire once the storage transaction has committed, the group commit
//! lock has been released, **and** the per-group sync mutex has been dropped —
//! so an implementation is free to publish the result of its merge with
//! `update_app_data` on the same group. That call re-enters `sync_with_conn`
//! and takes the same sync mutex, which is why dispatch cannot happen inline
//! during message processing; see
//! [`crate::groups::MlsGroup::dispatch_app_data_changes`].
//!
//! Concretely, that means the sync path delivers a batch after the whole sync
//! completes rather than between messages, and the stream path delivers each
//! change as its message is processed. Changes are always awaited one at a
//! time, in the order they were observed.
//!
//! A callback observes the *net* change across one processed message, and fires
//! for local commits as well as remote ones — an implementation that reacts by
//! writing must make its merge idempotent, or it will chase its own echo.
//!
//! # Stability
//!
//! Pre-release. The shape of the payloads and of the registry may change
//! without a major version bump until this graduates onto the stable client
//! surface.

use std::sync::Arc;
use xmtp_common::{MaybeSend, MaybeSync};

/// A change to a group's opaque `app_data` slot, observed after it was applied
/// to local state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDataChange {
    /// The group whose `app_data` changed.
    pub group_id: Vec<u8>,
    /// Value before the message was processed. `None` when the group had no
    /// `app_data` set (or predates the field).
    pub old_value: Option<String>,
    /// Value after the message was processed. `None` when the field was
    /// cleared.
    pub new_value: Option<String>,
}

/// Notified whenever a processed message changed a group's `app_data`.
///
/// Async so an implementation can do the read-modify-write of a semantic merge
/// — including publishing the merged result — before returning.
#[xmtp_common::async_trait]
pub trait AppDataChangeCallback: MaybeSend + MaybeSync {
    async fn on_app_data_changed(&self, change: AppDataChange);
}

/// The set of change callbacks registered on a client.
///
/// Cloned into [`crate::context::XmtpMlsLocalContext`] at build time; an unset
/// field costs a single `Option` check on the message-processing path.
#[derive(Default, Clone)]
pub struct UnstableChangeCallbacks {
    /// Fires when a processed message changed the group's `app_data`.
    pub app_data: Option<Arc<dyn AppDataChangeCallback>>,
    // Future fields (name, description, image_url, admin_list, permissions,
    // disappearing_settings) go here. Each must default to `None`, and each
    // FFI mirror must carry a binding-level default, so adding one stays
    // non-breaking for compiled SDK callers.
}

impl UnstableChangeCallbacks {
    /// Whether anything is watching `app_data`. Gates the before/after
    /// snapshot in message processing so unregistered clients pay nothing.
    pub fn watches_app_data(&self) -> bool {
        self.app_data.is_some()
    }
}

impl std::fmt::Debug for UnstableChangeCallbacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnstableChangeCallbacks")
            .field("app_data", &self.app_data.is_some())
            .finish()
    }
}
