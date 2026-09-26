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
    use crate::utils::test::backend::EphemeralBackend;
    use xmtp_proto::backend_v1::CreateUploadResponse;
    let backend = EphemeralBackend::start(
        "[auth]\nenabled = true\n[auth.api_keys]\nci = 'attachment-credential-unique-0123456789'",
    )
    .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let sender = tempfile::tempdir()?;
    let recipient = tempfile::tempdir()?;
    tester!(alix, backend: &backend, auth: Arc::new(KnownCredential(calls.clone())), attachments_dir: sender.path(), configured: offer, disable_workers);
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
    let sender_client = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let recipient_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
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

// verifies: ATCH-030, ATCH-033
#[xmtp_common::test(unwrap_try = true)]
async fn create_preconditions() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: |_configuration: &mut ServerConfiguration| {}, disable_workers);
    let error = alix
        .client
        .attachments()
        .create(bytes())
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.cause, Cause::NotOffered);
    assert!(!alix.client.attachments().offered());
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    let dir2 = tempfile::tempdir()?;
    tester!(bo, attachments_dir: dir2.path(), configured: |configuration: &mut ServerConfiguration| {
            offer(configuration);
            configuration.attachments.as_mut().unwrap().max_upload_bytes = 10;
        }, disable_workers);
    let error = bo
        .client
        .attachments()
        .create(bytes())
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.cause, Cause::TooLarge);
    assert!(bo.client.attachments().list_pending().await?.is_empty());
}

// verifies: ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn exact_upload_limit_is_allowed() {
    let dir = tempfile::tempdir()?;
    let limit = ciphertext_len(encoded_prefix(Some("note.txt"), "text/plain", 18).len(), 18);
    tester!(alix, attachments_dir: dir.path(), configured: move |configuration: &mut ServerConfiguration| {
            offer(configuration);
            configuration.attachments.as_mut().unwrap().max_upload_bytes = limit;
        }, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    assert_eq!(
        pending.remote_attachment().content_length,
        Some(limit as u32)
    );
}

// verifies: ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn create_retained_fields_limit() {
    use xmtp_content_types::{
        ContentCodec as _,
        attachment::{Attachment, AttachmentCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    let retained_len = |filename: Option<&str>, mime_type: &str| {
        let encoded = AttachmentCodec::encode(Attachment {
            filename: filename.map(str::to_owned),
            mime_type: mime_type.to_owned(),
            content: Vec::new(),
        })
        .expect("attachment encoding has no failure path");
        EncodedContent {
            r#type: encoded.r#type,
            parameters: encoded.parameters,
            compression: encoded.compression,
            ..Default::default()
        }
        .encoded_len()
    };
    let mut low: usize = 0;
    let mut high: usize = 65_536;
    while low < high {
        let middle = (low + high).div_ceil(2);
        if retained_len(Some(&"f".repeat(middle)), "text/plain") <= 65_536 {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let filename = "f".repeat(low);
    assert_eq!(retained_len(Some(&filename), "text/plain"), 65_536);
    assert_eq!(
        retained_len(Some(&format!("{filename}f")), "text/plain"),
        65_537
    );

    let allowed_dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: allowed_dir.path(), configured: offer, disable_workers);
    alix.client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"x".to_vec(),
            filename: Some(filename.clone()),
            mime_type: "text/plain".into(),
        })
        .await?;

    let denied_dir = tempfile::tempdir()?;
    tester!(bo, attachments_dir: denied_dir.path(), configured: offer, disable_workers);
    for (filename, mime_type) in [
        (Some(format!("{filename}f")), "text/plain".to_owned()),
        (None, "m".repeat(65_536)),
    ] {
        let error = bo
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"x".to_vec(),
                filename,
                mime_type,
            })
            .await
            .err()
            .expect("oversized retained fields must fail");
        assert_eq!(error.cause, Cause::TooLarge);
        assert!(bo.client.attachments().list_pending().await?.is_empty());
        assert!(bo.client.attachments().list_local().await?.is_empty());
        assert!(
            bo.client
                .attachments()
                .runtime()
                .store()?
                .list_files()
                .await?
                .is_empty()
        );
    }
}

// verifies: ATCH-030
#[xmtp_common::test(unwrap_try = true)]
async fn empty_retained_values_use_decoder_limit() {
    let allowed_dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: allowed_dir.path(), configured: offer, disable_workers);
    let denied_dir = tempfile::tempdir()?;
    tester!(bo, attachments_dir: denied_dir.path(), configured: offer, disable_workers);

    for empty_mime in [false, true] {
        let fields = |length: usize| {
            if empty_mime {
                (Some("f".repeat(length)), String::new())
            } else {
                (Some(String::new()), "m".repeat(length))
            }
        };
        let fits = |length| {
            let (filename, mime_type) = fields(length);
            AttachmentDecoder::new()
                .push(&encoded_prefix(filename.as_deref(), &mime_type, 0))
                .is_ok()
        };
        let mut low = 0usize;
        let mut high = 65_536usize;
        while low < high {
            let middle = (low + high).div_ceil(2);
            if fits(middle) {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        assert!(fits(low));
        assert!(!fits(low + 1));

        let (filename, mime_type) = fields(low);
        let pending = alix
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"x".to_vec(),
                filename: filename.clone(),
                mime_type: mime_type.clone(),
            })
            .await?;
        let remote = pending.remote_attachment();
        let staged = tokio::fs::read(
            allowed_dir
                .path()
                .join(staged_path(&remote.content_digest)?),
        )
        .await?;
        let mut decrypted = Vec::new();
        let mut decryptor = GcmDecryptor::new(&KeyMaterial::from_remote(remote)?);
        decryptor.update(&staged, &mut decrypted)?;
        decryptor.finish()?;
        let mut decoder = AttachmentDecoder::new();
        let mut content = Vec::new();
        for bytes in decrypted.chunks(8192) {
            for event in decoder.push(bytes)? {
                match event {
                    xmtp_attachments::ContentChunk::Reset => content.clear(),
                    xmtp_attachments::ContentChunk::Bytes(bytes) => {
                        content.extend_from_slice(bytes);
                    }
                }
            }
        }
        let decoded = decoder.finish(&mut std::io::Cursor::new(&content), &mut Vec::new())?;
        assert_eq!(decoded.filename, filename);
        assert_eq!(decoded.mime_type, mime_type);
        assert_eq!(content, b"x");

        let (filename, mime_type) = fields(low + 1);
        let error = bo
            .client
            .attachments()
            .create(AttachmentSource::Bytes {
                bytes: b"x".to_vec(),
                filename,
                mime_type,
            })
            .await
            .err()
            .expect("oversized retained fields must fail");
        assert_eq!(error.cause, Cause::TooLarge);
        assert!(bo.client.attachments().list_pending().await?.is_empty());
        assert!(bo.client.attachments().list_local().await?.is_empty());
        assert!(
            bo.client
                .attachments()
                .runtime()
                .store()?
                .list_files()
                .await?
                .is_empty()
        );
    }
}

