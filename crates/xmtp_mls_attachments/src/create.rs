//! Attachment creation: staging the source, moving it into the store, and rolling back
//! a create that does not finish.

use super::*;

#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
static CREATE_MOVE_PAUSE: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>> =
    Mutex::new(None);
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
#[doc(hidden)]
pub fn pause_next_create_move(entered: Arc<tokio::sync::Notify>, resume: Arc<tokio::sync::Notify>) {
    *CREATE_MOVE_PAUSE.lock() = Some((entered, resume));
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
async fn rollback_create<Context: AttachmentContext>(
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
struct CreateRollbackGuard<Context: AttachmentContext + 'static> {
    context: Context,
    store: Arc<dyn LocalStore>,
    state: CreateRollbackState,
    publication: Option<OwnedMutexGuard<()>>,
    #[cfg(target_arch = "wasm32")]
    in_flight_move: Option<InFlightCreateMove>,
    committed: bool,
}
#[derive(Clone, Copy)]
enum CreateMoveKind {
    Plain,
    Staged,
}
#[cfg(target_arch = "wasm32")]
struct InFlightCreateMove {
    receiver: tokio::sync::oneshot::Receiver<Result<(), StoreMoveError>>,
    destination: String,
    kind: CreateMoveKind,
}
#[cfg(target_arch = "wasm32")]
impl InFlightCreateMove {
    fn record(&self, state: &mut CreateRollbackState) {
        match self.kind {
            CreateMoveKind::Plain => {
                state.plain_temp = None;
                state.final_plain = Some(self.destination.clone());
            }
            CreateMoveKind::Staged => {
                state.staged_temp = None;
                state.final_staged = Some(self.destination.clone());
            }
        }
    }
}
impl<Context: AttachmentContext + 'static> CreateRollbackGuard<Context> {
    #[cfg(not(target_arch = "wasm32"))]
    fn record_move(&mut self, kind: CreateMoveKind, destination: &str) {
        match kind {
            CreateMoveKind::Plain => {
                self.state.plain_temp = None;
                self.state.final_plain = Some(destination.to_owned());
            }
            CreateMoveKind::Staged => {
                self.state.staged_temp = None;
                self.state.final_staged = Some(destination.to_owned());
            }
        }
    }

    async fn rename_owned(
        &mut self,
        from: &str,
        to: &str,
        kind: CreateMoveKind,
    ) -> Result<(), StoreMoveError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = self.store.rename(from, to).await;
            if result.is_ok() {
                self.record_move(kind, to);
            }
            result
        }
        #[cfg(target_arch = "wasm32")]
        {
            let store = self.store.clone();
            let source = from.to_owned();
            let destination = to.to_owned();
            let (sender, receiver) = tokio::sync::oneshot::channel();
            drop(xmtp_common::task::spawn(async move {
                let result = store.rename(&source, &destination).await;
                #[cfg(feature = "test-hooks")]
                if matches!(kind, CreateMoveKind::Plain) && result.is_ok() {
                    let pause = CREATE_MOVE_PAUSE.lock().take();
                    if let Some((entered, resume)) = pause {
                        entered.notify_one();
                        resume.notified().await;
                    }
                }
                let _ = sender.send(result);
            }));
            self.in_flight_move = Some(InFlightCreateMove {
                receiver,
                destination: to.to_owned(),
                kind,
            });
            let result = match self.in_flight_move.as_mut() {
                Some(moving) => (&mut moving.receiver).await.unwrap_or_else(|_| {
                    Err(StoreMoveError::Other(AttachmentError::new(
                        Cause::LocalStorage,
                    )))
                }),
                None => Err(StoreMoveError::Other(AttachmentError::new(
                    Cause::LocalStorage,
                ))),
            };
            if let Some(moving) = self.in_flight_move.take()
                && result.is_ok()
            {
                moving.record(&mut self.state);
            }
            result
        }
    }

    async fn rollback(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(mut moving) = self.in_flight_move.take()
            && matches!((&mut moving.receiver).await, Ok(Ok(())))
        {
            moving.record(&mut self.state);
        }
        rollback_create(&self.context, &self.store, &self.state).await;
        self.committed = true;
    }
}
impl<Context: AttachmentContext + 'static> Drop for CreateRollbackGuard<Context> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let context = self.context.clone();
        let store = self.store.clone();
        let state = self.state.clone();
        #[cfg(target_arch = "wasm32")]
        let mut state = state;
        let publication = self.publication.take();
        #[cfg(target_arch = "wasm32")]
        let in_flight_move = self.in_flight_move.take();
        drop(xmtp_common::task::spawn(async move {
            let _publication = publication;
            #[cfg(target_arch = "wasm32")]
            if let Some(mut moving) = in_flight_move
                && matches!((&mut moving.receiver).await, Ok(Ok(())))
            {
                moving.record(&mut state);
            }
            rollback_create(&context, &store, &state).await;
        }));
    }
}

impl<Context: AttachmentContext> Attachments<Context> {
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
        #[cfg(not(target_arch = "wasm32"))]
        let mut native_source_file: Option<tokio::fs::File> = None;
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
                    #[cfg(unix)]
                    use std::os::unix::fs::OpenOptionsExt as _;
                    let mut options = std::fs::OpenOptions::new();
                    options.read(true);
                    #[cfg(unix)]
                    options.custom_flags(libc::O_NONBLOCK);
                    let file = options
                        .open(path)
                        .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                    let metadata = file
                        .metadata()
                        .map_err(|_| AttachmentClientError::new(Cause::SourceUnreadable))?;
                    if !metadata.is_file() {
                        return Err(AttachmentClientError::new(Cause::SourceUnreadable));
                    }
                    native_source_file = Some(tokio::fs::File::from_std(file));
                    metadata.len()
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
            #[cfg(target_arch = "wasm32")]
            in_flight_move: None,
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
                        let _ = path;
                        let mut file = native_source_file
                            .take()
                            .ok_or_else(|| AttachmentClientError::new(Cause::SourceUnreadable))?;
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
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            let create_pause = { self.runtime().create_move_pause.lock().clone() };
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
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
            match rollback
                .rename_owned(&plain_temp, &local, CreateMoveKind::Plain)
                .await
            {
                Ok(()) => {}
                Err(StoreMoveError::DestinationExists) => {
                    if !store.is_regular_file(&local).await? {
                        return Err(AttachmentClientError::new(Cause::LocalStorage));
                    }
                    store.remove_file(&plain_temp).await?;
                    rollback.state.plain_temp = None;
                }
                Err(error) => return Err(error.into()),
            }
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            let publish_pause = { self.runtime().create_publish_pause.lock().clone() };
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            if let Some(pause) = publish_pause
                && !pause.after_staged
            {
                *pause.path.lock() = Some(local.clone());
                *pause.staged_path.lock() = Some(ciphertext.clone());
                pause.entered.notify_one();
                pause.resume.notified().await;
            }
            match rollback
                .rename_owned(&staged_temp, &ciphertext, CreateMoveKind::Staged)
                .await
            {
                Ok(()) => {}
                Err(StoreMoveError::DestinationExists) => {
                    let valid = match store.open_read(&ciphertext).await {
                        Ok(file) => matches!(
                            file.sha256(length).await,
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
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            let publish_pause = { self.runtime().create_publish_pause.lock().clone() };
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
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
}
