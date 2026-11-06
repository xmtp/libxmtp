//! Upload tests: local collisions.

use super::*;

// verifies: ATCH-045, ATCH-046
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn conflicting_plaintext_move_keeps_existing_file_and_record() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pause = Arc::new(CreateMovePause {
        path: Mutex::new(None),
        staged_path: Mutex::new(None),
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    });
    alix.client
        .context
        .attachment_runtime()
        .set_create_move_pause(Some(pause.clone()));
    let client = alix.client.clone();
    let create =
        xmtp_common::task::spawn(async move { client.attachments().create(bytes()).await });
    tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
    let path = pause
        .path
        .lock()
        .clone()
        .expect("creation reached the first move");
    let staged = pause.staged_path.lock().clone().expect("staged path");
    let destination = dir.path().join(&path);
    tokio::fs::create_dir_all(destination.parent().expect("key directory")).await?;
    tokio::fs::write(&destination, b"existing attachment").await?;
    alix.client.context.db().insert_or_ignore_local_attachment(
        &path,
        123,
        Some("image/png".into()),
        Some("kept.png".into()),
    )?;
    let original_row = alix
        .client
        .context
        .db()
        .get_local_attachment(&path)?
        .expect("record");
    pause.resume.notify_one();
    let created = create.await??;
    assert_eq!(plaintext_rel_path(created.remote_attachment())?, path);
    assert_eq!(tokio::fs::read(&destination).await?, b"existing attachment");
    assert_eq!(
        alix.client.context.db().get_local_attachment(&path)?,
        Some(original_row)
    );
    assert_eq!(alix.client.attachments().list_pending().await?.len(), 1);
    let files = alix
        .client
        .context
        .attachment_runtime()
        .local_store()?
        .list_files()
        .await?;
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|file| file.path == path));
    assert!(files.iter().any(|file| file.path == staged));
}

// verifies: ATCH-045, ATCH-046
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn existing_plaintext_without_record_is_adopted_on_create() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pause = Arc::new(CreateMovePause {
        path: Mutex::new(None),
        staged_path: Mutex::new(None),
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    });
    alix.client
        .context
        .attachment_runtime()
        .set_create_move_pause(Some(pause.clone()));
    let client = alix.client.clone();
    let create =
        xmtp_common::task::spawn(async move { client.attachments().create(bytes()).await });
    tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
    let path = pause.path.lock().clone().expect("plaintext path");
    let destination = dir.path().join(&path);
    tokio::fs::create_dir_all(destination.parent().expect("key directory")).await?;
    tokio::fs::write(&destination, b"existing attachment").await?;
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&path)?
            .is_none()
    );
    pause.resume.notify_one();
    let created = create.await??;
    assert_eq!(plaintext_rel_path(created.remote_attachment())?, path);
    assert_eq!(tokio::fs::read(&destination).await?, b"existing attachment");
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&path)?
            .is_some()
    );
    assert_eq!(alix.client.attachments().list_pending().await?.len(), 1);
}

// verifies: ATCH-032, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn directory_plaintext_collision_rejects_create() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pause = Arc::new(CreateMovePause {
        path: Mutex::new(None),
        staged_path: Mutex::new(None),
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    });
    alix.client
        .context
        .attachment_runtime()
        .set_create_move_pause(Some(pause.clone()));
    let client = alix.client.clone();
    let create =
        xmtp_common::task::spawn(async move { client.attachments().create(bytes()).await });
    tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
    let path = pause.path.lock().clone().expect("plaintext path");
    let staged = pause.staged_path.lock().clone().expect("staged path");
    let destination = dir.path().join(&path);
    tokio::fs::create_dir_all(&destination).await?;
    pause.resume.notify_one();
    let result = create.await?;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert!(destination.is_dir());
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&path)?
            .is_none()
    );
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    assert!(!dir.path().join(staged).exists());
    assert!(
        tokio::fs::read_dir(dir.path().join(".tmp"))
            .await?
            .next_entry()
            .await?
            .is_none()
    );
}

