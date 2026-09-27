//! Pending remote attachments owned by a client.

#[cfg(test)]
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

impl crate::worker::NeedsDbReconnect for AttachmentClientError {
    fn needs_db_reconnect(&self) -> bool {
        false
    }
}
use xmtp_events::{AttachmentFailed, AttachmentRef, ClientEvent, EventWriter as _};
use xmtp_proto::{api::grpc_status, backend_v1::CreateUploadRequest};

use crate::{client::Client, context::XmtpSharedContext};

const DEFAULT_MAX_PENDING_AGE: Duration = Duration::from_secs(86_400);
const RECONCILE_AGE: Duration = Duration::from_secs(3_600);
const CHUNK: usize = 64 * 1024;
const LEASE_DURATION: Duration = Duration::from_secs(120);
const LEASE_RENEW: Duration = Duration::from_secs(30);
const LEASE_POLL: Duration = Duration::from_secs(1);
#[cfg(test)]
static FAIL_NEXT_RECONCILES: AtomicUsize = AtomicUsize::new(0);
static PUBLICATION_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn shared_publication_lock(dir: Option<&PathBuf>) -> Arc<AsyncMutex<()>> {
    let Some(dir) = dir else {
        return Arc::new(AsyncMutex::new(()));
    };
    #[cfg(not(target_arch = "wasm32"))]
    let path = std::path::absolute(dir).unwrap_or_else(|_| dir.clone());
    #[cfg(target_arch = "wasm32")]
    let path = dir.clone();
    let mut locks = PUBLICATION_LOCKS.lock();
    if let Some(lock) = locks.get(&path).and_then(Weak::upgrade) {
        return lock;
    }
    locks.retain(|_, lock| lock.strong_count() != 0);
    let lock = Arc::new(AsyncMutex::new(()));
    locks.insert(path, Arc::downgrade(&lock));
    lock
}

#[derive(Clone, Copy)]
struct LeaseTiming {
    duration: Duration,
    renew: Duration,
    poll: Duration,
}

impl Default for LeaseTiming {
    fn default() -> Self {
        Self {
            duration: LEASE_DURATION,
            renew: LEASE_RENEW,
            poll: LEASE_POLL,
        }
    }
}

