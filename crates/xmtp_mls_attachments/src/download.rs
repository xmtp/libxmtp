//! Attachment downloads: fetching, decrypting and decoding into the store, and the
//! shared outcome concurrent downloads wait on.

use super::*;

pub(super) const CHUNK: usize = 64 * 1024;
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
static DOWNLOAD_MOVE_PAUSE: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>> =
    Mutex::new(None);
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
static DOWNLOAD_TEST_BODY: Mutex<Option<Vec<u8>>> = Mutex::new(None);
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
#[doc(hidden)]
pub fn pause_next_download_move(
    entered: Arc<tokio::sync::Notify>,
    resume: Arc<tokio::sync::Notify>,
) {
    *DOWNLOAD_MOVE_PAUSE.lock() = Some((entered, resume));
}
#[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
#[doc(hidden)]
pub fn set_next_download_body(body: Vec<u8>) {
    *DOWNLOAD_TEST_BODY.lock() = Some(body);
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
pub struct DownloadedAttachment {
    pub path: PathBuf,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}
pub(super) struct DownloadShared {
    pub(super) outcome: watch::Sender<Option<Result<DownloadedAttachment, AttachmentClientError>>>,
    pub(super) cancel: CancellationToken,
}
struct DownloadAttemptGuard<Context: AttachmentContext + 'static> {
    context: Context,
    remote: RemoteAttachment,
    relative: String,
    key: String,
    shared: Arc<DownloadShared>,
    publication: Option<tokio::sync::OwnedMutexGuard<()>>,
    finished: bool,
}
async fn publish_download_outcome<Context: AttachmentContext>(
    context: &Context,
    remote: &RemoteAttachment,
    relative: &str,
    key: &str,
    shared: &Arc<DownloadShared>,
    publication: Option<tokio::sync::OwnedMutexGuard<()>>,
    result: Result<DownloadedAttachment, AttachmentClientError>,
) {
    let runtime = context.attachment_runtime();
    let _publication = match publication {
        Some(guard) => guard,
        None => runtime.publication_lock.clone().lock_owned().await,
    };
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
impl<Context: AttachmentContext + 'static> DownloadAttemptGuard<Context> {
    async fn finish(&mut self, result: Result<DownloadedAttachment, AttachmentClientError>) {
        publish_download_outcome(
            &self.context,
            &self.remote,
            &self.relative,
            &self.key,
            &self.shared,
            self.publication.take(),
            result,
        )
        .await;
        self.finished = true;
    }
}
impl<Context: AttachmentContext + 'static> Drop for DownloadAttemptGuard<Context> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let context = self.context.clone();
        let remote = self.remote.clone();
        let relative = self.relative.clone();
        let key = self.key.clone();
        let shared = self.shared.clone();
        let publication = self.publication.take();
        drop(xmtp_common::task::spawn(async move {
            publish_download_outcome(
                &context,
                &remote,
                &relative,
                &key,
                &shared,
                publication,
                Err(AttachmentClientError::new(Cause::LocalStorage)),
            )
            .await;
        }));
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

