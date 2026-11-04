//! Upload lifecycle tests: outcome writes.

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
            .attachment_runtime()
            .outcome_write_errors()
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
            .attachment_runtime()
            .outcome_write_errors()
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
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::OutcomeLock,
        Some((entered_outcome.clone(), resume_outcome.clone())),
    );
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_outcome.notified()).await?;

    let event_lock = alix
        .client
        .context
        .attachment_runtime()
        .event_lock_for_test(&key);
    let guard = event_lock.lock().await;
    let entered_delete = Arc::new(tokio::sync::Notify::new());
    let resume_delete = Arc::new(tokio::sync::Notify::new());
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::Delete,
        Some((entered_delete.clone(), resume_delete.clone())),
    );
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    client.context.attachment_runtime().set_lease_timing(
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
        while client.context.attachment_runtime().outcome_write_errors() == 0 {
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
    alix.client.context.attachment_runtime().set_lease_timing(
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
            .attachment_runtime()
            .outcome_write_errors(),
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
    let other_client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
            .attachment_runtime()
            .outcome_write_errors(),
        1
    );
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
        while client.context.attachment_runtime().outcome_write_errors() == 0 {
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
