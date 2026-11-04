//! Download tests: publication.

use super::*;

// verifies: EVENT-057
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn download_completion_keeps_publication_order_for_one_key() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut first_remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(
        sender
            .path()
            .join(staged_path(&first_remote.content_digest)?),
    )
    .await?;
    let (second_url, _) = serve_body(body.clone()).await;
    let (url, _) = serve_body(body).await;
    first_remote.url = url;
    let mut second_remote = first_remote.clone();
    second_remote.filename = Some("second.txt".into());
    second_remote.url = second_url;
    assert_eq!(
        attachment_key(&first_remote)?,
        attachment_key(&second_remote)?
    );
    assert_ne!(
        plaintext_rel_path(&first_remote)?,
        plaintext_rel_path(&second_remote)?
    );

    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentDownloadStarted,
        EventKind::AttachmentDownloadCompleted,
    ]))?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    client.context.attachment_runtime().set_pause(
        PausePoint::DownloadCompletion,
        Some((entered.clone(), resume.clone())),
    );
    let first_client = client.clone();
    let first = first_remote.clone();
    let first_download =
        tokio::spawn(async move { first_client.attachments().download(&first).await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    assert!(
        client
            .context
            .db()
            .get_local_attachment(&plaintext_rel_path(&first_remote)?)?
            .is_some()
    );

    let second_client = client.clone();
    let second = second_remote.clone();
    let mut second_download =
        tokio::spawn(async move { second_client.attachments().download(&second).await });
    let second_finished_early =
        tokio::time::timeout(Duration::from_millis(500), &mut second_download).await;
    let passed_before_first_event = second_finished_early.is_ok();
    resume.notify_one();
    tokio::time::timeout(Duration::from_secs(5), first_download).await???;
    match second_finished_early {
        Ok(result) => {
            result??;
        }
        Err(_) => {
            tokio::time::timeout(Duration::from_secs(5), second_download).await???;
        }
    }
    assert!(
        !passed_before_first_event,
        "second download passed the first completion event"
    );
    let completed: Vec<_> = events
        .drain()
        .into_iter()
        .filter_map(|event| match event.client {
            Some(ClientEvent::AttachmentDownloadCompleted(reference)) => Some(reference),
            _ => None,
        })
        .collect();
    assert_eq!(completed.len(), 2);
    assert_eq!(completed[0].url, first_remote.url);
    assert_eq!(completed[1].url, second_remote.url);
}

// verifies: EVENT-057
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn failed_download_waits_for_same_key_publication_event() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut published = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&published.content_digest)?)).await?;
    let (good_url, _) = serve_body(body).await;
    let (bad_url, _) = serve_body(b"forged".to_vec()).await;
    published.url = good_url;
    let mut failed = published.clone();
    failed.url = bad_url;
    failed.filename = Some("failed.txt".into());
    assert_eq!(attachment_key(&published)?, attachment_key(&failed)?);
    assert_ne!(
        plaintext_rel_path(&published)?,
        plaintext_rel_path(&failed)?
    );

    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentDownloadStarted,
        EventKind::AttachmentDownloadCompleted,
        EventKind::AttachmentDownloadFailed,
    ]))?;
    let failed_entered = Arc::new(tokio::sync::Notify::new());
    let failed_resume = Arc::new(tokio::sync::Notify::new());
    client.context.attachment_runtime().set_pause(
        PausePoint::DownloadFailure,
        Some((failed_entered.clone(), failed_resume.clone())),
    );
    let second_client = client.clone();
    let second_remote = failed.clone();
    let mut second_download =
        tokio::spawn(async move { second_client.attachments().download(&second_remote).await });
    tokio::time::timeout(Duration::from_secs(5), failed_entered.notified()).await?;

    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    client.context.attachment_runtime().set_pause(
        PausePoint::DownloadCompletion,
        Some((entered.clone(), resume.clone())),
    );
    let first_client = client.clone();
    let first_remote = published.clone();
    let first_download =
        tokio::spawn(async move { first_client.attachments().download(&first_remote).await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    assert!(
        client
            .context
            .db()
            .get_local_attachment(&plaintext_rel_path(&published)?)?
            .is_some()
    );

    failed_resume.notify_one();
    let early = tokio::time::timeout(Duration::from_millis(500), &mut second_download).await;
    let passed_before_completion = early.is_ok();
    resume.notify_one();
    tokio::time::timeout(Duration::from_secs(5), first_download).await???;
    let failure = match early {
        Ok(result) => result?.expect_err("forged download must fail"),
        Err(_) => tokio::time::timeout(Duration::from_secs(5), second_download)
            .await??
            .expect_err("forged download must fail"),
    };
    assert!(
        !passed_before_completion,
        "failure passed the first completion event"
    );
    let terminal: Vec<_> = events
        .drain()
        .into_iter()
        .filter_map(|event| match event.client {
            Some(ClientEvent::AttachmentDownloadCompleted(reference)) => {
                Some((reference.url, None))
            }
            Some(ClientEvent::AttachmentDownloadFailed(failed)) => {
                Some((failed.url, Some(failed.cause)))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        terminal,
        [
            (published.url.clone(), None),
            (failed.url.clone(), Some(failure.cause.as_str().to_owned())),
        ]
    );
}

// verifies: ATCH-051, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn failed_metadata_write_removes_downloaded_file() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_attachment_metadata BEFORE INSERT ON local_attachments \
                 BEGIN SELECT RAISE(ABORT, 'metadata rejected'); END",
        )
        .execute(conn)
    })?;
    let path = client.attachments().local_path(&remote)?;
    let error = client.attachments().download(&remote).await.unwrap_err();
    assert_eq!(error.cause, Cause::LocalStorage);
    assert!(!path.exists());
    assert!(client.context.db().list_local_attachments()?.is_empty());
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER reject_attachment_metadata").execute(conn)
    })?;
    let downloaded = client.attachments().download(&remote).await?;
    assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(tokio::fs::read(path).await?, b"attachment content");
}

