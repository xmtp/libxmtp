//! Pending remote attachments owned by a client.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Weak},
    time::Duration,
};

use parking_lot::Mutex;
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio_util::sync::CancellationToken;
use xmtp_attachments::{
    AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, AttachmentOptions,
    DownloadSink as _, GcmDecryptor, GcmEncryptor, KeyMaterial, LocalStore, StoreWriter, Transfer,
    UploadRequest, attachment_key, ciphertext_len, download_cap, encoded_prefix,
    plaintext_rel_path, remote_attachment, staged_path, temporary_path,
};
use xmtp_common::{RetryableError as _, time::now_ns};
use xmtp_content_types::remote_attachment::RemoteAttachment;
use xmtp_db::prelude::*;

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

/// A stable attachment cause with the credential detail, when applicable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("attachment failure: {}", cause.as_str())]
pub struct AttachmentClientError {
    pub cause: Cause,
    pub credential_kind: Option<CredentialFailureKind>,
    pub retryable: bool,
}

impl AttachmentClientError {
    fn new(cause: Cause) -> Self {
        Self {
            cause,
            credential_kind: None,
            retryable: false,
        }
    }
}

impl From<AttachmentError> for AttachmentClientError {
    fn from(error: AttachmentError) -> Self {
        Self::new(error.cause)
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
            self.writer.write(content).await?;
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

struct PendingShared {
    state: AsyncMutex<PendingAttachmentStatus>,
    watch: watch::Sender<PendingAttachmentStatus>,
    permanent: Mutex<Option<AttachmentClientError>>,
    cancel: CancellationToken,
}

impl PendingShared {
    fn new() -> Self {
        let (watch, _) = watch::channel(PendingAttachmentStatus::Waiting);
        Self {
            state: AsyncMutex::new(PendingAttachmentStatus::Waiting),
            watch,
            permanent: Mutex::new(None),
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
    pending: Mutex<HashMap<String, Arc<PendingShared>>>,
    downloads: Mutex<HashMap<String, Arc<DownloadShared>>>,
    deleting: Mutex<HashMap<String, usize>>,
    event_locks: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    #[cfg(test)]
    sweep_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(test)]
    delete_pause: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
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
            #[cfg(test)]
            sweep_pause: Mutex::new(None),
            #[cfg(test)]
            delete_pause: Mutex::new(None),
        }
    }
}

impl AttachmentRuntime {
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
            #[cfg(test)]
            sweep_pause: Mutex::new(None),
            #[cfg(test)]
            delete_pause: Mutex::new(None),
        })
    }

    fn store(&self) -> Result<&Arc<dyn LocalStore>, AttachmentClientError> {
        self.store
            .as_ref()
            .ok_or_else(|| AttachmentClientError::new(Cause::LocalStorage))
    }

    fn shared(&self, digest: &str) -> Arc<PendingShared> {
        let mut pending = self.pending.lock();
        if let Some(shared) = pending.get(digest) {
            return shared.clone();
        }
        let shared = Arc::new(PendingShared::new());
        pending.insert(digest.to_owned(), shared.clone());
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
            let state = shared.state.lock().await;
            if matches!(*state, PendingAttachmentStatus::Uploading) {
                continue;
            }
            #[cfg(test)]
            {
                let pause = self.sweep_pause.lock().clone();
                if let Some((entered, resume)) = pause {
                    entered.notify_one();
                    resume.notified().await;
                }
            }
            context
                .db()
                .delete_pending_attachment(&digest)
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
            let path = staged_path(&digest)?;
            if store.exists(&path).await? {
                store.remove_file(&path).await?;
            }
            self.pending.lock().remove(&digest);
            drop(state);
        }
        Ok(())
    }

    pub(crate) async fn reconcile<Context: XmtpSharedContext>(
        &self,
        context: &Context,
    ) -> Result<(), AttachmentClientError> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let pending: HashSet<_> = context
            .db()
            .list_pending_attachments_since(0)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            .into_iter()
            .map(|row| row.content_digest)
            .collect();
        let records = context
            .db()
            .list_local_attachments()
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        let recorded: HashSet<_> = records.iter().map(|row| row.path.as_str()).collect();
        let files = store.list_files().await?;
        let present: HashSet<_> = files.iter().map(|file| file.path.as_str()).collect();
        let cutoff = now_ns().saturating_sub(RECONCILE_AGE.as_nanos() as i64);
        for file in &files {
            if file.path.starts_with(".tmp/") {
                if file.modified_at_ns < cutoff {
                    store.remove_file(&file.path).await?;
                }
            } else if let Some(digest) = file.path.strip_prefix(".staged/") {
                if !pending.contains(digest) && file.modified_at_ns < cutoff {
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
        let relative = plaintext_rel_path(remote)?;
        let path = self.local_path(remote)?;
        let key = attachment_key(remote)?;
        let store = self.runtime().store()?;
        let lock = self.runtime().event_lock(&key);
        let shared = {
            let _guard = lock.lock().await;
            if self.runtime().deleting.lock().contains_key(&relative) {
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
        let relative = plaintext_rel_path(remote)?;
        let staged = staged_path(&remote.content_digest)?;
        let store = self.runtime().store()?;
        let lock = self.runtime().event_lock(&key);
        let (upload, download, _deleting) = {
            let _guard = lock.lock().await;
            let deleting = DeleteInProgress::new(&self.runtime().deleting, relative.clone());
            let upload = self
                .runtime()
                .pending
                .lock()
                .get(&remote.content_digest)
                .cloned();
            let download = self.runtime().downloads.lock().get(&relative).cloned();
            if let Some(shared) = &upload {
                shared.cancel.cancel();
            }
            if let Some(shared) = &download {
                shared.cancel.cancel();
            }
            (upload, download, deleting)
        };
        #[cfg(test)]
        {
            let pause = self.runtime().delete_pause.lock().clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
            }
        }
        if let Some(shared) = upload {
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
        if let Some(shared) = download {
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
            .delete_local_attachment(&relative)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            != 0;
        changed |= self
            .context
            .db()
            .delete_pending_attachment(&remote.content_digest)
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
        let (filename, mime_type, size) = match &source {
            AttachmentSource::Path {
                path,
                filename,
                mime_type,
            } => {
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
                    let fallback = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    (filename.clone().or(fallback), mime_type.clone(), file.len())
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let size = tokio::fs::metadata(path)
                        .await
                        .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?
                        .len();
                    let fallback = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    (filename.clone().or(fallback), mime_type.clone(), size)
                }
            }
            AttachmentSource::Bytes {
                bytes,
                filename,
                mime_type,
            } => (filename.clone(), mime_type.clone(), bytes.len() as u64),
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
        let rows = self
            .context
            .db()
            .list_pending_attachments_since(self.runtime().cutoff())
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        let row = rows
            .into_iter()
            .find(|row| row.content_digest == remote.content_digest)
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
        self.context
            .db()
            .list_pending_attachments_since(self.runtime().cutoff())
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            .into_iter()
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
        self.shared.watch.borrow().clone()
    }

    pub fn watch_status(&self) -> watch::Receiver<PendingAttachmentStatus> {
        self.shared.watch.subscribe()
    }

    fn reference(&self) -> AttachmentRef {
        AttachmentRef {
            attachment_key: xmtp_attachments::attachment_key(&self.remote).unwrap_or_default(),
            url: self.remote.url.clone(),
            content_digest: self.remote.content_digest.clone(),
        }
    }

    pub async fn upload(&self) -> Result<(), AttachmentClientError> {
        let mut watch = self.shared.watch.subscribe();
        {
            let event_lock = self
                .context
                .attachment_runtime()
                .event_lock(&self.reference().attachment_key);
            let _event_guard = event_lock.lock().await;
            let mut state = self.shared.state.lock().await;
            match &*state {
                PendingAttachmentStatus::Complete => return Ok(()),
                PendingAttachmentStatus::Uploading => {}
                _ => {
                    *state = PendingAttachmentStatus::Uploading;
                    self.shared.watch.send_replace(state.clone());
                    self.context.events().emit(
                        Some(ClientEvent::AttachmentUploadStarted(self.reference())),
                        None,
                    );
                    let pending = PendingAttachment {
                        context: self.context.context_ref().clone(),
                        remote: self.remote.clone(),
                        shared: self.shared.clone(),
                    };
                    // The task keeps the attempt alive when a caller stops waiting.
                    drop(xmtp_common::task::spawn(async move {
                        pending.run_attempt().await;
                    }));
                }
            }
        }
        loop {
            let status = watch.borrow_and_update().clone();
            match status {
                PendingAttachmentStatus::Complete => return Ok(()),
                PendingAttachmentStatus::Failed(error) => return Err(error),
                _ => watch
                    .changed()
                    .await
                    .map_err(|_| AttachmentClientError::new(Cause::Network))?,
            }
        }
    }

    async fn run_attempt(&self) {
        let result = tokio::select! {
            biased;
            _ = self.shared.cancel.cancelled() => Err(AttachmentClientError::new(Cause::Deleted)),
            result = self.upload_once() => result,
        };
        let event_lock = self
            .context
            .attachment_runtime()
            .event_lock(&self.reference().attachment_key);
        let _event_guard = event_lock.lock().await;
        let mut state = self.shared.state.lock().await;
        *state = match &result {
            Ok(()) => PendingAttachmentStatus::Complete,
            Err(error) => PendingAttachmentStatus::Failed(error.clone()),
        };
        let reference = self.reference();
        let event = match &result {
            Ok(()) => ClientEvent::AttachmentUploadCompleted(reference),
            Err(error) => ClientEvent::AttachmentUploadFailed(AttachmentFailed {
                attachment_key: reference.attachment_key,
                url: reference.url,
                content_digest: reference.content_digest,
                cause: error.cause.as_str().to_owned(),
            }),
        };
        self.context.events().emit(Some(event), None);
        if result.is_ok() {
            self.context
                .attachment_runtime()
                .pending
                .lock()
                .remove(&self.remote.content_digest);
        }
        self.shared.watch.send_replace(state.clone());
    }

    async fn upload_once(&self) -> Result<(), AttachmentClientError> {
        self.context
            .server_configuration()
            .check()
            .map_err(|_| AttachmentClientError::new(Cause::ConnectionBlocked))?;
        if let Some(error) = self.shared.permanent.lock().clone() {
            return Err(error);
        }
        let runtime = self.context.attachment_runtime();
        let store = runtime.store()?;
        let path = staged_path(&self.remote.content_digest)?;
        let staged = store
            .open_read(&path)
            .await
            .map_err(|_| AttachmentClientError::new(Cause::StagedUnusable))?;
        let (digest, length) = staged
            .sha256()
            .await
            .map_err(|_| AttachmentClientError::new(Cause::StagedUnusable))?;
        if hex::encode(digest) != self.remote.content_digest
            || Some(length as u32) != self.remote.content_length
            || length > u32::MAX as u64
        {
            return Err(AttachmentClientError::new(Cause::StagedUnusable));
        }
        let response = self
            .context
            .api()
            .create_upload(CreateUploadRequest {
                content_digest: digest.to_vec(),
                content_length: length,
            })
            .await
            .map_err(|error| {
                let error = api_error(error);
                if error.cause == Cause::BackendRejected {
                    *self.shared.permanent.lock() = Some(error.clone());
                }
                error
            })?;
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
        let transfer = Transfer::new(runtime.options.clone())?;
        transfer.put(&request, staged).await?;
        // Delete the row first, so a stopped client cannot resume a missing file.
        self.context
            .db()
            .delete_pending_attachment(&self.remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        if let Err(error) = store.remove_file(&path).await {
            tracing::warn!(%error, "staged ciphertext cleanup will be retried by reconciliation");
        }
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

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use crate::{Client, utils::test::identity_setup};
    use xmtp_configuration::{AttachmentsConfiguration, ServerConfiguration};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_proto::backend_v1::{GetInboxIdsResponse, get_inbox_ids_response};

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    // verifies: ATCH-048
    #[xmtp_common::test(unwrap_try = true)]
    async fn client_create_writes_plaintext_to_opfs() {
        let root = format!(
            "attachment-client-tests/{}",
            hex::encode(xmtp_common::rand_array::<16>())
        );
        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_get_inbox_ids().times(1).returning(|request| {
            Ok(GetInboxIdsResponse {
                responses: request
                    .requests
                    .into_iter()
                    .map(|request| get_inbox_ids_response::Response {
                        identifier: request.identifier,
                        identifier_kind: request.identifier_kind,
                        inbox_id: None,
                    })
                    .collect(),
            })
        });
        let client = Client::builder(identity_setup(generate_local_wallet()))
            .api_client(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .temp_store()
            .await
            .default_mls_store()?
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                |configuration: &mut ServerConfiguration| {
                    configuration.attachments = Some(AttachmentsConfiguration {
                        base_url: "https://example.com/attachments".into(),
                        max_upload_bytes: 1_048_576,
                        retention_seconds: 0,
                    });
                },
            )))
            .attachments_dir(root.clone())
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await?;
        let pending = client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"opfs plaintext".to_vec(),
                filename: Some("proof.txt".into()),
                mime_type: "text/plain".into(),
            })
            .await?;
        let remote = pending.remote_attachment();
        let local = plaintext_rel_path(remote)?;
        let staged = staged_path(&remote.content_digest)?;
        let root_store = xmtp_attachments::OpfsStore::new_root().await?;
        let local_file = root_store.open_read(&format!("{root}/{local}")).await?;
        assert_eq!(local_file.read_chunk(0, 64).await?, b"opfs plaintext");
        assert!(root_store.exists(&format!("{root}/{staged}")).await?);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::{server_configuration::BlockedConnection, tester};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use xmtp_attachments::GcmDecryptor;
    use xmtp_common::StreamHandle as _;
    use xmtp_configuration::{AttachmentsConfiguration, ServerConfiguration};
    use xmtp_db::{ConnectionExt as _, diesel::RunQueryDsl as _};
    use xmtp_events::{EventFilter, EventKind};

    fn bytes() -> AttachmentSource {
        AttachmentSource::Bytes {
            bytes: b"attachment content".to_vec(),
            filename: Some("note.txt".into()),
            mime_type: "text/plain".into(),
        }
    }

    fn offer(configuration: &mut ServerConfiguration) {
        configuration.attachments = Some(AttachmentsConfiguration {
            base_url: "http://localhost:5050/attachments".into(),
            max_upload_bytes: 10_485_760,
            retention_seconds: 0,
        });
    }

    async fn serve_body(body: Vec<u8>) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind attachment test server");
        let url = format!(
            "http://{}/file",
            listener.local_addr().expect("test server address")
        );
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        drop(xmtp_common::task::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                seen.fetch_add(1, Ordering::SeqCst);
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request).await;
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if stream.write_all(header.as_bytes()).await.is_err() {
                    break;
                }
                if stream.write_all(&body).await.is_err() {
                    break;
                }
            }
        }));
        (url, requests)
    }

    async fn capture_transfers(body: Vec<u8>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind capture server");
        let url = format!(
            "http://{}/attachment",
            listener.local_addr().expect("capture address")
        );
        let captured = Arc::new(Mutex::new(Vec::new()));
        let seen = captured.clone();
        drop(xmtp_common::task::spawn(async move {
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut request = Vec::new();
                let mut chunk = [0u8; 4096];
                let header_end = loop {
                    let Ok(count) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if count == 0 {
                        return;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let head = String::from_utf8_lossy(&request[..header_end]).into_owned();
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while request.len() - header_end < length {
                    let Ok(count) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if count == 0 {
                        return;
                    }
                    request.extend_from_slice(&chunk[..count]);
                }
                seen.lock().push(head.clone());
                let content = if head.starts_with("GET ") {
                    body.as_slice()
                } else {
                    &[]
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    content.len()
                );
                if stream.write_all(response.as_bytes()).await.is_err() {
                    return;
                }
                if stream.write_all(content).await.is_err() {
                    return;
                }
            }
        }));
        (url, captured)
    }

    struct KnownCredential(Arc<AtomicUsize>);

    #[xmtp_common::async_trait]
    impl xmtp_api_backend::AuthCallback for KnownCredential {
        async fn on_auth_required(
            &self,
        ) -> Result<xmtp_api_backend::Credential, xmtp_common::BoxDynError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(xmtp_api_backend::Credential::new(
                None,
                "Bearer attachment-credential-unique-0123456789".parse()?,
                i64::MAX,
            ))
        }
    }

    // verifies: ATCH-027
    #[xmtp_common::test(unwrap_try = true)]
    async fn no_backend_credentials_on_storage_requests() {
        use crate::utils::test::backend::EphemeralBackend;
        use xmtp_proto::backend_v1::CreateUploadResponse;
        let backend = EphemeralBackend::start(
            "[auth]\nenabled = true\n[auth.api_keys]\nci = 'attachment-credential-unique-0123456789'",
        )
        .await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, backend: &backend, auth: Arc::new(KnownCredential(calls.clone())), attachments_dir: sender.path(), configured: offer, disable_workers);
        assert!(calls.load(Ordering::SeqCst) > 0);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        let body =
            tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
        let (url, captured) = capture_transfers(body).await;
        let mut mock = xmtp_api_backend::MockBackendClient::new();
        let put_url = url.clone();
        mock.expect_create_upload().times(1).returning(move |_| {
            Ok(CreateUploadResponse {
                method: "PUT".into(),
                url: put_url.clone(),
                headers: vec![],
                expires_in_seconds: 3600,
            })
        });
        let sender_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .api_client(Arc::new(mock))
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                offer,
            )))
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await?;
        sender_client
            .attachments()
            .pending(&remote)
            .await?
            .upload()
            .await?;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let recipient_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        let mut remote = remote;
        remote.url = url;
        recipient_client.attachments().download(&remote).await?;
        let requests = captured.lock();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("PUT "));
        assert!(requests[1].starts_with("GET "));
        let inbox = alix.inbox_id().to_string().to_ascii_lowercase();
        let installation = hex::encode(alix.client.context.installation_id());
        for request in requests.iter() {
            let lower = request.to_ascii_lowercase();
            for forbidden in [
                "attachment-credential-unique-0123456789",
                "authorization:",
                "cookie:",
                "referer:",
                inbox.as_str(),
                installation.as_str(),
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "storage request carried client identity"
                );
            }
        }
        let downloader_request = requests[1].to_ascii_lowercase();
        assert!(!downloader_request.contains(&bo.inbox_id().to_string().to_ascii_lowercase()));
        assert!(!downloader_request.contains(&hex::encode(bo.client.context.installation_id())));
    }

    // verifies: ATCH-030, ATCH-031, ATCH-011, ATCH-012, ATCH-049
    #[xmtp_common::test(unwrap_try = true)]
    async fn remote_attachment_before_request() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment();
        assert_eq!(
            remote.url,
            format!(
                "http://localhost:5050/attachments/{}",
                remote.content_digest
            )
        );
        assert_eq!(remote.scheme, "http://");
        assert_eq!(
            remote.content_length,
            Some(
                ciphertext_len(encoded_prefix(Some("note.txt"), "text/plain", 18).len(), 18) as u32
            )
        );
        assert_eq!(remote.filename.as_deref(), Some("note.txt"));
        let staged_relative = staged_path(&remote.content_digest)?;
        let staged_file = dir.path().join(&staged_relative);
        assert_eq!(
            staged_file.parent(),
            Some(dir.path().join(".staged").as_path())
        );
        assert_eq!(staged_relative.split('/').count(), 2);
        assert!(!staged_file.starts_with(dir.path().join(attachment_key(remote)?)));
        let staged = tokio::fs::read(&staged_file).await?;
        assert_eq!(hex::encode(Sha256::digest(&staged)), remote.content_digest);
        assert_eq!(remote.content_length, Some(staged.len() as u32));
        let material = KeyMaterial::from_remote(remote)?;
        let mut decrypted = Vec::new();
        let mut decryptor = GcmDecryptor::new(&material);
        decryptor.update(&staged, &mut decrypted)?;
        decryptor.finish()?;
        let mut encoded = encoded_prefix(Some("note.txt"), "text/plain", 18);
        encoded.extend_from_slice(b"attachment content");
        assert_eq!(decrypted, encoded);
        assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
        assert_eq!(
            tokio::fs::read(pending.local_path()?).await?,
            b"attachment content"
        );
        let unnamed = alix
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"unnamed".to_vec(),
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await?;
        assert_eq!(unnamed.remote_attachment().filename, None);
    }

    // verifies: ATCH-030, ATCH-033
    #[xmtp_common::test(unwrap_try = true)]
    async fn create_preconditions() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: |_configuration: &mut ServerConfiguration| {}, disable_workers);
        let error = alix
            .client
            .attachments()
            .create(bytes())
            .await
            .err()
            .expect("creation must fail");
        assert_eq!(error.cause, Cause::NotOffered);
        assert!(!alix.client.attachments().offered());
        assert!(alix.client.attachments().list_pending().await?.is_empty());
        let dir2 = tempfile::tempdir()?;
        tester!(bo, attachments_dir: dir2.path(), configured: |configuration: &mut ServerConfiguration| {
            offer(configuration);
            configuration.attachments.as_mut().unwrap().max_upload_bytes = 10;
        }, disable_workers);
        let error = bo
            .client
            .attachments()
            .create(bytes())
            .await
            .err()
            .expect("creation must fail");
        assert_eq!(error.cause, Cause::TooLarge);
        assert!(bo.client.attachments().list_pending().await?.is_empty());
    }

    // verifies: ATCH-030
    #[xmtp_common::test(unwrap_try = true)]
    async fn exact_upload_limit_is_allowed() {
        let dir = tempfile::tempdir()?;
        let limit = ciphertext_len(encoded_prefix(Some("note.txt"), "text/plain", 18).len(), 18);
        tester!(alix, attachments_dir: dir.path(), configured: move |configuration: &mut ServerConfiguration| {
            offer(configuration);
            configuration.attachments.as_mut().unwrap().max_upload_bytes = limit;
        }, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        assert_eq!(
            pending.remote_attachment().content_length,
            Some(limit as u32)
        );
    }

    // verifies: ATCH-032, ATCH-011
    #[xmtp_common::test(unwrap_try = true)]
    async fn source_moved_after_create() {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source.txt");
        tokio::fs::write(&source, b"saved source").await?;
        tester!(alix, attachments_dir: dir.path().join("attachments"), disable_workers);
        let pending = alix
            .client
            .attachments()
            .create(AttachmentSource::Path {
                path: source.clone(),
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await?;
        tokio::fs::rename(source, dir.path().join("moved.txt")).await?;
        assert_eq!(
            pending.remote_attachment().filename.as_deref(),
            Some("source.txt")
        );
        assert_eq!(
            tokio::fs::read(pending.local_path()?).await?,
            b"saved source"
        );
        pending.upload().await?;
        assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
    }

    // verifies: ATCH-033, ATCH-060
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_create_leaves_nothing() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let error = alix
            .client
            .attachments()
            .create(AttachmentSource::Path {
                path: dir.path().join("absent"),
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await
            .err()
            .expect("creation must fail");
        assert_eq!(error.cause, Cause::SourceUnreadable);
        assert!(alix.client.attachments().list_pending().await?.is_empty());
        assert!(
            tokio::fs::read_dir(dir.path())
                .await?
                .next_entry()
                .await?
                .is_none()
        );
    }

    // verifies: ATCH-035, ATCH-038, ATCH-067
    #[xmtp_common::test(unwrap_try = true)]
    async fn one_pending_per_digest() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let first = alix.client.attachments().create(bytes()).await?;
        let second = alix
            .client
            .attachments()
            .pending(first.remote_attachment())
            .await?;
        assert!(Arc::ptr_eq(&first.shared, &second.shared));
        assert_eq!(alix.client.attachments().list_pending().await?.len(), 1);
    }

    // verifies: ATCH-036, ATCH-034, EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn staged_unusable() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let staged = dir
            .path()
            .join(staged_path(&pending.remote_attachment().content_digest)?);
        tokio::fs::write(staged, b"damaged").await?;
        let events = alix
            .client
            .context
            .events()
            .subscribe_app(EventFilter::new([
                EventKind::AttachmentUploadStarted,
                EventKind::AttachmentUploadFailed,
            ]))?;
        assert_eq!(
            pending.upload().await.unwrap_err().cause,
            Cause::StagedUnusable
        );
        assert!(matches!(
            pending.status(),
            PendingAttachmentStatus::Failed(_)
        ));
        let emitted = events.drain();
        assert_eq!(emitted.len(), 2);
        let key = pending.reference().attachment_key;
        assert!(matches!(
            &emitted[0].client,
            Some(ClientEvent::AttachmentUploadStarted(reference)) if reference.attachment_key == key
        ));
        assert!(matches!(
            &emitted[1].client,
            Some(ClientEvent::AttachmentUploadFailed(failed)) if failed.attachment_key == key
        ));
    }

    // verifies: ATCH-026, ATCH-034
    #[xmtp_common::test(unwrap_try = true)]
    async fn blocked_connection_upload_fails() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut mock = xmtp_api_backend::MockBackendClient::new();
        mock.expect_create_upload().times(0);
        let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .api_client(Arc::new(mock))
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                offer,
            )))
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await?;
        let pending = client
            .attachments()
            .pending(pending.remote_attachment())
            .await?;
        client
            .context
            .server_configuration
            .block_connection(BlockedConnection::BackendMismatch {
                stored: "one".into(),
                received: "two".into(),
            });
        assert_eq!(
            pending.upload().await.unwrap_err().cause,
            Cause::ConnectionBlocked
        );
    }

    // verifies: ATCH-060, ATCH-061
    #[xmtp_common::test(unwrap_try = true)]
    fn causes_map() {
        use xmtp_proto::api::{ApiClientError, AuthError};
        let credential = api_error(xmtp_api::ApiError::Auth(AuthError::CredentialRejected {
            retryable: true,
        }));
        assert_eq!(credential.cause, Cause::Credential);
        assert_eq!(
            credential.credential_kind,
            Some(CredentialFailureKind::CredentialRejected)
        );
        assert!(credential.retryable);
        let no_credential = api_error(xmtp_api::ApiError::Auth(AuthError::MissingCredential));
        assert_eq!(
            no_credential.credential_kind,
            Some(CredentialFailureKind::MissingCredential)
        );
        assert!(!no_credential.retryable);
        for code in [
            tonic::Code::InvalidArgument,
            tonic::Code::OutOfRange,
            tonic::Code::Unimplemented,
        ] {
            let network = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                tonic::Status::new(code, "rejected"),
            ));
            assert_eq!(
                api_error(xmtp_api::dyn_err(network)).cause,
                Cause::BackendRejected
            );
        }
        let unavailable = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
            tonic::Status::unavailable("offline"),
        ));
        assert_eq!(
            api_error(xmtp_api::dyn_err(unavailable)).cause,
            Cause::BackendUnavailable
        );
        let target = AttachmentClientError::from(AttachmentError::new(Cause::TargetRejected));
        assert_eq!(target.cause, Cause::TargetRejected);
    }

    // verifies: ATCH-067, ATCH-068
    #[xmtp_common::test(unwrap_try = true)]
    async fn pending_expire() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let digest = &pending.remote_attachment().content_digest;
        alix.client.context.db().delete_pending_attachment(digest)?;
        alix.client
            .context
            .db()
            .insert_or_ignore_pending_attachment(
                digest,
                &pending.remote_attachment().encode_to_vec(),
                now_ns() - 172_800_000_000_000,
            )?;
        assert!(alix.client.attachments().list_pending().await?.is_empty());
        alix.client
            .context
            .attachments
            .sweep(&alix.client.context)
            .await?;
        assert!(!dir.path().join(staged_path(digest)?).exists());
        let registry = &alix.client.context.attachments.pending;
        assert!(!registry.lock().contains_key(digest));
    }

    // verifies: ATCH-068
    #[xmtp_common::test(unwrap_try = true)]
    async fn sweep_holds_upload_state_until_expired_file_is_removed() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let digest = pending.remote_attachment().content_digest.clone();
        let db = alix.client.context.db();
        db.delete_pending_attachment(&digest)?;
        db.insert_or_ignore_pending_attachment(
            &digest,
            &pending.remote_attachment().encode_to_vec(),
            now_ns() - 172_800_000_000_000,
        )?;
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        *alix.client.context.attachments.sweep_pause.lock() =
            Some((entered.clone(), resume.clone()));
        let context = alix.client.context.clone();
        let sweep =
            xmtp_common::task::spawn(async move { context.attachments.sweep(&context).await });
        entered.notified().await;
        assert!(pending.shared.state.try_lock().is_err());
        let upload = xmtp_common::task::spawn(async move { pending.upload().await });
        resume.notify_one();
        sweep.await??;
        assert_eq!(upload.await?.unwrap_err().cause, Cause::StagedUnusable);
        assert!(!dir.path().join(staged_path(&digest)?).exists());
    }

    // verifies: ATCH-068
    #[xmtp_common::test(unwrap_try = true)]
    async fn sweep_error_does_not_block_build() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let digest = &pending.remote_attachment().content_digest;
        alix.client.context.db().delete_pending_attachment(digest)?;
        alix.client
            .context
            .db()
            .insert_or_ignore_pending_attachment(
                digest,
                &pending.remote_attachment().encode_to_vec(),
                now_ns() - 172_800_000_000_000,
            )?;
        let staged = dir.path().join(staged_path(digest)?);
        tokio::fs::remove_file(&staged).await?;
        tokio::fs::create_dir(&staged).await?;
        let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                offer,
            )))
            .with_disable_workers(true)
            .build()
            .await?;
        assert!(next.attachments().offered());
        assert!(staged.is_dir());
    }

    // verifies: ATCH-008, ATCH-009, ATCH-030
    #[xmtp_common::test(unwrap_try = true)]
    async fn offer_readable() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let offered = alix
            .client
            .server_configuration()
            .attachments
            .as_ref()
            .expect("offer");
        assert_eq!(offered.max_upload_bytes, 10_485_760);
        assert_eq!(offered.base_url, "http://localhost:5050/attachments");
        assert!(alix.client.attachments().offered());
    }

    // verifies: ATCH-067
    #[xmtp_common::test(unwrap_try = true)]
    async fn list_pending() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let one = alix.client.attachments().create(bytes()).await?;
        let two = alix
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"second".to_vec(),
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await?;
        let listed = alix.client.attachments().list_pending().await?;
        assert_eq!(listed.len(), 2);
        assert_eq!(
            listed[0].remote_attachment().content_digest,
            one.remote_attachment().content_digest
        );
        assert_eq!(
            listed[1].remote_attachment().content_digest,
            two.remote_attachment().content_digest
        );
    }

    // verifies: ATCH-038
    #[xmtp_common::test(unwrap_try = true)]
    async fn resume_by_remote() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let resumed = alix
            .client
            .attachments()
            .pending(pending.remote_attachment())
            .await?;
        assert_eq!(resumed.status(), PendingAttachmentStatus::Waiting);
        assert_eq!(
            resumed.remote_attachment().url,
            pending.remote_attachment().url
        );
    }

    // verifies: ATCH-029, ATCH-034
    #[xmtp_common::test(unwrap_try = true)]
    async fn permanent_rejection_not_resent() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        drop(pending);
        let mut mock = xmtp_api_backend::MockBackendClient::new();
        mock.expect_create_upload().times(1).returning(|_| {
            Err(xmtp_proto::api::ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument(
                    "permanent",
                )),
            ))
        });
        let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .api_client(Arc::new(mock))
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                offer,
            )))
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await?;
        let pending = client.attachments().pending(&remote).await?;
        let first_error = pending.upload().await.unwrap_err();
        assert_eq!(first_error.cause, Cause::BackendRejected);
        drop(pending);
        let pending = client.attachments().pending(&remote).await?;
        assert!(matches!(
            pending.status(),
            PendingAttachmentStatus::Failed(AttachmentClientError {
                cause: Cause::BackendRejected,
                ..
            })
        ));
        assert_eq!(pending.upload().await.unwrap_err(), first_error);
    }

    // verifies: ATCH-061
    #[xmtp_common::test(unwrap_try = true)]
    fn credential_kind_kept() {
        use xmtp_proto::api::AuthError;
        let cases = [
            (
                AuthError::CallbackFailed { retryable: true },
                CredentialFailureKind::CallbackFailed,
                true,
            ),
            (
                AuthError::Exhausted,
                CredentialFailureKind::Exhausted,
                false,
            ),
            (
                AuthError::ExhaustedAfterAttempt,
                CredentialFailureKind::Exhausted,
                false,
            ),
        ];
        for (auth, kind, retryable) in cases {
            let error = api_error(xmtp_api::ApiError::Auth(auth));
            assert_eq!(error.cause, Cause::Credential);
            assert_eq!(error.credential_kind, Some(kind));
            assert_eq!(error.retryable, retryable);
        }
    }

    // verifies: ATCH-035, ATCH-066
    #[xmtp_common::test(unwrap_try = true)]
    async fn pending_persist_restart() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                offer,
            )))
            .build()
            .await?;
        let resumed = next.attachments().pending(&remote).await?;
        assert_eq!(resumed.status(), PendingAttachmentStatus::Waiting);
    }

    // verifies: ATCH-024, ATCH-025, ATCH-037, ATCH-034, EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn put_as_signed() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let path = dir
            .path()
            .join(staged_path(&pending.remote_attachment().content_digest)?);
        let events = alix
            .client
            .context
            .events()
            .subscribe_app(EventFilter::new([
                EventKind::AttachmentUploadStarted,
                EventKind::AttachmentUploadCompleted,
            ]))?;
        pending.upload().await?;
        assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
        assert!(!path.exists());
        assert!(alix.client.attachments().list_pending().await?.is_empty());
        let emitted = events.drain();
        assert_eq!(emitted.len(), 2);
        let key = pending.reference().attachment_key;
        assert!(matches!(
            &emitted[0].client,
            Some(ClientEvent::AttachmentUploadStarted(reference)) if reference.attachment_key == key
        ));
        assert!(matches!(
            &emitted[1].client,
            Some(ClientEvent::AttachmentUploadCompleted(reference)) if reference.attachment_key == key
        ));
    }

    // verifies: ATCH-034, ATCH-035, EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn dropped_upload_waiter_does_not_cancel_attempt() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let events = alix
            .client
            .context
            .events()
            .subscribe_app(EventFilter::new([
                EventKind::AttachmentUploadStarted,
                EventKind::AttachmentUploadCompleted,
            ]))?;
        let mut upload = Box::pin(pending.upload());
        assert!(matches!(
            futures::poll!(upload.as_mut()),
            std::task::Poll::Pending
        ));
        drop(upload);
        tokio::time::timeout(Duration::from_secs(15), pending.upload()).await??;
        assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
        let emitted = events.drain();
        assert_eq!(emitted.len(), 2);
        let key = pending.reference().attachment_key;
        assert!(matches!(
            &emitted[0].client,
            Some(ClientEvent::AttachmentUploadStarted(reference)) if reference.attachment_key == key
        ));
        assert!(matches!(
            &emitted[1].client,
            Some(ClientEvent::AttachmentUploadCompleted(reference)) if reference.attachment_key == key
        ));
    }

    // verifies: ATCH-025, ATCH-034
    #[xmtp_common::test(unwrap_try = true)]
    async fn upload_table() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
        let events = alix
            .client
            .context
            .events()
            .subscribe_app(EventFilter::new([
                EventKind::AttachmentUploadStarted,
                EventKind::AttachmentUploadCompleted,
            ]))?;
        let (first, second) = tokio::join!(pending.upload(), pending.upload());
        first?;
        second?;
        pending.upload().await?;
        assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
        assert_eq!(events.drain().len(), 2);
    }

    // verifies: ATCH-025, ATCH-037
    #[xmtp_common::test(unwrap_try = true)]
    async fn complete_releases_staged() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let staged = dir
            .path()
            .join(staged_path(&pending.remote_attachment().content_digest)?);
        assert!(staged.exists());
        pending.upload().await?;
        assert!(!staged.exists());
        assert!(alix.client.attachments().list_pending().await?.is_empty());
    }

    // verifies: ATCH-025
    #[xmtp_common::test(unwrap_try = true)]
    async fn already_stored_is_complete() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        let staged = dir.path().join(staged_path(&remote.content_digest)?);
        let ciphertext = tokio::fs::read(&staged).await?;
        pending.upload().await?;
        tokio::fs::write(&staged, ciphertext).await?;
        alix.client
            .context
            .db()
            .insert_or_ignore_pending_attachment(
                &remote.content_digest,
                &remote.encode_to_vec(),
                now_ns(),
            )?;
        let second = PendingAttachment {
            context: alix.client.context.clone(),
            remote,
            shared: Arc::new(PendingShared::new()),
        };
        second.upload().await?;
        assert_eq!(second.status(), PendingAttachmentStatus::Complete);
    }

    // verifies: ATCH-043, ATCH-050, ATCH-051, ATCH-063
    #[xmtp_common::test(unwrap_try = true)]
    async fn plaintext_content_exact() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        pending.upload().await?;
        tester!(bo, attachments_dir: recipient.path(), configured: |_config: &mut ServerConfiguration| {}, disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                |_config: &mut ServerConfiguration| {},
            )))
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        assert!(!client.attachments().offered());
        let downloaded = client.attachments().download(&remote).await?;
        assert_eq!(downloaded.path, client.attachments().local_path(&remote)?);
        assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(downloaded.filename.as_deref(), Some("note.txt"));
        assert_eq!(
            tokio::fs::read(&downloaded.path).await?,
            b"attachment content"
        );
        assert_eq!(client.attachments().list_local().await?.len(), 1);
    }

    // verifies: ATCH-052, ATCH-063
    #[xmtp_common::test(unwrap_try = true)]
    async fn existing_file_keeps_decoded_metadata() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"image bytes".to_vec(),
                filename: Some("photo.png".into()),
                mime_type: "image/png".into(),
            })
            .await?;
        let remote = pending.remote_attachment().clone();
        pending.upload().await?;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        for _ in 0..2 {
            let downloaded = client.attachments().download(&remote).await?;
            assert_eq!(downloaded.mime_type.as_deref(), Some("image/png"));
            assert_eq!(downloaded.filename.as_deref(), Some("photo.png"));
            assert_eq!(tokio::fs::read(&downloaded.path).await?, b"image bytes");
        }
    }

    // verifies: ATCH-051, ATCH-063
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_metadata_write_removes_downloaded_file() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        pending.upload().await?;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        client.context.db().raw_query(|conn| {
            xmtp_db::diesel::sql_query(
                "CREATE TRIGGER reject_attachment_metadata BEFORE INSERT ON local_attachments \
                 BEGIN SELECT RAISE(ABORT, 'metadata rejected'); END",
            )
            .execute(conn)
        })?;
        let path = client.attachments().local_path(&remote)?;
        let error = client.attachments().download(&remote).await.unwrap_err();
        assert_eq!(error.cause, Cause::LocalStorage);
        assert!(!path.exists());
        assert!(client.context.db().list_local_attachments()?.is_empty());
        client.context.db().raw_query(|conn| {
            xmtp_db::diesel::sql_query("DROP TRIGGER reject_attachment_metadata").execute(conn)
        })?;
        let downloaded = client.attachments().download(&remote).await?;
        assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(tokio::fs::read(path).await?, b"attachment content");
    }

    // verifies: ATCH-044, ATCH-059
    #[xmtp_common::test(unwrap_try = true)]
    async fn local_path_no_io() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut remote = pending.remote_attachment().clone();
        let path = alix.client.attachments().local_path(&remote)?;
        assert!(path.ends_with("note.txt"));
        remote.secret.pop();
        assert_eq!(
            alix.client
                .attachments()
                .local_path(&remote)
                .unwrap_err()
                .cause,
            Cause::Malformed
        );
        assert_eq!(
            alix.client
                .attachments()
                .download(&remote)
                .await
                .unwrap_err()
                .cause,
            Cause::Malformed
        );
    }

    // verifies: ATCH-045, ATCH-046, ATCH-052, ATCH-065
    #[xmtp_common::test(unwrap_try = true)]
    async fn existing_not_fetched() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut remote = pending.remote_attachment().clone();
        let (url, requests) = serve_body(b"forged".to_vec()).await;
        remote.url = url;
        pending.upload().await?;
        let path = alix.client.attachments().download(&remote).await?.path;
        assert_eq!(tokio::fs::read(path).await?, b"attachment content");
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    // verifies: ATCH-047, ATCH-062, ATCH-063
    #[xmtp_common::test(unwrap_try = true)]
    async fn list_local_exact() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let one = alix.client.attachments().create(bytes()).await?;
        let two = alix
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"second".to_vec(),
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await?;
        let listed = alix.client.attachments().list_local().await?;
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].path, plaintext_rel_path(one.remote_attachment())?);
        assert_eq!(listed[1].path, plaintext_rel_path(two.remote_attachment())?);
        alix.client
            .attachments()
            .delete_local(one.remote_attachment())
            .await?;
        assert!(!one.local_path()?.exists());
        assert_eq!(alix.client.attachments().list_local().await?.len(), 1);
    }

    // verifies: ATCH-051, ATCH-056, ATCH-060
    #[xmtp_common::test(unwrap_try = true)]
    async fn forged_body_leaves_nothing() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut remote = pending.remote_attachment().clone();
        let (url, requests) = serve_body(b"forged".to_vec()).await;
        remote.url = url;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(
            client
                .attachments()
                .download(&remote)
                .await
                .unwrap_err()
                .cause,
            Cause::DigestMismatch
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(!client.attachments().local_path(&remote)?.exists());
        assert!(client.attachments().list_local().await?.is_empty());
    }

    // verifies: ATCH-058
    #[xmtp_common::test(unwrap_try = true)]
    async fn one_fetch_per_path() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut remote = pending.remote_attachment().clone();
        let staged =
            tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
        let (url, requests) = serve_body(staged).await;
        remote.url = url;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        let attachments = client.attachments();
        let (first, second) =
            tokio::join!(attachments.download(&remote), attachments.download(&remote));
        assert_eq!(first?.path, second?.path);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }

    // verifies: ATCH-047, ATCH-051, EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn delete_cancels_running() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut remote = pending.remote_attachment().clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        remote.url = format!("http://{}/file", listener.local_addr()?);
        let (connected, ready) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\npartial")
                .await
                .unwrap();
            let _ = connected.send(());
            std::future::pending::<()>().await;
        }));
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        tokio::fs::create_dir_all(recipient.path().join(attachment_key(&remote)?)).await?;
        let attachments = client.attachments();
        let events = client.context.events().subscribe_app(EventFilter::new([
            EventKind::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadFailed,
            EventKind::AttachmentDeleted,
        ]))?;
        let running_client = client.clone();
        let running_remote = remote.clone();
        let download = xmtp_common::spawn(None, async move {
            running_client.attachments().download(&running_remote).await
        });
        tokio::time::timeout(Duration::from_secs(5), ready).await??;
        tokio::time::timeout(Duration::from_secs(5), attachments.delete_local(&remote)).await??;
        let outcome = tokio::time::timeout(Duration::from_secs(5), download.join()).await??;
        assert_eq!(outcome.unwrap_err().cause, Cause::Deleted);
        assert!(!attachments.local_path(&remote)?.exists());
        assert!(attachments.list_local().await?.is_empty());
        let emitted = events.drain();
        let key = attachment_key(&remote)?;
        assert_eq!(emitted.len(), 3);
        assert!(
            matches!(&emitted[0].client, Some(ClientEvent::AttachmentDownloadStarted(reference)) if reference.attachment_key == key)
        );
        assert!(
            matches!(&emitted[1].client, Some(ClientEvent::AttachmentDownloadFailed(failed)) if failed.attachment_key == key && failed.cause == "deleted")
        );
        assert!(
            matches!(&emitted[2].client, Some(ClientEvent::AttachmentDeleted(reference)) if reference.attachment_key == key)
        );
    }

    // verifies: ATCH-047
    #[xmtp_common::test(unwrap_try = true)]
    async fn delete_refuses_download_started_during_deletion() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let mut remote = pending.remote_attachment().clone();
        let body =
            tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
        let (url, requests) = serve_body(body).await;
        remote.url = url;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        *client.context.attachments.delete_pause.lock() = Some((entered.clone(), resume.clone()));
        let deleting_client = client.clone();
        let deleting_remote = remote.clone();
        let deletion = xmtp_common::task::spawn(async move {
            deleting_client
                .attachments()
                .delete_local(&deleting_remote)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
        let download = tokio::time::timeout(
            Duration::from_secs(5),
            client.attachments().download(&remote),
        )
        .await?;
        resume.notify_one();
        tokio::time::timeout(Duration::from_secs(5), deletion).await???;
        assert_eq!(download.unwrap_err().cause, Cause::Deleted);
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(!client.attachments().local_path(&remote)?.exists());
    }

    // verifies: ATCH-047, EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn delete_cancels_upload() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let events = alix
            .client
            .context
            .events()
            .subscribe_app(EventFilter::new([
                EventKind::AttachmentUploadStarted,
                EventKind::AttachmentUploadFailed,
                EventKind::AttachmentDeleted,
            ]))?;
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        let mut upload = Box::pin(pending.upload());
        assert!(matches!(
            futures::poll!(upload.as_mut()),
            std::task::Poll::Pending
        ));
        alix.client.attachments().delete_local(&remote).await?;
        assert_eq!(upload.await.unwrap_err().cause, Cause::Deleted);
        assert!(matches!(
            pending.status(),
            PendingAttachmentStatus::Failed(AttachmentClientError {
                cause: Cause::Deleted,
                ..
            })
        ));
        assert!(
            alix.client
                .context
                .db()
                .list_pending_attachments_since(0)?
                .iter()
                .all(|row| row.content_digest != remote.content_digest)
        );
        let emitted = events.drain();
        let key = attachment_key(&remote)?;
        assert_eq!(emitted.len(), 3);
        assert!(
            matches!(&emitted[0].client, Some(ClientEvent::AttachmentUploadStarted(reference)) if reference.attachment_key == key)
        );
        assert!(
            matches!(&emitted[1].client, Some(ClientEvent::AttachmentUploadFailed(failed)) if failed.attachment_key == key && failed.cause == "deleted")
        );
        assert!(
            matches!(&emitted[2].client, Some(ClientEvent::AttachmentDeleted(reference)) if reference.attachment_key == key)
        );
    }

    // verifies: ATCH-059
    #[xmtp_common::test(unwrap_try = true)]
    async fn malformed_never_fetched() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let (url, requests) = serve_body(b"body".to_vec()).await;
        for field in 0..4 {
            let mut remote = pending.remote_attachment().clone();
            remote.url = url.clone();
            match field {
                0 => remote.content_digest.make_ascii_uppercase(),
                1 => {
                    remote.secret.pop();
                }
                2 => {
                    remote.salt.pop();
                }
                _ => {
                    remote.nonce.pop();
                }
            }
            assert_eq!(
                alix.client
                    .attachments()
                    .local_path(&remote)
                    .unwrap_err()
                    .cause,
                Cause::Malformed
            );
            assert_eq!(
                alix.client
                    .attachments()
                    .download(&remote)
                    .await
                    .unwrap_err()
                    .cause,
                Cause::Malformed
            );
        }
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    // verifies: ATCH-056
    #[xmtp_common::test(unwrap_try = true)]
    async fn download_size_limit() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        pending.upload().await?;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                max_download_bytes: Some(1),
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(
            client
                .attachments()
                .download(&remote)
                .await
                .unwrap_err()
                .cause,
            Cause::TooLarge
        );
        assert!(!client.attachments().local_path(&remote)?.exists());
    }

    // verifies: ATCH-046, ATCH-063, P22, P24
    #[xmtp_common::test(unwrap_try = true)]
    async fn reconcile_after_crash_points() {
        use std::time::UNIX_EPOCH;
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let original = pending.local_path()?;
        let relative = plaintext_rel_path(pending.remote_attachment())?;
        alix.client
            .context
            .db()
            .delete_local_attachment(&relative)?;
        let missing = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/absent";
        alix.client
            .context
            .db()
            .insert_or_ignore_local_attachment(missing, 1, None, None)?;
        let temp = dir.path().join(".tmp/stale");
        tokio::fs::create_dir_all(temp.parent().unwrap()).await?;
        tokio::fs::write(&temp, b"temporary").await?;
        let staged = dir.path().join(format!(".staged/{}", "a".repeat(64)));
        tokio::fs::create_dir_all(staged.parent().unwrap()).await?;
        tokio::fs::write(&staged, b"orphan").await?;
        let fresh_temp = dir.path().join(".tmp/fresh");
        let fresh_staged = dir.path().join(format!(".staged/{}", "c".repeat(64)));
        tokio::fs::write(&fresh_temp, b"new temporary").await?;
        tokio::fs::write(&fresh_staged, b"new orphan").await?;
        let owned_staged = dir
            .path()
            .join(staged_path(&pending.remote_attachment().content_digest)?);
        assert!(owned_staged.exists());
        for file in [&temp, &staged, &owned_staged] {
            std::fs::File::open(file)?.set_modified(UNIX_EPOCH + Duration::from_secs(1))?;
        }
        let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .with_disable_workers(true)
            .build()
            .await?;
        assert!(original.exists());
        assert!(!temp.exists());
        assert!(!staged.exists());
        assert!(fresh_temp.exists());
        assert!(fresh_staged.exists());
        assert!(owned_staged.exists());
        assert!(next.attachments().list_pending().await?.iter().any(|row| {
            row.remote_attachment().content_digest == pending.remote_attachment().content_digest
        }));
        let listed = next.attachments().list_local().await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, relative);
        assert!(listed[0].created_at_ns > 0);
        let adopted = next
            .attachments()
            .download(pending.remote_attachment())
            .await?;
        assert_eq!(adopted.mime_type, None);
        assert_eq!(adopted.filename, None);
    }

    // verifies: ATCH-063, P24
    #[xmtp_common::test(unwrap_try = true)]
    async fn reconcile_ignores_stray_and_nested_files() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let valid = format!("{}/plain.txt", "a".repeat(64));
        let nested = format!("{}/nested/extra.txt", "a".repeat(64));
        for path in [&valid, &nested, "misc/file.txt", ".DS_Store"] {
            let path = dir.path().join(path);
            tokio::fs::create_dir_all(path.parent().unwrap()).await?;
            tokio::fs::write(path, b"app file").await?;
        }
        let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .with_disable_workers(true)
            .build()
            .await?;
        let listed = next.attachments().list_local().await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, valid);
        assert!(dir.path().join(nested).exists());
        assert!(dir.path().join("misc/file.txt").exists());
        assert!(dir.path().join(".DS_Store").exists());
    }

    // verifies: ATCH-025, ATCH-037, P22
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn pending_row_removed_before_staged_file() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let digest = pending.remote_attachment().content_digest.clone();
        let staged_dir = dir.path().join(".staged");
        std::fs::set_permissions(&staged_dir, std::fs::Permissions::from_mode(0o555))?;
        let result = pending.upload().await;
        std::fs::set_permissions(&staged_dir, std::fs::Permissions::from_mode(0o755))?;
        result?;
        assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
        assert!(
            alix.client
                .context
                .db()
                .list_pending_attachments_since(0)?
                .iter()
                .all(|row| row.content_digest != digest)
        );
        assert!(dir.path().join(staged_path(&digest)?).exists());
    }

    // verifies: EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn attachment_event_order() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        let events = client.context.events().subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadCompleted,
            EventKind::AttachmentDeleted,
        ]))?;
        let pending = client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        pending.upload().await?;
        client.attachments().delete_local(&remote).await?;
        client.attachments().download(&remote).await?;
        client.attachments().delete_local(&remote).await?;
        let kinds: Vec<_> = events
            .drain()
            .into_iter()
            .map(|event| {
                let event = event.client.unwrap();
                let key = attachment_key(&remote).unwrap();
                match &event {
                    ClientEvent::AttachmentUploadStarted(reference)
                    | ClientEvent::AttachmentUploadCompleted(reference)
                    | ClientEvent::AttachmentDownloadStarted(reference)
                    | ClientEvent::AttachmentDownloadCompleted(reference)
                    | ClientEvent::AttachmentDeleted(reference) => {
                        assert_eq!(reference.attachment_key, key)
                    }
                    _ => panic!("unexpected event"),
                }
                event.kind()
            })
            .collect();
        assert_eq!(
            kinds,
            [
                EventKind::AttachmentUploadStarted,
                EventKind::AttachmentUploadCompleted,
                EventKind::AttachmentDeleted,
                EventKind::AttachmentDownloadStarted,
                EventKind::AttachmentDownloadCompleted,
                EventKind::AttachmentDeleted,
            ]
        );
    }

    // verifies: EVENT-001, EVENT-055
    #[xmtp_common::test(unwrap_try = true)]
    async fn attachment_event_kinds() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        let events = client.context.events().subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadCompleted,
            EventKind::AttachmentDownloadFailed,
            EventKind::AttachmentDeleted,
        ]))?;
        let failed = client.attachments().create(bytes()).await?;
        let staged = dir
            .path()
            .join(staged_path(&failed.remote_attachment().content_digest)?);
        tokio::fs::write(staged, b"damaged").await?;
        assert_eq!(
            failed.upload().await.unwrap_err().cause,
            Cause::StagedUnusable
        );
        client
            .attachments()
            .delete_local(failed.remote_attachment())
            .await?;
        let pending = client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        pending.upload().await?;
        client.attachments().delete_local(&remote).await?;
        client.attachments().download(&remote).await?;
        client.attachments().delete_local(&remote).await?;
        let (url, _) = serve_body(b"forged".to_vec()).await;
        let mut forged = remote;
        forged.url = url;
        assert_eq!(
            client
                .attachments()
                .download(&forged)
                .await
                .unwrap_err()
                .cause,
            Cause::DigestMismatch
        );
        let kinds: Vec<_> = events
            .drain()
            .into_iter()
            .map(|entry| entry.client.unwrap().kind())
            .collect();
        for kind in [
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadCompleted,
            EventKind::AttachmentDownloadFailed,
            EventKind::AttachmentDeleted,
        ] {
            assert!(kinds.contains(&kind), "missing {kind:?}");
        }
    }

    // verifies: ATCH-065
    #[xmtp_common::test(unwrap_try = true)]
    async fn no_auto_download() {
        use xmtp_content_types::{ContentCodec, remote_attachment::RemoteAttachmentCodec};
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path(), disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let (url, requests) = serve_body(b"ciphertext".to_vec()).await;
        let mut remote = pending.remote_attachment().clone();
        remote.url = url;
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        let received = bo.sync_welcomes().await?;
        let bo_group = received.first()?.clone();
        let encoded = RemoteAttachmentCodec::encode(remote.clone())?.encode_to_vec();
        group.send_message(&encoded, Default::default()).await?;
        bo_group.sync().await?;
        assert_eq!(bo_group.test_last_message_bytes().await??, encoded);
        xmtp_common::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    // verifies: ATCH-043, ATCH-051
    #[xmtp_common::test(unwrap_try = true)]
    async fn compressed_content_two_pass() {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write as _;
        use xmtp_proto::xmtp::mls::message_contents::{
            Compression as WireCompression, EncodedContent,
        };
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), disable_workers);
        let content = b"compressed attachment content";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(content)?;
        let mut envelope =
            EncodedContent::decode(encoded_prefix(None, "text/plain", 0).as_slice())?;
        envelope.compression = Some(WireCompression::Gzip as i32);
        envelope.content = encoder.finish()?;
        let material = KeyMaterial::random();
        let mut cipher = GcmEncryptor::new(&material);
        let mut body = Vec::new();
        cipher.update(&envelope.encode_to_vec(), &mut body)?;
        body.extend_from_slice(&cipher.finish());
        let digest = hex::encode(Sha256::digest(&body));
        let (url, _) = serve_body(body.clone()).await;
        let mut remote = remote_attachment(
            "http://localhost",
            &digest,
            &material,
            body.len() as u32,
            None,
        );
        remote.url = url;
        let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        let path = client.attachments().download(&remote).await?.path;
        assert_eq!(tokio::fs::read(path).await?, content);
    }

    // verifies: ATCH-025, ATCH-038, ATCH-047, ATCH-050, ATCH-051, P23
    #[xmtp_common::test(unwrap_try = true)]
    async fn s3_end_to_end() {
        let sender = tempfile::tempdir()?;
        let recipient = tempfile::tempdir()?;
        tester!(alix, attachments_dir: sender.path().join("attachments"), disable_workers);
        let source = sender.path().join("source.txt");
        tokio::fs::write(&source, b"from path").await?;
        let path_pending = alix
            .client
            .attachments()
            .create(AttachmentSource::Path {
                path: source,
                filename: None,
                mime_type: "text/plain".into(),
            })
            .await?;
        let path_remote = path_pending.remote_attachment().clone();
        let bytes_pending = alix.client.attachments().create(bytes()).await?;
        let bytes_remote = bytes_pending.remote_attachment().clone();
        let staged = sender
            .path()
            .join("attachments")
            .join(staged_path(&bytes_remote.content_digest)?);
        let ciphertext = tokio::fs::read(&staged).await?;
        path_pending.upload().await?;
        bytes_pending.upload().await?;
        tokio::fs::write(&staged, ciphertext).await?;
        alix.client
            .context
            .db()
            .insert_or_ignore_pending_attachment(
                &bytes_remote.content_digest,
                &bytes_remote.encode_to_vec(),
                now_ns(),
            )?;
        let repeated = alix.client.attachments().pending(&bytes_remote).await?;
        repeated.upload().await?;
        assert_eq!(repeated.status(), PendingAttachmentStatus::Complete);
        tester!(bo, attachments_dir: recipient.path(), disable_workers);
        let recipient_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(
            tokio::fs::read(
                recipient_client
                    .attachments()
                    .download(&path_remote)
                    .await?
                    .path
            )
            .await?,
            b"from path"
        );
        assert_eq!(
            tokio::fs::read(
                recipient_client
                    .attachments()
                    .download(&bytes_remote)
                    .await?
                    .path
            )
            .await?,
            b"attachment content"
        );
        let (url, _) = serve_body(b"forged".to_vec()).await;
        let mut forged = bytes_remote.clone();
        forged.url = url;
        let forged_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .attachments_dir(recipient.path().join("forged"))
            .attachment_options(AttachmentOptions {
                allow_private_network: true,
                ..Default::default()
            })
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(
            forged_client
                .attachments()
                .download(&forged)
                .await
                .unwrap_err()
                .cause,
            Cause::DigestMismatch
        );
        let restart_pending = alix.client.attachments().create(bytes()).await?;
        let restart_remote = restart_pending.remote_attachment().clone();
        let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
            .with_disable_workers(true)
            .build()
            .await?;
        assert!(
            restarted
                .attachments()
                .list_pending()
                .await?
                .iter()
                .any(|p| p.remote_attachment().content_digest == restart_remote.content_digest)
        );
        restarted
            .attachments()
            .pending(&restart_remote)
            .await?
            .upload()
            .await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut running = restart_remote;
        running.url = format!("http://{}/running", listener.local_addr()?);
        let (connected, ready) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\npartial")
                .await
                .unwrap();
            let _ = connected.send(());
            std::future::pending::<()>().await;
        }));
        let attachments = forged_client.attachments();
        let running_client = forged_client.clone();
        let running_remote = running.clone();
        let download = xmtp_common::spawn(None, async move {
            running_client.attachments().download(&running_remote).await
        });
        tokio::time::timeout(Duration::from_secs(5), ready).await??;
        tokio::time::timeout(Duration::from_secs(5), attachments.delete_local(&running)).await??;
        let outcome = tokio::time::timeout(Duration::from_secs(5), download.join()).await??;
        assert_eq!(outcome.unwrap_err().cause, Cause::Deleted);
    }
}
