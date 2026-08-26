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
//! SDKs already construct — the same reason the bindings' `UpdateAppDataOptions`
//! and the other FFI options records are structs.
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
//! change as its message is processed. Changes are dispatched one at a time, in
//! the order they were observed — but see [Timeouts](#timeouts): that ordering
//! holds only for callbacks that return within their budget.
//!
//! A callback observes the *net* change across one processed message, and fires
//! for local commits as well as remote ones — an implementation that reacts by
//! writing must make its merge idempotent, or it will chase its own echo.
//!
//! # Timeouts
//!
//! Each callback is given [`UnstableChangeCallbacks::app_data_timeout`] to
//! return. A host that overruns it is abandoned: the expiry is logged and the
//! rest of that batch is dropped, and neither the sync nor the stream fails,
//! because the change being reported is already durably committed and the
//! callback is advisory. Nothing is permanently lost — merges are idempotent,
//! so the next change re-triggers one from current state.
//!
//! Abandoning is not cancelling. libxmtp drops the future and stops waiting;
//! whether the host's own work stops is up to the binding. uniffi notifies the
//! foreign side that the future was dropped, which its Kotlin and Swift
//! bindings can wire to cancelling the task, whereas a JS promise behind napi
//! or wasm-bindgen keeps running to completion — or never resolves — with
//! nothing left listening.
//!
//! That has a consequence worth designing around: on a binding that cannot
//! cancel, an abandoned callback may still publish *after* a later one already
//! did, landing a merge derived from state that has since moved on. The budget
//! bounds how long sync waits; it cannot unwind work the host has already
//! started. **Pass the compare-and-swap guard** — `update_app_data(merged,
//! Some(value_you_were_handed))` — so a late write is superseded at publish
//! time instead of clobbering the newer one. Hosts that publish unguarded get
//! last-writer-wins, and after a timeout "last" is not necessarily "latest".
//!
//! The budget also only bounds callbacks that *yield*. A handler that blocks
//! its thread — synchronous FFI work, a blocking lock, `Thread.sleep` — stalls
//! the task the timer lives on, so the timeout cannot fire and sync waits as
//! long as the host does. This is inherent to async: a future that never
//! returns from `poll` cannot be timed out from inside the same runtime.
//! Callbacks must not block; do blocking work on the host's own executor and
//! await its completion.
//!
//! # Stability
//!
//! Pre-release. The shape of the payloads and of the registry may change
//! without a major version bump until this graduates onto the stable client
//! surface.

use std::sync::Arc;
use xmtp_common::{MaybeSend, MaybeSync, time::Duration};

/// How long a single `app_data` callback may run before it is abandoned.
///
/// Sized against the round trip the callback exists to perform: merge, then
/// publish the result with `update_app_data`, which commits, publishes, and
/// waits for the intent to resolve — a few seconds on a poor mobile network.
/// Ten leaves room for that while keeping the worst case a host can inflict on
/// its own `sync()` call short enough to sit behind a spinner. Raising it buys
/// slow networks more headroom at the cost of a longer visible stall; the
/// balance is why this is a field rather than a hard-coded constant.
pub const DEFAULT_APP_DATA_CALLBACK_TIMEOUT: Duration = Duration::from_secs(10);

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
///
/// Two requirements on an implementation, both from
/// [the module's timeout rules](self#timeouts):
///
/// - **Do not block the calling thread.** Await instead. A handler that blocks
///   cannot be timed out, and stalls the group's sync for as long as it runs.
/// - **Publish with the compare-and-swap guard** if it publishes at all —
///   `update_app_data(merged, Some(change.new_value))`. A handler abandoned at
///   the budget may still be running, and the guard is what stops its late
///   write from overwriting a newer one.
#[xmtp_common::async_trait]
pub trait AppDataChangeCallback: MaybeSend + MaybeSync {
    async fn on_app_data_changed(&self, change: AppDataChange);
}

/// The set of change callbacks registered on a client.
///
/// Cloned into [`crate::context::XmtpMlsLocalContext`] at build time; an unset
/// field costs a single `Option` check on the message-processing path.
#[derive(Clone)]
pub struct UnstableChangeCallbacks {
    /// Fires when a processed message changed the group's `app_data`.
    pub app_data: Option<Arc<dyn AppDataChangeCallback>>,
    /// How long [`Self::app_data`] may run before it is abandoned. Defaults to
    /// [`DEFAULT_APP_DATA_CALLBACK_TIMEOUT`].
    ///
    /// Deliberately *not* mirrored on the FFI registries yet: no SDK caller has
    /// asked to tune it, and exposing it costs a field on each of the uniffi,
    /// napi, and wasm records plus the SDK surfaces above them. It lives here
    /// so tests can shorten it, and so the knob is already additive the day
    /// someone does ask.
    pub app_data_timeout: Duration,
    // Future fields (name, description, image_url, admin_list, permissions,
    // disappearing_settings) go here. Each must default to `None`, and each
    // FFI mirror must carry a binding-level default, so adding one stays
    // non-breaking for compiled SDK callers.
}

impl Default for UnstableChangeCallbacks {
    /// Hand-written rather than derived: the derive would default
    /// `app_data_timeout` to [`Duration::ZERO`], which expires every callback
    /// before it starts.
    fn default() -> Self {
        Self {
            app_data: None,
            app_data_timeout: DEFAULT_APP_DATA_CALLBACK_TIMEOUT,
        }
    }
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
            .field("app_data_timeout", &self.app_data_timeout)
            .finish()
    }
}