impl LeaseTiming {
    fn duration_ns(self) -> i64 {
        self.duration.as_nanos().min(i64::MAX as u128) as i64
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    fn for_test(duration: Duration, renew: Duration, poll: Duration) -> Self {
        Self {
            duration,
            renew,
            poll,
        }
    }
}

#[derive(Clone, Debug)]
pub enum AttachmentSource {
    Path {
        path: PathBuf,
        filename: Option<String>,
        mime_type: String,
    },
    Bytes {
        bytes: Vec<u8>,
        filename: Option<String>,
        mime_type: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialFailureKind {
    CredentialRejected,
    CallbackFailed,
    Exhausted,
    MissingCredential,
}

impl CredentialFailureKind {
    pub const ALL: [Self; 4] = [
        Self::CredentialRejected,
        Self::CallbackFailed,
        Self::Exhausted,
        Self::MissingCredential,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CredentialRejected => "credential_rejected",
            Self::CallbackFailed => "callback_failed",
            Self::Exhausted => "exhausted",
            Self::MissingCredential => "missing_credential",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "credential_rejected" => Some(Self::CredentialRejected),
            "callback_failed" => Some(Self::CallbackFailed),
            "exhausted" => Some(Self::Exhausted),
            "missing_credential" => Some(Self::MissingCredential),
            _ => None,
        }
    }
}

/// A stable attachment cause with the credential detail, when applicable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("attachment failure: {}", cause.as_str())]
pub struct AttachmentClientError {
    pub cause: Cause,
    pub credential_kind: Option<CredentialFailureKind>,
    pub retryable: bool,
    /// True when the backend rejected the credential's scope.
    pub missing_scope: bool,
    /// Final status from the storage target or download host.
    pub http_status: Option<u16>,
}

impl AttachmentClientError {
    fn new(cause: Cause) -> Self {
        Self {
            cause,
            credential_kind: None,
            retryable: false,
            missing_scope: false,
            http_status: None,
        }
    }
}

impl From<AttachmentError> for AttachmentClientError {
    fn from(error: AttachmentError) -> Self {
        Self {
            http_status: error.http_status,
            ..Self::new(error.cause)
        }
    }
}

impl From<StoreMoveError> for AttachmentClientError {
    fn from(error: StoreMoveError) -> Self {
        match error {
            StoreMoveError::DestinationExists => Self::new(Cause::LocalStorage),
            StoreMoveError::Other(error) => error.into(),
        }
    }
}

fn api_error(error: xmtp_api::ApiError) -> AttachmentClientError {
    if let xmtp_api::ApiError::Auth(auth) = &error {
        use xmtp_proto::api::AuthError;
        let credential_kind = match auth {
            AuthError::CredentialRejected { .. } => CredentialFailureKind::CredentialRejected,
            AuthError::CallbackFailed { .. } => CredentialFailureKind::CallbackFailed,
            AuthError::Exhausted | AuthError::ExhaustedAfterAttempt => {
                CredentialFailureKind::Exhausted
            }
            AuthError::MissingCredential => CredentialFailureKind::MissingCredential,
        };
        return AttachmentClientError {
            cause: Cause::Credential,
            credential_kind: Some(credential_kind),
            retryable: auth.is_retryable(),
            missing_scope: false,
            http_status: None,
        };
    }
    if grpc_status(&error).is_some_and(|status| status.code() == tonic::Code::PermissionDenied) {
        return AttachmentClientError {
            missing_scope: true,
            ..AttachmentClientError::new(Cause::Credential)
        };
    }
    let rejected = grpc_status(&error).is_some_and(|status| {
        matches!(
            status.code(),
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange | tonic::Code::Unimplemented
        )
    });
    AttachmentClientError::new(if rejected {
        Cause::BackendRejected
    } else {
        Cause::BackendUnavailable
    })
}

fn attachment_reference(remote: &RemoteAttachment, key: &str) -> AttachmentRef {
    AttachmentRef {
        attachment_key: key.to_owned(),
        url: remote.url.clone(),
        content_digest: remote.content_digest.clone(),
    }
}

struct DecoderSink {
    writer: StoreWriter,
    hash: Sha256,
    cipher: GcmDecryptor,
    decoder: AttachmentDecoder,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl xmtp_attachments::DownloadSink for DecoderSink {
    async fn write(&mut self, bytes: &[u8]) -> Result<(), AttachmentError> {
        self.hash.update(bytes);
        let mut plaintext = Vec::with_capacity(bytes.len());
        self.cipher.update(bytes, &mut plaintext)?;
        for content in self.decoder.push(&plaintext)? {
            self.writer.write_content(content).await?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingAttachmentStatus {
    Waiting,
    Uploading,
    Complete,
    Failed(AttachmentClientError),
}

fn failure_from_row(row: &StoredPendingAttachment) -> AttachmentClientError {
    let cause = row
        .failure_cause
        .as_deref()
        .and_then(Cause::parse)
        .unwrap_or_else(|| {
            tracing::warn!(stored = ?row.failure_cause, "unknown stored attachment failure cause");
            Cause::LocalStorage
        });
    let credential_kind = row.failure_credential_kind.as_deref().and_then(|stored| {
        let parsed = CredentialFailureKind::parse(stored);
        if parsed.is_none() {
            tracing::warn!(stored, "unknown stored attachment credential failure kind");
        }
        parsed
    });
    AttachmentClientError {
        cause,
        credential_kind,
        retryable: row.failure_retryable.unwrap_or(false),
        missing_scope: row.failure_missing_scope.unwrap_or(false),
        http_status: row
            .failure_http_status
            .and_then(|status| u16::try_from(status).ok()),
    }
}

fn status_from_row(row: &StoredPendingAttachment, now: i64) -> PendingAttachmentStatus {
    match row.effective_status(now) {
        "waiting" => PendingAttachmentStatus::Waiting,
        "uploading" => PendingAttachmentStatus::Uploading,
        "complete" => PendingAttachmentStatus::Complete,
        "failed" => PendingAttachmentStatus::Failed(failure_from_row(row)),
        _ => PendingAttachmentStatus::Failed(AttachmentClientError::new(Cause::LocalStorage)),
    }
}

fn outcome_write_is_retryable(error: &xmtp_db::StorageError) -> bool {
    use xmtp_db::{ConnectionError, PlatformStorageError, StorageError, diesel::result::Error};

    fn diesel_locked(error: &Error) -> bool {
        let Error::DatabaseError(_, information) = error else {
            return false;
        };
        matches!(
            information.message(),
            "database is locked"
                | "database table is locked"
                | "database schema is locked"
                | "database is busy"
        )
    }

    fn platform_transient(error: &PlatformStorageError) -> bool {
        match error {
            PlatformStorageError::DieselResult(error) => diesel_locked(error),
            #[cfg(not(target_arch = "wasm32"))]
            PlatformStorageError::Pool(_)
            | PlatformStorageError::DbConnection(_)
            | PlatformStorageError::PoolNeedsConnection
            | PlatformStorageError::DatabaseLocked
            | PlatformStorageError::DieselConnect(_) => true,
            #[cfg(target_arch = "wasm32")]
            PlatformStorageError::SAH(_)
            | PlatformStorageError::Connection(_)
            | PlatformStorageError::Disconnected
            | PlatformStorageError::DatabaseInUse => true,
            _ => false,
        }
    }

    match error {
        StorageError::DieselResult(error)
        | StorageError::Connection(ConnectionError::Database(error)) => diesel_locked(error),
        StorageError::Platform(error)
        | StorageError::Connection(ConnectionError::Platform(error)) => platform_transient(error),
        StorageError::DieselConnect(_) => true,
        _ => false,
    }
}

struct PendingShared {
    state: AsyncMutex<()>,
    watch: watch::Sender<PendingAttachmentStatus>,
    lease: Mutex<Option<([u8; 16], i64)>>,
    attempt: Mutex<Option<Arc<PendingAttempt>>>,
    cancel: CancellationToken,
}

struct PendingAttempt {
    outcome: watch::Sender<Option<Result<(), AttachmentClientError>>>,
    done: CancellationToken,
}

struct PendingAttemptDoneGuard {
    shared: Arc<PendingShared>,
    attempt: Arc<PendingAttempt>,
}

impl Drop for PendingAttemptDoneGuard {
    fn drop(&mut self) {
        let mut active = self.shared.attempt.lock();
        let was_active = active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &self.attempt));
        if was_active {
            *self.shared.lease.lock() = None;
            *active = None;
        }
        drop(active);
        if was_active {
            let status = self.shared.watch.borrow().clone();
            self.shared.watch.send_replace(status);
        }
        self.attempt.done.cancel();
    }
}

impl PendingAttempt {
    fn new() -> Self {
        let (outcome, _) = watch::channel(None);
        Self {
            outcome,
            done: CancellationToken::new(),
        }
    }
}

impl PendingShared {
    fn new() -> Self {
        let (watch, _) = watch::channel(PendingAttachmentStatus::Waiting);
        Self {
            state: AsyncMutex::new(()),
            watch,
            lease: Mutex::new(None),
            attempt: Mutex::new(None),
            cancel: CancellationToken::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadedAttachment {
    pub path: PathBuf,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAttachment {
    pub path: String,
    pub created_at_ns: i64,
}

struct DownloadShared {
    outcome: watch::Sender<Option<Result<DownloadedAttachment, AttachmentClientError>>>,
    cancel: CancellationToken,
}

struct DownloadAttemptGuard<Context: XmtpSharedContext + 'static> {
    context: Context,
    remote: RemoteAttachment,
    relative: String,
    key: String,
    shared: Arc<DownloadShared>,
    finished: bool,
}

async fn publish_download_outcome<Context: XmtpSharedContext>(
    context: &Context,
    remote: &RemoteAttachment,
    relative: &str,
    key: &str,
    shared: &Arc<DownloadShared>,
    result: Result<DownloadedAttachment, AttachmentClientError>,
) {
    let runtime = context.attachment_runtime();
    let lock = runtime.event_lock(key);
    let _guard = lock.lock().await;
    if shared.outcome.borrow().is_some() {
        return;
    }
    let reference = attachment_reference(remote, key);
    let event = match &result {
        Ok(_) => ClientEvent::AttachmentDownloadCompleted(reference),
        Err(error) => ClientEvent::AttachmentDownloadFailed(AttachmentFailed {
            attachment_key: key.to_owned(),
            url: reference.url,
            content_digest: reference.content_digest,
            cause: error.cause.as_str().to_owned(),
        }),
    };
    context.events().emit(Some(event), None);
    let mut downloads = runtime.downloads.lock();
    if downloads
        .get(relative)
        .is_some_and(|current| Arc::ptr_eq(current, shared))
    {
        downloads.remove(relative);
    }
    shared.outcome.send_replace(Some(result));
}

impl<Context: XmtpSharedContext + 'static> DownloadAttemptGuard<Context> {
    async fn finish(&mut self, result: Result<DownloadedAttachment, AttachmentClientError>) {
        publish_download_outcome(
            &self.context,
            &self.remote,
            &self.relative,
            &self.key,
            &self.shared,
            result,
        )
        .await;
        self.finished = true;
    }
}

impl<Context: XmtpSharedContext + 'static> Drop for DownloadAttemptGuard<Context> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let context = self.context.clone();
        let remote = self.remote.clone();
        let relative = self.relative.clone();
        let key = self.key.clone();
        let shared = self.shared.clone();
        drop(xmtp_common::task::spawn(async move {
            publish_download_outcome(
                &context,
                &remote,
                &relative,
                &key,
                &shared,
                Err(AttachmentClientError::new(Cause::LocalStorage)),
            )
            .await;
        }));
    }
}

struct DeleteInProgress<'a> {
    paths: &'a Mutex<HashMap<String, usize>>,
    path: String,
}

impl<'a> DeleteInProgress<'a> {
    fn new(paths: &'a Mutex<HashMap<String, usize>>, path: String) -> Self {
        *paths.lock().entry(path.clone()).or_default() += 1;
        Self { paths, path }
    }
}

impl Drop for DeleteInProgress<'_> {
    fn drop(&mut self) {
        let mut paths = self.paths.lock();
        if let Some(count) = paths.get_mut(&self.path) {
            *count -= 1;
            if *count == 0 {
                paths.remove(&self.path);
            }
        }
    }
}

impl DownloadShared {
    fn new() -> Self {
        let (outcome, _) = watch::channel(None);
        Self {
            outcome,
            cancel: CancellationToken::new(),
        }
    }
}

#[derive(Clone, Default)]
struct CreateRollbackState {
    plain_temp: Option<String>,
    staged_temp: Option<String>,
    final_plain: Option<String>,
    final_staged: Option<String>,
    created_key_dir: Option<String>,
    local_row: Option<String>,
    pending_row: Option<String>,
}

async fn rollback_create<Context: XmtpSharedContext>(
    context: &Context,
    store: &Arc<dyn LocalStore>,
    state: &CreateRollbackState,
) {
    if let Some(digest) = &state.pending_row {
        let _ = context.db().delete_pending_attachment(digest);
    }
    if let Some(path) = &state.local_row {
        let _ = context.db().delete_local_attachment(path);
    }
    for path in [
        &state.plain_temp,
        &state.staged_temp,
        &state.final_plain,
        &state.final_staged,
    ]
    .into_iter()
    .flatten()
    {
        let _ = store.remove_file(path).await;
    }
    if let Some(key_dir) = &state.created_key_dir {
        let _ = store.remove_empty_dir(key_dir).await;
    }
}

struct CreateRollbackGuard<Context: XmtpSharedContext + 'static> {
    context: Context,
    store: Arc<dyn LocalStore>,
    state: CreateRollbackState,
    publication: Option<OwnedMutexGuard<()>>,
    committed: bool,
}

impl<Context: XmtpSharedContext + 'static> CreateRollbackGuard<Context> {
    async fn rollback(&mut self) {
        rollback_create(&self.context, &self.store, &self.state).await;
        self.committed = true;
    }
}

impl<Context: XmtpSharedContext + 'static> Drop for CreateRollbackGuard<Context> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let context = self.context.clone();
        let store = self.store.clone();
        let state = self.state.clone();
        let publication = self.publication.take();
        drop(xmtp_common::task::spawn(async move {
            let _publication = publication;
            rollback_create(&context, &store, &state).await;
        }));
    }
}

