use super::*;

// verifies: ATCH-067, ATCH-068
#[xmtp_common::test(unwrap_try = true)]
async fn pending_expire() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = &pending.remote_attachment().content_digest;
    alix.client.context.db().delete_pending_attachment(digest)?;
    alix.client
        .context
        .db()
        .insert_or_ignore_pending_attachment(
            digest,
            &pending.remote_attachment().encode_to_vec(),
            now_ns() - 172_800_000_000_000,
        )?;
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    alix.client
        .context
        .attachments
        .sweep(&alix.client.context)
        .await?;
    assert!(!dir.path().join(staged_path(digest)?).exists());
    let registry = &alix.client.context.attachments.pending;
    assert!(!registry.lock().contains_key(digest));
}

// verifies: ATCH-068
#[xmtp_common::test(unwrap_try = true)]
async fn disabled_workers_still_sweep_on_next_client_creation() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = &pending.remote_attachment().content_digest;
    let staged = dir.path().join(staged_path(digest)?);
    assert!(staged.exists());
    let db = alix.client.context.db();
    db.delete_pending_attachment(digest)?;
    db.insert_or_ignore_pending_attachment(
        digest,
        &pending.remote_attachment().encode_to_vec(),
        now_ns() - 172_800_000_000_000,
    )?;
    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(next.context.db().get_pending_attachment(digest)?.is_none());
    assert!(!staged.exists());
    assert!(
        !next
            .workers
            .registered_kinds()
            .contains(&crate::worker::WorkerKind::AttachmentCleanup)
    );
}

// verifies: ATCH-068
#[xmtp_common::test(unwrap_try = true)]
async fn sweep_holds_upload_state_until_expired_file_is_removed() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = pending.remote_attachment().content_digest.clone();
    let db = alix.client.context.db();
    db.delete_pending_attachment(&digest)?;
    db.insert_or_ignore_pending_attachment(
        &digest,
        &pending.remote_attachment().encode_to_vec(),
        now_ns() - 172_800_000_000_000,
    )?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.sweep_pause.lock() = Some((entered.clone(), resume.clone()));
    let context = alix.client.context.clone();
    let sweep = xmtp_common::task::spawn(async move { context.attachments.sweep(&context).await });
    entered.notified().await;
    assert!(pending.shared.state.try_lock().is_err());
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    resume.notify_one();
    sweep.await??;
    assert_eq!(upload.await?.unwrap_err().cause, Cause::StagedUnusable);
    assert!(!dir.path().join(staged_path(&digest)?).exists());
}

// verifies: ATCH-068
#[xmtp_common::test(unwrap_try = true)]
async fn sweep_error_does_not_block_build() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = &pending.remote_attachment().content_digest;
    alix.client.context.db().delete_pending_attachment(digest)?;
    alix.client
        .context
        .db()
        .insert_or_ignore_pending_attachment(
            digest,
            &pending.remote_attachment().encode_to_vec(),
            now_ns() - 172_800_000_000_000,
        )?;
    let staged = dir.path().join(staged_path(digest)?);
    tokio::fs::remove_file(&staged).await?;
    tokio::fs::create_dir(&staged).await?;
    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(next.attachments().offered());
    assert!(staged.is_dir());
}

// verifies: ATCH-067
#[xmtp_common::test(unwrap_try = true)]
async fn list_pending() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let one = alix.client.attachments().create(bytes()).await?;
    let two = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"second".to_vec(),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    let listed = alix.client.attachments().list_pending().await?;
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[0].remote_attachment().content_digest,
        one.remote_attachment().content_digest
    );
    assert_eq!(
        listed[1].remote_attachment().content_digest,
        two.remote_attachment().content_digest
    );
}

// verifies: ATCH-038
#[xmtp_common::test(unwrap_try = true)]
async fn resume_by_remote() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let resumed = alix
        .client
        .attachments()
        .pending(pending.remote_attachment())
        .await?;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Waiting);
    assert_eq!(
        resumed.remote_attachment().url,
        pending.remote_attachment().url
    );
}

