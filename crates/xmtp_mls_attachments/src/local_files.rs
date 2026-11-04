//! Local attachment files: their paths, listing and deletion.

use super::*;

pub(super) fn attachment_reference(remote: &RemoteAttachment, key: &str) -> AttachmentRef {
    AttachmentRef {
        attachment_key: key.to_owned(),
        url: remote.url.clone(),
        content_digest: remote.content_digest.clone(),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAttachment {
    pub path: String,
    pub created_at_ns: i64,
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

impl<Context: AttachmentContext> Attachments<Context> {
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
        let rows = self
            .context
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
        let (upload, upload_attempt, downloads, _deleting) = {
            let _guard = lock.lock().await;
            // Marks the whole key directory, so no download into it can start.
            let deleting = DeleteInProgress::new(&self.runtime().deleting, key.clone());
            let pending_matches = self
                .context
                .db()
                .get_pending_attachment(&remote.content_digest)
                .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
                .is_some_and(|row| pending_row_matches_key(&row, &key));
            let upload = pending_matches
                .then(|| {
                    self.runtime()
                        .pending
                        .lock()
                        .get(&remote.content_digest)
                        .and_then(Weak::upgrade)
                })
                .flatten();
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
                shared.cancel.lock().cancel();
            }
            for shared in &downloads {
                shared.cancel.cancel();
            }
            (upload, upload_attempt, downloads, deleting)
        };
        #[cfg(any(test, feature = "test-hooks"))]
        {
            let pause = self.runtime().delete_pause.lock().clone();
            if let Some((entered, resume)) = pause {
                entered.notify_one();
                resume.notified().await;
            }
        }
        let cancelled_upload = upload_attempt.is_some();
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
        let mut restore_cancel = RestorePendingCancel(upload.clone());
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
        let pending_row = self
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?;
        let key_exists = store.prepare_remove_dir(&key).await?;
        let deleted_pending =
            if let Some(row) = pending_row.filter(|row| pending_row_matches_key(row, &key)) {
                self.context
                    .db()
                    .delete_pending_attachment_if_remote_matches(
                        &remote.content_digest,
                        &row.remote_attachment,
                    )
                    .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))?
                    != 0
            } else {
                false
            };
        if deleted_pending {
            restore_cancel.0 = None;
            if cancelled_upload {
                let deleted = AttachmentClientError::new(Cause::Deleted);
                if let Some(shared) = &upload {
                    shared
                        .watch
                        .send_replace(PendingAttachmentStatus::Failed(deleted));
                }
                let reference = attachment_reference(remote, &key);
                self.context.events().emit(
                    Some(ClientEvent::AttachmentUploadFailed(AttachmentFailed {
                        attachment_key: key.clone(),
                        url: reference.url,
                        content_digest: reference.content_digest,
                        cause: Cause::Deleted.as_str().to_owned(),
                    })),
                    None,
                );
            }
        }
        if key_exists {
            if let Err(error) = store.remove_dir_all(&key).await {
                // A removal that fails part way has still deleted local files.
                if error.removed_any {
                    emit_deleted();
                }
                return Err(error.error.into());
            }
            emit_deleted();
        }
        if deleted_pending && store.exists(&staged).await? {
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
        if deleted_pending {
            self.runtime().pending.lock().remove(&remote.content_digest);
        }
        Ok(())
    }
}