#[doc(hidden)]
pub struct AttachmentRuntime {
    pub(crate) store: Option<Arc<dyn LocalStore>>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) options: AttachmentOptions,
    pending: Mutex<HashMap<String, Weak<PendingShared>>>,
    downloads: Mutex<HashMap<String, Arc<DownloadShared>>>,
    deleting: Mutex<HashMap<String, usize>>,
    event_locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    publication_lock: Arc<AsyncMutex<()>>,
    reconciled: OnceCell<()>,
    lease_timing: Mutex<LeaseTiming>,
    #[cfg(test)]
    sweep_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(test)]
    delete_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    outcome_lock_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    create_move_pause: Mutex<Option<Arc<CreateMovePause>>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    create_publish_pause: Mutex<Option<Arc<CreatePublishPause>>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    attempt_panic_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    download_panic_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    download_move_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    reconcile_snapshot_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(test)]
    outcome_write_errors: AtomicUsize,
    #[cfg(test)]
    lease_extension_errors: AtomicUsize,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
struct CreateMovePause {
    path: Mutex<Option<String>>,
    staged_path: Mutex<Option<String>>,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
struct CreatePublishPause {
    after_staged: bool,
    path: Mutex<Option<String>>,
    staged_path: Mutex<Option<String>>,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

impl Default for AttachmentRuntime {
    fn default() -> Self {
        Self {
            store: None,
            dir: None,
            options: AttachmentOptions::default(),
            pending: Mutex::new(HashMap::new()),
            downloads: Mutex::new(HashMap::new()),
            deleting: Mutex::new(HashMap::new()),
            event_locks: Mutex::new(HashMap::new()),
            publication_lock: Arc::new(AsyncMutex::new(())),
            reconciled: OnceCell::new(),
            lease_timing: Mutex::new(LeaseTiming::default()),
            #[cfg(test)]
            sweep_pause: Mutex::new(None),
            #[cfg(test)]
            delete_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            outcome_lock_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            create_move_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            create_publish_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            attempt_panic_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            download_panic_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            download_move_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            reconcile_snapshot_pause: Mutex::new(None),
            #[cfg(test)]
            outcome_write_errors: AtomicUsize::new(0),
            #[cfg(test)]
            lease_extension_errors: AtomicUsize::new(0),
        }
    }
}

impl AttachmentRuntime {
    pub(crate) async fn ensure_reconciled<Context: XmtpSharedContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        self.reconciled
            .get_or_try_init(|| self.reconcile(context))
            .await
            .map(|_| ())
    }

    pub(crate) async fn new(
        dir: Option<PathBuf>,
        options: AttachmentOptions,
    ) -> Result<Self, AttachmentClientError> {
        let publication_lock = shared_publication_lock(dir.as_ref());
        let store: Option<Arc<dyn LocalStore>> = match dir.as_ref() {
            None => None,
            #[cfg(not(target_arch = "wasm32"))]
            Some(path) => Some(Arc::new(xmtp_attachments::NativeStore::new(path).await?)),
            #[cfg(target_arch = "wasm32")]
            Some(path) => Some(Arc::new(
                xmtp_attachments::OpfsStore::new(&path.to_string_lossy()).await?,
            )),
        };
        Ok(Self {
            store,
            dir,
            options,
            pending: Mutex::new(HashMap::new()),
            downloads: Mutex::new(HashMap::new()),
            deleting: Mutex::new(HashMap::new()),
            event_locks: Mutex::new(HashMap::new()),
            publication_lock,
            reconciled: OnceCell::new(),
            lease_timing: Mutex::new(LeaseTiming::default()),
            #[cfg(test)]
            sweep_pause: Mutex::new(None),
            #[cfg(test)]
            delete_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            outcome_lock_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            create_move_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            create_publish_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            attempt_panic_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            download_panic_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            download_move_pause: Mutex::new(None),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            reconcile_snapshot_pause: Mutex::new(None),
            #[cfg(test)]
            outcome_write_errors: AtomicUsize::new(0),
            #[cfg(test)]
            lease_extension_errors: AtomicUsize::new(0),
        })
    }

    fn store(&self) -> Result<&Arc<dyn LocalStore>, AttachmentClientError> {
        self.store
            .as_ref()
            .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))
    }

    fn shared(&self, digest: &str) -> Arc<PendingShared> {
        let mut pending = self.pending.lock();
        if let Some(shared) = pending.get(digest).and_then(Weak::upgrade) {
            return shared;
        }
        let shared = Arc::new(PendingShared::new());
        pending.insert(digest.to_owned(), Arc::downgrade(&shared));
        shared
    }

    fn event_lock(&self, key: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.event_locks.lock();
        if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(AsyncMutex::new(()));
        locks.insert(key.to_owned(), Arc::downgrade(&lock));
        lock
    }

    fn cutoff(&self) -> i64 {
        let age = self
            .options
            .max_pending_age
            .unwrap_or(DEFAULT_MAX_PENDING_AGE);
        now_ns().saturating_sub(age.as_nanos().min(i64::MAX as u128) as i64)
    }

    /// Delete expired rows before their staged files. An active upload stays valid.
    pub(crate) async fn sweep<Context: XmtpSharedContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let rows = context
            .db()
            .pending_attachment_sweep_candidates(self.cutoff())
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        for digest in rows {
            let shared = self.shared(&digest);
            let _state = shared.state.lock().await;
            #[cfg(test)]
            {
                let pause = self.sweep_pause.lock().clone();
                if let Some((entered, resume)) = pause {
                    entered.notify_one();
                    resume.notified().await;
                }
            }
            let deleted = context
                .db()
                .sweep_pending_attachment(&digest, self.cutoff(), now_ns())
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            if deleted == 0 {
                continue;
            }
            let path = staged_path(&digest)?;
            if store.exists(&path).await? {
                store.remove_file(&path).await?;
            }
            self.pending.lock().remove(&digest);
        }
        Ok(())
    }

    pub(crate) async fn reconcile<Context: XmtpSharedContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        let _publication = self.publication_lock.lock().await;
        #[cfg(test)]
        if FAIL_NEXT_RECONCILES
            .fetch_update(AtomicOrdering::SeqCst, AtomicOrdering::SeqCst, |count| {
                count.checked_sub(1)
            })
            .is_ok()
        {
            return Err(AttachmentClientError::new(Cause::LocalStorage));
        }
        let Some(store) = &self.store else {
            return Ok(());
        };
        let pending: HashMap<_, _> = context
            .db()
            .list_pending_attachments_since(0)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            .into_iter()
            .map(|row| (row.content_digest, row.status))
            .collect();
        let records = context
            .db()
            .list_local_attachments()
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        let recorded: HashSet<_> = records.iter().map(|row| row.path.as_str()).collect();
        let files = store.list_files().await?;
        let present: HashSet<_> = files.iter().map(|file| file.path.as_str()).collect();
        #[cfg(all(test, not(target_arch = "wasm32")))]
        {
            let pause = self.reconcile_snapshot_pause.lock().clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
            }
        }
        let now = now_ns();
        let cutoff = now.saturating_sub(RECONCILE_AGE.as_nanos() as i64);
        let staged_age = self
            .options
            .max_pending_age
            .unwrap_or(DEFAULT_MAX_PENDING_AGE)
            .min(RECONCILE_AGE);
        let staged_cutoff = now.saturating_sub(staged_age.as_nanos() as i64);
        for file in &files {
            if file.path.starts_with(".tmp/") {
                if file.modified_at_ns < cutoff {
                    store.remove_file(&file.path).await?;
                }
            } else if let Some(digest) = file.path.strip_prefix(".staged/") {
                if pending
                    .get(digest)
                    .is_some_and(|status| status == "complete")
                    || (!pending.contains_key(digest) && file.modified_at_ns < staged_cutoff)
                {
                    store.remove_file(&file.path).await?;
                }
            } else if file.path.split_once('/').is_some_and(|(key, name)| {
                key.len() == 64
                    && key
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    && !name.is_empty()
                    && !name.contains('/')
            }) && !recorded.contains(file.path.as_str())
            {
                context
                    .db()
                    .insert_or_ignore_local_attachment(&file.path, file.modified_at_ns, None, None)
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            }
        }
        for record in records {
            if !present.contains(record.path.as_str()) {
                context
                    .db()
                    .delete_local_attachment(&record.path)
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            }
        }
        Ok(())
    }
}

pub struct Attachments<Context> {
    context: Context,
}

