use super::*;

// verifies: ATCH-041, ATCH-042
#[xmtp_common::test(unwrap_try = true)]
async fn local_path_uses_remote_material_and_name_table() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let filename = "folder/ ..CON?.txt ";
    let pending = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"path proof".to_vec(),
            filename: Some(filename.into()),
            mime_type: "text/plain".into(),
        })
        .await?;
    let remote = pending.remote_attachment();
    assert_eq!(remote.filename.as_deref(), Some(filename));
    let digest = hex::decode(&remote.content_digest)?;
    let mut material = Vec::with_capacity(108);
    material.extend_from_slice(&digest);
    material.extend_from_slice(&remote.secret);
    material.extend_from_slice(&remote.salt);
    material.extend_from_slice(&remote.nonce);
    assert_eq!(material.len(), 108);
    let key = hex::encode(Sha256::digest(&material));
    let expected = dir.path().join(key).join("_CON.txt");
    assert_eq!(alix.client.attachments().local_path(remote)?, expected);
    assert_eq!(pending.local_path()?, expected);
    assert_eq!(tokio::fs::read(expected).await?, b"path proof");
}

// verifies: ATCH-008, ATCH-009, ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn offer_readable() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let offered = alix
        .client
        .server_configuration()
        .attachments
        .as_ref()
        .expect("offer");
    assert_eq!(offered.max_upload_bytes, 10_485_760);
    assert_eq!(offered.base_url, "http://localhost:5050/attachments");
    assert!(alix.client.attachments().offered());
}

// verifies: ATCH-043, ATCH-050, ATCH-051, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn plaintext_content_exact() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    tester!(bo, attachments_dir: recipient.path(), configured: |_config: &mut ServerConfiguration| {}, disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            |_config: &mut ServerConfiguration| {},
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(!client.attachments().offered());
    let downloaded = client.attachments().download(&remote).await?;
    assert_eq!(downloaded.path, client.attachments().local_path(&remote)?);
    assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(downloaded.filename.as_deref(), Some("note.txt"));
    assert_eq!(
        tokio::fs::read(&downloaded.path).await?,
        b"attachment content"
    );
    assert_eq!(client.attachments().list_local().await?.len(), 1);
}

// verifies: ATCH-052, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn existing_file_keeps_decoded_metadata() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"image bytes".to_vec(),
            filename: Some("photo.png".into()),
            mime_type: "image/png".into(),
        })
        .await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    for _ in 0..2 {
        let downloaded = client.attachments().download(&remote).await?;
        assert_eq!(downloaded.mime_type.as_deref(), Some("image/png"));
        assert_eq!(downloaded.filename.as_deref(), Some("photo.png"));
        assert_eq!(tokio::fs::read(&downloaded.path).await?, b"image bytes");
    }
}

