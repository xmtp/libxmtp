//! Upload tests: transfer.

use super::*;

struct KnownCredential(Arc<AtomicUsize>);
#[xmtp_common::async_trait]
impl xmtp_api_backend::AuthCallback for KnownCredential {
    async fn on_auth_required(
        &self,
    ) -> Result<xmtp_api_backend::Credential, xmtp_common::BoxDynError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(xmtp_api_backend::Credential::new(
            None,
            "Bearer attachment-credential-unique-0123456789".parse()?,
            i64::MAX,
        ))
    }
}

// verifies: ATCH-027
#[xmtp_common::test(unwrap_try = true)]
async fn no_backend_credentials_on_storage_requests() {
    use xmtp_backend::test_support::TestServer;
    use xmtp_proto::backend_v1::CreateUploadResponse;
    let backend = TestServer::from_toml(
        "[auth]\nenabled = true\n[auth.api_keys]\nci = 'attachment-credential-unique-0123456789'",
    )
    .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, backend_url: backend.url.as_str(), auth: Arc::new(KnownCredential(calls.clone())), attachments_dir: sender.path(), configured: offer, disable_workers);
    assert!(calls.load(Ordering::SeqCst) > 0);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let body = tokio::fs::read(sender.path().join(staged_path(&remote.content_digest)?)).await?;
    let (url, captured) = capture_transfers(body).await;
    let mut mock = xmtp_api_backend::MockBackendClient::new();
    let put_url = url.clone();
    mock.expect_create_upload().times(1).returning(move |_| {
        Ok(CreateUploadResponse {
            method: "PUT".into(),
            url: put_url.clone(),
            headers: vec![],
            expires_in_seconds: 3600,
        })
    });
    let sender_client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(mock))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    sender_client
        .attachments()
        .pending(&remote)
        .await?
        .upload()
        .await?;
    tester!(bo, attachments_dir: recipient.path(), disable_workers);
    let recipient_client = xmtp_mls::builder::ClientBuilder::from_client(bo.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let mut remote = remote;
    remote.url = url;
    recipient_client.attachments().download(&remote).await?;
    let requests = captured.lock();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("PUT "));
    assert!(requests[1].starts_with("GET "));
    let inbox = alix.inbox_id().to_string().to_ascii_lowercase();
    let installation = hex::encode(alix.client.context.installation_id());
    for request in requests.iter() {
        let lower = request.to_ascii_lowercase();
        for forbidden in [
            "attachment-credential-unique-0123456789",
            "authorization:",
            "cookie:",
            "referer:",
            inbox.as_str(),
            installation.as_str(),
        ] {
            assert!(
                !lower.contains(forbidden),
                "storage request carried client identity"
            );
        }
    }
    let downloader_request = requests[1].to_ascii_lowercase();
    assert!(!downloader_request.contains(&bo.inbox_id().to_string().to_ascii_lowercase()));
    assert!(!downloader_request.contains(&hex::encode(bo.client.context.installation_id())));
}