impl<Context: XmtpSharedContext> Client<Context> {
    pub fn attachments(&self) -> Attachments<Context> {
        Attachments {
            context: self.context.clone(),
        }
    }
}

impl<Context: XmtpSharedContext> Attachments<Context> {
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

    /// Derive the plaintext path without touching the file system.
    pub fn local_path(&self, remote: &RemoteAttachment) -> Result<PathBuf, AttachmentClientError> {
        let relative = plaintext_rel_path(remote)?;
        let dir = self
            .runtime()
            .dir
            .as_ref()
            .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))?;
        Ok(dir.join(relative))
    }

    pub async fn list_local(&self) -> Result<Vec<LocalAttachment>, AttachmentClientError> {
        self.runtime().ensure_reconciled(&self.context).await?;
        let _publication = self.runtime().publication_lock.lock().await;
        let store = self.runtime().store()?;
        let rows = self.context
            .db()
            .list_local_attachments()
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        let mut local = Vec::with_capacity(rows.len());
        for row in rows {
            if store.exists(&row.path).await? && store.is_regular_file(&row.path).await? {
                local.push(LocalAttachment {
                    path: row.path,
                    created_at_ns: row.created_at_ns,
                });
            }
        }
        Ok(local)
    }

    /// Fetch one verified attachment when its plaintext file is absent.
    pub async fn download(
        &self,
        remote: &RemoteAttachment,
    ) -> Result<DownloadedAttachment, AttachmentClientError> {
        self.runtime().ensure_reconciled(&self.context).await?;
        let relative = plaintext_rel_path(remote)?;
        let path = self.local_path(remote)?;
        let key = attachment_key(remote)?;
        let store = self.runtime().store()?;
        let lock = self.runtime().event_lock(&key);
        let shared = {
            let _guard = lock.lock().await;
            if self.runtime().deleting.lock().contains_key(&key) {
                return Err(AttachmentClientError::new(Cause::Deleted));
            }
            if store.exists(&relative).await? {
                if !store.is_regular_file(&relative).await? {
                    return Err(AttachmentClientError::new(Cause::LocalStorage));
                }
                self.context
                    .db()
                    .insert_or_ignore_local_attachment(&relative, now_ns(), None, None)
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
                let record = self
                    .context
                    .db()
                    .get_local_attachment(&relative)
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
                    .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))?;
                return Ok(DownloadedAttachment {
                    path,
                    mime_type: record.mime_type,
                    filename: record.filename,
                });
            }
            if let Some(shared) = self.runtime().downloads.lock().get(&relative).cloned() {
                shared
            } else {
                let shared = Arc::new(DownloadShared::new());
                self.runtime()
                    .downloads
                    .lock()
                    .insert(relative.clone(), shared.clone());
                let reference = attachment_reference(remote, &key);
                self.context.events().emit(
                    Some(ClientEvent::AttachmentDownloadStarted(reference)),
                    None,
                );
                let task = Attachments {
                    context: self.context.context_ref().clone(),
                };
                let attempt = DownloadAttemptGuard {
                    context: task.context.clone(),
                    remote: remote.clone(),
                    relative: relative.clone(),
                    key: key.clone(),
                    shared: shared.clone(),
                    finished: false,
                };
                drop(xmtp_common::task::spawn(async move {
                    let mut attempt = attempt;
                    #[cfg(all(test, not(target_arch = "wasm32")))]
                    let panic_pause = { task.runtime().download_panic_pause.lock().take() };
                    #[cfg(all(test, not(target_arch = "wasm32")))]
                    if let Some((entered, resume)) = panic_pause {
                        entered.notify_one();
                        resume.notified().await;
                        panic!("forced download attempt panic");
                    }
                    let result = task
                        .download_once(&attempt.remote, &attempt.relative, &attempt.shared.cancel)
                        .await;
                    attempt.finish(result).await;
                }));
                shared
            }
        };
        let mut outcome = shared.outcome.subscribe();
        loop {
            let current = outcome.borrow_and_update().clone();
            if let Some(result) = current {
                return result;
            }
            outcome
                .changed()
                .await
                .map_err(|_| AttachmentClientError::new(Cause::Network))?;
        }
    }

    async fn download_once(
        &self,
        remote: &RemoteAttachment,
        relative: &str,
        cancel: &CancellationToken,
    ) -> Result<DownloadedAttachment, AttachmentClientError> {
        let store = self.runtime().store()?;
        let material = KeyMaterial::from_remote(remote)?;
        let suffix = hex::encode(xmtp_common::rand_array::<16>());
        let content_tmp = temporary_path(&format!("{suffix}-content"))?;
        let decoded_tmp = temporary_path(&format!("{suffix}-decoded"))?;
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(AttachmentClientError::new(Cause::Deleted)),
            result = async {
            let writer = store.create_temp(&content_tmp).await?;
            let mut sink = DecoderSink {
                writer, hash: Sha256::new(),
                cipher: GcmDecryptor::new(&material), decoder: AttachmentDecoder::new(),
            };
            let snapshot_limit = self.context.server_configuration().configuration().attachments
                .as_ref().map_or(xmtp_configuration::BACKEND_DEFAULT_MAX_UPLOAD_BYTES, |offer| offer.max_upload_bytes);
            let cap = download_cap(remote.content_length.map(u64::from), snapshot_limit, &self.runtime().options);
            Transfer::new(self.runtime().options.clone())?.get(&remote.url, cap, &mut sink).await?;
            let DecoderSink { mut writer, hash, cipher, decoder } = sink;
            store.sync(&mut writer).await?;
            drop(writer);
            if hex::encode(hash.finalize()) != remote.content_digest {
                return Err(AttachmentClientError::new(Cause::DigestMismatch));
            }
            cipher.finish()?;
            let meta = store.finish_decode(decoder, &content_tmp, &decoded_tmp).await?;
            let final_tmp = if meta.compressed { &decoded_tmp } else { &content_tmp };
            #[cfg(all(test, not(target_arch = "wasm32")))]
            {
                let pause = self.runtime().download_move_pause.lock().clone();
                if let Some((entered, resume)) = pause {
                    entered.notify_one();
                    resume.notified().await;
                }
            }
            match store.rename(final_tmp, relative).await {
                Ok(()) => {}
                Err(StoreMoveError::DestinationExists) => {
                    if !store.is_regular_file(relative).await? {
                        return Err(AttachmentClientError::new(Cause::LocalStorage));
                    }
                    self.context.db().insert_or_ignore_local_attachment(
                        relative, now_ns(), None, None
                    ).map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
                    let record = self.context.db().get_local_attachment(relative)
                        .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
                        .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))?;
                    return Ok(DownloadedAttachment {
                        path: self.local_path(remote)?,
                        mime_type: record.mime_type,
                        filename: record.filename,
                    });
                }
                Err(error) => return Err(error.into()),
            }
            if self.context.db().insert_or_ignore_local_attachment(
                relative, now_ns(), Some(meta.mime_type.clone()), meta.filename.clone()
            )
            .is_err() {
                if let Err(error) = store.remove_file(relative).await {
                    tracing::warn!(%error, "could not remove downloaded file after metadata write failed");
                }
                return Err(AttachmentClientError::new(Cause::LocalStorage));
            }
            Ok(DownloadedAttachment {
                path: self.local_path(remote)?, mime_type: Some(meta.mime_type), filename: meta.filename,
            })
            } => result,
        };
        for temp in [&content_tmp, &decoded_tmp] {
            if store.exists(temp).await.unwrap_or(false) {
                let _ = store.remove_file(temp).await;
            }
        }
        result
    }

    /// Delete all local files and records for a remote attachment.
    pub async fn delete_local(
        &self,
        remote: &RemoteAttachment,
    ) -> Result<(), AttachmentClientError> {
        let task = Attachments {
            context: self.context.context_ref().clone(),
        };
        let remote = remote.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let result = task.delete_local_inner(&remote).await;
            let _ = sender.send(result);
        }));
        receiver
            .await
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
    }

    async fn delete_local_inner(
        &self,
        remote: &RemoteAttachment,
    ) -> Result<(), AttachmentClientError> {
        let key = attachment_key(remote)?;
        let staged = staged_path(&remote.content_digest)?;
        let store = self.runtime().store()?;
        let lock = self.runtime().event_lock(&key);
        let (upload_attempt, downloads, _deleting) = {
            let _guard = lock.lock().await;
            // Marks the whole key directory, so no download into it can start.
            let deleting = DeleteInProgress::new(&self.runtime().deleting, key.clone());
            let upload = self
                .runtime()
                .pending
                .lock()
                .get(&remote.content_digest)
                .and_then(Weak::upgrade);
            let prefix = format!("{key}/");
            let downloads: Vec<_> = self
                .runtime()
                .downloads
                .lock()
                .iter()
                .filter(|(path, _)| path.starts_with(&prefix))
                .map(|(_, shared)| shared.clone())
                .collect();
            let upload_attempt = upload
                .as_ref()
                .filter(|shared| shared.lease.lock().is_some())
                .and_then(|shared| shared.attempt.lock().clone());
            if let Some(shared) = &upload {
                shared.cancel.cancel();
            }
            for shared in &downloads {
                shared.cancel.cancel();
            }
            (upload_attempt, downloads, deleting)
        };
        #[cfg(test)]
        {
            let pause = self.runtime().delete_pause.lock().clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
            }
        }
        if let Some(attempt) = upload_attempt {
            attempt.done.cancelled().await;
        }
        for shared in downloads {
            let mut outcome = shared.outcome.subscribe();
            while outcome.borrow_and_update().clone().is_none() {
                outcome
                    .changed()
                    .await
                    .map_err(|_| AttachmentClientError::new(Cause::Network))?;
            }
        }
        let _publication = self.runtime().publication_lock.lock().await;
        let _guard = lock.lock().await;
        let mut emitted = false;
        let mut emit_deleted = || {
            if !emitted {
                self.context.events().emit(
                    Some(ClientEvent::AttachmentDeleted(attachment_reference(
                        remote, &key,
                    ))),
                    None,
                );
                emitted = true;
            }
        };
        // Remove the row first. An upload in another client sees the deletion
        // before this client removes its staged ciphertext.
        self.context
            .db()
            .delete_pending_attachment(&remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        if store.exists(&key).await? {
            store.remove_dir_all(&key).await?;
            emit_deleted();
        }
        if store.exists(&staged).await? {
            store.remove_file(&staged).await?;
            emit_deleted();
        }
        let removed_rows = self
            .context
            .db()
            .delete_local_attachments_in_dir(&key)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        if removed_rows != 0 {
            emit_deleted();
        }
        self.runtime().pending.lock().remove(&remote.content_digest);
        Ok(())
    }

    /// Stage one source and return its complete remote description before any request.
    pub async fn create(
        &self,
        source: AttachmentSource,
    ) -> Result<PendingAttachment<Context>, AttachmentClientError> {
        let offer = self
            .context
            .server_configuration()
            .configuration()
            .attachments
            .as_ref()
            .ok_or_else(|| AttachmentClientError::new(Cause::NotOffered))?;
        let store = self.runtime().store()?;
        let fallback = match &source {
            AttachmentSource::Path {
                path,
                filename: None,
                ..
            } => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            _ => None,
        };
        let (source_filename, source_mime_type) = match &source {
            AttachmentSource::Path {
                filename,
                mime_type,
                ..
            }
            | AttachmentSource::Bytes {
                filename,
                mime_type,
                ..
            } => (
                filename.as_deref().or(fallback.as_deref()),
                mime_type.as_str(),
            ),
        };
        if !retained_fields_fit(source_filename, source_mime_type) {
            return Err(AttachmentClientError::new(Cause::TooLarge));
        }
        let filename = source_filename.map(str::to_owned);
        let mime_type = source_mime_type.to_owned();
        let size = match &source {
            AttachmentSource::Path { path, .. } => {
                #[cfg(target_arch = "wasm32")]
                {
                    let source_store = xmtp_attachments::OpfsStore::new_root()
                        .await
                        .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                    let source_path = path.to_string_lossy().into_owned();
                    source_store
                        .open_read(&source_path)
                        .await
                        .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?
                        .len()
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::fs::metadata(path)
                        .await
                        .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?
                        .len()
                }
            }
            AttachmentSource::Bytes { bytes, .. } => bytes.len() as u64,
        };
        let prefix = encoded_prefix(filename.as_deref(), &mime_type, size);
        let length = ciphertext_len(prefix.len(), size);
        if length > offer.max_upload_bytes || length > u32::MAX as u64 {
            return Err(AttachmentClientError::new(Cause::TooLarge));
        }
        let id: [u8; 16] = xmtp_common::rand_array();
        let plain_temp = temporary_path(&format!("{}-plain", hex::encode(id)))?;
        let staged_temp = temporary_path(&format!("{}-staged", hex::encode(id)))?;
        let publication = self.runtime().publication_lock.clone().lock_owned().await;
        let mut rollback = CreateRollbackGuard {
            context: self.context.context_ref().clone(),
            store: store.clone(),
            state: CreateRollbackState {
                plain_temp: Some(plain_temp.clone()),
                staged_temp: Some(staged_temp.clone()),
                ..Default::default()
            },
            publication: Some(publication),
            committed: false,
        };
        let result: Result<PendingAttachment<Context>, AttachmentClientError> = async {
            let mut plain = store.create_temp(&plain_temp).await?;
            let mut staged = store.create_temp(&staged_temp).await?;
            let material = KeyMaterial::random();
            let mut encryptor = GcmEncryptor::new(&material);
            let mut hash = Sha256::new();
            let mut encrypted = Vec::with_capacity(CHUNK + prefix.len());
            encryptor.update(&prefix, &mut encrypted)?;
            staged.write(&encrypted).await?;
            hash.update(&encrypted);
            encrypted.clear();
            match source {
                AttachmentSource::Path { path, .. } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        use tokio::io::AsyncReadExt as _;
                        let mut file = tokio::fs::File::open(path)
                            .await
                            .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                        let mut chunk = [0u8; CHUNK];
                        let mut read_total = 0u64;
                        loop {
                            let count = file
                                .read(&mut chunk)
                                .await
                                .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                            if count == 0 {
                                break;
                            }
                            read_total += count as u64;
                            if read_total > size {
                                return Err(AttachmentClientError::new(Cause::SourceUnreadable));
                            }
                            plain.write(&chunk[..count]).await?;
                            encryptor.update(&chunk[..count], &mut encrypted)?;
                            staged.write(&encrypted).await?;
                            hash.update(&encrypted);
                            encrypted.clear();
                        }
                        if read_total != size {
                            return Err(AttachmentClientError::new(Cause::SourceUnreadable));
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let source_store = xmtp_attachments::OpfsStore::new_root()
                            .await
                            .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                        let source_path = path.to_string_lossy().into_owned();
                        let file = source_store
                            .open_read(&source_path)
                            .await
                            .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                        let mut read_total = 0u64;
                        loop {
                            let chunk = file
                                .read_chunk(read_total, CHUNK)
                                .await
                                .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                            if chunk.is_empty() {
                                break;
                            }
                            read_total += chunk.len() as u64;
                            if read_total > size {
                                return Err(AttachmentClientError::new(Cause::SourceUnreadable));
                            }
                            plain.write(&chunk).await?;
                            encryptor.update(&chunk, &mut encrypted)?;
                            staged.write(&encrypted).await?;
                            hash.update(&encrypted);
                            encrypted.clear();
                        }
                        if read_total != size {
                            return Err(AttachmentClientError::new(Cause::SourceUnreadable));
                        }
                    }
                }
                AttachmentSource::Bytes { bytes, .. } => {
                    for chunk in bytes.chunks(CHUNK) {
                        plain.write(chunk).await?;
                        encryptor.update(chunk, &mut encrypted)?;
                        staged.write(&encrypted).await?;
                        hash.update(&encrypted);
                        encrypted.clear();
                    }
                }
            }
            let tag = encryptor.finish();
            staged.write(&tag).await?;
            hash.update(tag);
            store.sync(&mut plain).await?;
            store.sync(&mut staged).await?;
            drop(plain);
            drop(staged);
            let hex_digest = hex::encode(hash.finalize());
            let remote = remote_attachment(
                &offer.base_url,
                &hex_digest,
                &material,
                length as u32,
                filename.as_deref(),
            );
            let local = plaintext_rel_path(&remote)?;
            let ciphertext = staged_path(&hex_digest)?;
            #[cfg(all(test, not(target_arch = "wasm32")))]
            let create_pause = { self.runtime().create_move_pause.lock().clone() };
            #[cfg(all(test, not(target_arch = "wasm32")))]
            if let Some(pause) = create_pause {
                *pause.path.lock() = Some(local.clone());
                *pause.staged_path.lock() = Some(ciphertext.clone());
                pause.entered.notify_one();
                pause.resume.notified().await;
            }
            let key_dir = attachment_key(&remote)?;
            if store.create_dir_if_absent(&key_dir).await? {
                rollback.state.created_key_dir = Some(key_dir);
            }
            match store.rename(&plain_temp, &local).await {
                Ok(()) => {
                    rollback.state.plain_temp = None;
                    rollback.state.final_plain = Some(local.clone());
                }
                Err(StoreMoveError::DestinationExists) => {
                    if !store.is_regular_file(&local).await? {
                        return Err(AttachmentClientError::new(Cause::LocalStorage));
                    }
                    store.remove_file(&plain_temp).await?;
                    rollback.state.plain_temp = None;
                }
                Err(error) => return Err(error.into()),
            }
            #[cfg(all(test, not(target_arch = "wasm32")))]
            let publish_pause = { self.runtime().create_publish_pause.lock().clone() };
            #[cfg(all(test, not(target_arch = "wasm32")))]
            if let Some(pause) = publish_pause
                && !pause.after_staged
            {
                *pause.path.lock() = Some(local.clone());
                *pause.staged_path.lock() = Some(ciphertext.clone());
                pause.entered.notify_one();
                pause.resume.notified().await;
            }
            match store.rename(&staged_temp, &ciphertext).await {
                Ok(()) => {
                    rollback.state.staged_temp = None;
                    rollback.state.final_staged = Some(ciphertext.clone());
                }
                Err(StoreMoveError::DestinationExists) => {
                    let valid = match store.open_read(&ciphertext).await {
                        Ok(file) => matches!(
                            file.sha256().await,
                            Ok((digest, stored_length))
                                if stored_length == length && hex::encode(digest) == hex_digest
                        ),
                        Err(_) => false,
                    };
                    if valid {
                        store.remove_file(&staged_temp).await?;
                        rollback.state.staged_temp = None;
                    } else {
                        store.replace(&staged_temp, &ciphertext).await?;
                        rollback.state.staged_temp = None;
                        rollback.state.final_staged = Some(ciphertext.clone());
                    }
                }
                Err(error) => return Err(error.into()),
            }
            #[cfg(all(test, not(target_arch = "wasm32")))]
            let publish_pause = { self.runtime().create_publish_pause.lock().clone() };
            #[cfg(all(test, not(target_arch = "wasm32")))]
            if let Some(pause) = publish_pause
                && pause.after_staged
            {
                *pause.path.lock() = Some(local.clone());
                *pause.staged_path.lock() = Some(ciphertext.clone());
                pause.entered.notify_one();
                pause.resume.notified().await;
            }
            let db = self.context.db();
            let inserted = db
                .insert_local_attachment_if_absent(
                    &local,
                    now_ns(),
                    Some(mime_type.clone()),
                    filename.clone(),
                )
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            if inserted {
                rollback.state.local_row = Some(local.clone());
            }
            let inserted = db
                .insert_pending_attachment_if_absent(&hex_digest, &remote.encode_to_vec(), now_ns())
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            if !inserted {
                return Err(AttachmentClientError::new(Cause::LocalStorage));
            }
            rollback.state.pending_row = Some(hex_digest.clone());
            Ok(PendingAttachment {
                context: self.context.clone(),
                remote,
                shared: self.runtime().shared(&hex_digest),
            })
        }
        .await;
        if result.is_err() {
            rollback.rollback().await;
        } else {
            rollback.committed = true;
        }
        result
    }

    /// Return the held pending attachment for this digest.
    pub async fn pending(
        &self,
        remote: &RemoteAttachment,
    ) -> Result<PendingAttachment<Context>, AttachmentClientError> {
        self.runtime().ensure_reconciled(&self.context).await?;
        let row = self
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            .filter(|row| {
                row.created_at_ns >= self.runtime().cutoff()
                    || row.status == "uploading"
                        && row.lease_expires_at_ns.is_some_and(|end| end >= now_ns())
            })
            .ok_or_else(|| AttachmentClientError::new(Cause::StagedUnusable))?;
        let remote = RemoteAttachment::decode(row.remote_attachment.as_slice())
            .map_err(|_| AttachmentClientError::new(Cause::StagedUnusable))?;
        Ok(PendingAttachment {
            context: self.context.clone(),
            shared: self.runtime().shared(&remote.content_digest),
            remote,
        })
    }

    pub async fn list_pending(
        &self,
    ) -> Result<Vec<PendingAttachment<Context>>, AttachmentClientError> {
        self.runtime().ensure_reconciled(&self.context).await?;
        self.context
            .db()
            .list_pending_attachments_since(0)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            .into_iter()
            .filter(|row| {
                row.status != "complete"
                    && (row.created_at_ns >= self.runtime().cutoff()
                        || row.status == "uploading"
                            && row.lease_expires_at_ns.is_some_and(|end| end >= now_ns()))
            })
            .map(|row| {
                let remote = RemoteAttachment::decode(row.remote_attachment.as_slice())
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
                Ok(PendingAttachment {
                    context: self.context.clone(),
                    shared: self.runtime().shared(&remote.content_digest),
                    remote,
                })
            })
            .collect()
    }
}

