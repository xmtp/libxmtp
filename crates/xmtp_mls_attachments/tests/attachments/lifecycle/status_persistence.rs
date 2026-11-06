//! Upload lifecycle tests: status persistence.

use super::*;

// verifies: ATCH-029, ATCH-066
#[xmtp_common::test(unwrap_try = true)]
async fn failed_status_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let mut rejected = xmtp_api_backend::MockBackendClient::new();
    rejected.expect_create_upload().times(1).returning(|_| {
        Err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument("permanent")),
        ))
    });
    let first = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(rejected))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        first
            .attachments()
            .pending(&remote)
            .await?
            .upload()
            .await
            .unwrap_err()
            .cause,
        Cause::BackendRejected
    );
    drop(first);
    drop(created);
    let mut no_request = xmtp_api_backend::MockBackendClient::new();
    no_request.expect_create_upload().times(0);
    let restarted = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(no_request))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = restarted.attachments().pending(&remote).await?;
    assert!(
        matches!(pending.status(), PendingAttachmentStatus::Failed(error) if error.cause == Cause::BackendRejected)
    );
    assert_eq!(
        pending.upload().await.unwrap_err().cause,
        Cause::BackendRejected
    );
}

// verifies: ATCH-037, ATCH-038, ATCH-067
#[xmtp_common::test(unwrap_try = true)]
async fn complete_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let staged = dir.path().join(staged_path(&remote.content_digest)?);
    pending.upload().await?;
    assert!(!staged.exists());
    let mut no_request = xmtp_api_backend::MockBackendClient::new();
    no_request.expect_create_upload().times(0);
    let restarted = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(no_request))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let resumed = restarted.attachments().pending(&remote).await?;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Complete);
    resumed.upload().await?;
    assert!(restarted.attachments().list_pending().await?.is_empty());
    assert_eq!(
        restarted
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
}

// verifies: ATCH-029
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_record_is_not_touched() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let token = [9u8; 16];
    let db = alix.client.context.db();
    db.claim_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        LEASE_DURATION.as_nanos() as i64,
    )?;
    db.finish_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        PendingAttachmentOutcome::failed("backend_rejected", None, Some(false)),
    )?;
    let before = db.get_pending_attachment(&remote.content_digest)?.unwrap();
    let mut no_request = xmtp_api_backend::MockBackendClient::new();
    no_request.expect_create_upload().times(0);
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(no_request))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadFailed,
    ]))?;
    client
        .context
        .server_configuration
        .block_connection(BlockedConnection::BackendMismatch {
            stored: "one".into(),
            received: "two".into(),
        });
    let pending = client.attachments().pending(&remote).await?;
    assert_eq!(
        pending.upload().await.unwrap_err().cause,
        Cause::BackendRejected
    );
    assert_eq!(
        db.get_pending_attachment(&remote.content_digest)?.unwrap(),
        before
    );
    assert!(events.drain().is_empty());
}

// verifies: ATCH-061
#[xmtp_common::test(unwrap_try = true)]
async fn credential_failure_kind_persists() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let token = [8u8; 16];
    let db = alix.client.context.db();
    db.claim_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        LEASE_DURATION.as_nanos() as i64,
    )?;
    db.finish_pending_attachment(
        &remote.content_digest,
        &token,
        now_ns(),
        PendingAttachmentOutcome::failed("credential", Some("callback_failed"), Some(true)),
    )?;
    let restarted = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let status = restarted.attachments().pending(&remote).await?.status();
    assert!(matches!(
        status,
        PendingAttachmentStatus::Failed(AttachmentClientError {
            cause: Cause::Credential,
            credential_kind: Some(CredentialFailureKind::CallbackFailed),
            retryable: true,
            ..
        })
    ));
}

struct FailingCredential(Arc<AtomicUsize>);

#[xmtp_common::async_trait]
impl xmtp_api_backend::AuthCallback for FailingCredential {
    async fn on_auth_required(
        &self,
    ) -> Result<xmtp_api_backend::Credential, xmtp_common::BoxDynError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(std::io::Error::other("attachment credential callback failed").into())
    }
}

// verifies: ATCH-061, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn real_credential_failure_kind_survives_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let api = xmtp_api_backend::MessageBackendBuilder::new()
        .host(xmtp_configuration::backend_test_url())
        .maybe_auth_callback(Some(Arc::new(FailingCredential(calls.clone()))))
        .build()?;
    let first = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(api)
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let first_error = first
        .attachments()
        .pending(&remote)
        .await?
        .upload()
        .await
        .unwrap_err();
    assert!(calls.load(Ordering::SeqCst) > 0);
    assert_eq!(first_error.cause, Cause::Credential);
    assert_eq!(
        first_error.credential_kind,
        Some(CredentialFailureKind::CallbackFailed)
    );
    assert!(first_error.retryable);
    drop(first);
    let restarted = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    assert_eq!(
        restarted.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Failed(first_error)
    );
}

// verifies: ATCH-035, ATCH-066
#[xmtp_common::test(unwrap_try = true)]
async fn pending_persist_restart() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let next = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .build()
        .await?;
    let resumed = next.attachments().pending(&remote).await?;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Waiting);
}
