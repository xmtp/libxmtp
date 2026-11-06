//! Attachment storage tests: reconcile.

use super::*;

// verifies: ATCH-047, ATCH-062, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_cannot_race_reconciliation_snapshot() {
    #[cfg(unix)]
    let cases = [
        (false, false, false),
        (true, false, false),
        (true, false, true),
        (true, true, false),
    ];
    #[cfg(not(unix))]
    let cases = [(false, false, false), (true, false, false)];
    for (separate_client, reconcile_via_alias, delete_via_alias) in cases {
        let dir = tempfile::tempdir()?;
        let real = dir.path().join("attachments");
        std::fs::create_dir(&real)?;
        #[cfg(unix)]
        let alias = dir.path().join("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &alias)?;
        #[cfg(not(unix))]
        let alias = real.clone();
        let first_root = if reconcile_via_alias { &alias } else { &real };
        let second_root = if delete_via_alias { &alias } else { &real };
        tester!(alix, attachments_dir: first_root, disable_workers);
        let pending = alix.client.attachments().create(bytes()).await?;
        let remote = pending.remote_attachment().clone();
        let second = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
            .attachments_dir(second_root)
            .with_disable_workers(true)
            .build()
            .await?;
        let relative = plaintext_rel_path(&remote)?;
        alix.client
            .context
            .db()
            .delete_local_attachment(&relative)?;
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        alix.client.context.attachment_runtime().set_pause(
            PausePoint::ReconcileSnapshot,
            Some((entered.clone(), resume.clone())),
        );
        let context = alix.client.context.clone();
        let reconcile =
            tokio::spawn(
                async move { context.attachment_runtime().reconcile_again(&context).await },
            );
        tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
        let deleting_client = if separate_client {
            second.clone()
        } else {
            alix.client.clone()
        };
        let deleting_remote = remote.clone();
        let mut deletion = tokio::spawn(async move {
            deleting_client
                .attachments()
                .delete_local(&deleting_remote)
                .await
        });
        let finished_early =
            match tokio::time::timeout(Duration::from_millis(250), &mut deletion).await {
                Ok(result) => {
                    result??;
                    true
                }
                Err(_) => false,
            };
        resume.notify_one();
        tokio::time::timeout(Duration::from_secs(5), reconcile).await???;
        if !finished_early {
            tokio::time::timeout(Duration::from_secs(5), deletion).await???;
        }
        assert!(!alix.client.attachments().local_path(&remote)?.exists());
        assert!(
            alix.client
                .context
                .db()
                .get_local_attachment(&relative)?
                .is_none()
        );
        assert!(alix.client.attachments().list_local().await?.is_empty());
    }
}

// verifies: ATCH-046, ATCH-063, ATCH-076
// Covers plan P22 and P24.
#[xmtp_common::test(unwrap_try = true)]
async fn reconcile_after_crash_points() {
    use std::time::UNIX_EPOCH;
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let original = pending.local_path()?;
    let relative = plaintext_rel_path(pending.remote_attachment())?;
    let adopted_mtime_ns = 1_234_567_000_000_000_i64;
    std::fs::File::open(&original)?.set_modified(UNIX_EPOCH + Duration::from_secs(1_234_567))?;
    alix.client
        .context
        .db()
        .delete_local_attachment(&relative)?;
    let missing = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/absent";
    alix.client
        .context
        .db()
        .insert_or_ignore_local_attachment(missing, 1, None, None)?;
    let temp = dir.path().join(".tmp/stale");
    tokio::fs::create_dir_all(temp.parent().unwrap()).await?;
    tokio::fs::write(&temp, b"temporary").await?;
    let staged = dir.path().join(format!(".staged/{}", "a".repeat(64)));
    tokio::fs::create_dir_all(staged.parent().unwrap()).await?;
    tokio::fs::write(&staged, b"orphan").await?;
    let fresh_temp = dir.path().join(".tmp/fresh");
    let fresh_staged = dir.path().join(format!(".staged/{}", "c".repeat(64)));
    tokio::fs::write(&fresh_temp, b"new temporary").await?;
    tokio::fs::write(&fresh_staged, b"new orphan").await?;
    let owned_staged = dir
        .path()
        .join(staged_path(&pending.remote_attachment().content_digest)?);
    assert!(owned_staged.exists());
    let completed = alix.client.attachments().create(bytes()).await?;
    let completed_staged = dir
        .path()
        .join(staged_path(&completed.remote_attachment().content_digest)?);
    let completed_body = tokio::fs::read(&completed_staged).await?;
    completed.upload().await?;
    tokio::fs::write(&completed_staged, completed_body).await?;
    assert_eq!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&completed.remote_attachment().content_digest)?
            .unwrap()
            .status,
        "complete"
    );
    for file in [&temp, &staged, &owned_staged] {
        std::fs::File::open(file)?.set_modified(UNIX_EPOCH + Duration::from_secs(1))?;
    }
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(original.exists());
    assert!(!temp.exists());
    assert!(!staged.exists());
    assert!(fresh_temp.exists());
    assert!(fresh_staged.exists());
    assert!(owned_staged.exists());
    assert!(!completed_staged.exists());
    assert!(next.attachments().list_pending().await?.iter().any(|row| {
        row.remote_attachment().content_digest == pending.remote_attachment().content_digest
    }));
    let listed = next.attachments().list_local().await?;
    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .any(|row| row.path == relative && row.created_at_ns == adopted_mtime_ns)
    );
    let adopted = next
        .attachments()
        .download(pending.remote_attachment())
        .await?;
    assert_eq!(adopted.mime_type, None);
    assert_eq!(adopted.filename, None);
}

