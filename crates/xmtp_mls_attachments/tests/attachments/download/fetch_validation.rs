//! Download tests: fetch validation.

use super::*;

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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
    let recipient_client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let forged_client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let restarted = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
