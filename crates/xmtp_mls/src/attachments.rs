//! Pending remote attachments owned by a client.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Weak},
    time::Duration,
};

use parking_lot::Mutex;
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use tokio::sync::{Mutex as AsyncMutex, watch};
use xmtp_attachments::{
    AttachmentError, AttachmentFailureCause as Cause, AttachmentOptions, DownloadSink as _,
    GcmEncryptor, KeyMaterial, LocalStore, Transfer, UploadRequest, ciphertext_len, encoded_prefix,
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
}

impl PendingShared {
    fn new() -> Self {
        let (watch, _) = watch::channel(PendingAttachmentStatus::Waiting);
        Self {
            state: AsyncMutex::new(PendingAttachmentStatus::Waiting),
            watch,
            permanent: Mutex::new(None),
        }
    }
}

#[doc(hidden)]
pub struct AttachmentRuntime {
    pub(crate) store: Option<Arc<dyn LocalStore>>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) options: AttachmentOptions,
    pending: Mutex<HashMap<String, Weak<PendingShared>>>,
}

impl Default for AttachmentRuntime {
    fn default() -> Self {
        Self {
            store: None,
            dir: None,
            options: AttachmentOptions::default(),
            pending: Mutex::new(HashMap::new()),
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
            let shared = self.pending.lock().get(&digest).and_then(Weak::upgrade);
            if let Some(shared) = shared
                && matches!(
                    *shared.state.lock().await,
                    PendingAttachmentStatus::Uploading
                )
            {
                continue;
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
        if length >= offer.max_upload_bytes || length > u32::MAX as u64 {
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
            encryptor.update(&prefix, &mut encrypted);
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
                            encryptor.update(&chunk[..count], &mut encrypted);
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
                            encryptor.update(&chunk, &mut encrypted);
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
                        encryptor.update(chunk, &mut encrypted);
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
            db.insert_or_ignore_local_attachment(&local, now_ns())
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
            let mut state = self.shared.state.lock().await;
            match &*state {
                PendingAttachmentStatus::Complete => return Ok(()),
                PendingAttachmentStatus::Uploading => {
                    drop(state);
                    loop {
                        watch
                            .changed()
                            .await
                            .map_err(|_| AttachmentClientError::new(Cause::Network))?;
                        match watch.borrow_and_update().clone() {
                            PendingAttachmentStatus::Complete => return Ok(()),
                            PendingAttachmentStatus::Failed(error) => return Err(error),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            *state = PendingAttachmentStatus::Uploading;
            self.shared.watch.send_replace(state.clone());
            self.context.events().emit(
                Some(ClientEvent::AttachmentUploadStarted(self.reference())),
                None,
            );
        }
        let result = self.upload_once().await;
        let mut state = self.shared.state.lock().await;
        *state = match &result {
            Ok(()) => PendingAttachmentStatus::Complete,
            Err(error) => PendingAttachmentStatus::Failed(error.clone()),
        };
        self.shared.watch.send_replace(state.clone());
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
        result
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
        store.remove_file(&path).await?;
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
                self.context
                    .attachment_runtime()
                    .sweep(&self.context)
                    .await
                    .map_err(|error| Box::new(error) as Box<_>)?;
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::{server_configuration::BlockedConnection, tester};
    use xmtp_configuration::{AttachmentsConfiguration, ServerConfiguration};
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

    // verifies: ATCH-030, ATCH-031, ATCH-011, ATCH-012
    #[xmtp_common::test(unwrap_try = true)]
    async fn remote_attachment_before_request() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment();
        assert!(remote.url.ends_with(&remote.content_digest));
        assert_eq!(remote.scheme, "http://");
        assert_eq!(
            remote.content_length,
            Some(
                ciphertext_len(encoded_prefix(Some("note.txt"), "text/plain", 18).len(), 18) as u32
            )
        );
        assert_eq!(remote.filename.as_deref(), Some("note.txt"));
        let staged = tokio::fs::read(dir.path().join(staged_path(&remote.content_digest)?)).await?;
        assert_eq!(hex::encode(Sha256::digest(&staged)), remote.content_digest);
        assert_eq!(remote.content_length, Some(staged.len() as u32));
        KeyMaterial::from_remote(remote)?;
        assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
        assert_eq!(
            tokio::fs::read(pending.local_path()?).await?,
            b"attachment content"
        );
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
        assert_eq!(events.drain().len(), 2);
    }

    // verifies: ATCH-026, ATCH-034
    #[xmtp_common::test(unwrap_try = true)]
    async fn blocked_connection_upload_fails() {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        alix.client.context.server_configuration.block_connection(
            BlockedConnection::BackendMismatch {
                stored: "one".into(),
                received: "two".into(),
            },
        );
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
    }

    // verifies: ATCH-008, ATCH-030
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
        let pending = client
            .attachments()
            .pending(pending.remote_attachment())
            .await?;
        assert_eq!(
            pending.upload().await.unwrap_err().cause,
            Cause::BackendRejected
        );
        assert_eq!(
            pending.upload().await.unwrap_err().cause,
            Cause::BackendRejected
        );
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
        assert_eq!(events.drain().len(), 2);
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
}