// verifies: ATCH-076
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn reconcile_keeps_pre_epoch_modification_time() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let relative = plaintext_rel_path(pending.remote_attachment())?;
    let path = pending.local_path()?;
    alix.client
        .context
        .db()
        .delete_local_attachment(&relative)?;
    std::fs::File::open(path)?.set_modified(std::time::UNIX_EPOCH - Duration::from_secs(5))?;
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let row = next
        .context
        .db()
        .get_local_attachment(&relative)?
        .expect("reconciliation adopts the file");
    assert_eq!(row.created_at_ns, -5_000_000_000);
}

// verifies: ATCH-068, ATCH-076
#[xmtp_common::test(unwrap_try = true)]
async fn staged_orphan_older_than_pending_age_is_removed() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let old = alix.client.attachments().create(bytes()).await?;
    let old_digest = &old.remote_attachment().content_digest;
    let old_path = dir.path().join(staged_path(old_digest)?);
    alix.client
        .context
        .db()
        .delete_pending_attachment(old_digest)?;
    std::fs::File::open(&old_path)?
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(120))?;

    let recent = alix.client.attachments().create(bytes()).await?;
    let recent_digest = &recent.remote_attachment().content_digest;
    let recent_path = dir.path().join(staged_path(recent_digest)?);
    alix.client
        .context
        .db()
        .delete_pending_attachment(recent_digest)?;
    std::fs::File::open(&recent_path)?
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(10))?;

    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .attachment_options(AttachmentOptions {
            max_pending_age: Some(Duration::from_secs(60)),
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(!old_path.exists());
    assert!(recent_path.exists());
    assert!(
        next.context
            .db()
            .get_pending_attachment(old_digest)?
            .is_none()
    );
    assert!(
        next.context
            .db()
            .get_pending_attachment(recent_digest)?
            .is_none()
    );

    let default_dir = tempfile::tempdir()?;
    tester!(bo, attachments_dir: default_dir.path(), configured: offer, disable_workers);
    let default_pending = bo.client.attachments().create(bytes()).await?;
    let default_digest = &default_pending.remote_attachment().content_digest;
    let default_path = default_dir.path().join(staged_path(default_digest)?);
    bo.client
        .context
        .db()
        .delete_pending_attachment(default_digest)?;
    std::fs::File::open(&default_path)?
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(1_800))?;
    let expired_default = bo.client.attachments().create(bytes()).await?;
    let expired_default_digest = &expired_default.remote_attachment().content_digest;
    let expired_default_path = default_dir
        .path()
        .join(staged_path(expired_default_digest)?);
    bo.client
        .context
        .db()
        .delete_pending_attachment(expired_default_digest)?;
    std::fs::File::open(&expired_default_path)?
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(7_200))?;
    let _next_default = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(default_path.exists());
    assert!(!expired_default_path.exists());
}

// verifies: ATCH-062, ATCH-076
#[xmtp_common::test(unwrap_try = true)]
async fn list_local_retries_reconcile_after_build_error() {
    use std::time::UNIX_EPOCH;

    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let relative = format!("{}/adopted", "a".repeat(64));
    let file = dir.path().join(&relative);
    tokio::fs::create_dir_all(file.parent().unwrap()).await?;
    tokio::fs::write(&file, b"adopt me").await?;
    let modified = 1_234_567_000_000_000_i64;
    std::fs::File::open(&file)?.set_modified(UNIX_EPOCH + Duration::from_secs(1_234_567))?;
    fail_next_reconciles(1);
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let listed = next.attachments().list_local().await?;
    assert!(
        listed
            .iter()
            .any(|row| row.path == relative && row.created_at_ns == modified)
    );
    fail_next_reconciles(0);
}

// verifies: ATCH-062, ATCH-076
#[xmtp_common::test(unwrap_try = true)]
async fn reconcile_failure_blocks_reads_and_download() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let remote = alix
        .client
        .attachments()
        .create(bytes())
        .await?
        .remote_attachment()
        .clone();
    let (url, requests) = serve_body(Vec::new()).await;
    let mut remote = remote;
    remote.url = url;
    fail_next_reconciles(5);
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(
        matches!(next.attachments().list_local().await, Err(error) if error.cause == Cause::LocalStorage)
    );
    assert!(
        matches!(next.attachments().download(&remote).await, Err(error) if error.cause == Cause::LocalStorage)
    );
    assert!(
        matches!(next.attachments().list_pending().await, Err(error) if error.cause == Cause::LocalStorage)
    );
    assert!(
        matches!(next.attachments().pending(&remote).await, Err(error) if error.cause == Cause::LocalStorage)
    );
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    fail_next_reconciles(0);
}

// verifies: ATCH-063, ATCH-076
// Covers plan P24.
#[xmtp_common::test(unwrap_try = true)]
async fn reconcile_ignores_stray_and_nested_files() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let valid = format!("{}/plain.txt", "a".repeat(64));
    let nested = format!("{}/nested/extra.txt", "a".repeat(64));
    for path in [&valid, &nested, "misc/file.txt", ".DS_Store"] {
        let path = dir.path().join(path);
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::write(path, b"app file").await?;
    }
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let listed = next.attachments().list_local().await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, valid);
    assert!(dir.path().join(nested).exists());
    assert!(dir.path().join("misc/file.txt").exists());
    assert!(dir.path().join(".DS_Store").exists());
}

// verifies: ATCH-025, ATCH-037
// Covers plan P22.
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn complete_recorded_before_staged_file() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = pending.remote_attachment().content_digest.clone();
    alix.client
        .context
        .attachment_runtime()
        .fail_next(Fault::StagedRemoval);
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
    assert_eq!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&digest)?
            .unwrap()
            .status,
        "complete"
    );
    assert!(dir.path().join(staged_path(&digest)?).exists());
}