// verifies: ATCH-032, ATCH-011
#[xmtp_common::test(unwrap_try = true)]
async fn source_moved_after_create() {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source.txt");
    tokio::fs::write(&source, b"saved source").await?;
    tester!(alix, attachments_dir: dir.path().join("attachments"), disable_workers);
    let pending = alix
        .client
        .attachments()
        .create(AttachmentSource::Path {
            path: source.clone(),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    tokio::fs::rename(source, dir.path().join("moved.txt")).await?;
    assert_eq!(
        pending.remote_attachment().filename.as_deref(),
        Some("source.txt")
    );
    assert_eq!(
        tokio::fs::read(pending.local_path()?).await?,
        b"saved source"
    );
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
}

// verifies: ATCH-033, ATCH-060
#[xmtp_common::test(unwrap_try = true)]
async fn failed_create_leaves_nothing() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let error = alix
        .client
        .attachments()
        .create(AttachmentSource::Path {
            path: dir.path().join("absent"),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.cause, Cause::SourceUnreadable);
    assert!(alix.client.attachments().list_pending().await?.is_empty());
    assert!(
        tokio::fs::read_dir(dir.path())
            .await?
            .next_entry()
            .await?
            .is_none()
    );
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
    assert!(Arc::ptr_eq(&first.shared, &second.shared));
    assert_eq!(alix.client.attachments().list_pending().await?.len(), 1);
}

// verifies: ATCH-036, ATCH-034, EVENT-055
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
    let key = pending.reference().attachment_key;
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

// verifies: ATCH-026, ATCH-034
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_connection_upload_fails() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let mut mock = xmtp_api_backend::MockBackendClient::new();
    mock.expect_create_upload().times(0);
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
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

// verifies: ATCH-060, ATCH-061
#[xmtp_common::test(unwrap_try = true)]
fn causes_map() {
    use xmtp_proto::api::{ApiClientError, AuthError};
    let credential = api_error(xmtp_api::ApiError::Auth(AuthError::CredentialRejected {
        retryable: true,
    }));
    assert_eq!(credential.cause, Cause::Credential);
    assert_eq!(
        credential.credential_kind,
        Some(CredentialFailureKind::CredentialRejected)
    );
    assert!(credential.retryable);
    let no_credential = api_error(xmtp_api::ApiError::Auth(AuthError::MissingCredential));
    assert_eq!(
        no_credential.credential_kind,
        Some(CredentialFailureKind::MissingCredential)
    );
    assert!(!no_credential.retryable);
    for code in [
        tonic::Code::InvalidArgument,
        tonic::Code::OutOfRange,
        tonic::Code::Unimplemented,
    ] {
        let network = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
            tonic::Status::new(code, "rejected"),
        ));
        assert_eq!(
            api_error(xmtp_api::dyn_err(network)).cause,
            Cause::BackendRejected
        );
    }
    let unavailable = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
        tonic::Status::unavailable("offline"),
    ));
    assert_eq!(
        api_error(xmtp_api::dyn_err(unavailable)).cause,
        Cause::BackendUnavailable
    );
    let target = AttachmentClientError::from(AttachmentError::new(Cause::TargetRejected));
    assert_eq!(target.cause, Cause::TargetRejected);
}

// verifies: ATCH-061
#[xmtp_common::test(unwrap_try = true)]
fn credential_failure_kind_strings_round_trip() {
    fn index(kind: CredentialFailureKind) -> usize {
        match kind {
            CredentialFailureKind::CredentialRejected => 0,
            CredentialFailureKind::CallbackFailed => 1,
            CredentialFailureKind::Exhausted => 2,
            CredentialFailureKind::MissingCredential => 3,
        }
    }
    for (expected, kind) in CredentialFailureKind::ALL.into_iter().enumerate() {
        assert_eq!(index(kind), expected);
        assert_eq!(CredentialFailureKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(CredentialFailureKind::parse("unknown"), None);
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
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let first = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let renewed = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let first = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let restarted = crate::builder::ClientBuilder::from_client(alix.client.clone())
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
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
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

// verifies: ATCH-024, ATCH-025, ATCH-037, ATCH-034, EVENT-055
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
    let key = pending.reference().attachment_key;
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
