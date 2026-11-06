//! Download tests: local files.

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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
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
    let second = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
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
    second.context.attachment_runtime().set_pause(
        PausePoint::DownloadExisting,
        Some((entered.clone(), resume.clone())),
    );
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
