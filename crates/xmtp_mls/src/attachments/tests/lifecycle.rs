use super::*;

// verifies: ATCH-025, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn locked_outcome_write_is_retried() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER lock_attachment_outcome BEFORE UPDATE OF status ON pending_attachments \
                 WHEN NEW.status = 'complete' \
                 BEGIN SELECT RAISE(ABORT, 'database table is locked'); END",
        )
        .execute(conn)
    })?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while alix
            .client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst)
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER lock_attachment_outcome").execute(conn)
    })?;
    tokio::time::timeout(Duration::from_secs(5), upload).await???;
    let row = alix
        .client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap();
    assert_eq!(row.status, "complete");
}

// verifies: ATCH-047, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn delete_stops_retrying_unrecorded_upload_outcome() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDeleted,
        ]))?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER lock_attachment_outcome_before_delete BEFORE UPDATE OF status ON pending_attachments \
             WHEN NEW.status = 'complete' \
             BEGIN SELECT RAISE(ABORT, 'database table is locked'); END",
        )
        .execute(conn)
    })?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while alix
            .client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst)
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    tokio::time::timeout(
        Duration::from_secs(2),
        alix.client.attachments().delete_local(&remote),
    )
    .await??;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), upload)
            .await??
            .unwrap_err()
            .cause,
        Cause::Deleted
    );
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    assert!(!dir.path().join(attachment_key(&remote)?).exists());
    assert!(
        !dir.path()
            .join(staged_path(&remote.content_digest)?)
            .exists()
    );
    let kinds: Vec<_> = events
        .drain()
        .into_iter()
        .map(|event| event.client.unwrap().kind())
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDeleted
        ]
    );
}

// verifies: ATCH-047, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_wins_before_outcome_write_under_event_lock() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let key = attachment_key(&remote)?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDeleted,
        ]))?;
    let entered_outcome = Arc::new(tokio::sync::Notify::new());
    let resume_outcome = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.outcome_lock_pause.lock() =
        Some((entered_outcome.clone(), resume_outcome.clone()));
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_outcome.notified()).await?;

    let event_lock = alix.client.context.attachments.event_lock(&key);
    let guard = event_lock.lock().await;
    let entered_delete = Arc::new(tokio::sync::Notify::new());
    let resume_delete = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.delete_pause.lock() =
        Some((entered_delete.clone(), resume_delete.clone()));
    let client = alix.client.clone();
    let deleting_remote = remote.clone();
    let deletion = xmtp_common::task::spawn(async move {
        client.attachments().delete_local(&deleting_remote).await
    });
    tokio::task::yield_now().await;
    drop(guard);
    tokio::time::timeout(Duration::from_secs(5), entered_delete.notified()).await?;
    resume_outcome.notify_one();
    let upload_result = tokio::time::timeout(Duration::from_secs(2), upload).await??;
    resume_delete.notify_one();
    tokio::time::timeout(Duration::from_secs(2), deletion).await???;
    assert_eq!(upload_result.unwrap_err().cause, Cause::Deleted);
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    assert!(!dir.path().join(&key).exists());
    assert!(
        !dir.path()
            .join(staged_path(&remote.content_digest)?)
            .exists()
    );
    let kinds: Vec<_> = events
        .drain()
        .into_iter()
        .map(|event| event.client.unwrap().kind())
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDeleted
        ]
    );
}

// verifies: ATCH-025, ATCH-074, EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn outcome_retry_renews_lease_without_another_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    *client.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_millis(300),
        Duration::from_millis(100),
        Duration::from_millis(10),
    );
    let events = client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentUploadStarted]))?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER lock_complete_outcome BEFORE UPDATE OF status ON pending_attachments \
                 WHEN NEW.status = 'complete' \
                 BEGIN SELECT RAISE(ABORT, 'database table is locked'); END",
        )
        .execute(conn)
    })?;
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    release.send(())?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst)
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER lock_complete_outcome").execute(conn)
    })?;
    tokio::time::timeout(Duration::from_secs(5), upload).await???;
    let row = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap();
    assert_eq!(row.status, "complete");
    let started = events
        .drain()
        .into_iter()
        .filter(|event| matches!(event.client, Some(ClientEvent::AttachmentUploadStarted(_))))
        .count();
    assert_eq!(started, 1);
}

