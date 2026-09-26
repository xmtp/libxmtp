//! Pending remote attachments owned by a client.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Weak},
    time::Duration,
};

use futures::StreamExt as _;
use parking_lot::Mutex;
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, watch};
use tokio_util::sync::CancellationToken;
use xmtp_attachments::{
    AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, AttachmentOptions,
    DownloadSink as _, GcmDecryptor, GcmEncryptor, KeyMaterial, LocalStore, StoreWriter, Transfer,
    UploadRequest, attachment_key, ciphertext_len, download_cap, encoded_prefix,
    plaintext_rel_path, remote_attachment, retained_fields_fit, staged_path, temporary_path,
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
    use xmtp_db::diesel::result::{DatabaseErrorKind, Error};

    let diesel_error = match error {
        xmtp_db::StorageError::DieselResult(error)
        | xmtp_db::StorageError::Connection(xmtp_db::ConnectionError::Database(error))
        | xmtp_db::StorageError::Platform(xmtp_db::PlatformStorageError::DieselResult(error))
        | xmtp_db::StorageError::Connection(xmtp_db::ConnectionError::Platform(
            xmtp_db::PlatformStorageError::DieselResult(error),
        )) => Some(error),
        _ => None,
    };
    !matches!(
        diesel_error,
        Some(Error::DatabaseError(
            DatabaseErrorKind::CheckViolation
                | DatabaseErrorKind::NotNullViolation
                | DatabaseErrorKind::UniqueViolation
                | DatabaseErrorKind::ForeignKeyViolation,
            _
        ))
    )
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
}