// verifies: ATCH-010, ATCH-030, ATCH-031, ATCH-011, ATCH-012, ATCH-049
#[xmtp_common::test(unwrap_try = true)]
async fn remote_attachment_before_request() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment();
    assert_eq!(
        remote.url,
        format!(
            "http://localhost:5050/attachments/{}",
            remote.content_digest
        )
    );
    assert_eq!(remote.scheme, "http://");
    assert_eq!(
        remote.content_length,
        Some(ciphertext_len(encoded_prefix(Some("note.txt"), "text/plain", 18).len(), 18) as u32)
    );
    assert_eq!(remote.filename.as_deref(), Some("note.txt"));
    let staged_relative = staged_path(&remote.content_digest)?;
    let staged_file = dir.path().join(&staged_relative);
    assert_eq!(
        staged_file.parent(),
        Some(dir.path().join(".staged").as_path())
    );
    assert_eq!(staged_relative.split('/').count(), 2);
    assert!(!staged_file.starts_with(dir.path().join(attachment_key(remote)?)));
    let staged = tokio::fs::read(&staged_file).await?;
    assert_eq!(hex::encode(Sha256::digest(&staged)), remote.content_digest);
    assert_eq!(remote.content_length, Some(staged.len() as u32));
    let material = KeyMaterial::from_remote(remote)?;
    let mut decrypted = Vec::new();
    let mut decryptor = GcmDecryptor::new(&material);
    decryptor.update(&staged, &mut decrypted)?;
    decryptor.finish()?;
    let mut encoded = encoded_prefix(Some("note.txt"), "text/plain", 18);
    encoded.extend_from_slice(b"attachment content");
    assert_eq!(decrypted, encoded);
    assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
    assert_eq!(
        tokio::fs::read(pending.local_path()?).await?,
        b"attachment content"
    );
    let unnamed = alix
        .client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"unnamed".to_vec(),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    assert_eq!(unnamed.remote_attachment().filename, None);
    let unnamed_remote = unnamed.remote_attachment();
    let unnamed_staged = tokio::fs::read(
        dir.path()
            .join(staged_path(&unnamed_remote.content_digest)?),
    )
    .await?;
    let unnamed_material = KeyMaterial::from_remote(unnamed_remote)?;
    let mut unnamed_plaintext = Vec::new();
    let mut unnamed_decryptor = GcmDecryptor::new(&unnamed_material);
    unnamed_decryptor.update(&unnamed_staged, &mut unnamed_plaintext)?;
    unnamed_decryptor.finish()?;
    let unnamed_encoded = xmtp_proto::xmtp::mls::message_contents::EncodedContent::decode(
        unnamed_plaintext.as_slice(),
    )?;
    assert!(!unnamed_encoded.parameters.contains_key("filename"));
    assert_eq!(unnamed_encoded.content, b"unnamed");
}

// verifies: ATCH-026, ATCH-034
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_connection_upload_fails() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut mock = xmtp_api_backend::MockBackendClient::new();
    mock.expect_create_upload().times(0);
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(mock))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client
        .attachments()
        .pending(pending.remote_attachment())
        .await?;
    client
        .context
        .server_configuration
        .block_connection(BlockedConnection::BackendMismatch {
            stored: "one".into(),
            received: "two".into(),
        });
    assert_eq!(
        pending.upload().await.unwrap_err().cause,
        Cause::ConnectionBlocked
    );
}

// verifies: ATCH-029, ATCH-034
#[xmtp_common::test(unwrap_try = true)]
async fn permanent_rejection_not_resent() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    drop(pending);
    let mut mock = xmtp_api_backend::MockBackendClient::new();
    mock.expect_create_upload().times(1).returning(|_| {
        Err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("permanent")),
        ))
    });
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(mock))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let first_error = pending.upload().await.unwrap_err();
    assert_eq!(first_error.cause, Cause::BackendRejected);
    drop(pending);
    let pending = client.attachments().pending(&remote).await?;
    assert!(matches!(
        pending.status(),
        PendingAttachmentStatus::Failed(AttachmentClientError {
            cause: Cause::BackendRejected,
            ..
        })
    ));
    assert_eq!(pending.upload().await.unwrap_err(), first_error);
}

// verifies: ATCH-078
#[xmtp_common::test(unwrap_try = true)]
async fn permission_denied_is_missing_scope_credential() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let mut denied = xmtp_api_backend::MockBackendClient::new();
    denied.expect_create_upload().times(1).returning(|_| {
        Err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::permission_denied(
                "missing attachment scope",
            )),
        ))
    });
    let first = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(denied))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let error = first
        .attachments()
        .pending(&remote)
        .await?
        .upload()
        .await
        .unwrap_err();
    assert_eq!(error.cause, Cause::Credential);
    assert!(error.missing_scope);
    assert!(!error.retryable);
    let row = alix
        .client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap();
    assert_eq!(row.status, "failed");
    assert_eq!(row.failure_cause.as_deref(), Some("credential"));
    assert_eq!(row.failure_missing_scope, Some(true));
    drop(first);
    let renewed = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_disable_workers(true)
        .build()
        .await?;
    let resumed = renewed.attachments().pending(&remote).await?;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Failed(error));
    resumed.upload().await?;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Complete);
}