// verifies: ATCH-025, ATCH-074, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn deterministic_outcome_error_fails_with_local_storage() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    *alix.client.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_secs(2),
        Duration::from_millis(100),
        Duration::from_millis(10),
    );
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentUploadFailed,
        ]))?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TABLE attachment_outcome_check (value INTEGER CHECK (value = 1))",
        )
        .execute(conn)
    })?;
    alix.client.context.db().raw_query(|conn| {
            xmtp_db::diesel::sql_query(
                "CREATE TRIGGER reject_attachment_outcome BEFORE UPDATE OF status ON pending_attachments \
                 WHEN NEW.status = 'complete' \
                 BEGIN INSERT INTO attachment_outcome_check (value) VALUES (2); END",
            )
            .execute(conn)
        })?;
    let error = tokio::time::timeout(Duration::from_secs(5), pending.upload())
        .await?
        .unwrap_err();
    assert_eq!(error.cause, Cause::LocalStorage);
    let row = alix
        .client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap();
    assert_eq!(row.status, "uploading");
    assert_eq!(pending.status(), PendingAttachmentStatus::Uploading);
    assert_eq!(
        alix.client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst),
        1
    );
    assert_eq!(
        row.effective_status(row.lease_expires_at_ns.unwrap().saturating_add(1)),
        "waiting"
    );
    let first_events = events.drain();
    assert_eq!(first_events.len(), 1);
    assert!(matches!(
        first_events[0].client,
        Some(ClientEvent::AttachmentUploadStarted(_))
    ));
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER reject_attachment_outcome").execute(conn)
    })?;
    let mut premature = Box::pin(pending.upload());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut premature)
            .await
            .is_err()
    );
    drop(premature);
    tokio::time::timeout(Duration::from_secs(3), async {
        while pending.status() != PendingAttachmentStatus::Waiting {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let (url, entered, release) = paused_put(200).await;
    let other_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let other_pending = other_client.attachments().pending(&remote).await?;
    let other_upload = xmtp_common::task::spawn(async move { other_pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    assert_eq!(pending.status(), PendingAttachmentStatus::Uploading);
    let mut joined = Box::pin(pending.upload());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut joined)
            .await
            .is_err()
    );
    release.send(()).expect("release second PUT response");
    tokio::time::timeout(Duration::from_secs(5), other_upload).await???;
    tokio::time::timeout(Duration::from_secs(5), joined).await??;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
    assert!(events.drain().is_empty());
}

// verifies: ATCH-025, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn permanent_non_constraint_outcome_error_stops_retrying() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_permanent_attachment_outcome BEFORE UPDATE OF status ON pending_attachments \
             WHEN NEW.status = 'complete' \
             BEGIN SELECT RAISE(ABORT, 'permanent outcome error'); END",
        )
        .execute(conn)
    })?;
    let result = tokio::time::timeout(Duration::from_secs(2), pending.upload()).await?;
    assert_eq!(result.unwrap_err().cause, Cause::LocalStorage);
    assert_eq!(
        alix.client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst),
        1
    );
}

// verifies: ATCH-047, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn delete_finishes_after_unrecorded_upload_outcome() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered_put, release_put) = paused_put(200).await;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadFailed,
        EventKind::AttachmentDeleted,
    ]))?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TABLE delete_outcome_check (value INTEGER CHECK (value = 1))",
        )
        .execute(conn)
    })?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_deleted_outcome BEFORE UPDATE OF status ON pending_attachments \
             WHEN NEW.status = 'failed' \
             BEGIN INSERT INTO delete_outcome_check (value) VALUES (2); END",
        )
        .execute(conn)
    })?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_put).await??;
    let entered_delete = Arc::new(tokio::sync::Notify::new());
    let resume_delete = Arc::new(tokio::sync::Notify::new());
    *client.context.attachments.delete_pause.lock() =
        Some((entered_delete.clone(), resume_delete.clone()));
    let deleting_client = client.clone();
    let deleting_remote = remote.clone();
    let delete = xmtp_common::task::spawn(async move {
        deleting_client
            .attachments()
            .delete_local(&deleting_remote)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered_delete.notified()).await?;
    resume_delete.notify_one();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), upload)
            .await??
            .unwrap_err()
            .cause,
        Cause::Deleted
    );
    tokio::time::timeout(Duration::from_secs(2), delete).await???;
    assert_eq!(
        client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst),
        0
    );
    let _ = release_put.send(());
    assert!(
        client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    assert!(!dir.path().join(attachment_key(&remote)?).exists());
    assert!(
        !dir.path()
            .join(staged_path(&remote.content_digest)?)
            .exists()
    );
    let kinds: Vec<_> = events
        .drain()
        .into_iter()
        .map(|event| event.client.unwrap().kind())
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadFailed,
            EventKind::AttachmentDeleted
        ]
    );
}

