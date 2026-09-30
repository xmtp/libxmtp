use super::*;

use std::sync::atomic::AtomicUsize;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::{
    AttachmentFailure, AttachmentFailureCause, AttachmentOptions, AttachmentSource,
    CredentialFailureKind, ErrorCategory, MessageContent, PendingAttachmentStatus,
    RemoteAttachment,
};

/// A client whose files live under `root`, allowed to reach the loopback
/// storage target of the test stack.
fn file_options(root: &std::path::Path) -> ClientOptions {
    let mut settings = options();
    settings.storage.location = StorageLocation::Directory {
        directory: root.to_string_lossy().into_owned(),
    };
    settings.attachments = Some(AttachmentOptions {
        allow_private_network: true,
        ..Default::default()
    });
    settings
}

async fn attachments_dir(client: &Client) -> Result<std::path::PathBuf, XmtpError> {
    let database = client.storage().path().await?.expect("file database");
    Ok(std::path::Path::new(&database).with_file_name("attachments"))
}

fn bytes_source(bytes: &[u8]) -> AttachmentSource {
    AttachmentSource::Bytes {
        bytes: bytes.to_vec(),
        filename: Some("note.txt".into()),
        mime_type: "text/plain".into(),
    }
}

/// Answer every request with one status and body, and count the requests.
async fn serve(status: u16, body: Vec<u8>) -> Result<(String, Arc<AtomicUsize>), XmtpError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(XmtpError::unknown)?;
    let url = format!(
        "http://{}/file",
        listener.local_addr().map_err(XmtpError::unknown)?
    );
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = requests.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request).await;
            let header = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        }
    });
    Ok((url, requests))
}

/// Read attachment events up to the HMAC sentinel this emits.
async fn drain_attachment_events(
    client: &Client,
    reader: &crate::EventReader,
) -> Result<Vec<ClientEvent>, XmtpError> {
    emit_hmac(client);
    let mut events = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(5), reader.next())
            .await
            .map_err(XmtpError::unknown)??
        {
            Some(ClientEvent::HmacKeysUpdated) | None => return Ok(events),
            Some(event) => events.push(event),
        }
    }
}

fn attachment_reader_filter() -> EventFilter {
    let mut kinds = ATTACHMENT_KINDS.to_vec();
    kinds.push(EventKind::HmacKeysUpdated);
    event_filter(kinds)
}