// verifies: ATCH-047, ATCH-063
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn existing_file_record_cannot_race_another_clients_deletion() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let second = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    *second.context.attachments.download_existing_pause.lock() =
        Some((entered.clone(), resume.clone()));
    let downloading_client = second.clone();
    let downloading_remote = remote.clone();
    let downloading = tokio::spawn(async move {
        downloading_client
            .attachments()
            .download(&downloading_remote)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;

    let deleting_client = alix.client.clone();
    let deleting_remote = remote.clone();
    let mut deleting = tokio::spawn(async move {
        deleting_client
            .attachments()
            .delete_local(&deleting_remote)
            .await
    });
    let deleted_before_record =
        match tokio::time::timeout(Duration::from_millis(250), &mut deleting).await {
            Ok(result) => {
                result??;
                true
            }
            Err(_) => false,
        };
    resume.notify_one();
    let downloaded = tokio::time::timeout(Duration::from_secs(5), downloading).await???;
    if !deleted_before_record {
        tokio::time::timeout(Duration::from_secs(5), deleting).await???;
    }
    assert!(
        !deleted_before_record,
        "deletion crossed the existing-file record insert"
    );
    assert!(!downloaded.path.exists());
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_none()
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
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
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
    *client.context.attachments.download_publish_pause.lock() =
        Some((entered.clone(), resume.clone()));
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
    let mut reconcile = tokio::spawn(async move { context.attachments.reconcile(&context).await });
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
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
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
    *client.context.attachments.download_publish_pause.lock() =
        Some((entered.clone(), resume.clone()));
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
    let reconcile = tokio::spawn(async move { context.attachments.reconcile(&context).await });
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

// verifies: ATCH-044, ATCH-059
#[xmtp_common::test(unwrap_try = true)]
async fn local_path_no_io() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let path = alix.client.attachments().local_path(&remote)?;
    assert!(path.ends_with("note.txt"));
    remote.secret.pop();
    assert_eq!(
        alix.client
            .attachments()
            .local_path(&remote)
            .unwrap_err()
            .cause,
        Cause::Malformed
    );
    assert_eq!(
        alix.client
            .attachments()
            .download(&remote)
            .await
            .unwrap_err()
            .cause,
        Cause::Malformed
    );
}

// verifies: ATCH-045, ATCH-046, ATCH-052, ATCH-065
#[xmtp_common::test(unwrap_try = true)]
async fn existing_not_fetched() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let (url, requests) = serve_body(b"forged".to_vec()).await;
    remote.url = url;
    pending.upload().await?;
    let path = alix.client.attachments().download(&remote).await?.path;
    assert_eq!(tokio::fs::read(path).await?, b"attachment content");
    assert_eq!(requests.load(Ordering::SeqCst), 0);
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
    let first = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let second = crate::builder::ClientBuilder::from_client(first.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    *first.context.attachments.download_move_pause.lock() = Some((entered.clone(), resume.clone()));
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

// verifies: ATCH-052, ATCH-062
#[xmtp_common::test(unwrap_try = true)]
async fn directory_at_plaintext_path_is_not_a_download() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let relative = plaintext_rel_path(&remote)?;
    let path = alix.client.attachments().local_path(&remote)?;
    alix.client
        .context
        .db()
        .delete_local_attachment(&relative)?;
    tokio::fs::remove_file(&path).await?;
    tokio::fs::create_dir(&path).await?;
    let result = alix.client.attachments().download(&remote).await;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert!(
        alix.client
            .context
            .db()
            .get_local_attachment(&relative)?
            .is_none()
    );
    assert!(path.is_dir());
}

// verifies: ATCH-047, ATCH-058, EVENT-001
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn panicked_download_releases_waiters_and_deletion() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let relative = plaintext_rel_path(&remote)?;
    alix.client
        .context
        .db()
        .delete_local_attachment(&relative)?;
    tokio::fs::remove_file(alix.client.attachments().local_path(&remote)?).await?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadFailed,
        ]))?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.download_panic_pause.lock() =
        Some((entered.clone(), resume.clone()));
    let client = alix.client.clone();
    let first_remote = remote.clone();
    let first = tokio::spawn(async move { client.attachments().download(&first_remote).await });
    tokio::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    let shared = alix
        .client
        .context
        .attachments
        .downloads
        .lock()
        .get(&relative)
        .cloned()
        .expect("download attempt");
    let client = alix.client.clone();
    let second_remote = remote.clone();
    let second = tokio::spawn(async move { client.attachments().download(&second_remote).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while shared.outcome.receiver_count() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let client = alix.client.clone();
    let deleting_remote = remote.clone();
    let delete_entered = Arc::new(tokio::sync::Notify::new());
    let delete_resume = Arc::new(tokio::sync::Notify::new());
    *alix.client.context.attachments.delete_pause.lock() =
        Some((delete_entered.clone(), delete_resume.clone()));
    let delete =
        tokio::spawn(async move { client.attachments().delete_local(&deleting_remote).await });
    tokio::time::timeout(Duration::from_secs(3), delete_entered.notified()).await?;
    assert!(shared.cancel.is_cancelled());
    resume.notify_one();
    delete_resume.notify_one();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), first)
            .await??
            .unwrap_err()
            .cause,
        Cause::LocalStorage
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), second)
            .await??
            .unwrap_err()
            .cause,
        Cause::LocalStorage
    );
    tokio::time::timeout(Duration::from_secs(2), delete).await???;
    let emitted = events.drain();
    assert_eq!(emitted.len(), 2);
    assert!(matches!(
        emitted[0].client,
        Some(ClientEvent::AttachmentDownloadStarted(_))
    ));
    assert!(matches!(
        emitted[1].client,
        Some(ClientEvent::AttachmentDownloadFailed(_))
    ));
    assert!(
        !alix
            .client
            .context
            .attachments
            .downloads
            .lock()
            .contains_key(&relative)
    );
    let mut retry_remote = remote;
    retry_remote.url = "http://127.0.0.1:9/missing".into();
    let _ = tokio::time::timeout(
        Duration::from_secs(3),
        alix.client.attachments().download(&retry_remote),
    )
    .await?;
    assert!(matches!(
        events
            .drain()
            .first()
            .and_then(|event| event.client.as_ref()),
        Some(ClientEvent::AttachmentDownloadStarted(_))
    ));
}