// verifies: ATCH-047, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn delete_from_other_client_fails_upload_once() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered_put, release_put) = paused_put(200).await;
    // The uploader shares the database but has its own runtime and event bus.
    let uploader = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let events = uploader.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadCompleted,
        EventKind::AttachmentUploadFailed,
    ]))?;
    let pending = uploader.attachments().pending(&remote).await?;
    let uploading = pending.clone();
    let upload = xmtp_common::task::spawn(async move { uploading.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_put).await??;
    tokio::time::timeout(
        Duration::from_secs(5),
        alix.client.attachments().delete_local(&remote),
    )
    .await??;
    release_put.send(()).expect("release PUT response");
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), upload)
            .await??
            .unwrap_err()
            .cause,
        Cause::Deleted
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while pending.shared.attempt.lock().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        *pending.shared.watch.borrow(),
        PendingAttachmentStatus::Failed(AttachmentClientError::new(Cause::Deleted))
    );
    let emitted = events.drain();
    assert_eq!(emitted.len(), 2);
    assert!(matches!(
        emitted[0].client,
        Some(ClientEvent::AttachmentUploadStarted(_))
    ));
    assert!(matches!(
        &emitted[1].client,
        Some(ClientEvent::AttachmentUploadFailed(failed))
            if failed.cause == Cause::Deleted.as_str()
                && failed.content_digest == remote.content_digest
    ));
}

// verifies: ATCH-047
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_delete_caller_does_not_leave_a_cancelled_pending_row() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered_put, release_put) = paused_put(200).await;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let upload = tokio::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_put).await??;

    let entered_delete = Arc::new(tokio::sync::Notify::new());
    let resume_delete = Arc::new(tokio::sync::Notify::new());
    *client.context.attachments.delete_pause.lock() =
        Some((entered_delete.clone(), resume_delete.clone()));
    let deleting_client = client.clone();
    let deleting_remote = remote.clone();
    let deletion = tokio::spawn(async move {
        deleting_client
            .attachments()
            .delete_local(&deleting_remote)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered_delete.notified()).await?;
    deletion.abort();
    resume_delete.notify_one();
    let _ = release_put.send(());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), upload)
            .await??
            .unwrap_err()
            .cause,
        Cause::Deleted
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_some()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, xmtp_db::StorageError>(())
    })
    .await??;
    assert!(!dir.path().join(attachment_key(&remote)?).exists());
    assert!(
        !dir.path()
            .join(staged_path(&remote.content_digest)?)
            .exists()
    );
}

// verifies: ATCH-047
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn delete_finishes_after_upload_task_panics() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let entered_attempt = Arc::new(tokio::sync::Notify::new());
    let resume_attempt = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.attempt_panic_pause.lock() =
        Some((entered_attempt.clone(), resume_attempt.clone()));
    let upload = tokio::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_attempt.notified()).await?;

    let entered_delete = Arc::new(tokio::sync::Notify::new());
    let resume_delete = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.delete_pause.lock() =
        Some((entered_delete.clone(), resume_delete.clone()));
    let client = alix.client.clone();
    let deleting_remote = remote.clone();
    let deletion =
        tokio::spawn(async move { client.attachments().delete_local(&deleting_remote).await });
    tokio::time::timeout(Duration::from_secs(5), entered_delete.notified()).await?;
    resume_delete.notify_one();
    resume_attempt.notify_one();
    tokio::time::timeout(Duration::from_secs(2), deletion).await???;
    upload.abort();
    let _ = upload.await;
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    assert!(!dir.path().join(attachment_key(&remote)?).exists());
    assert!(
        !dir.path()
            .join(staged_path(&remote.content_digest)?)
            .exists()
    );
}

// verifies: ATCH-074
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn panicked_attempt_can_be_claimed_after_lease_expiry() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    *alix.client.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_millis(300),
        Duration::from_millis(100),
        Duration::from_millis(10),
    );
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let resumed = alix.client.attachments().pending(&remote).await?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentUploadStarted]))?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.attempt_panic_pause.lock() =
        Some((entered.clone(), release.clone()));
    let first = tokio::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    let attempt = resumed
        .shared
        .attempt
        .lock()
        .clone()
        .expect("running attempt");
    first.abort();
    let _ = first.await;
    *alix.client.context.attachments.attempt_panic_pause.lock() = None;
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), attempt.done.cancelled()).await?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while resumed.status() != PendingAttachmentStatus::Waiting {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    tokio::time::timeout(Duration::from_secs(5), resumed.upload()).await??;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Complete);
    assert_eq!(events.drain().len(), 2);
}