fn thrown_failure<T>(result: Result<T, XmtpError>) -> AttachmentFailure {
    match result {
        Err(XmtpError::Attachment(details, failure)) => {
            assert_eq!(details.code, "Attachment");
            failure
        }
        other => panic!("expected an attachment error, got {:?}", other.err()),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn attachment_uploads_after_its_record_is_sent_and_downloads_on_another_client() {
    let root = temp_root("attachment-flow");
    let signer = crate::generate_local_signer().await;
    let sender = Client::create(signer.clone(), file_options(&root.join("sender"))).await?;
    let receiver = Client::create(
        crate::generate_local_signer().await,
        file_options(&root.join("receiver")),
    )
    .await?;
    let attachments = sender.attachments();
    assert!(attachments.offered());
    let events = sender.events(attachment_reader_filter()).await?;

    let content = b"attachment bytes".to_vec();
    let from_bytes = attachments.create(bytes_source(&content)).await?;
    let remote = from_bytes.remote_attachment();
    assert!(matches!(
        from_bytes.status().await?,
        PendingAttachmentStatus::Waiting
    ));
    assert_eq!(std::fs::read(from_bytes.local_path().await?)?, content);
    assert_eq!(
        attachments.local_path(remote.clone()).await?,
        from_bytes.local_path().await?
    );

    // The SDK copies a path source at create, so moving it changes nothing.
    std::fs::create_dir_all(&root)?;
    let source = root.join("photo.bin");
    std::fs::write(&source, b"path bytes")?;
    let from_path = attachments
        .create(AttachmentSource::Path {
            path: source.to_string_lossy().into_owned(),
            filename: None,
            mime_type: "application/octet-stream".into(),
        })
        .await?;
    std::fs::rename(&source, root.join("moved.bin"))?;
    let path_remote = from_path.remote_attachment();
    assert_eq!(std::fs::read(from_path.local_path().await?)?, b"path bytes");
    assert!(drain_attachment_events(&sender, &events).await?.is_empty());

    // The record is complete before any upload, so the app sends it first.
    let dm = sender
        .conversations()
        .create_dm(receiver.inbox_id(), None)
        .await?;
    let sent = dm.send_remote_attachment(remote.clone(), None).await?;

    // Concurrent uploads of one attachment share one transfer.
    let (first, second) = futures::join!(from_bytes.upload(), from_bytes.upload());
    first?;
    second?;
    assert!(matches!(
        from_bytes.status().await?,
        PendingAttachmentStatus::Complete
    ));
    let uploaded = drain_attachment_events(&sender, &events).await?;
    let [
        ClientEvent::AttachmentUploadStarted {
            attachment: started,
        },
        ClientEvent::AttachmentUploadCompleted {
            attachment: completed,
        },
    ] = uploaded.as_slice()
    else {
        panic!("expected one shared upload, got {uploaded:?}");
    };
    assert_eq!(started, completed);
    assert_eq!(started.url, remote.url);
    assert_eq!(started.content_digest, remote.content_digest);
    events.end().await?;
    sender.end().await?;

    // A reopened client lists the upload it did not finish and resumes it.
    let sender = Client::build(
        signer::identity(signer).await?,
        file_options(&root.join("sender")),
        None,
    )
    .await?;
    let attachments = sender.attachments();
    let listed = attachments.list_pending().await?;
    let [listed] = listed.as_slice() else {
        panic!("expected one pending upload, got {}", listed.len());
    };
    assert_eq!(
        listed.remote_attachment().content_digest,
        path_remote.content_digest
    );
    assert!(matches!(
        attachments.pending(remote.clone()).await?.status().await?,
        PendingAttachmentStatus::Complete
    ));
    let resumed = attachments.pending(path_remote.clone()).await?;
    assert!(matches!(
        resumed.status().await?,
        PendingAttachmentStatus::Waiting
    ));
    resumed.upload().await?;
    assert!(matches!(
        resumed.status().await?,
        PendingAttachmentStatus::Complete
    ));
    assert!(attachments.list_pending().await?.is_empty());

    // The receiver downloads the record it was sent.
    receiver.conversations().sync_all(None).await?;
    let message = receiver
        .conversations()
        .get_message_by_id(sent)
        .await?
        .expect("sent attachment");
    let MessageContent::RemoteAttachment(received) = message.0.content else {
        panic!("expected a remote attachment, got {:?}", message.0.content);
    };
    let receiving = receiver.attachments();
    let downloaded = receiving.download(received.clone()).await?;
    assert_eq!(std::fs::read(&downloaded.path)?, content);
    assert_eq!(downloaded.filename.as_deref(), Some("note.txt"));
    assert_eq!(downloaded.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(
        receiving.local_path(received.clone()).await?,
        downloaded.path
    );
    let path_download = receiving.download(path_remote).await?;
    assert_eq!(std::fs::read(&path_download.path)?, b"path bytes");
    let mut local: Vec<_> = receiving
        .list_local()
        .await?
        .into_iter()
        .map(|file| std::path::PathBuf::from(file.path))
        .collect();
    local.sort();
    let mut expected = vec![
        relative_to(&attachments_dir(&receiver).await?, &downloaded.path),
        relative_to(&attachments_dir(&receiver).await?, &path_download.path),
    ];
    expected.sort();
    assert_eq!(local, expected);

    receiving.delete_local(received).await?;
    assert_eq!(receiving.list_local().await?.len(), 1);
    assert!(!std::path::Path::new(&downloaded.path).exists());
    sender.end().await?;
    receiver.end().await?;
    std::fs::remove_dir_all(root)?;
}

fn relative_to(root: &std::path::Path, path: &str) -> std::path::PathBuf {
    std::path::Path::new(path)
        .strip_prefix(root)
        .expect("file in the attachments directory")
        .to_path_buf()
}

#[xmtp_common::test(unwrap_try = true)]
async fn attachment_failures_carry_one_record_in_errors_and_status() {
    let root = temp_root("attachment-failures");
    let client = Client::create(crate::generate_local_signer().await, file_options(&root)).await?;
    let attachments = client.attachments();
    let events = client.events(attachment_reader_filter()).await?;

    // Missing staged data fails the upload before any request.
    let pending = attachments.create(bytes_source(b"staged")).await?;
    let digest = pending.remote_attachment().content_digest;
    let staged = attachments_dir(&client)
        .await?
        .join(".staged")
        .join(&digest);
    let ciphertext = std::fs::read(&staged)?;
    std::fs::remove_file(&staged)?;
    let other = attachments.create(bytes_source(b"other")).await?;
    let other_remote = other.remote_attachment();
    let other_ciphertext = std::fs::read(
        attachments_dir(&client)
            .await?
            .join(".staged")
            .join(&other_remote.content_digest),
    )?;
    let thrown = thrown_failure(pending.upload().await);
    assert_eq!(
        thrown,
        AttachmentFailure {
            cause: AttachmentFailureCause::StagedUnusable,
            credential_kind: None,
            retryable: false,
            missing_scope: false,
            http_status: None,
        }
    );
    assert!(matches!(
        pending.status().await?,
        PendingAttachmentStatus::Failed(recorded) if recorded == thrown
    ));
    let failed = drain_attachment_events(&client, &events).await?;
    assert!(matches!(
        failed.as_slice(),
        [
            ClientEvent::AttachmentUploadStarted { .. },
            ClientEvent::AttachmentUploadFailed { attachment },
        ] if attachment.cause == AttachmentFailureCause::StagedUnusable
            && attachment.content_digest == digest
    ));

    // A source the SDK cannot read fails create.
    let unreadable = attachments
        .create(AttachmentSource::Path {
            path: root.join("missing.bin").to_string_lossy().into_owned(),
            filename: None,
            mime_type: "application/octet-stream".into(),
        })
        .await;
    assert_eq!(
        thrown_failure(unreadable).cause,
        AttachmentFailureCause::SourceUnreadable
    );

    events.end().await?;

    // Download failures keep the host's status and the verification step.
    // The creating client holds the plaintext, so another client downloads.
    let downloader = Client::create(
        crate::generate_local_signer().await,
        file_options(&root.join("downloader")),
    )
    .await?;
    let downloads = downloader.attachments();
    let events = downloader.events(attachment_reader_filter()).await?;
    let remote = pending.remote_attachment();
    let (unavailable, requests) = serve(503, Vec::new()).await?;
    let error = downloads
        .download(RemoteAttachment {
            url: unavailable,
            ..remote.clone()
        })
        .await;
    let Err(XmtpError::Attachment(details, failure)) = &error else {
        panic!("expected an attachment error, got {error:?}");
    };
    assert_eq!(
        failure,
        &AttachmentFailure {
            cause: AttachmentFailureCause::HttpStatus,
            credential_kind: None,
            retryable: false,
            missing_scope: false,
            http_status: Some(503),
        }
    );
    assert!(details.retryable);
    assert!(matches!(details.category, ErrorCategory::Network));
    assert_eq!(requests.load(Ordering::SeqCst), 1, "the SDK does not retry");

    // Another attachment's object decrypts and decodes, so only its digest
    // differs from the record.
    let (substituted, _) = serve(200, other_ciphertext).await?;
    let error = downloads
        .download(RemoteAttachment {
            url: substituted,
            content_digest: remote.content_digest.clone(),
            ..other_remote
        })
        .await;
    assert_eq!(
        thrown_failure(error).cause,
        AttachmentFailureCause::DigestMismatch
    );

    // A changed tag byte with a matching digest fails only the decryption.
    let mut tampered_bytes = ciphertext;
    *tampered_bytes.last_mut().expect("ciphertext has a tag") ^= 1;
    let tampered_digest = hex::encode(xmtp_cryptography::hash::sha256_bytes(&tampered_bytes));
    let (tampered, _) = serve(200, tampered_bytes).await?;
    let error = downloads
        .download(RemoteAttachment {
            url: tampered.clone(),
            content_digest: tampered_digest,
            ..remote.clone()
        })
        .await;
    assert_eq!(
        thrown_failure(error).cause,
        AttachmentFailureCause::DecryptionFailed
    );

    let error = downloads
        .download(RemoteAttachment {
            url: tampered,
            secret: vec![7; 3],
            ..remote.clone()
        })
        .await;
    assert_eq!(
        thrown_failure(error).cause,
        AttachmentFailureCause::Malformed
    );
    let failed = drain_attachment_events(&downloader, &events).await?;
    let causes: Vec<_> = failed
        .iter()
        .filter_map(|event| match event {
            ClientEvent::AttachmentDownloadFailed { attachment } => Some(attachment.cause),
            _ => None,
        })
        .collect();
    assert_eq!(
        causes,
        [
            AttachmentFailureCause::HttpStatus,
            AttachmentFailureCause::DigestMismatch,
            AttachmentFailureCause::DecryptionFailed,
        ]
    );
    events.end().await?;
    downloader.end().await?;
    client.end().await?;
    std::fs::remove_dir_all(root)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn terminal_backend_rejection_is_not_resent() {
    use xmtp_db::attachments::{PendingAttachmentOutcome, QueryPendingAttachment as _};

    let root = temp_root("attachment-rejected");
    let client = Client::create(crate::generate_local_signer().await, file_options(&root)).await?;
    let pending = client
        .attachments()
        .create(bytes_source(b"rejected"))
        .await?;
    let digest = pending.remote_attachment().content_digest;
    let db = client.inner.context.db();
    let lease = xmtp_common::rand_array::<16>();
    let now = xmtp_common::time::now_ns();
    assert_eq!(
        db.claim_pending_attachment(&digest, &lease, now, 60_000_000_000)?,
        1
    );
    db.finish_pending_attachment(
        &digest,
        &lease,
        now,
        PendingAttachmentOutcome::Failed {
            cause: "backend_rejected",
            credential_kind: None,
            retryable: Some(false),
            missing_scope: Some(false),
            http_status: None,
        },
    )?;
    let recorded = AttachmentFailure {
        cause: AttachmentFailureCause::BackendRejected,
        credential_kind: None,
        retryable: false,
        missing_scope: false,
        http_status: None,
    };

    let events = client.events(attachment_reader_filter()).await?;
    assert_eq!(thrown_failure(pending.upload().await), recorded);
    assert_eq!(thrown_failure(pending.upload().await), recorded);
    assert!(matches!(
        pending.status().await?,
        PendingAttachmentStatus::Failed(failure) if failure == recorded
    ));
    assert!(drain_attachment_events(&client, &events).await?.is_empty());
    events.end().await?;
    client.end().await?;
    std::fs::remove_dir_all(root)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn attachment_calls_fail_closed_after_end() {
    let root = temp_root("attachment-closed");
    let client = Client::create(crate::generate_local_signer().await, file_options(&root)).await?;
    let attachments = client.attachments();
    let pending = attachments.create(bytes_source(b"closed")).await?;
    let remote = pending.remote_attachment();
    client.end().await?;

    assert!(attachments.offered());
    assert_eq!(
        pending.remote_attachment().content_digest,
        remote.content_digest
    );
    assert!(matches!(
        attachments.create(bytes_source(b"late")).await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        attachments.local_path(remote.clone()).await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        attachments.list_local().await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        pending.status().await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        pending.upload().await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        client.attachments().download(remote).await,
        Err(XmtpError::ClientClosed(_))
    ));
    std::fs::remove_dir_all(root)?;
}

#[xmtp_common::test(unwrap_try = true)]
fn attachment_error_category_and_retry_follow_the_cause() {
    use AttachmentFailureCause::*;
    use ErrorCategory::{Callback, Configuration, Input, Network as Net, Storage};

    let cases = [
        (NotOffered, None, false, Configuration, false),
        (TooLarge, None, false, Input, false),
        (SourceUnreadable, None, false, Input, false),
        (LocalStorage, None, false, Storage, true),
        (StagedUnusable, None, false, Storage, false),
        (ConnectionBlocked, None, false, Configuration, false),
        (Credential, None, true, Callback, true),
        (Credential, None, false, Callback, false),
        (BackendRejected, None, false, Net, false),
        (BackendUnavailable, None, false, Net, true),
        (TargetRejected, Some(403), false, Net, true),
        (Network, None, false, Net, true),
        (InsecureUrl, None, false, Input, false),
        (BlockedAddress, None, false, Net, false),
        (TooManyRedirects, None, false, Net, false),
        (NotFound, Some(404), false, Net, true),
        (HttpStatus, Some(408), false, Net, true),
        (HttpStatus, Some(429), false, Net, true),
        (HttpStatus, Some(500), false, Net, true),
        (HttpStatus, Some(599), false, Net, true),
        (HttpStatus, Some(403), false, Net, false),
        (HttpStatus, Some(600), false, Net, false),
        (Malformed, None, false, Input, false),
        (DigestMismatch, None, false, Input, false),
        (DecryptionFailed, None, false, Input, false),
        (NotAnAttachment, None, false, Input, false),
        (Deleted, None, false, Storage, true),
    ];
    for (cause, http_status, failure_retryable, category, retryable) in cases {
        let failure = AttachmentFailure {
            cause,
            credential_kind: (cause == Credential).then_some(CredentialFailureKind::Exhausted),
            retryable: failure_retryable,
            missing_scope: false,
            http_status,
        };
        let XmtpError::Attachment(details, carried) = XmtpError::attachment(failure.clone()) else {
            panic!("expected an attachment error");
        };
        assert_eq!(carried, failure);
        assert_eq!(
            std::mem::discriminant(&details.category),
            std::mem::discriminant(&category),
            "{cause:?}"
        );
        assert_eq!(details.retryable, retryable, "{cause:?} {http_status:?}");
    }
}
