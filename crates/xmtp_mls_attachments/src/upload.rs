//! Attachment uploads: the lease, the attempt and the published outcome.

use super::*;

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

impl<Context: AttachmentContext> PendingAttachment<Context> {
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
        let cancel = self.shared.cancel_token();
        #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
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
                _ = cancel.cancelled() => break Err(AttachmentClientError::new(Cause::Deleted)),
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
                            #[cfg(any(test, feature = "test-hooks"))]
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
        {
            // Read the record before this attempt ends. A delete in this client
            // waits for that end before it removes the record and reports the
            // failure, so a record that is gone here was removed by a delete
            // that did not report it, for example one in another client.
            let event_lock = self
                .context
                .attachment_runtime()
                .event_lock(&self.reference().attachment_key);
            let _event_guard = event_lock.lock().await;
            match self.record() {
                Ok(Some(row)) => {
                    self.shared
                        .watch
                        .send_replace(status_from_row(&row, now_ns()));
                }
                Ok(None) => {
                    self.publish_upload_outcome(&Err(AttachmentClientError::new(Cause::Deleted)));
                }
                Err(error) => {
                    tracing::warn!(%error, "lost attachment lease status read will be retried")
                }
            }
        }
        self.end_local_attempt(attempt, None);
    }

    async fn finish_attempt(
        &self,
        token: &[u8; 16],
        result: Result<(), AttachmentClientError>,
        attempt: &Arc<PendingAttempt>,
    ) {
        let cancel = self.shared.cancel_token();
        let mut delay = Duration::from_millis(100);
        let mut retrying = false;
        let timing = *self.context.attachment_runtime().lease_timing.lock();
        let mut renew = Box::pin(xmtp_common::time::interval_stream(timing.renew));
        #[cfg(not(target_arch = "wasm32"))]
        renew.next().await;
        loop {
            if retrying && cancel.is_cancelled() {
                self.end_cancelled_attempt(attempt);
                return;
            }
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            let outcome_pause = self
                .context
                .attachment_runtime()
                .outcome_lock_pause
                .lock()
                .take();
            #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
            if let Some((entered, resume)) = outcome_pause {
                entered.notify_one();
                resume.notified().await;
            }
            let event_lock = self
                .context
                .attachment_runtime()
                .event_lock(&self.reference().attachment_key);
            let _event_guard = event_lock.lock().await;
            if cancel.is_cancelled() {
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
            #[cfg(any(test, feature = "test-hooks"))]
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
                    {
                        #[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
                        let removal = if self
                            .context
                            .attachment_runtime()
                            .fail_next_staged_removal
                            .swap(false, AtomicOrdering::SeqCst)
                        {
                            Err(AttachmentError::new(Cause::LocalStorage))
                        } else {
                            store.remove_file(&path).await
                        };
                        #[cfg(not(all(
                            any(test, feature = "test-hooks"),
                            not(target_arch = "wasm32")
                        )))]
                        let removal = store.remove_file(&path).await;
                        if let Err(error) = removal {
                            tracing::warn!(%error, "staged ciphertext cleanup will be retried by reconciliation");
                        }
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
                    _ = cancel.cancelled() => {
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
                                #[cfg(any(test, feature = "test-hooks"))]
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

    pub(super) async fn upload_once(&self, token: &[u8; 16]) -> Result<(), AttachmentClientError> {
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
        let expected_length = self
            .remote
            .content_length
            .map(u64::from)
            .ok_or_else(|| AttachmentClientError::new(Cause::StagedUnusable))?;
        let staged = store.open_read(&path).await.map_err(storage_error)?;
        let (digest, length) = staged
            .sha256(expected_length)
            .await
            .map_err(storage_error)?;
        if hex::encode(digest) != self.remote.content_digest || length != expected_length {
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