// verifies: ATCH-029, ATCH-066
#[xmtp_common::test(unwrap_try = true)]
async fn failed_status_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let mut rejected = xmtp_api_backend::MockBackendClient::new();
    rejected.expect_create_upload().times(1).returning(|_| {
        Err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("permanent")),
        ))
    });
    let first = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(rejected))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        first
            .attachments()
            .pending(&remote)
            .await?
            .upload()
            .await
            .unwrap_err()
            .cause,
        Cause::BackendRejected
    );
    drop(first);
    drop(created);
    let mut no_request = xmtp_api_backend::MockBackendClient::new();
    no_request.expect_create_upload().times(0);
    let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(no_request))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = restarted.attachments().pending(&remote).await?;
    assert!(
        matches!(pending.status(), PendingAttachmentStatus::Failed(error) if error.cause == Cause::BackendRejected)
    );
    assert_eq!(
        pending.upload().await.unwrap_err().cause,
        Cause::BackendRejected
    );
}

// verifies: ATCH-037, ATCH-038, ATCH-067
#[xmtp_common::test(unwrap_try = true)]
async fn complete_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let staged = dir.path().join(staged_path(&remote.content_digest)?);
    pending.upload().await?;
    assert!(!staged.exists());
    let mut no_request = xmtp_api_backend::MockBackendClient::new();
    no_request.expect_create_upload().times(0);
    let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(no_request))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let resumed = restarted.attachments().pending(&remote).await?;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Complete);
    resumed.upload().await?;
    assert!(restarted.attachments().list_pending().await?.is_empty());
    assert_eq!(
        restarted
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
}

// verifies: ATCH-029
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_record_is_not_touched() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let token = [9u8; 16];
    let db = alix.client.context.db();
    db.claim_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        LEASE_DURATION.as_nanos() as i64,
    )?;
    db.finish_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        PendingAttachmentOutcome::failed("backend_rejected", None, Some(false)),
    )?;
    let before = db.get_pending_attachment(&remote.content_digest)?.unwrap();
    let mut no_request = xmtp_api_backend::MockBackendClient::new();
    no_request.expect_create_upload().times(0);
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(no_request))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadFailed,
    ]))?;
    client
        .context
        .server_configuration
        .block_connection(BlockedConnection::BackendMismatch {
            stored: "one".into(),
            received: "two".into(),
        });
    let pending = client.attachments().pending(&remote).await?;
    assert_eq!(
        pending.upload().await.unwrap_err().cause,
        Cause::BackendRejected
    );
    assert_eq!(
        db.get_pending_attachment(&remote.content_digest)?.unwrap(),
        before
    );
    assert!(events.drain().is_empty());
}

// verifies: ATCH-061
#[xmtp_common::test(unwrap_try = true)]
async fn credential_failure_kind_persists() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let token = [8u8; 16];
    let db = alix.client.context.db();
    db.claim_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        LEASE_DURATION.as_nanos() as i64,
    )?;
    db.finish_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        PendingAttachmentOutcome::failed("credential", Some("callback_failed"), Some(true)),
    )?;
    let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let status = restarted.attachments().pending(&remote).await?.status();
    assert!(matches!(
        status,
        PendingAttachmentStatus::Failed(AttachmentClientError {
            cause: Cause::Credential,
            credential_kind: Some(CredentialFailureKind::CallbackFailed),
            retryable: true,
            ..
        })
    ));
}

struct FailingCredential(Arc<AtomicUsize>);

#[xmtp_common::async_trait]
impl xmtp_api_backend::AuthCallback for FailingCredential {
    async fn on_auth_required(
        &self,
    ) -> Result<xmtp_api_backend::Credential, xmtp_common::BoxDynError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(std::io::Error::other("attachment credential callback failed").into())
    }
}

