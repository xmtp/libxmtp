//! Attachment storage tests: deletion.

use super::*;

// verifies: ATCH-047
#[xmtp_common::test(unwrap_try = true)]
async fn delete_ends_other_clients_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let uploader = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    uploader.context.attachment_runtime().set_lease_timing(
        Duration::from_millis(300),
        Duration::from_millis(30),
        Duration::from_millis(10),
    );
    let deleting_client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = uploader.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    deleting_client.attachments().delete_local(&remote).await?;
    let _ = release.send(());
    let error = tokio::time::timeout(Duration::from_secs(10), upload)
        .await??
        .unwrap_err();
    assert_eq!(error.cause, Cause::Deleted);
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
}

// verifies: ATCH-047
#[xmtp_common::test(unwrap_try = true)]
async fn delete_same_digest_with_another_key_keeps_pending_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let original = created.remote_attachment().clone();
    let mut other = original.clone();
    other.secret[0] ^= 1;
    let other_key = attachment_key(&other)?;
    assert_ne!(attachment_key(&original)?, other_key);
    let other_relative = plaintext_rel_path(&other)?;
    std::fs::create_dir(dir.path().join(&other_key))?;
    std::fs::write(dir.path().join(&other_relative), b"other local file")?;
    alix.client.context.db().insert_or_ignore_local_attachment(
        &other_relative,
        now_ns(),
        None,
        None,
    )?;
    let staged = dir.path().join(staged_path(&original.content_digest)?);
    let (url, entered, release) = paused_put(200).await;
    let uploader = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = uploader.attachments().pending(&original).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    alix.client.attachments().delete_local(&other).await?;
    assert!(!dir.path().join(&other_relative).exists());
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&other_relative)?
            .is_none()
    );
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&original.content_digest)?
            .is_some()
    );
    assert!(staged.exists());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), upload).await???;
    assert_eq!(created.status(), PendingAttachmentStatus::Complete);
}

// verifies: ATCH-047, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn delete_with_watcher_ends_other_clients_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let uploader = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let deleting_client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    deleting_client
        .context
        .attachment_runtime()
        .set_lease_timing(LEASE_DURATION, LEASE_RENEW, Duration::from_millis(20));
    let watcher = deleting_client.attachments().pending(&remote).await?;
    let mut status = watcher.watch_status();
    let pending = uploader.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    tokio::time::timeout(Duration::from_secs(2), async {
        while *status.borrow_and_update() != PendingAttachmentStatus::Uploading {
            status.changed().await.unwrap();
        }
    })
    .await?;
    tokio::time::timeout(
        Duration::from_secs(1),
        deleting_client.attachments().delete_local(&remote),
    )
    .await??;
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    release.send(()).unwrap();
    let error = tokio::time::timeout(Duration::from_secs(10), upload)
        .await??
        .unwrap_err();
    assert_eq!(error.cause, Cause::Deleted);
}

// verifies: ATCH-047, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn delete_removes_every_record_in_key_directory() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let created = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"shared".to_vec(),
            filename: Some("one.txt".into()),
            mime_type: "text/plain".into(),
        })
        .await?;
    let unrelated = alix.client.attachments().create(bytes()).await?;
    let first = created.remote_attachment().clone();
    let mut second = first.clone();
    second.filename = Some("two.txt".into());
    let key = attachment_key(&first)?;
    assert_eq!(attachment_key(&second)?, key);
    let second_relative = plaintext_rel_path(&second)?;
    assert_ne!(second_relative, plaintext_rel_path(&first)?);
    std::fs::write(dir.path().join(&second_relative), b"shared")?;
    // A file already in place is recorded without a request.
    alix.client.attachments().download(&second).await?;
    assert_eq!(alix.client.attachments().list_local().await?.len(), 3);
    alix.client.attachments().delete_local(&first).await?;
    let listed = alix.client.attachments().list_local().await?;
    assert_eq!(
        listed
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        vec![plaintext_rel_path(unrelated.remote_attachment())?]
    );
    assert!(!dir.path().join(&key).exists());
}

// verifies: ATCH-047, ATCH-077
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn delete_repairs_inaccessible_key_before_removing_pending_row() {
    use std::os::unix::fs::PermissionsExt as _;

    struct RestoreMode(std::path::PathBuf);
    impl Drop for RestoreMode {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        }
    }

    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let key = attachment_key(&remote)?;
    let key_path = dir.path().join(&key);
    let relative = plaintext_rel_path(&remote)?;
    let _restore = RestoreMode(key_path.clone());
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o000))?;

    alix.client.attachments().delete_local(&remote).await?;
    assert!(!key_path.exists());
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_none()
    );
}

// verifies: ATCH-047, ATCH-077
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn delete_rejects_symlinked_key_before_removing_pending_row() {
    let dir = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("file"), b"outside")?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let key = attachment_key(&remote)?;
    tokio::fs::remove_dir_all(dir.path().join(&key)).await?;
    std::os::unix::fs::symlink(outside.path(), dir.path().join(&key))?;

    alix.client.context.attachment_runtime().set_lease_timing(
        Duration::from_millis(300),
        Duration::from_millis(50),
        Duration::from_millis(20),
    );
    let mut upload = Box::pin(pending.upload());
    assert!(matches!(
        futures::poll!(upload.as_mut()),
        std::task::Poll::Pending
    ));

    let result = alix.client.attachments().delete_local(&remote).await;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert_eq!(upload.await.unwrap_err().cause, Cause::Deleted);
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_some()
    );
    assert_eq!(std::fs::read(outside.path().join("file"))?, b"outside");
    std::fs::remove_file(dir.path().join(&key))?;
    std::fs::create_dir(dir.path().join(&key))?;
    tokio::time::sleep(Duration::from_millis(350)).await;
    tokio::time::timeout(Duration::from_secs(5), pending.upload()).await??;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
}

// verifies: ATCH-047, EVENT-001, EVENT-057
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
        matches!(&emitted[1].client, Some(ClientEvent::AttachmentUploadFailed(failed)) if failed.attachment_key == key && failed.url == remote.url && failed.content_digest == remote.content_digest && failed.cause == Cause::Deleted.as_str())
    );
    assert!(
        matches!(&emitted[2].client, Some(ClientEvent::AttachmentDeleted(reference)) if reference.attachment_key == key)
    );
    alix.client.attachments().delete_local(&remote).await?;
    assert!(events.drain().is_empty());
}