impl<Context: AttachmentContext> Attachments<Context> {
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
        // Keep the file check and its row insert behind the same lock as deletion.
        let publication = self.runtime().publication_lock.lock().await;
        let shared = {
            let _guard = lock.lock().await;
            if self.runtime().deleting.lock().contains_key(&key) {
                return Err(AttachmentClientError::new(Cause::Deleted));
            }
            if store.exists(&relative).await? {
                if !store.is_regular_file(&relative).await? {
                    return Err(AttachmentClientError::new(Cause::LocalStorage));
                }
                #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                let pause = { self.runtime().download_existing_pause.lock().take() };
                #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                if let Some((entered, resume)) = pause {
                    entered.notify_one();
                    resume.notified().await;
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
                    publication: None,
                    finished: false,
                };
                drop(xmtp_common::task::spawn(async move {
                    let mut attempt = attempt;
                    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                    let panic_pause = { task.runtime().download_panic_pause.lock().take() };
                    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                    if let Some((entered, resume)) = panic_pause {
                        entered.notify_one();
                        resume.notified().await;
                        panic!("forced download attempt panic");
                    }
                    let result = task
                        .download_once(
                            &attempt.remote,
                            &attempt.relative,
                            &attempt.shared.cancel,
                            &mut attempt.publication,
                        )
                        .await;
                    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                    let completion_pause = result
                        .is_ok()
                        .then(|| task.runtime().download_completion_pause.lock().take())
                        .flatten();
                    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                    if let Some((entered, resume)) = completion_pause {
                        entered.notify_one();
                        resume.notified().await;
                    }
                    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                    let failure_pause = result
                        .is_err()
                        .then(|| task.runtime().download_failure_pause.lock().take())
                        .flatten();
                    #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                    if let Some((entered, resume)) = failure_pause {
                        entered.notify_one();
                        resume.notified().await;
                    }
                    attempt.finish(result).await;
                }));
                shared
            }
        };
        drop(publication);
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
        publication_guard: &mut Option<tokio::sync::OwnedMutexGuard<()>>,
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
            #[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
            let test_body = { DOWNLOAD_TEST_BODY.lock().take() };
            #[cfg(all(target_arch = "wasm32", feature = "test-hooks"))]
            if let Some(body) = test_body {
                sink.write(&body).await?;
            } else {
                Transfer::new(self.runtime().options.clone())?.get(&remote.url, cap, &mut sink).await?;
            }
            #[cfg(not(all(target_arch = "wasm32", feature = "test-hooks")))]
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
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            {
                let pause = self.runtime().download_move_pause.lock().clone();
                if let Some((entered, resume)) = pause {
                    entered.notify_one();
                    resume.notified().await;
                }
            }
            let publication = self.runtime().publication_lock.clone().lock_owned().await;
            #[cfg(not(target_arch = "wasm32"))]
            let (move_result, move_publication) = (store.rename(final_tmp, relative).await, publication);
            #[cfg(target_arch = "wasm32")]
            let (move_result, move_publication) = {
                // Keep the lock in the task until the OPFS move promise settles.
                let store = store.clone();
                let source = final_tmp.to_owned();
                let destination = relative.to_owned();
                let (sender, receiver) = tokio::sync::oneshot::channel();
                drop(xmtp_common::task::spawn(async move {
                    let result = store.rename(&source, &destination).await;
                    #[cfg(feature = "test-hooks")]
                    if result.is_ok() {
                        let pause = { DOWNLOAD_MOVE_PAUSE.lock().take() };
                        if let Some((entered, resume)) = pause {
                            entered.notify_one();
                            resume.notified().await;
                        }
                    }
                    let _ = sender.send((result, publication));
                }));
                receiver
                    .await
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
            };
            *publication_guard = Some(move_publication);
            match move_result {
                Ok(()) => {}
                Err(StoreMoveError::DestinationExists) => {
                    if !store.is_regular_file(relative).await? {
                        return Err(AttachmentClientError::new(Cause::LocalStorage));
                    }
                    // The file is readable at the path, so the download completes even
                    // without a record; crash recovery adds the record at the next client creation.
                    let record = self.context.db()
                        .insert_or_ignore_local_attachment(relative, now_ns(), None, None)
                        .and_then(|()| self.context.db().get_local_attachment(relative));
                    let (mime_type, filename) = match record {
                        Ok(Some(record)) => (record.mime_type, record.filename),
                        _ => (Some(meta.mime_type), meta.filename),
                    };
                    return Ok(DownloadedAttachment {
                        path: self.local_path(remote)?, mime_type, filename,
                    });
                }
                Err(error) => return Err(error.into()),
            }
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            let publish_pause = { self.runtime().download_publish_pause.lock().take() };
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            if let Some((entered, resume)) = publish_pause {
                entered.notify_one();
                resume.notified().await;
            }
            if self.context.db().insert_or_ignore_local_attachment(
                relative, now_ns(), Some(meta.mime_type.clone()), meta.filename.clone()
            )
            .is_err() {
                #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                let removal = if self.runtime().fail_next_download_cleanup.swap(false, AtomicOrdering::SeqCst) {
                    Err(AttachmentError::new(Cause::LocalStorage))
                } else {
                    store.remove_file(relative).await
                };
                #[cfg(not(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32"))))]
                let removal = store.remove_file(relative).await;
                // A file that stays is complete and verified, so the download completes;
                // crash recovery adds its record at the next client creation.
                match removal {
                    Ok(()) => return Err(AttachmentClientError::new(Cause::LocalStorage)),
                    Err(error) => tracing::warn!(%error, "could not remove downloaded file after metadata write failed"),
                }
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
}