// verifies: ATCH-061, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn real_credential_failure_kind_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let api = xmtp_api_backend::MessageBackendBuilder::new()
        .host(xmtp_configuration::backend_test_url())
        .maybe_auth_callback(Some(Arc::new(FailingCredential(calls.clone()))))
        .build()?;
    let first = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(api)
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let first_error = first
        .attachments()
        .pending(&remote)
        .await?
        .upload()
        .await
        .unwrap_err();
    assert!(calls.load(Ordering::SeqCst) > 0);
    assert_eq!(first_error.cause, Cause::Credential);
    assert_eq!(
        first_error.credential_kind,
        Some(CredentialFailureKind::CallbackFailed)
    );
    assert!(first_error.retryable);
    drop(first);
    let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        restarted.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Failed(first_error)
    );
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_stops_before_create_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_create_upload().times(0);
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(api))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let token = [11u8; 16];
    *pending.shared.lease.lock() = Some((token, now_ns() - 1));
    assert_eq!(
        pending.upload_once(&token).await.unwrap_err().cause,
        Cause::Network
    );
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_stops_before_put() {
    use xmtp_proto::backend_v1::CreateUploadResponse;
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, puts) = serve_body(Vec::new()).await;
    let shared = Arc::new(Mutex::new(None::<Arc<PendingShared>>));
    let expire = shared.clone();
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_create_upload().times(1).returning(move |_| {
        let current = expire.lock();
        let pending = current.as_ref().expect("pending attachment shared state");
        let (token, _) = (*pending.lease.lock()).expect("local lease");
        *pending.lease.lock() = Some((token, now_ns() - 1));
        Ok(CreateUploadResponse {
            method: "PUT".into(),
            url: url.clone(),
            headers: vec![],
            expires_in_seconds: 3600,
        })
    });
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(api))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    *shared.lock() = Some(pending.shared.clone());
    let token = [12u8; 16];
    *pending.shared.lease.lock() = Some((token, now_ns() + 10_000_000_000));
    assert_eq!(
        pending.upload_once(&token).await.unwrap_err().cause,
        Cause::Network
    );
    assert_eq!(puts.load(Ordering::SeqCst), 0);
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_reads_waiting() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = &pending.remote_attachment().content_digest;
    let token = [7u8; 16];
    alix.client.context.db().claim_pending_attachment(
        digest,
        &token,
        now_ns() - 200_000_000_000,
        LEASE_DURATION.as_nanos() as i64,
    )?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
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

// verifies: ATCH-034, ATCH-035, EVENT-057
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

// verifies: ATCH-025, ATCH-066
#[xmtp_common::test(unwrap_try = true)]
async fn outcome_written_after_reconnect() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers, persistent_db);
    let original = alix.client.attachments().create(bytes()).await?;
    let remote = original.remote_attachment().clone();
    let staged = dir.path().join(staged_path(&remote.content_digest)?);
    let (url, entered, release) = paused_put(200).await;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    client.release_db_connection()?;
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client
            .context
            .attachments
            .outcome_write_errors
            .load(AtomicOrdering::SeqCst)
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    client.reconnect_db()?;
    tokio::time::timeout(Duration::from_secs(10), upload).await???;
    assert_eq!(
        client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
    assert!(!staged.exists());
}

// verifies: ATCH-035, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn second_client_joins_running_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let first = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let second = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(
            "http://127.0.0.1:1/unused".into(),
            0,
        )))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    *second.context.attachments.lease_timing.lock() =
        LeaseTiming::for_test(LEASE_DURATION, LEASE_RENEW, Duration::from_millis(20));
    let a = first.attachments().pending(&remote).await?;
    let b = second.attachments().pending(&remote).await?;
    let mut observed = b.watch_status();
    let first_upload = xmtp_common::task::spawn(async move { a.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    tokio::time::timeout(Duration::from_secs(2), async {
        while *observed.borrow_and_update() != PendingAttachmentStatus::Uploading {
            observed.changed().await.unwrap();
        }
    })
    .await?;
    let second_upload = xmtp_common::task::spawn(async move { b.upload().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!second_upload.is_finished());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), first_upload).await???;
    tokio::time::timeout(Duration::from_secs(10), second_upload).await???;
    tokio::time::timeout(Duration::from_secs(2), async {
        while *observed.borrow_and_update() != PendingAttachmentStatus::Complete {
            observed.changed().await.unwrap();
        }
    })
    .await?;
    assert_eq!(
        second.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Complete
    );
}

