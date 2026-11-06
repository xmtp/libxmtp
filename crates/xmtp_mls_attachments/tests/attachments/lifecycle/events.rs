//! Upload lifecycle tests: events.

use super::*;

// verifies: EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn attachment_event_order() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadCompleted,
        EventKind::AttachmentDownloadStarted,
        EventKind::AttachmentDownloadCompleted,
        EventKind::AttachmentDeleted,
    ]))?;
    let pending = client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    client.attachments().delete_local(&remote).await?;
    client.attachments().download(&remote).await?;
    client.attachments().delete_local(&remote).await?;
    let kinds: Vec<_> = events
        .drain()
        .into_iter()
        .map(|event| {
            let event = event.client.unwrap();
            let key = attachment_key(&remote).unwrap();
            match &event {
                ClientEvent::AttachmentUploadStarted(reference)
                | ClientEvent::AttachmentUploadCompleted(reference)
                | ClientEvent::AttachmentDownloadStarted(reference)
                | ClientEvent::AttachmentDownloadCompleted(reference)
                | ClientEvent::AttachmentDeleted(reference) => {
                    assert_eq!(reference.attachment_key, key)
                }
                _ => panic!("unexpected event"),
            }
            event.kind()
        })
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
            EventKind::AttachmentDeleted,
            EventKind::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadCompleted,
            EventKind::AttachmentDeleted,
        ]
    );
}

// verifies: EVENT-001, EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn attachment_event_kinds() {
    fn expected_reference(remote: &RemoteAttachment) -> AttachmentRef {
        AttachmentRef {
            attachment_key: attachment_key(remote).expect("attachment key"),
            url: remote.url.clone(),
            content_digest: remote.content_digest.clone(),
        }
    }

    fn expected_failure(reference: &AttachmentRef, cause: Cause) -> AttachmentFailed {
        AttachmentFailed {
            attachment_key: reference.attachment_key.clone(),
            url: reference.url.clone(),
            content_digest: reference.content_digest.clone(),
            cause: cause.as_str().to_owned(),
        }
    }

    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .attachment_options(AttachmentOptions {
            allow_private_network: true,
            ..Default::default()
        })
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadCompleted,
        EventKind::AttachmentUploadFailed,
        EventKind::AttachmentDownloadStarted,
        EventKind::AttachmentDownloadCompleted,
        EventKind::AttachmentDownloadFailed,
        EventKind::AttachmentDeleted,
    ]))?;
    let failed = client.attachments().create(bytes()).await?;
    let failed_remote = failed.remote_attachment().clone();
    let staged = dir
        .path()
        .join(staged_path(&failed.remote_attachment().content_digest)?);
    tokio::fs::write(staged, b"damaged").await?;
    assert_eq!(
        failed.upload().await.unwrap_err().cause,
        Cause::StagedUnusable
    );
    client
        .attachments()
        .delete_local(failed.remote_attachment())
        .await?;
    let pending = client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    pending.upload().await?;
    client.attachments().delete_local(&remote).await?;
    client.attachments().download(&remote).await?;
    client.attachments().delete_local(&remote).await?;
    let (url, _) = serve_body(b"forged".to_vec()).await;
    let mut forged = remote.clone();
    forged.url = url;
    assert_eq!(
        client
            .attachments()
            .download(&forged)
            .await
            .unwrap_err()
            .cause,
        Cause::DigestMismatch
    );
    let emitted: Vec<_> = events
        .drain()
        .into_iter()
        .map(|entry| entry.client.expect("attachment event"))
        .collect();
    let failed_ref = expected_reference(&failed_remote);
    let remote_ref = expected_reference(&remote);
    let forged_ref = expected_reference(&forged);
    assert_eq!(
        emitted,
        vec![
            ClientEvent::AttachmentUploadStarted(failed_ref.clone()),
            ClientEvent::AttachmentUploadFailed(expected_failure(
                &failed_ref,
                Cause::StagedUnusable
            )),
            ClientEvent::AttachmentDeleted(failed_ref),
            ClientEvent::AttachmentUploadStarted(remote_ref.clone()),
            ClientEvent::AttachmentUploadCompleted(remote_ref.clone()),
            ClientEvent::AttachmentDeleted(remote_ref.clone()),
            ClientEvent::AttachmentDownloadStarted(remote_ref.clone()),
            ClientEvent::AttachmentDownloadCompleted(remote_ref.clone()),
            ClientEvent::AttachmentDeleted(remote_ref),
            ClientEvent::AttachmentDownloadStarted(forged_ref.clone()),
            ClientEvent::AttachmentDownloadFailed(expected_failure(
                &forged_ref,
                Cause::DigestMismatch
            )),
        ]
    );
}