pub struct PendingAttachment<Context> {
    context: Context,
    remote: RemoteAttachment,
    shared: Arc<PendingShared>,
}

impl<Context: Clone> Clone for PendingAttachment<Context> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            remote: self.remote.clone(),
            shared: self.shared.clone(),
        }
    }
}

impl<Context: XmtpSharedContext> PendingAttachment<Context> {
    pub fn remote_attachment(&self) -> &RemoteAttachment {
        &self.remote
    }

    pub fn local_path(&self) -> Result<PathBuf, AttachmentClientError> {
        let dir = self
            .context
            .attachment_runtime()
            .dir
            .as_ref()
            .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))?;
        Ok(dir.join(plaintext_rel_path(&self.remote)?))
    }

    pub fn status(&self) -> PendingAttachmentStatus {
        if self
            .shared
            .lease
            .lock()
            .as_ref()
            .is_some_and(|(_, until)| now_ns() < *until)
            && matches!(
                *self.shared.watch.borrow(),
                PendingAttachmentStatus::Uploading
            )
        {
            return PendingAttachmentStatus::Uploading;
        }
        match self.record() {
            Ok(Some(row)) => status_from_row(&row, now_ns()),
            Ok(None) => PendingAttachmentStatus::Failed(AttachmentClientError::new(
                if self.shared.cancel.is_cancelled() {
                    Cause::Deleted
                } else {
                    Cause::StagedUnusable
                },
            )),
            Err(_) => self.shared.watch.borrow().clone(),
        }
    }

    pub fn watch_status(&self) -> watch::Receiver<PendingAttachmentStatus> {
        self.shared.watch.send_replace(self.status());
        let receiver = self.shared.watch.subscribe();
        let pending = PendingAttachment {
            context: self.context.context_ref().clone(),
            remote: self.remote.clone(),
            shared: self.shared.clone(),
        };
        drop(xmtp_common::task::spawn(async move {
            while pending.shared.watch.receiver_count() > 0 {
                let poll = pending
                    .context
                    .attachment_runtime()
                    .lease_timing
                    .lock()
                    .poll;
                xmtp_common::time::sleep(poll).await;
                let status = pending.status();
                if *pending.shared.watch.borrow() != status {
                    pending.shared.watch.send_replace(status);
                }
            }
        }));
        receiver
    }

    fn reference(&self) -> AttachmentRef {
        AttachmentRef {
            attachment_key: xmtp_attachments::attachment_key(&self.remote).unwrap_or_default(),
            url: self.remote.url.clone(),
            content_digest: self.remote.content_digest.clone(),
        }
    }

    fn record(&self) -> Result<Option<StoredPendingAttachment>, AttachmentClientError> {
        self.context
            .db()
            .get_pending_attachment(&self.remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))
    }

    async fn wait_for_record_change(
        &self,
        local_attempt: Option<Arc<PendingAttempt>>,
    ) -> Result<PendingAttachmentStatus, AttachmentClientError> {
        let mut watch = self.shared.watch.subscribe();
        let mut attempt_watch = local_attempt.map(|attempt| attempt.outcome.subscribe());
        loop {
            if let Some(result) = attempt_watch
                .as_ref()
                .and_then(|receiver| receiver.borrow().clone())
            {
                return Ok(match result {
                    Ok(()) => PendingAttachmentStatus::Complete,
                    Err(error) => PendingAttachmentStatus::Failed(error),
                });
            }
            match self.record() {
                Ok(Some(row)) => {
                    let status = status_from_row(&row, now_ns());
                    if status != PendingAttachmentStatus::Uploading
                        && self.shared.lease.lock().is_none()
                    {
                        self.shared.watch.send_replace(status.clone());
                        return Ok(status);
                    }
                }
                Ok(None) => {
                    return Err(AttachmentClientError::new(Cause::Deleted));
                }
                Err(error) => {
                    tracing::warn!(%error, "pending attachment status read will be retried")
                }
            }
            let poll = self.context.attachment_runtime().lease_timing.lock().poll;
            tokio::select! {
                _ = xmtp_common::time::sleep(poll) => {},
                changed = watch.changed() => {
                    if changed.is_err() {
                        return Err(AttachmentClientError::new(Cause::LocalStorage));
                    }
                }
                changed = async {
                    if let Some(receiver) = attempt_watch.as_mut() {
                        receiver.changed().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if changed.is_err() {
                        attempt_watch = None;
                    }
                }
            }
        }
    }

    pub async fn upload(&self) -> Result<(), AttachmentClientError> {
        loop {
            let mut claim_missed = false;
            let (should_wait, local_attempt) = {
                let _state = self.shared.state.lock().await;
                let row = self
                    .record()?
                    .ok_or_else(|| AttachmentClientError::new(Cause::StagedUnusable))?;
                match status_from_row(&row, now_ns()) {
                    PendingAttachmentStatus::Complete => return Ok(()),
                    PendingAttachmentStatus::Failed(error)
                        if error.cause == Cause::BackendRejected =>
                    {
                        return Err(error);
                    }
                    PendingAttachmentStatus::Uploading => {
                        (true, self.shared.attempt.lock().clone())
                    }
                    PendingAttachmentStatus::Waiting | PendingAttachmentStatus::Failed(_)
                        if self.shared.lease.lock().is_some() =>
                    {
                        (true, self.shared.attempt.lock().clone())
                    }
                    PendingAttachmentStatus::Waiting | PendingAttachmentStatus::Failed(_) => {
                        let token = xmtp_common::rand_array::<16>();
                        let now = now_ns();
                        let timing = *self.context.attachment_runtime().lease_timing.lock();
                        let event_lock = self
                            .context
                            .attachment_runtime()
                            .event_lock(&self.reference().attachment_key);
                        let _event_guard = event_lock.lock().await;
                        let claimed = self
                            .context
                            .db()
                            .claim_pending_attachment(
                                &self.remote.content_digest,
                                &token,
                                now,
                                timing.duration_ns(),
                            )
                            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
                        if claimed == 0 {
                            claim_missed = true;
                            (false, None)
                        } else {
                            let attempt = Arc::new(PendingAttempt::new());
                            *self.shared.attempt.lock() = Some(attempt.clone());
                            *self.shared.lease.lock() =
                                Some((token, now.saturating_add(timing.duration_ns())));
                            self.shared
                                .watch
                                .send_replace(PendingAttachmentStatus::Uploading);
                            self.context.events().emit(
                                Some(ClientEvent::AttachmentUploadStarted(self.reference())),
                                None,
                            );
                            let pending = PendingAttachment {
                                context: self.context.context_ref().clone(),
                                remote: self.remote.clone(),
                                shared: self.shared.clone(),
                            };
                            // The registry entry owns the attempt after the caller stops waiting.
                            let local_attempt = attempt.clone();
                            let done_guard = PendingAttemptDoneGuard {
                                shared: self.shared.clone(),
                                attempt: attempt.clone(),
                            };
                            drop(xmtp_common::task::spawn(async move {
                                let _done_guard = done_guard;
                                pending.run_attempt(token, attempt).await;
                            }));
                            (true, Some(local_attempt))
                        }
                    }
                }
            };
            if claim_missed {
                let row = self
                    .record()?
                    .ok_or_else(|| AttachmentClientError::new(Cause::StagedUnusable))?;
                match status_from_row(&row, now_ns()) {
                    PendingAttachmentStatus::Complete => return Ok(()),
                    PendingAttachmentStatus::Failed(error) => return Err(error),
                    PendingAttachmentStatus::Waiting => continue,
                    PendingAttachmentStatus::Uploading => {}
                }
            }
            if should_wait || claim_missed {
                match self.wait_for_record_change(local_attempt).await? {
                    PendingAttachmentStatus::Complete => return Ok(()),
                    PendingAttachmentStatus::Failed(error) => return Err(error),
                    PendingAttachmentStatus::Waiting => continue,
                    PendingAttachmentStatus::Uploading => continue,
                }
            }
        }
    }

    fn lease_is_current(&self, token: &[u8; 16]) -> bool {
        self.shared
            .lease
            .lock()
            .as_ref()
            .is_some_and(|(owner, end)| owner == token && now_ns() < *end)
    }

    async fn run_attempt(&self, token: [u8; 16], attempt: Arc<PendingAttempt>) {
        #[cfg(all(test, not(target_arch = "wasm32")))]
        {
            let pause = self
                .context
                .attachment_runtime()
                .attempt_panic_pause
                .lock()
                .clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
                panic!("forced attachment upload panic");
            }
        }
        let timing = *self.context.attachment_runtime().lease_timing.lock();
        let mut tick = Box::pin(xmtp_common::time::interval_stream(timing.renew));
        #[cfg(not(target_arch = "wasm32"))]
        tick.next().await;
        let mut transfer = Box::pin(self.upload_once(&token));
        let result = loop {
            tokio::select! {
                biased;
                _ = self.shared.cancel.cancelled() => break Err(AttachmentClientError::new(Cause::Deleted)),
                result = &mut transfer => break result,
                _ = tick.next() => {
                    let now = now_ns();
                    let extension = self.context.db().extend_pending_attachment(
                        &self.remote.content_digest, &token, now, timing.duration_ns()
                    );
                    match extension {
                        Ok(1) => *self.shared.lease.lock() = Some((token, now.saturating_add(timing.duration_ns()))),
                        Ok(_) => {
                            self.lost_lease(&attempt).await;
                            return;
                        }
                        Err(error) => {
                            #[cfg(test)]
                            self.context.attachment_runtime().lease_extension_errors.fetch_add(1, AtomicOrdering::SeqCst);
                            tracing::warn!(%error, "attachment lease extension will be retried");
                        },
                    }
                }
            }
        };
        drop(transfer);
        self.finish_attempt(&token, result, &attempt).await;
    }

    fn end_local_attempt(
        &self,
        attempt: &Arc<PendingAttempt>,
        result: Option<&Result<(), AttachmentClientError>>,
    ) {
        let mut active = self.shared.attempt.lock();
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, attempt))
        {
            *active = None;
        }
        drop(active);
        if let Some(result) = result {
            attempt.outcome.send_replace(Some(result.clone()));
        }
        attempt.done.cancel();
    }

    fn end_cancelled_attempt(&self, attempt: &Arc<PendingAttempt>) {
        *self.shared.lease.lock() = None;
        let deleted = Err(AttachmentClientError::new(Cause::Deleted));
        self.end_local_attempt(attempt, Some(&deleted));
    }

    async fn lost_lease(&self, attempt: &Arc<PendingAttempt>) {
        *self.shared.lease.lock() = None;
        self.end_local_attempt(attempt, None);
        match self.record() {
            Ok(Some(row)) => {
                self.shared
                    .watch
                    .send_replace(status_from_row(&row, now_ns()));
            }
            Ok(None) => {
                self.shared
                    .watch
                    .send_replace(PendingAttachmentStatus::Failed(AttachmentClientError::new(
                        Cause::Deleted,
                    )));
            }
            Err(error) => {
                tracing::warn!(%error, "lost attachment lease status read will be retried")
            }
        }
    }

    async fn finish_attempt(
        &self,
        token: &[u8; 16],
        result: Result<(), AttachmentClientError>,
        attempt: &Arc<PendingAttempt>,
    ) {
        let mut delay = Duration::from_millis(100);
        let mut retrying = false;
        let timing = *self.context.attachment_runtime().lease_timing.lock();
        let mut renew = Box::pin(xmtp_common::time::interval_stream(timing.renew));
        #[cfg(not(target_arch = "wasm32"))]
        renew.next().await;
        loop {
            if retrying && self.shared.cancel.is_cancelled() {
                self.end_cancelled_attempt(attempt);
                return;
            }
            #[cfg(all(test, not(target_arch = "wasm32")))]
            let outcome_pause = self
                .context
                .attachment_runtime()
                .outcome_lock_pause
                .lock()
                .take();
            #[cfg(all(test, not(target_arch = "wasm32")))]
            if let Some((entered, resume)) = outcome_pause {
                entered.notify_one();
                resume.notified().await;
            }
            let event_lock = self
                .context
                .attachment_runtime()
                .event_lock(&self.reference().attachment_key);
            let _event_guard = event_lock.lock().await;
            if self.shared.cancel.is_cancelled() {
                drop(_event_guard);
                self.end_cancelled_attempt(attempt);
                return;
            }
            let recorded = match &result {
                Ok(()) => PendingAttachmentOutcome::Complete,
                Err(error) => PendingAttachmentOutcome::Failed {
                    cause: error.cause.as_str(),
                    credential_kind: error.credential_kind.map(CredentialFailureKind::as_str),
                    retryable: Some(error.retryable),
                    missing_scope: Some(error.missing_scope),
                    http_status: error.http_status,
                },
            };
            let outcome = self.context.db().finish_pending_attachment(
                &self.remote.content_digest,
                token,
                now_ns(),
                recorded,
            );
            #[cfg(test)]
            if outcome.is_err() {
                self.context
                    .attachment_runtime()
                    .outcome_write_errors
                    .fetch_add(1, AtomicOrdering::SeqCst);
            }
            match outcome {
                Ok(1) => {
                    *self.shared.lease.lock() = None;
                    if result.is_ok()
                        && let Ok(store) = self.context.attachment_runtime().store()
                        && let Ok(path) = staged_path(&self.remote.content_digest)
                        && let Err(error) = store.remove_file(&path).await
                    {
                        tracing::warn!(%error, "staged ciphertext cleanup will be retried by reconciliation");
                    }
                    self.publish_upload_outcome(&result);
                    self.end_local_attempt(attempt, Some(&result));
                    return;
                }
                Ok(_) => {
                    drop(_event_guard);
                    self.lost_lease(attempt).await;
                    return;
                }
                Err(error) if outcome_write_is_retryable(&error) => {
                    retrying = true;
                    tracing::warn!(%error, "attachment outcome write will be retried");
                }
                Err(error) => {
                    tracing::error!(%error, "attachment outcome write cannot be retried");
                    // Nothing was recorded, so no event is sent. The record
                    // stays `uploading` until its lease expires.
                    *self.shared.lease.lock() = None;
                    let failed = Err(AttachmentClientError::new(Cause::LocalStorage));
                    self.end_local_attempt(attempt, Some(&failed));
                    return;
                }
            }
            drop(_event_guard);
            let retry_delay = xmtp_common::time::sleep(delay);
            tokio::pin!(retry_delay);
            loop {
                tokio::select! {
                    biased;
                    _ = self.shared.cancel.cancelled() => {
                        self.end_cancelled_attempt(attempt);
                        return;
                    }
                    _ = &mut retry_delay => break,
                    _ = renew.next() => {
                        let now = now_ns();
                        let extension = self.context.db().extend_pending_attachment(
                            &self.remote.content_digest,
                            token,
                            now,
                            timing.duration_ns(),
                        );
                        match extension {
                            Ok(1) => *self.shared.lease.lock() =
                                Some((*token, now.saturating_add(timing.duration_ns()))),
                            Ok(_) => {
                                self.lost_lease(attempt).await;
                                return;
                            }
                            Err(error) => {
                                #[cfg(test)]
                                self.context
                                    .attachment_runtime()
                                    .lease_extension_errors
                                    .fetch_add(1, AtomicOrdering::SeqCst);
                                tracing::warn!(%error, "attachment lease extension will be retried");
                            }
                        }
                    }
                }
            }
            delay = delay.saturating_mul(2).min(Duration::from_secs(5));
        }
    }

    fn publish_upload_outcome(&self, result: &Result<(), AttachmentClientError>) {
        let next = match result {
            Ok(()) => PendingAttachmentStatus::Complete,
            Err(error) => PendingAttachmentStatus::Failed(error.clone()),
        };
        self.shared.watch.send_replace(next);
        let reference = self.reference();
        let event = match result {
            Ok(()) => ClientEvent::AttachmentUploadCompleted(reference),
            Err(error) => ClientEvent::AttachmentUploadFailed(AttachmentFailed {
                attachment_key: reference.attachment_key,
                url: reference.url,
                content_digest: reference.content_digest,
                cause: error.cause.as_str().to_owned(),
            }),
        };
        self.context.events().emit(Some(event), None);
    }

    async fn upload_once(&self, token: &[u8; 16]) -> Result<(), AttachmentClientError> {
        self.context
            .server_configuration()
            .check()
            .map_err(|_| AttachmentClientError::new(Cause::ConnectionBlocked))?;
        let runtime = self.context.attachment_runtime();
        let store = runtime.store()?;
        let path = staged_path(&self.remote.content_digest)?;
        let storage_error = |_| AttachmentClientError {
            retryable: true,
            ..AttachmentClientError::new(Cause::LocalStorage)
        };
        if !store.exists(&path).await.map_err(storage_error)? {
            return Err(AttachmentClientError::new(Cause::StagedUnusable));
        }
        let staged = store.open_read(&path).await.map_err(storage_error)?;
        let (digest, length) = staged.sha256().await.map_err(storage_error)?;
        if hex::encode(digest) != self.remote.content_digest
            || Some(length as u32) != self.remote.content_length
            || length > u32::MAX as u64
        {
            return Err(AttachmentClientError::new(Cause::StagedUnusable));
        }
        if !self.lease_is_current(token) {
            return Err(AttachmentClientError::new(Cause::Network));
        }
        let response = self
            .context
            .api()
            .create_upload(CreateUploadRequest {
                content_digest: digest.to_vec(),
                content_length: length,
            })
            .await
            .map_err(api_error)?;
        let request = UploadRequest {
            method: response.method,
            url: response.url,
            headers: response
                .headers
                .into_iter()
                .map(|header| (header.name, header.value))
                .collect(),
            expires_in_seconds: response.expires_in_seconds,
        };
        if !self.lease_is_current(token) {
            return Err(AttachmentClientError::new(Cause::Network));
        }
        let transfer = Transfer::new(runtime.options.clone())?;
        transfer.put(&request, staged).await?;
        Ok(())
    }
}