// verifies: ATCH-074, ATCH-035
#[xmtp_common::test(unwrap_try = true)]
async fn stale_holder_yields_to_new_claim() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let staged = dir.path().join(staged_path(&remote.content_digest)?);
    let (a_url, a_entered, a_release) = paused_put(500).await;
    let (b_url, b_entered, b_release) = paused_put(200).await;
    let a_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(a_url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let b_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(b_url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    *a_client.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_millis(100),
        Duration::from_secs(1),
        Duration::from_millis(10),
    );
    let a = a_client.attachments().pending(&remote).await?;
    let events = a_client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadFailed,
    ]))?;
    let a_upload = xmtp_common::task::spawn(async move { a.upload().await });
    tokio::time::timeout(Duration::from_secs(5), a_entered).await??;
    let old_token = alix
        .client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_id
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let b = b_client.attachments().pending(&remote).await?;
    let b_upload = xmtp_common::task::spawn(async move { b.upload().await });
    tokio::time::timeout(Duration::from_secs(5), b_entered).await??;
    // The old token cannot write while the replacement still uploads.
    assert_eq!(
        alix.client.context.db().finish_pending_attachment(
            &remote.content_digest,
            &old_token,
            now_ns(),
            PendingAttachmentOutcome::failed("network", None, None)
        )?,
        0
    );
    b_release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), b_upload).await???;
    a_release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), a_upload).await???;
    assert_eq!(
        a_client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
    assert_eq!(
        a_client.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Complete
    );
    assert!(!staged.exists());
    let emitted = events.drain();
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        &emitted[0].client,
        Some(ClientEvent::AttachmentUploadStarted(_))
    ));
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn lease_extended_while_running() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    *client.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_millis(300),
        Duration::from_millis(30),
        Duration::from_millis(10),
    );
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    let initial = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_expires_at_ns
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let extended = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_expires_at_ns
        .unwrap();
    assert!(extended > initial);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), upload).await???;
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn extension_storage_error_is_retried() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    *client.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_millis(500),
        Duration::from_millis(30),
        Duration::from_millis(10),
    );
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER lock_attachment_extension BEFORE UPDATE OF lease_expires_at_ns \
                 ON pending_attachments WHEN OLD.status = 'uploading' AND NEW.status = 'uploading' \
                 BEGIN SELECT RAISE(ABORT, 'database table is locked'); END",
        )
        .execute(conn)
    })?;
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    let initial = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_expires_at_ns
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client
            .context
            .attachments
            .lease_extension_errors
            .load(AtomicOrdering::SeqCst)
            == 0
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER lock_attachment_extension").execute(conn)
    })?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)
            .unwrap()
            .unwrap()
            .lease_expires_at_ns
            .unwrap()
            <= initial
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), upload).await???;
    assert_eq!(
        client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
}

// verifies: EVENT-057
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

// verifies: EVENT-001, EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn attachment_event_kinds() {
    fn expected_reference(remote: &RemoteAttachment) -> AttachmentRef {
        AttachmentRef {
            attachment_key: attachment_key(remote).expect("attachment key"),
            url: remote.url.clone(),
            content_digest: remote.content_digest.clone(),
        }
    }

    fn expected_failure(reference: &AttachmentRef, cause: Cause) -> AttachmentFailed {
        AttachmentFailed {
            attachment_key: reference.attachment_key.clone(),
            url: reference.url.clone(),
            content_digest: reference.content_digest.clone(),
            cause: cause.as_str().to_owned(),
        }
    }

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
    let failed_remote = failed.remote_attachment().clone();
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
    let mut forged = remote.clone();
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
    let emitted: Vec<_> = events
        .drain()
        .into_iter()
        .map(|entry| entry.client.expect("attachment event"))
        .collect();
    let failed_ref = expected_reference(&failed_remote);
    let remote_ref = expected_reference(&remote);
    let forged_ref = expected_reference(&forged);
    assert_eq!(
        emitted,
        vec![
            ClientEvent::AttachmentUploadStarted(failed_ref.clone()),
            ClientEvent::AttachmentUploadFailed(expected_failure(
                &failed_ref,
                Cause::StagedUnusable
            )),
            ClientEvent::AttachmentDeleted(failed_ref),
            ClientEvent::AttachmentUploadStarted(remote_ref.clone()),
            ClientEvent::AttachmentUploadCompleted(remote_ref.clone()),
            ClientEvent::AttachmentDeleted(remote_ref.clone()),
            ClientEvent::AttachmentDownloadStarted(remote_ref.clone()),
            ClientEvent::AttachmentDownloadCompleted(remote_ref.clone()),
            ClientEvent::AttachmentDeleted(remote_ref),
            ClientEvent::AttachmentDownloadStarted(forged_ref.clone()),
            ClientEvent::AttachmentDownloadFailed(expected_failure(
                &forged_ref,
                Cause::DigestMismatch
            )),
        ]
    );
}
