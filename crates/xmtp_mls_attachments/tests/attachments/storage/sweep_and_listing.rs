//! Attachment storage tests: sweep and listing.

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
        .attachment_runtime()
        .sweep(&alix.client.context)
        .await?;
    assert!(!dir.path().join(staged_path(digest)?).exists());
    assert!(
        !alix
            .client
            .context
            .attachment_runtime()
            .holds_pending(digest)
    );
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
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(next.context.db().get_pending_attachment(digest)?.is_none());
    assert!(!staged.exists());
    assert!(
        !next
            .registered_worker_kinds()
            .contains(&xmtp_mls::worker::WorkerKind::AttachmentCleanup)
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
    alix.client
        .context
        .attachment_runtime()
        .set_pause(PausePoint::Sweep, Some((entered.clone(), resume.clone())));
    let context = alix.client.context.clone();
    let sweep =
        xmtp_common::task::spawn(async move { context.attachment_runtime().sweep(&context).await });
    entered.notified().await;
    assert!(pending.probe().state_is_held());
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
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
        .attachment_runtime()
        .sweep(&alix.client.context)
        .await?;
    assert!(db.get_pending_attachment(&leased_digest)?.is_some());
    assert!(dir.path().join(staged_path(&leased_digest)?).exists());
    assert!(db.get_pending_attachment(&waiting_digest)?.is_none());
    assert!(!dir.path().join(staged_path(&waiting_digest)?).exists());
    assert!(db.get_pending_attachment(&complete_digest)?.is_none());
    assert!(!dir.path().join(staged_path(&complete_digest)?).exists());
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