impl PendingAttempt {
    fn new() -> Self {
        let (outcome, _) = watch::channel(None);
        Self { outcome }
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

#[doc(hidden)]
pub struct AttachmentRuntime {
    pub(crate) store: Option<Arc<dyn LocalStore>>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) options: AttachmentOptions,
    pending: Mutex<HashMap<String, Weak<PendingShared>>>,
    downloads: Mutex<HashMap<String, Arc<DownloadShared>>>,
    deleting: Mutex<HashMap<String, usize>>,
    event_locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    reconciled: OnceCell<()>,
    lease_timing: Mutex<LeaseTiming>,
    #[cfg(test)]
    sweep_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(test)]
    delete_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(test)]
    outcome_write_errors: AtomicUsize,
    #[cfg(test)]
    lease_extension_errors: AtomicUsize,
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
            reconciled: OnceCell::new(),
            lease_timing: Mutex::new(LeaseTiming::default()),
            #[cfg(test)]
            sweep_pause: Mutex::new(None),
            #[cfg(test)]
            delete_pause: Mutex::new(None),
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
            reconciled: OnceCell::new(),
            lease_timing: Mutex::new(LeaseTiming::default()),
            #[cfg(test)]
            sweep_pause: Mutex::new(None),
            #[cfg(test)]
            delete_pause: Mutex::new(None),
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
        self.context
            .db()
            .list_local_attachments()
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))
            .map(|rows| {
                rows.into_iter()
                    .map(|row| LocalAttachment {
                        path: row.path,
                        created_at_ns: row.created_at_ns,
                    })
                    .collect()
            })
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
                let remote = remote.clone();
                let shared_for_task = shared.clone();
                drop(xmtp_common::task::spawn(async move {
                    task.run_download(remote, relative, key, shared_for_task)
                        .await;
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

    async fn run_download(
        &self,
        remote: RemoteAttachment,
        relative: String,
        key: String,
        shared: Arc<DownloadShared>,
    ) {
        let result = self.download_once(&remote, &relative, &shared.cancel).await;
        let lock = self.runtime().event_lock(&key);
        let _guard = lock.lock().await;
        let reference = attachment_reference(&remote, &key);
        let event = match &result {
            Ok(_) => ClientEvent::AttachmentDownloadCompleted(reference),
            Err(error) => ClientEvent::AttachmentDownloadFailed(AttachmentFailed {
                attachment_key: key,
                url: reference.url,
                content_digest: reference.content_digest,
                cause: error.cause.as_str().to_owned(),
            }),
        };
        self.context.events().emit(Some(event), None);
        self.runtime().downloads.lock().remove(&relative);
        shared.outcome.send_replace(Some(result));
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
            store.rename(final_tmp, relative).await?;
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
        let key = attachment_key(remote)?;
        let staged = staged_path(&remote.content_digest)?;
        let store = self.runtime().store()?;
        let lock = self.runtime().event_lock(&key);
        let (upload, owns_upload, downloads, _deleting) = {
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
            let owns_upload = upload
                .as_ref()
                .is_some_and(|shared| shared.lease.lock().is_some());
            if let Some(shared) = &upload {
                shared.cancel.cancel();
            }
            for shared in &downloads {
                shared.cancel.cancel();
            }
            (upload, owns_upload, downloads, deleting)
        };
        #[cfg(test)]
        {
            let pause = self.runtime().delete_pause.lock().clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
            }
        }
        if let Some(shared) = upload.filter(|_| owns_upload) {
            let mut status = shared.watch.subscribe();
            while matches!(
                status.borrow_and_update().clone(),
                PendingAttachmentStatus::Uploading
            ) {
                status
                    .changed()
                    .await
                    .map_err(|_| AttachmentClientError::new(Cause::Network))?;
            }
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
        let _guard = lock.lock().await;
        let mut changed = false;
        // Remove the row first. An upload in another client sees the deletion
        // before this client removes its staged ciphertext.
        changed |= self
            .context
            .db()
            .delete_pending_attachment(&remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            != 0;
        if store.exists(&key).await? {
            store.remove_dir_all(&key).await?;
            changed = true;
        }
        if store.exists(&staged).await? {
            store.remove_file(&staged).await?;
            changed = true;
        }
        changed |= self
            .context
            .db()
            .delete_local_attachments_in_dir(&key)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            != 0;
        self.runtime().pending.lock().remove(&remote.content_digest);
        if changed {
            self.context.events().emit(
                Some(ClientEvent::AttachmentDeleted(attachment_reference(
                    remote, &key,
                ))),
                None,
            );
        }
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
        let mut final_plain = None::<String>;
        let mut final_staged = None::<String>;
        let mut digest = None::<String>;
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
            digest = Some(hex_digest.clone());
            final_plain = Some(local.clone());
            final_staged = Some(ciphertext.clone());
            store.rename(&plain_temp, &local).await?;
            store.rename(&staged_temp, &ciphertext).await?;
            let db = self.context.db();
            db.insert_or_ignore_local_attachment(
                &local,
                now_ns(),
                Some(mime_type.clone()),
                filename.clone(),
            )
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            db.insert_or_ignore_pending_attachment(&hex_digest, &remote.encode_to_vec(), now_ns())
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            Ok(PendingAttachment {
                context: self.context.clone(),
                remote,
                shared: self.runtime().shared(&hex_digest),
            })
        }
        .await;
        if result.is_err() {
            if let Some(digest) = digest.as_ref() {
                let _ = self.context.db().delete_pending_attachment(digest);
            }
            if let Some(path) = final_plain.as_ref() {
                let _ = self.context.db().delete_local_attachment(path);
            }
            for path in [
                Some(&plain_temp),
                Some(&staged_temp),
                final_plain.as_ref(),
                final_staged.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                if store.exists(path).await.unwrap_or(false) {
                    let _ = store.remove_file(path).await;
                }
            }
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
                            drop(xmtp_common::task::spawn(async move {
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
        let timing = *self.context.attachment_runtime().lease_timing.lock();
        let mut renew = Box::pin(xmtp_common::time::interval_stream(timing.renew));
        #[cfg(not(target_arch = "wasm32"))]
        renew.next().await;
        loop {
            let event_lock = self
                .context
                .attachment_runtime()
                .event_lock(&self.reference().attachment_key);
            let _event_guard = event_lock.lock().await;
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
