//! Pending attachments: their stored status, the shared attempt state, and the reads
//! and watches of an attachment waiting to upload.

use super::*;

pub(super) fn pending_row_matches_key(row: &StoredPendingAttachment, key: &str) -> bool {
    RemoteAttachment::decode(row.remote_attachment.as_slice())
        .ok()
        .and_then(|remote| attachment_key(&remote).ok())
        .is_some_and(|stored_key| stored_key == key)
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
pub(super) fn status_from_row(row: &StoredPendingAttachment, now: i64) -> PendingAttachmentStatus {
    match row.effective_status(now) {
        "waiting" => PendingAttachmentStatus::Waiting,
        "uploading" => PendingAttachmentStatus::Uploading,
        "complete" => PendingAttachmentStatus::Complete,
        "failed" => PendingAttachmentStatus::Failed(failure_from_row(row)),
        _ => PendingAttachmentStatus::Failed(AttachmentClientError::new(Cause::LocalStorage)),
    }
}
pub(super) struct PendingShared {
    pub(super) state: AsyncMutex<()>,
    pub(super) watch: watch::Sender<PendingAttachmentStatus>,
    pub(super) lease: Mutex<Option<([u8; 16], i64)>>,
    pub(super) attempt: Mutex<Option<Arc<PendingAttempt>>>,
    pub(super) cancel: Mutex<CancellationToken>,
}
pub(super) struct RestorePendingCancel(pub(super) Option<Arc<PendingShared>>);
impl Drop for RestorePendingCancel {
    fn drop(&mut self) {
        if let Some(shared) = self.0.take() {
            *shared.cancel.lock() = CancellationToken::new();
        }
    }
}
pub(super) struct PendingAttempt {
    pub(super) outcome: watch::Sender<Option<Result<(), AttachmentClientError>>>,
    pub(super) done: CancellationToken,
}
pub(super) struct PendingAttemptDoneGuard {
    pub(super) shared: Arc<PendingShared>,
    pub(super) attempt: Arc<PendingAttempt>,
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
    pub(super) fn new() -> Self {
        let (outcome, _) = watch::channel(None);
        Self {
            outcome,
            done: CancellationToken::new(),
        }
    }
}
impl PendingShared {
    pub(super) fn new() -> Self {
        let (watch, _) = watch::channel(PendingAttachmentStatus::Waiting);
        Self {
            state: AsyncMutex::new(()),
            watch,
            lease: Mutex::new(None),
            attempt: Mutex::new(None),
            cancel: Mutex::new(CancellationToken::new()),
        }
    }

    pub(super) fn cancel_token(&self) -> CancellationToken {
        self.cancel.lock().clone()
    }
}

impl<Context: AttachmentContext> Attachments<Context> {
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
    pub(super) context: Context,
    pub(super) remote: RemoteAttachment,
    pub(super) shared: Arc<PendingShared>,
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

impl<Context: AttachmentContext> PendingAttachment<Context> {
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
                if self.shared.cancel_token().is_cancelled() {
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

    pub(super) fn reference(&self) -> AttachmentRef {
        AttachmentRef {
            attachment_key: xmtp_attachments::attachment_key(&self.remote).unwrap_or_default(),
            url: self.remote.url.clone(),
            content_digest: self.remote.content_digest.clone(),
        }
    }

    pub(super) fn record(&self) -> Result<Option<StoredPendingAttachment>, AttachmentClientError> {
        self.context
            .db()
            .get_pending_attachment(&self.remote.content_digest)
            .map_err(|_| AttachmentClientError::new(Cause::LocalStorage))
    }

    pub(super) async fn wait_for_record_change(
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
}