// verifies: ATCH-051, ATCH-056, ATCH-060
#[xmtp_common::test(unwrap_try = true)]
async fn forged_body_leaves_nothing() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let (url, requests) = serve_body(b"forged".to_vec()).await;
    remote.url = url;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        client
            .attachments()
            .download(&remote)
            .await
            .unwrap_err()
            .cause,
        Cause::DigestMismatch
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert!(!client.attachments().local_path(&remote)?.exists());
    assert!(client.attachments().list_local().await?.is_empty());
}

// verifies: ATCH-079
#[xmtp_common::test(unwrap_try = true)]
async fn download_http_status_is_exposed() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    remote.url = format!("http://{}/file", listener.local_addr()?);
    drop(xmtp_common::task::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept GET");
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).await;
        stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.expect("send 503");
    }));
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let error = client.attachments().download(&remote).await.unwrap_err();
    assert_eq!(error.cause, Cause::HttpStatus);
    assert_eq!(error.http_status, Some(503));
}

// verifies: ATCH-058
#[xmtp_common::test(unwrap_try = true)]
async fn one_fetch_per_path() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let staged = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, requests) = serve_body(staged).await;
    remote.url = url;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let attachments = client.attachments();
    let (first, second) =
        tokio::join!(attachments.download(&remote), attachments.download(&remote));
    assert_eq!(first?.path, second?.path);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

// verifies: ATCH-047, ATCH-051, EVENT-055
#[xmtp_common::test(unwrap_try = true)]
async fn delete_cancels_running() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    remote.url = format!("http://{}/file", listener.local_addr()?);
    let (connected, ready) = tokio::sync::oneshot::channel();
    drop(xmtp_common::task::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\npartial")
            .await
            .unwrap();
        let _ = connected.send(());
        std::future::pending::<()>().await;
    }));
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    tokio::fs::create_dir_all(recipient.path().join(attachment_key(&remote)?)).await?;
    let attachments = client.attachments();
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentDownloadStarted,
        EventKind::AttachmentDownloadFailed,
        EventKind::AttachmentDeleted,
    ]))?;
    let running_client = client.clone();
    let running_remote = remote.clone();
    let download = xmtp_common::spawn(None, async move {
        running_client.attachments().download(&running_remote).await
    });
    tokio::time::timeout(Duration::from_secs(5), ready).await??;
    tokio::time::timeout(Duration::from_secs(5), attachments.delete_local(&remote)).await??;
    let outcome = tokio::time::timeout(Duration::from_secs(5), download.join()).await??;
    assert_eq!(outcome.unwrap_err().cause, Cause::Deleted);
    assert!(!attachments.local_path(&remote)?.exists());
    assert!(attachments.list_local().await?.is_empty());
    let emitted = events.drain();
    let key = attachment_key(&remote)?;
    assert_eq!(emitted.len(), 3);
    assert!(
        matches!(&emitted[0].client, Some(ClientEvent::AttachmentDownloadStarted(reference)) if reference.attachment_key == key)
    );
    assert!(
        matches!(&emitted[1].client, Some(ClientEvent::AttachmentDownloadFailed(failed)) if failed.attachment_key == key && failed.cause == "deleted")
    );
    assert!(
        matches!(&emitted[2].client, Some(ClientEvent::AttachmentDeleted(reference)) if reference.attachment_key == key)
    );
}