// verifies: ATCH-046
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn conflicting_pending_row_rolls_back_only_owned_files() {
    for preexisting_key_dir in [false, true] {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pause = Arc::new(CreateMovePause {
            path: Mutex::new(None),
            staged_path: Mutex::new(None),
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        alix.client
            .context
            .attachment_runtime()
            .set_create_move_pause(Some(pause.clone()));
        let client = alix.client.clone();
        let create =
            xmtp_common::task::spawn(async move { client.attachments().create(bytes()).await });
        tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
        let plain = pause.path.lock().clone().expect("plaintext path");
        let key_dir = dir.path().join(&plain);
        let key_dir = key_dir.parent().expect("key directory");
        if preexisting_key_dir {
            tokio::fs::create_dir_all(key_dir).await?;
        }
        let staged = pause.staged_path.lock().clone().expect("staged path");
        let digest = staged.strip_prefix(".staged/").expect("digest");
        let staged_file = dir.path().join(&staged);
        alix.client
            .context
            .db()
            .insert_or_ignore_pending_attachment(digest, b"existing record", 123)?;
        let original_row = alix
            .client
            .context
            .db()
            .get_pending_attachment(digest)?
            .expect("record");
        pause.resume.notify_one();
        let error = create.await?.err().expect("staged move must fail");
        assert_eq!(error.cause, Cause::LocalStorage);
        assert!(!dir.path().join(&plain).exists());
        assert_eq!(key_dir.exists(), preexisting_key_dir);
        assert!(!staged_file.exists());
        assert_eq!(
            alix.client.context.db().get_pending_attachment(digest)?,
            Some(original_row)
        );
        assert!(
            alix.client
                .context
                .db()
                .get_local_attachment(&plain)?
                .is_none()
        );
        let files = alix
            .client
            .context
            .attachment_runtime()
            .local_store()?
            .list_files()
            .await?;
        assert!(files.is_empty());
    }
}

// verifies: ATCH-045, ATCH-046
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn existing_staged_ciphertext_is_reused_on_create() {
    use std::os::unix::fs::MetadataExt as _;

    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pause = Arc::new(CreateMovePause {
        path: Mutex::new(None),
        staged_path: Mutex::new(None),
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    });
    alix.client
        .context
        .attachment_runtime()
        .set_create_move_pause(Some(pause.clone()));
    let client = alix.client.clone();
    let create =
        xmtp_common::task::spawn(async move { client.attachments().create(bytes()).await });
    tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
    let staged = pause.staged_path.lock().clone().expect("staged path");
    let staged_file = dir.path().join(&staged);
    let mut temp_files = tokio::fs::read_dir(dir.path().join(".tmp")).await?;
    let mut ciphertext = None;
    while let Some(entry) = temp_files.next_entry().await? {
        if entry.file_name().to_string_lossy().ends_with("-staged") {
            ciphertext = Some(tokio::fs::read(entry.path()).await?);
        }
    }
    let ciphertext = ciphertext.expect("staged temporary file");
    tokio::fs::create_dir_all(staged_file.parent().expect("staged directory")).await?;
    tokio::fs::write(&staged_file, &ciphertext).await?;
    let existing_inode = tokio::fs::metadata(&staged_file).await?.ino();
    pause.resume.notify_one();
    let created = create.await??;
    assert_eq!(
        staged_path(&created.remote_attachment().content_digest)?,
        staged
    );
    assert_eq!(tokio::fs::read(&staged_file).await?, ciphertext);
    assert_eq!(
        tokio::fs::metadata(&staged_file).await?.ino(),
        existing_inode
    );
    assert!(
        alix.client
            .context
            .db()
            .get_pending_attachment(&created.remote_attachment().content_digest)?
            .is_some()
    );
    let mut temp_files = tokio::fs::read_dir(dir.path().join(".tmp")).await?;
    assert!(temp_files.next_entry().await?.is_none());
}

// verifies: ATCH-045, ATCH-046
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn damaged_staged_ciphertext_is_replaced_on_create() {
    use std::os::unix::fs::MetadataExt as _;

    for truncate in [false, true] {
        let dir = tempfile::tempdir()?;
        tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
        let pause = Arc::new(CreateMovePause {
            path: Mutex::new(None),
            staged_path: Mutex::new(None),
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        alix.client
            .context
            .attachment_runtime()
            .set_create_move_pause(Some(pause.clone()));
        let client = alix.client.clone();
        let create =
            xmtp_common::task::spawn(async move { client.attachments().create(bytes()).await });
        tokio::time::timeout(Duration::from_secs(3), pause.entered.notified()).await?;
        let staged = pause.staged_path.lock().clone().expect("staged path");
        let staged_file = dir.path().join(&staged);
        let mut temp_files = tokio::fs::read_dir(dir.path().join(".tmp")).await?;
        let mut ciphertext = None;
        while let Some(entry) = temp_files.next_entry().await? {
            if entry.file_name().to_string_lossy().ends_with("-staged") {
                ciphertext = Some(tokio::fs::read(entry.path()).await?);
            }
        }
        let mut ciphertext = ciphertext.expect("staged temporary file");
        if truncate {
            ciphertext.pop();
        } else {
            ciphertext[0] ^= 1;
        }
        tokio::fs::create_dir_all(staged_file.parent().expect("staged directory")).await?;
        tokio::fs::write(&staged_file, &ciphertext).await?;
        let damaged_inode = tokio::fs::metadata(&staged_file).await?.ino();
        pause.resume.notify_one();
        let created = create.await??;
        let expected_length = u64::from(
            created
                .remote_attachment()
                .content_length
                .expect("content length"),
        );
        let (digest, length) = alix
            .client
            .context
            .attachment_runtime()
            .local_store()?
            .open_read(&staged)
            .await?
            .sha256(expected_length)
            .await?;
        assert_eq!(
            hex::encode(digest),
            created.remote_attachment().content_digest
        );
        assert_eq!(length, expected_length);
        assert_ne!(
            tokio::fs::metadata(&staged_file).await?.ino(),
            damaged_inode
        );
        created.upload().await?;
        assert_eq!(created.status(), PendingAttachmentStatus::Complete);
    }
}

// verifies: ATCH-035, ATCH-038, ATCH-067
#[xmtp_common::test(unwrap_try = true)]
async fn one_pending_per_digest() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let first = alix.client.attachments().create(bytes()).await?;
    let second = alix
        .client
        .attachments()
        .pending(first.remote_attachment())
        .await?;
    assert!(first.probe().shares_with(&second.probe()));
    assert_eq!(alix.client.attachments().list_pending().await?.len(), 1);
}

// verifies: ATCH-036, ATCH-034, EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn staged_unusable() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let staged = dir
        .path()
        .join(staged_path(&pending.remote_attachment().content_digest)?);
    tokio::fs::write(staged, b"damaged").await?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadFailed,
        ]))?;
    assert_eq!(
        pending.upload().await.unwrap_err().cause,
        Cause::StagedUnusable
    );
    assert!(matches!(
        pending.status(),
        PendingAttachmentStatus::Failed(_)
    ));
    let emitted = events.drain();
    assert_eq!(emitted.len(), 2);
    let key = pending.reference_for_test().attachment_key;
    assert!(matches!(
        &emitted[0].client,
        Some(ClientEvent::AttachmentUploadStarted(reference)) if reference.attachment_key == key
    ));
    assert!(matches!(
        &emitted[1].client,
        Some(ClientEvent::AttachmentUploadFailed(failed)) if failed.attachment_key == key
    ));
}

// verifies: ATCH-036, ATCH-060
#[cfg(unix)]
#[xmtp_common::test(unwrap_try = true)]
async fn staged_read_error_is_retryable_local_storage() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = &pending.remote_attachment().content_digest;
    let staged = dir.path().join(staged_path(digest)?);
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o000))?;
    let error = pending.upload().await.unwrap_err();
    assert_eq!(error.cause, Cause::LocalStorage);
    assert!(error.retryable);
    let row = alix
        .client
        .context
        .db()
        .get_pending_attachment(digest)?
        .unwrap();
    assert_eq!(row.status, "failed");
    assert_eq!(row.failure_cause.as_deref(), Some("local_storage"));
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600))?;
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);

    let missing = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"missing staged file".to_vec(),
            filename: Some("missing.txt".into()),
            mime_type: "text/plain".into(),
        })
        .await?;
    let missing_path = dir
        .path()
        .join(staged_path(&missing.remote_attachment().content_digest)?);
    tokio::fs::remove_file(missing_path).await?;
    assert_eq!(
        missing.upload().await.unwrap_err().cause,
        Cause::StagedUnusable
    );
}