// verifies: EVENT-001, ATCH-051
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn failed_metadata_write_with_kept_file_completes_download() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_attachment_metadata BEFORE INSERT ON local_attachments \
                 BEGIN SELECT RAISE(ABORT, 'metadata rejected'); END",
        )
        .execute(conn)
    })?;
    client
        .context
        .attachment_runtime()
        .fail_next(Fault::DownloadCleanup);
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentDownloadCompleted,
        EventKind::AttachmentDownloadFailed,
    ]))?;
    let path = client.attachments().local_path(&remote)?;
    let downloaded = client.attachments().download(&remote).await?;
    assert_eq!(downloaded.path, path);
    assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(downloaded.filename.as_deref(), Some("note.txt"));
    assert_eq!(tokio::fs::read(&path).await?, b"attachment content");
    let emitted = events.drain();
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        &emitted[0].client,
        Some(ClientEvent::AttachmentDownloadCompleted(_))
    ));
}

// verifies: ATCH-062, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn reconcile_waits_for_download_metadata_publication() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, _) = serve_body(body).await;
    remote.url = url;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let relative = plaintext_rel_path(&remote)?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    client.context.attachment_runtime().set_pause(
        PausePoint::DownloadPublish,
        Some((entered.clone(), resume.clone())),
    );
    let downloading_client = client.clone();
    let downloading_remote = remote.clone();
    let download = tokio::spawn(async move {
        downloading_client
            .attachments()
            .download(&downloading_remote)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    assert!(client.attachments().local_path(&remote)?.exists());
    assert!(
        client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_none()
    );
    let context = client.context.clone();
    let mut reconcile =
        tokio::spawn(async move { context.attachment_runtime().reconcile_again(&context).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut reconcile)
            .await
            .is_err()
    );
    resume.notify_one();
    let downloaded = tokio::time::timeout(Duration::from_secs(5), download).await???;
    tokio::time::timeout(Duration::from_secs(5), reconcile).await???;
    assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(downloaded.filename.as_deref(), Some("note.txt"));
    let record = client
        .context
        .db()
        .get_local_attachment(&relative)?
        .expect("record");
    assert_eq!(record.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(record.filename.as_deref(), Some("note.txt"));
}

// verifies: ATCH-062, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn failed_download_publication_cannot_leave_reconciled_row() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, _) = serve_body(body).await;
    remote.url = url;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let relative = plaintext_rel_path(&remote)?;
    let path = client.attachments().local_path(&remote)?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    client.context.attachment_runtime().set_pause(
        PausePoint::DownloadPublish,
        Some((entered.clone(), resume.clone())),
    );
    let downloading_client = client.clone();
    let downloading_remote = remote.clone();
    let download = tokio::spawn(async move {
        downloading_client
            .attachments()
            .download(&downloading_remote)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    let context = client.context.clone();
    let reconcile =
        tokio::spawn(async move { context.attachment_runtime().reconcile_again(&context).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_publication_metadata BEFORE INSERT ON local_attachments \
             BEGIN SELECT RAISE(ABORT, 'metadata rejected'); END",
        )
        .execute(conn)
    })?;
    resume.notify_one();
    let error = tokio::time::timeout(Duration::from_secs(5), download)
        .await??
        .unwrap_err();
    assert_eq!(error.cause, Cause::LocalStorage);
    tokio::time::timeout(Duration::from_secs(5), reconcile).await???;
    assert!(!path.exists());
    assert!(
        client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_none()
    );
}

// verifies: ATCH-045, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn another_client_publishes_plaintext_during_download() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, requests) = serve_body(body).await;
    remote.url = url;

    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let first = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let second = xmtp_mls::builder::ClientBuilder::from_client(first.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    first.context.attachment_runtime().set_pause(
        PausePoint::DownloadMove,
        Some((entered.clone(), resume.clone())),
    );
    let first_remote = remote.clone();
    let first_client = first.clone();
    let first_download =
        tokio::spawn(async move { first_client.attachments().download(&first_remote).await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    let published = second.attachments().download(&remote).await?;
    let first_bytes = tokio::fs::read(&published.path).await?;
    let relative = plaintext_rel_path(&remote)?;
    let row_before = first.context.db().get_local_attachment(&relative)?;
    resume.notify_one();
    let joined = tokio::time::timeout(Duration::from_secs(5), first_download).await???;
    assert_eq!(joined, published);
    assert_eq!(tokio::fs::read(&joined.path).await?, first_bytes);
    assert_eq!(
        first.context.db().get_local_attachment(&relative)?,
        row_before
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

// verifies: EVENT-001, ATCH-051
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn published_file_without_record_completes_download() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, _) = serve_body(body).await;
    remote.url = url;

    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let first = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let second = xmtp_mls::builder::ClientBuilder::from_client(first.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let events = first.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentDownloadCompleted,
        EventKind::AttachmentDownloadFailed,
    ]))?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    first.context.attachment_runtime().set_pause(
        PausePoint::DownloadMove,
        Some((entered.clone(), resume.clone())),
    );
    let first_remote = remote.clone();
    let first_client = first.clone();
    let first_download =
        tokio::spawn(async move { first_client.attachments().download(&first_remote).await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    let published = second.attachments().download(&remote).await?;
    let relative = plaintext_rel_path(&remote)?;
    first.context.db().delete_local_attachment(&relative)?;
    first.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER reject_attachment_metadata BEFORE INSERT ON local_attachments \
                 BEGIN SELECT RAISE(ABORT, 'metadata rejected'); END",
        )
        .execute(conn)
    })?;
    resume.notify_one();
    let joined = tokio::time::timeout(Duration::from_secs(5), first_download).await???;
    assert_eq!(joined, published);
    assert_eq!(tokio::fs::read(&joined.path).await?, b"attachment content");
    let emitted = events.drain();
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        &emitted[0].client,
        Some(ClientEvent::AttachmentDownloadCompleted(_))
    ));
}
