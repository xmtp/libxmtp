//! Download tests: cancellation.

use super::*;

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
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::DownloadPanic,
        Some((entered.clone(), resume.clone())),
    );
    let client = alix.client.clone();
    let first_remote = remote.clone();
    let first = tokio::spawn(async move { client.attachments().download(&first_remote).await });
    tokio::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    let shared = alix
        .client
        .context
        .attachment_runtime()
        .running_download(&relative)
        .expect("download attempt");
    let client = alix.client.clone();
    let second_remote = remote.clone();
    let second = tokio::spawn(async move { client.attachments().download(&second_remote).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while shared.waiters() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let client = alix.client.clone();
    let deleting_remote = remote.clone();
    let delete_entered = Arc::new(tokio::sync::Notify::new());
    let delete_resume = Arc::new(tokio::sync::Notify::new());
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::Delete,
        Some((delete_entered.clone(), delete_resume.clone())),
    );
    let delete =
        tokio::spawn(async move { client.attachments().delete_local(&deleting_remote).await });
    tokio::time::timeout(Duration::from_secs(3), delete_entered.notified()).await?;
    assert!(shared.is_cancelled());
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
            .attachment_runtime()
            .running_download(&relative)
            .is_some()
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

// verifies: ATCH-047, ATCH-051, EVENT-057
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    client
        .context
        .attachment_runtime()
        .set_pause(PausePoint::Delete, Some((entered.clone(), resume.clone())));
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
