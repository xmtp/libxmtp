//! Upload lifecycle tests: leases.

use super::*;

// verifies: ATCH-074
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn panicked_attempt_can_be_claimed_after_lease_expiry() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    alix.client.context.attachment_runtime().set_lease_timing(
        Duration::from_millis(300),
        Duration::from_millis(100),
        Duration::from_millis(10),
    );
    let pending = alix.client.attachments().create(bytes()).await?;
    let remote = pending.remote_attachment().clone();
    let resumed = alix.client.attachments().pending(&remote).await?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::AttachmentUploadStarted]))?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    alix.client.context.attachment_runtime().set_pause(
        PausePoint::AttemptPanic,
        Some((entered.clone(), release.clone())),
    );
    let first = tokio::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered.notified()).await?;
    let attempt = resumed.probe().attempt().expect("running attempt");
    first.abort();
    let _ = first.await;
    alix.client
        .context
        .attachment_runtime()
        .set_pause(PausePoint::AttemptPanic, None);
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), attempt.done().cancelled()).await?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while resumed.status() != PendingAttachmentStatus::Waiting {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    tokio::time::timeout(Duration::from_secs(5), resumed.upload()).await??;
    assert_eq!(resumed.status(), PendingAttachmentStatus::Complete);
    assert_eq!(events.drain().len(), 2);
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_stops_before_create_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_create_upload().times(0);
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(api))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let pending = client.attachments().pending(&remote).await?;
    let token = [11u8; 16];
    pending.probe().set_lease(Some((token, now_ns() - 1)));
    assert_eq!(
        pending
            .upload_once_for_test(&token)
            .await
            .unwrap_err()
            .cause,
        Cause::Network
    );
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_stops_before_put() {
    use xmtp_proto::backend_v1::CreateUploadResponse;
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, puts) = serve_body(Vec::new()).await;
    let shared = Arc::new(Mutex::new(None::<PendingProbe>));
    let expire = shared.clone();
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_create_upload().times(1).returning(move |_| {
        let current = expire.lock();
        let pending = current.as_ref().expect("pending attachment shared state");
        let (token, _) = pending.lease().expect("local lease");
        pending.set_lease(Some((token, now_ns() - 1)));
        Ok(CreateUploadResponse {
            method: "PUT".into(),
            url: url.clone(),
            headers: vec![],
            expires_in_seconds: 3600,
        })
    });
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(api))
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
    *shared.lock() = Some(pending.probe());
    let token = [12u8; 16];
    pending
        .probe()
        .set_lease(Some((token, now_ns() + 10_000_000_000)));
    assert_eq!(
        pending
            .upload_once_for_test(&token)
            .await
            .unwrap_err()
            .cause,
        Cause::Network
    );
    assert_eq!(puts.load(Ordering::SeqCst), 0);
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_reads_waiting() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let digest = &pending.remote_attachment().content_digest;
    let token = [7u8; 16];
    alix.client.context.db().claim_pending_attachment(
        digest,
        &token,
        now_ns() - 200_000_000_000,
        LEASE_DURATION.as_nanos() as i64,
    )?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Waiting);
    pending.upload().await?;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
}

// verifies: ATCH-034, ATCH-035, EVENT-057
#[xmtp_common::test(unwrap_try = true)]
async fn dropped_upload_waiter_does_not_cancel_attempt() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), disable_workers);
    let pending = alix.client.attachments().create(bytes()).await?;
    let events = alix
        .client
        .context
        .events()
        .subscribe_app(EventFilter::new([
            EventKind::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted,
        ]))?;
    let mut upload = Box::pin(pending.upload());
    assert!(matches!(
        futures::poll!(upload.as_mut()),
        std::task::Poll::Pending
    ));
    drop(upload);
    tokio::time::timeout(Duration::from_secs(15), pending.upload()).await??;
    assert_eq!(pending.status(), PendingAttachmentStatus::Complete);
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

// verifies: ATCH-035, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn second_client_joins_running_upload() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let first = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let second = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(
            "http://127.0.0.1:1/unused".into(),
            0,
        )))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    second.context.attachment_runtime().set_lease_timing(
        LEASE_DURATION,
        LEASE_RENEW,
        Duration::from_millis(20),
    );
    let a = first.attachments().pending(&remote).await?;
    let b = second.attachments().pending(&remote).await?;
    let mut observed = b.watch_status();
    let first_upload = xmtp_common::task::spawn(async move { a.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    tokio::time::timeout(Duration::from_secs(2), async {
        while *observed.borrow_and_update() != PendingAttachmentStatus::Uploading {
            observed.changed().await.unwrap();
        }
    })
    .await?;
    let second_upload = xmtp_common::task::spawn(async move { b.upload().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!second_upload.is_finished());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), first_upload).await???;
    tokio::time::timeout(Duration::from_secs(10), second_upload).await???;
    tokio::time::timeout(Duration::from_secs(2), async {
        while *observed.borrow_and_update() != PendingAttachmentStatus::Complete {
            observed.changed().await.unwrap();
        }
    })
    .await?;
    assert_eq!(
        second.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Complete
    );
}

