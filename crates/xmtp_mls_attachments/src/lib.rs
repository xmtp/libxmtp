//! Pending remote attachments owned by a client.

#[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
use std::sync::atomic::AtomicBool;
#[cfg(any(test, feature = "test-hooks"))]
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock, Weak},
    time::Duration,
};

use futures::StreamExt as _;
use parking_lot::Mutex;
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, OwnedMutexGuard, watch};
use tokio_util::sync::CancellationToken;
use xmtp_attachments::{
    AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, AttachmentOptions,
    DownloadSink as _, GcmDecryptor, GcmEncryptor, KeyMaterial, LocalStore, StoreMoveError,
    StoreWriter, Transfer, UploadRequest, attachment_key, ciphertext_len, download_cap,
    encoded_prefix, plaintext_rel_path, remote_attachment, retained_fields_fit, staged_path,
    temporary_path,
};
use xmtp_common::{RetryableError as _, time::now_ns};
use xmtp_content_types::remote_attachment::RemoteAttachment;
use xmtp_db::{
    attachments::{PendingAttachmentOutcome, StoredPendingAttachment},
    prelude::*,
};

use xmtp_events::{AttachmentFailed, AttachmentRef, ClientEvent, EventWriter as _};
use xmtp_proto::{api::grpc_status, backend_v1::CreateUploadRequest};

use xmtp_mls_core::context::CoreContext;

mod create;
mod download;
mod error;
mod local_files;
mod pending;
mod runtime;
mod upload;

pub use create::AttachmentSource;
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
pub use create::pause_next_create_move;
pub use download::DownloadedAttachment;
use download::{CHUNK, DownloadShared};
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
pub use download::{pause_next_download_move, set_next_download_body};
use error::api_error;
pub use error::{AttachmentClientError, CredentialFailureKind};
pub use local_files::LocalAttachment;
use local_files::attachment_reference;
pub use pending::{PendingAttachment, PendingAttachmentStatus};
use pending::{
    PendingAttempt, PendingAttemptDoneGuard, PendingShared, RestorePendingCancel,
    pending_row_matches_key, status_from_row,
};
pub use runtime::AttachmentRuntime;
#[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
use runtime::{LEASE_DURATION, LEASE_RENEW};
#[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
use runtime::{FAIL_NEXT_RECONCILES, LeaseTiming};

/// The attachments feature slot: the client's attachment runtime.
pub trait AttachmentFeatures {
    fn attachments(&self) -> &Arc<AttachmentRuntime>;
}

/// A context the attachment code runs on: core and the attachment runtime.
pub trait AttachmentContext: CoreContext<Features: AttachmentFeatures> {
    fn attachment_runtime(&self) -> &Arc<AttachmentRuntime> {
        self.features().attachments()
    }
}

impl<T: CoreContext<Features: AttachmentFeatures>> AttachmentContext for T {}

pub struct Attachments<Context> {
    pub(crate) context: Context,
}

impl<Context> Attachments<Context> {
    /// The attachment operations of the client on `context`.
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context: AttachmentContext> Attachments<Context> {
    fn runtime(&self) -> &Arc<AttachmentRuntime> {
        self.context.attachment_runtime()
    }

    /// Read whether this client's server configuration offers attachments.
    pub fn offered(&self) -> bool {
        self.context
            .server_configuration()
            .configuration()
            .attachments
            .is_some()
    }
}

/// The worker that sweeps expired pending attachments.
pub mod cleanup;

/// Reads and operations the attachment tests reach a runtime and its pending
/// attachments through, from outside the crate.
#[cfg(all(any(test, feature = "test-utils"), not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod test_support;

/// Controls the attachment tests pause, fail and time a runtime with, from
/// outside the crate. Each one changes what the next operation does.
#[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod test_hooks;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
