//! Attachment storage tests: deletion events.

use super::*;

// verifies: ATCH-062, ATCH-063, EVENT-010
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_event_hides_rows_after_metadata_delete_error() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let relative = plaintext_rel_path(&remote)?;
    let second = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentDeleted]))?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_local_delete BEFORE DELETE ON local_attachments \
             BEGIN SELECT RAISE(ABORT, 'metadata delete rejected'); END",
        )
        .execute(conn)
    })?;
    assert_eq!(
        alix.client
            .attachments()
            .delete_local(&remote)
            .await
            .unwrap_err()
            .cause,
        Cause::LocalStorage
    );
    assert!(!alix.client.attachments().local_path(&remote)?.exists());
    assert_eq!(events.drain().len(), 1);
    assert!(alix.client.attachments().list_local().await?.is_empty());
    assert!(second.attachments().list_local().await?.is_empty());
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_some()
    );
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER reject_local_delete").execute(conn)
    })?;
    second.attachments().delete_local(&remote).await?;
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_none()
    );
}

// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_event_survives_later_row_failure() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let key = attachment_key(&remote)?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentDeleted]))?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_local_attachment_delete BEFORE DELETE ON local_attachments \
             BEGIN SELECT RAISE(ABORT, 'local row delete failed'); END",
        )
        .execute(conn)
    })?;
    let result = alix.client.attachments().delete_local(&remote).await;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert!(!dir.path().join(key).exists());
    let emitted = events.drain();
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        emitted[0].client,
        Some(ClientEvent::AttachmentDeleted(_))
    ));
}

// verifies: EVENT-001
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn partial_key_removal_emits_one_deletion_event() {
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
    let plaintext = dir.path().join(plaintext_rel_path(&remote)?);
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentDeleted]))?;
    // The plaintext file can be removed, but not the key directory.
    let _restore = RestoreMode(dir.path().to_path_buf());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500))?;

    let result = alix.client.attachments().delete_local(&remote).await;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert!(!plaintext.exists());
    assert!(dir.path().join(&key).is_dir());
    let emitted = events.drain();
    assert_eq!(emitted.len(), 1);
    assert!(
        matches!(&emitted[0].client, Some(ClientEvent::AttachmentDeleted(reference)) if reference.attachment_key == key)
    );
}

// verifies: EVENT-001
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn failed_key_removal_without_removal_emits_no_deletion_event() {
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
    // Only an entry that cannot be removed stays in the key directory.
    std::fs::remove_file(dir.path().join(plaintext_rel_path(&remote)?))?;
    std::fs::create_dir(key_path.join("sub"))?;
    std::fs::write(key_path.join("sub/file"), b"app")?;
    let _restore = RestoreMode(key_path.join("sub"));
    std::fs::set_permissions(key_path.join("sub"), std::fs::Permissions::from_mode(0o500))?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentDeleted]))?;

    let result = alix.client.attachments().delete_local(&remote).await;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert!(key_path.join("sub/file").exists());
    assert!(events.drain().is_empty());
}

// verifies: EVENT-001, ATCH-047
#[xmtp_common::test(unwrap_try = true)]
async fn pending_row_only_delete_emits_no_deleted_event() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    let key = attachment_key(&remote)?;
    tokio::fs::remove_dir_all(dir.path().join(&key)).await?;
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(client.context.db().list_local_attachments()?.is_empty());
    assert!(
        client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_some()
    );
    let events = client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentDeleted]))?;
    client.attachments().delete_local(&remote).await?;
    assert!(
        client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .is_none()
    );
    assert!(events.drain().is_empty());
}
