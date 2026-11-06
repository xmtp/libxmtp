//! Upload lifecycle tests: deletion.

use super::*;

// verifies: ATCH-047, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn delete_finishes_after_unrecorded_upload_outcome() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered_put, release_put) = paused_put(200).await;
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
    client.context.attachment_runtime().set_pause(
        PausePoint::Delete,
        Some((entered_delete.clone(), resume_delete.clone())),
    );
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
        client.context.attachment_runtime().outcome_write_errors(),
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
    let uploader = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
        while pending.probe().attempt().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        pending.probe().watched_status(),
        PendingAttachmentStatus::Failed(AttachmentClientError::of_cause(Cause::Deleted))
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
    client.context.attachment_runtime().set_pause(
        PausePoint::Delete,
        Some((entered_delete.clone(), resume_delete.clone())),
    );
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
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::AttemptPanic,
        Some((entered_attempt.clone(), resume_attempt.clone())),
    );
    let upload = tokio::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered_attempt.notified()).await?;

    let entered_delete = Arc::new(tokio::sync::Notify::new());
    let resume_delete = Arc::new(tokio::sync::Notify::new());
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::Delete,
        Some((entered_delete.clone(), resume_delete.clone())),
    );
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