// verifies: ATCH-079
#[xmtp_common::test(unwrap_try = true)]
async fn http_status_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(403).await;
    let first = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = first.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    release.send(()).expect("release PUT response");
    let error = tokio::time::timeout(Duration::from_secs(10), upload)
        .await??
        .unwrap_err();
    assert_eq!(error.cause, Cause::TargetRejected);
    assert_eq!(error.http_status, Some(403));
    let row = alix
        .client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap();
    assert_eq!(row.failure_http_status, Some(403));
    drop(first);
    let restarted = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        restarted.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Failed(error)
    );
}

// verifies: ATCH-079
#[xmtp_common::test(unwrap_try = true)]
async fn put_status_999_is_recorded() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(999).await;
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    release.send(()).expect("release PUT response");
    let error = tokio::time::timeout(Duration::from_secs(5), upload)
        .await??
        .unwrap_err();
    assert_eq!(error.cause, Cause::TargetRejected);
    assert_eq!(error.http_status, Some(999));
    let row = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap();
    assert_eq!(row.status, "failed");
    assert_eq!(row.failure_http_status, Some(999));
}

// verifies: ATCH-024, ATCH-025, ATCH-037, ATCH-034, EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn put_as_signed() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let path = dir
        .path()
        .join(staged_path(&pending.remote_attachment().content_digest)?);
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
        ]))?;
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
    assert!(!path.exists());
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    let emitted = events.drain();
    assert_eq!(emitted.len(), 2);
    let key = pending.reference_for_test().attachment_key;
    assert!(matches!(
        &emitted[0].client,
        Some(ClientEvent::AttachmentUploadStarted(reference)) if reference.attachment_key == key
    ));
    assert!(matches!(
        &emitted[1].client,
        Some(ClientEvent::AttachmentUploadCompleted(reference)) if reference.attachment_key == key
    ));
}

// verifies: ATCH-025, ATCH-034
#[xmtp_common::test(unwrap_try = true)]
async fn upload_table() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
        ]))?;
    let (first, second) = tokio::join!(pending.upload(), pending.upload());
    first?;
    second?;
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
    assert_eq!(events.drain().len(), 2);
}

// verifies: ATCH-025, ATCH-037
#[xmtp_common::test(unwrap_try = true)]
async fn complete_releases_staged() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let staged = dir
        .path()
        .join(staged_path(&pending.remote_attachment().content_digest)?);
    assert!(staged.exists());
    pending.upload().await?;
    assert!(!staged.exists());
    assert!(alix.client.attachments().list_pending().await?.is_empty());
}

// verifies: ATCH-025
#[xmtp_common::test(unwrap_try = true)]
async fn already_stored_is_complete() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let staged = dir.path().join(staged_path(&remote.content_digest)?);
    let ciphertext = tokio::fs::read(&staged).await?;
    pending.upload().await?;
    tokio::fs::write(&staged, ciphertext).await?;
    // Model a client that stopped after the target stored the object but
    // before it recorded the successful outcome.
    alix.client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "UPDATE pending_attachments SET status = 'waiting' WHERE content_digest = ?",
        )
        .bind::<xmtp_db::diesel::sql_types::Text, _>(&remote.content_digest)
        .execute(conn)
    })?;
    let second = alix.client.attachments().pending(&remote).await?;
    second.upload().await?;
    assert_eq!(second.status(), PendingAttachmentStatus::Complete);
}