// verifies: ATCH-047
#[xmtp_common::test(unwrap_try = true)]
async fn delete_refuses_download_started_during_deletion() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, requests) = serve_body(body).await;
    remote.url = url;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    *client.context.attachments.delete_pause.lock() = Some((entered.clone(), resume.clone()));
    let deleting_client = client.clone();
    let deleting_remote = remote.clone();
    let deletion = xmtp_common::task::spawn(async move {
        deleting_client
            .attachments()
            .delete_local(&deleting_remote)
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    let download = tokio::time::timeout(
        Duration::from_secs(5),
        client.attachments().download(&remote),
    )
    .await?;
    resume.notify_one();
    tokio::time::timeout(Duration::from_secs(5), deletion).await???;
    assert_eq!(download.unwrap_err().cause, Cause::Deleted);
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    assert!(!client.attachments().local_path(&remote)?.exists());
}

// verifies: ATCH-059
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_never_fetched() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let (url, requests) = serve_body(b"body".to_vec()).await;
    for field in 0..4 {
        let mut remote = pending.remote_attachment().clone();
        remote.url = url.clone();
        match field {
            0 => remote.content_digest.make_ascii_uppercase(),
            1 => {
                remote.secret.pop();
            }
            2 => {
                remote.salt.pop();
            }
            _ => {
                remote.nonce.pop();
            }
        }
        assert_eq!(
            alix.client
                .attachments()
                .local_path(&remote)
                .unwrap_err()
                .cause,
            Cause::Malformed
        );
        assert_eq!(
            alix.client
                .attachments()
                .download(&remote)
                .await
                .unwrap_err()
                .cause,
            Cause::Malformed
        );
    }
    assert_eq!(requests.load(Ordering::SeqCst), 0);
}

// verifies: ATCH-056
#[xmtp_common::test(unwrap_try = true)]
async fn download_size_limit() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            max_download_bytes: Some(1),
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        client
            .attachments()
            .download(&remote)
            .await
            .unwrap_err()
            .cause,
        Cause::TooLarge
    );
    assert!(!client.attachments().local_path(&remote)?.exists());
}

// verifies: ATCH-065
#[xmtp_common::test(unwrap_try = true)]
async fn no_auto_download() {
    use xmtp_content_types::{ContentCodec, remote_attachment::RemoteAttachmentCodec};
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let (url, requests) = serve_body(b"ciphertext".to_vec()).await;
    let mut remote = pending.remote_attachment().clone();
    remote.url = url;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let received = bo.sync_welcomes().await?;
    let bo_group = received.first()?.clone();
    let encoded = RemoteAttachmentCodec::encode(remote.clone())?.encode_to_vec();
    group.send_message(&encoded, Default::default()).await?;
    bo_group.sync().await?;
    assert_eq!(bo_group.test_last_message_bytes().await??, encoded);
    xmtp_common::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(requests.load(Ordering::SeqCst), 0);
}

// verifies: ATCH-043, ATCH-051
#[xmtp_common::test(unwrap_try = true)]
async fn compressed_content_two_pass() {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write as _;
    use xmtp_proto::xmtp::mls::message_contents::{Compression as WireCompression, EncodedContent};
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let content = b"compressed attachment content";
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content)?;
    let mut envelope = EncodedContent::decode(encoded_prefix(None, "text/plain", 0).as_slice())?;
    envelope.compression = Some(WireCompression::Gzip as i32);
    envelope.content = encoder.finish()?;
    let material = KeyMaterial::random();
    let mut cipher = GcmEncryptor::new(&material);
    let mut body = Vec::new();
    cipher.update(&envelope.encode_to_vec(), &mut body)?;
    body.extend_from_slice(&cipher.finish());
    let digest = hex::encode(Sha256::digest(&body));
    let (url, _) = serve_body(body.clone()).await;
    let mut remote = remote_attachment(
        "http://localhost",
        &digest,
        &material,
        body.len() as u32,
        None,
    );
    remote.url = url;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let path = client.attachments().download(&remote).await?.path;
    assert_eq!(tokio::fs::read(path).await?, content);
}