pub(crate) mod cleanup {
    use super::*;
    use crate::worker::{BoxedWorker, DynMetrics, Worker, WorkerFactory, WorkerKind, WorkerResult};

    pub struct AttachmentCleanup<Context> {
        context: Context,
    }
    struct Factory<Context> {
        context: Context,
    }

    impl<Context: XmtpSharedContext + 'static> WorkerFactory for Factory<Context> {
        fn kind(&self) -> WorkerKind {
            WorkerKind::AttachmentCleanup
        }
        fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
            (
                Box::new(AttachmentCleanup {
                    context: self.context.clone(),
                }),
                metrics,
            )
        }
    }

    #[xmtp_common::async_trait]
    impl<Context: XmtpSharedContext + 'static> Worker for AttachmentCleanup<Context> {
        fn kind(&self) -> WorkerKind {
            WorkerKind::AttachmentCleanup
        }
        fn factory<C>(context: C) -> impl WorkerFactory + 'static
        where
            Self: Sized,
            C: XmtpSharedContext + 'static,
        {
            Factory { context }
        }
        async fn run_tasks(&mut self) -> WorkerResult<()> {
            loop {
                let (interval, _) = self
                    .context
                    .worker_interval(WorkerKind::AttachmentCleanup, Duration::from_secs(3600));
                xmtp_common::time::sleep(interval.min(Duration::from_secs(3600))).await;
                let runtime = self.context.attachment_runtime();
                if let Err(error) = runtime.sweep(&self.context).await {
                    tracing::warn!(%error, "attachment expiry sweep failed");
                }
                if let Err(error) = runtime.reconcile(&self.context).await {
                    tracing::warn!(%error, "attachment reconciliation failed");
                }
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