// verifies: ATCH-068, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn sweep_keeps_leased_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let leased = alix.client.attachments().create(bytes()).await?;
    let waiting = alix.client.attachments().create(bytes()).await?;
    let complete = alix.client.attachments().create(bytes()).await?;
    let leased_digest = leased.remote_attachment().content_digest.clone();
    let waiting_digest = waiting.remote_attachment().content_digest.clone();
    let complete_digest = complete.remote_attachment().content_digest.clone();
    let db = alix.client.context.db();
    db.raw_query(|conn| {
        xmtp_db::diesel::sql_query("UPDATE pending_attachments SET created_at_ns = 1").execute(conn)
    })?;
    let token = [3u8; 16];
    db.claim_pending_attachment(
        &leased_digest,
        &token,
        now_ns(),
        LEASE_DURATION.as_nanos() as i64,
    )?;
    db.claim_pending_attachment(
        &complete_digest,
        &token,
        now_ns(),
        LEASE_DURATION.as_nanos() as i64,
    )?;
    db.finish_pending_attachment(
        &complete_digest,
        &token,
        now_ns(),
        PendingAttachmentOutcome::Complete,
    )?;
    alix.client
        .context
        .attachments
        .sweep(&alix.client.context)
        .await?;
    assert!(db.get_pending_attachment(&leased_digest)?.is_some());
    assert!(dir.path().join(staged_path(&leased_digest)?).exists());
    assert!(db.get_pending_attachment(&waiting_digest)?.is_none());
    assert!(!dir.path().join(staged_path(&waiting_digest)?).exists());
    assert!(db.get_pending_attachment(&complete_digest)?.is_none());
    assert!(!dir.path().join(staged_path(&complete_digest)?).exists());
}

// verifies: ATCH-047
#[xmtp_common::test(unwrap_try = true)]
async fn delete_ends_other_clients_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let uploader = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    *uploader.context.attachments.lease_timing.lock() = LeaseTiming::for_test(
        Duration::from_millis(300),
        Duration::from_millis(30),
        Duration::from_millis(10),
    );
    let deleting_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
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

// verifies: ATCH-047, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn delete_with_watcher_ends_other_clients_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let uploader = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let deleting_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    *deleting_client.context.attachments.lease_timing.lock() =
        LeaseTiming::for_test(LEASE_DURATION, LEASE_RENEW, Duration::from_millis(20));
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

// verifies: ATCH-047, ATCH-062, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn list_local_exact() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let one = alix.client.attachments().create(bytes()).await?;
    let two = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"second".to_vec(),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    let listed = alix.client.attachments().list_local().await?;
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].path, plaintext_rel_path(one.remote_attachment())?);
    assert_eq!(listed[1].path, plaintext_rel_path(two.remote_attachment())?);
    alix.client
        .attachments()
        .delete_local(one.remote_attachment())
        .await?;
    assert!(!one.local_path()?.exists());
    assert_eq!(alix.client.attachments().list_local().await?.len(), 1);
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

// verifies: ATCH-047, EVENT-055
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
        matches!(&emitted[1].client, Some(ClientEvent::AttachmentUploadFailed(failed)) if failed.attachment_key == key && failed.cause == "deleted")
    );
    assert!(
        matches!(&emitted[2].client, Some(ClientEvent::AttachmentDeleted(reference)) if reference.attachment_key == key)
    );
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
    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
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

    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let _next_default = crate::builder::ClientBuilder::from_client(bo.client.clone())
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
    FAIL_NEXT_RECONCILES.store(1, AtomicOrdering::SeqCst);
    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let listed = next.attachments().list_local().await?;
    assert!(
        listed
            .iter()
            .any(|row| row.path == relative && row.created_at_ns == modified)
    );
    FAIL_NEXT_RECONCILES.store(0, AtomicOrdering::SeqCst);
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
    FAIL_NEXT_RECONCILES.store(5, AtomicOrdering::SeqCst);
    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    FAIL_NEXT_RECONCILES.store(0, AtomicOrdering::SeqCst);
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
    let next = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = pending.remote_attachment().content_digest.clone();
    let staged_dir = dir.path().join(".staged");
    std::fs::set_permissions(&staged_dir, std::fs::Permissions::from_mode(0o555))?;
    let result = pending.upload().await;
    std::fs::set_permissions(&staged_dir, std::fs::Permissions::from_mode(0o755))?;
    result?;
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