// verifies: ATCH-051
#[xmtp_common::test(unwrap_try = true)]
async fn repeated_content_download_keeps_last_field() {
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let mut envelope = EncodedContent::decode(encoded_prefix(None, "text/plain", 0).as_slice())?;
    envelope.content = vec![b'f'; CHUNK * 2];
    let mut plaintext = envelope.encode_to_vec();
    plaintext.extend_from_slice(b"\x22\x04last");
    let material = KeyMaterial::random();
    let mut cipher = GcmEncryptor::new(&material);
    let mut body = Vec::new();
    cipher.update(&plaintext, &mut body)?;
    body.extend_from_slice(&cipher.finish());
    let digest = hex::encode(Sha256::digest(&body));
    let (url, _) = serve_body(body.clone()).await;
    let mut remote = remote_attachment(
        "http://localhost",
        &digest,
        &material,
        body.len() as u32,
        None,
    );
    remote.url = url;
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let path = client.attachments().download(&remote).await?.path;
    assert!(
        tokio::fs::read(path).await? == b"last",
        "decoder kept earlier content"
    );
}

// verifies: ATCH-025, ATCH-038, ATCH-047, ATCH-050, ATCH-051
// Covers plan P23.
#[xmtp_common::test(unwrap_try = true)]
async fn s3_end_to_end() {
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, attachments_dir: sender.path().join("attachments"), disable_workers);
    let source = sender.path().join("source.txt");
    tokio::fs::write(&source, b"from path").await?;
    let path_pending = alix
        .client
        .attachments()
        .create(AttachmentSource::Path {
            path: source,
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    let path_remote = path_pending.remote_attachment().clone();
    let bytes_pending = alix.client.attachments().create(bytes()).await?;
    let bytes_remote = bytes_pending.remote_attachment().clone();
    let staged = sender
        .path()
        .join("attachments")
        .join(staged_path(&bytes_remote.content_digest)?);
    let ciphertext = tokio::fs::read(&staged).await?;
    path_pending.upload().await?;
    bytes_pending.upload().await?;
    tokio::fs::write(&staged, ciphertext).await?;
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "UPDATE pending_attachments SET status = 'waiting' WHERE content_digest = ?",
        )
        .bind::<xmtp_db::diesel::sql_types::Text, _>(&bytes_remote.content_digest)
        .execute(conn)
    })?;
    let repeated = alix.client.attachments().pending(&bytes_remote).await?;
    repeated.upload().await?;
    assert_eq!(repeated.status(), PendingAttachmentStatus::Complete);
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let recipient_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        tokio::fs::read(
            recipient_client
                .attachments()
                .download(&path_remote)
                .await?
                .path
        )
        .await?,
        b"from path"
    );
    assert_eq!(
        tokio::fs::read(
            recipient_client
                .attachments()
                .download(&bytes_remote)
                .await?
                .path
        )
        .await?,
        b"attachment content"
    );
    let (url, _) = serve_body(b"forged".to_vec()).await;
    let mut forged = bytes_remote.clone();
    forged.url = url;
    let forged_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
        .attachments_dir(recipient.path().join("forged"))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        forged_client
            .attachments()
            .download(&forged)
            .await
            .unwrap_err()
            .cause,
        Cause::DigestMismatch
    );
    let restart_pending = alix.client.attachments().create(bytes()).await?;
    let restart_remote = restart_pending.remote_attachment().clone();
    let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(
        restarted
            .attachments()
            .list_pending()
            .await?
            .iter()
            .any(|p| p.remote_attachment().content_digest == restart_remote.content_digest)
    );
    restarted
        .attachments()
        .pending(&restart_remote)
        .await?
        .upload()
        .await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut running = restart_remote;
    running.url = format!("http://{}/running", listener.local_addr()?);
    let (connected, ready) = tokio::sync::oneshot::channel();
    drop(xmtp_common::task::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\npartial")
            .await
            .unwrap();
        let _ = connected.send(());
        std::future::pending::<()>().await;
    }));
    let attachments = forged_client.attachments();
    let running_client = forged_client.clone();
    let running_remote = running.clone();
    let download = xmtp_common::spawn(None, async move {
        running_client.attachments().download(&running_remote).await
    });
    tokio::time::timeout(Duration::from_secs(5), ready).await??;
    tokio::time::timeout(Duration::from_secs(5), attachments.delete_local(&running)).await??;
    let outcome = tokio::time::timeout(Duration::from_secs(5), download.join()).await??;
    assert_eq!(outcome.unwrap_err().cause, Cause::Deleted);
}