// verifies: ATCH-074, ATCH-035
#[xmtp_common::test(unwrap_try = true)]
async fn stale_holder_yields_to_new_claim() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let staged = dir.path().join(staged_path(&remote.content_digest)?);
    let (a_url, a_entered, a_release) = paused_put(500).await;
    let (b_url, b_entered, b_release) = paused_put(200).await;
    let a_client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(a_url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let b_client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(b_url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    a_client.context.attachment_runtime().set_lease_timing(
        Duration::from_millis(100),
        Duration::from_secs(1),
        Duration::from_millis(10),
    );
    let a = a_client.attachments().pending(&remote).await?;
    let events = a_client.context.events().subscribe_app(EventFilter::new([
        EventKind::AttachmentUploadStarted,
        EventKind::AttachmentUploadFailed,
    ]))?;
    let a_upload = xmtp_common::task::spawn(async move { a.upload().await });
    tokio::time::timeout(Duration::from_secs(5), a_entered).await??;
    let old_token = alix
        .client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_id
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let b = b_client.attachments().pending(&remote).await?;
    let b_upload = xmtp_common::task::spawn(async move { b.upload().await });
    tokio::time::timeout(Duration::from_secs(5), b_entered).await??;
    // The old token cannot write while the replacement still uploads.
    assert_eq!(
        alix.client.context.db().finish_pending_attachment(
            &remote.content_digest,
            &old_token,
            now_ns(),
            PendingAttachmentOutcome::failed("network", None, None)
        )?,
        0
    );
    b_release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), b_upload).await???;
    a_release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), a_upload).await???;
    assert_eq!(
        a_client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
    assert_eq!(
        a_client.attachments().pending(&remote).await?.status(),
        PendingAttachmentStatus::Complete
    );
    assert!(!staged.exists());
    let emitted = events.drain();
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        &emitted[0].client,
        Some(ClientEvent::AttachmentUploadStarted(_))
    ));
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn lease_extended_while_running() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    client.context.attachment_runtime().set_lease_timing(
        Duration::from_millis(300),
        Duration::from_millis(30),
        Duration::from_millis(10),
    );
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    let initial = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_expires_at_ns
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let extended = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_expires_at_ns
        .unwrap();
    assert!(extended > initial);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), upload).await???;
}

// verifies: ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn extension_storage_error_is_retried() {
    let dir = tempfile::tempdir()?;
    tester!(alix, attachments_dir: dir.path(), configured: offer, disable_workers);
    let created = alix.client.attachments().create(bytes()).await?;
    let remote = created.remote_attachment().clone();
    let (url, entered, release) = paused_put(200).await;
    let client = xmtp_mls::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(Arc::new(signed_put_api(url, 1)))
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            offer,
        )))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    client.context.attachment_runtime().set_lease_timing(
        Duration::from_millis(500),
        Duration::from_millis(30),
        Duration::from_millis(10),
    );
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER lock_attachment_extension BEFORE UPDATE OF lease_expires_at_ns \
                 ON pending_attachments WHEN OLD.status = 'uploading' AND NEW.status = 'uploading' \
                 BEGIN SELECT RAISE(ABORT, 'database table is locked'); END",
        )
        .execute(conn)
    })?;
    let pending = client.attachments().pending(&remote).await?;
    let upload = xmtp_common::task::spawn(async move { pending.upload().await });
    tokio::time::timeout(Duration::from_secs(5), entered).await??;
    let initial = client
        .context
        .db()
        .get_pending_attachment(&remote.content_digest)?
        .unwrap()
        .lease_expires_at_ns
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client.context.attachment_runtime().lease_extension_errors() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    client.context.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query("DROP TRIGGER lock_attachment_extension").execute(conn)
    })?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)
            .unwrap()
            .unwrap()
            .lease_expires_at_ns
            .unwrap()
            <= initial
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), upload).await???;
    assert_eq!(
        client
            .context
            .db()
            .get_pending_attachment(&remote.content_digest)?
            .unwrap()
            .status,
        "complete"
    );
}
