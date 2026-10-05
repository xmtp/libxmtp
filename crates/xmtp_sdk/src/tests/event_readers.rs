use super::*;

// verifies: EVENT-014
// verifies: EVENT-015
#[xmtp_common::test(unwrap_try = true)]
async fn events_registered_before_return() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    emit_hmac(&client);
    let reader = client
        .events(event_filter(vec![
            EventKind::HmacKeysUpdated,
            EventKind::ArchiveRestored,
        ]))
        .await?;
    emit_hmac(&client);
    client.inner.context.events().emit(
        Some(xmtp_events::ClientEvent::ArchiveRestored(
            xmtp_events::ArchiveRestored { complete: true },
        )),
        None,
    );
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::ArchiveRestored { archive_restored }) if archive_restored.complete
    ));
    reader.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn reader_delivers_attachment_events_in_order_by_filter() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let mut kinds = ATTACHMENT_KINDS.to_vec();
    kinds.push(EventKind::HmacKeysUpdated);
    let reader = client.events(event_filter(kinds)).await?;
    let deleted = client
        .events(event_filter(vec![
            EventKind::AttachmentDeleted,
            EventKind::HmacKeysUpdated,
        ]))
        .await?;

    emit_attachment_kinds(&other);
    emit_attachment_kinds(&client);
    emit_hmac(&client);
    let mut events = Vec::new();
    for _ in ATTACHMENT_KINDS {
        events.push(
            tokio::time::timeout(Duration::from_secs(2), reader.next())
                .await??
                .expect("attachment event"),
        );
    }
    assert_attachment_kinds(&events);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), reader.next()).await??,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), deleted.next()).await??,
        Some(ClientEvent::AttachmentDeleted { attachment_deleted }) if attachment_deleted.attachment_key == "down"
    ));
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), deleted.next()).await??,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    reader.end().await?;
    deleted.end().await?;
    other.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn listener_delivers_attachment_events_in_order() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let mut kinds = ATTACHMENT_KINDS.to_vec();
    kinds.push(EventKind::HmacKeysUpdated);
    let (sender, mut received) = tokio::sync::mpsc::unbounded_channel();
    let id = client
        .start_listener(event_filter(kinds), Arc::new(EventCapture(sender)))
        .await?;

    emit_attachment_kinds(&client);
    emit_hmac(&client);
    let mut events = Vec::new();
    for _ in ATTACHMENT_KINDS {
        events.push(
            tokio::time::timeout(Duration::from_secs(2), received.recv())
                .await?
                .expect("attachment event"),
        );
    }
    assert_attachment_kinds(&events);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), received.recv()).await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    client.stop_listener(id).await;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
fn attachment_event_cause_preserves_core_value() {
    let event = ClientEvent::from_core(xmtp_events::ClientEvent::AttachmentDownloadFailed(
        core_attachment_failed("odd", "no_such_cause"),
    ));
    assert!(matches!(
        event,
        ClientEvent::AttachmentDownloadFailed { attachment_download_failed }
            if attachment_download_failed.cause == "no_such_cause"
                && attachment_download_failed.attachment_key == "odd"
    ));
}

// verifies: EVENT-013
#[xmtp_common::test(unwrap_try = true)]
async fn event_reader_and_listener_create_no_network_interest() {
    use xmtp_proto::api::HasStats;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let api = &client.inner.context.api().api_client;
    let mls = api.mls_stats();
    let identity = api.identity_stats();
    let api_counts = || {
        [
            mls.publish.get_count(),
            mls.query.get_count(),
            mls.query_newest.get_count(),
            mls.subscribe.get_count(),
            mls.subscribe_static.get_count(),
            identity.get_inbox_ids.get_count(),
            identity.verify_smart_contract_wallet_signatures.get_count(),
        ]
    };
    let lease_count = || {
        client
            .inner
            .context
            .incoming_runtime()
            .active_lease_count_for_test()
    };
    let baseline = (api_counts(), lease_count());

    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    assert_eq!((api_counts(), lease_count()), baseline, "reader start");
    let (listener, mut started) = event_probe(None, false, None, false);
    let listener_id = client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), listener)
        .await?;
    assert_eq!((api_counts(), lease_count()), baseline, "listener start");

    emit_hmac(&client);
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(0)
    );
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(
        (api_counts(), lease_count()),
        baseline,
        "held subscriptions"
    );

    client.stop_listener(listener_id).await;
    reader.end().await?;
    assert_eq!((api_counts(), lease_count()), baseline, "subscription end");
    client.end().await?;
}

// verifies: EVENT-016
// verifies: EVENT-053
// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn event_reader_stays_open_on_rejection_and_ends_on_close() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client
        .events(event_filter(vec![EventKind::ClientRejectedByServer]))
        .await?;
    client.inner.context.cancellation_token().cancel();
    client.inner.context.events().emit(
        Some(xmtp_events::ClientEvent::ClientRejectedByServer(
            xmtp_events::ClientRejectedByServer {
                cause: xmtp_events::RejectionCause::BackendMismatch,
                min_libxmtp_version: None,
            },
        )),
        None,
    );
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::ClientRejectedByServer { .. })
    ));
    let reading = reader.clone();
    let mut pending = tokio::spawn(async move { reading.next().await });
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut pending)
            .await
            .is_err()
    );
    client.end().await?;
    assert!(pending.await??.is_none());
    assert!(matches!(
        client.events(EventFilter::default()).await,
        Err(XmtpError::ClientClosed(_))
    ));
}

// verifies: EVENT-053
#[xmtp_common::test(unwrap_try = true)]
async fn event_reader_end_waits_for_in_flight_read() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    emit_hmac(&client);
    let reading = reader.clone();
    let read = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), gate.arrived.notified()).await?;
    let ending = reader.clone();
    let mut end = tokio::spawn(async move { ending.end().await });
    let ended_before_read = tokio::time::timeout(Duration::from_millis(50), &mut end)
        .await
        .is_ok();
    gate.release.notify_one();
    assert!(!ended_before_read, "end returned during an in-flight read");
    assert!(read.await??.is_none());
    end.await??;
    client.end().await?;
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn client_end_waits_for_in_flight_event_read() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    emit_hmac(&client);
    let reading = reader.clone();
    let read = tokio::spawn(async move { reading.next().await });
    tokio::time::timeout(Duration::from_secs(5), gate.arrived.notified()).await?;
    let ending = client.clone();
    let mut end = tokio::spawn(async move { ending.end().await });
    let ended_before_read = tokio::time::timeout(Duration::from_millis(50), &mut end)
        .await
        .is_ok();
    gate.release.notify_one();
    assert!(
        !ended_before_read,
        "client end returned during an event read"
    );
    assert!(read.await??.is_none());
    end.await??;
}

// verifies: EVENT-020
// verifies: EVENT-021
#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_selects_before_queueing() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let filter = EventFilter {
        content_types: Some(vec![crate::EventContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "reply".into(),
            version_major: 1,
        }]),
        references_own_messages: true,
        ..event_filter(vec![EventKind::MessageReceived])
    };
    let reader = client.events(filter).await?;
    let bus = client.inner.context.events();
    let message = xmtp_events::ClientEvent::MessageReceived(xmtp_events::MessageReceived {
        group_id: vec![1; 16],
        message_id: vec![2; 32],
        content_type: Some(xmtp_events::ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "reply".into(),
            version_major: 1,
        }),
        sender_inbox_id: client.inbox_id().into_checked()?,
    });
    bus.emit(Some(message.clone()), None);
    bus.emit_with_context(
        Some(message),
        None,
        xmtp_events::EventContext {
            references_own_messages: true,
            ..Default::default()
        },
    );
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::MessageReceived { .. })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), reader.next())
            .await
            .is_err()
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: EVENT-020
#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_matches_stitched_dm_identifier() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = client
        .inner
        .find_or_create_dm(other.inbox_id().into_checked()?, None)
        .await?;
    let dm_identifier = dm.dm_id.clone().expect("DM ID");
    let reader = client
        .events(EventFilter {
            group_ids: Some(vec![dm.group_id.as_slice().to_vec()]),
            ..event_filter(vec![EventKind::ConversationJoined])
        })
        .await?;
    let event = xmtp_events::ClientEvent::ConversationJoined(xmtp_events::ConversationJoined {
        group_id: vec![8; 16],
        conversation_type: xmtp_events::ConversationType::Dm,
        origin: xmtp_events::JoinOrigin::Welcomed,
        adder_inbox_id: None,
    });
    client
        .inner
        .context
        .events()
        .emit(Some(event.clone()), None);
    client.inner.context.events().emit_with_context(
        Some(event),
        None,
        xmtp_events::EventContext {
            dm_identifier: Some(dm_identifier.into_bytes()),
            ..Default::default()
        },
    );
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), reader.next()).await??,
        Some(ClientEvent::ConversationJoined { .. })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), reader.next())
            .await
            .is_err()
    );
    reader.end().await?;
    other.end().await?;
    client.end().await?;
}

// verifies: EVENT-020
#[xmtp_common::test(unwrap_try = true)]
async fn consent_event_for_stitched_dm_reaches_group_filter() {
    use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm_a = client
        .inner
        .find_or_create_dm(other.inbox_id().into_checked()?, None)
        .await?;
    let dm_b = other
        .inner
        .find_or_create_dm(client.inbox_id().into_checked()?, None)
        .await?;
    assert_ne!(dm_a.group_id, dm_b.group_id);
    client.inner.sync_welcomes().await?;
    let stitched = client.inner.group(&dm_b.group_id)?;
    assert_eq!(dm_a.dm_id, stitched.dm_id);

    let reader = client
        .events(EventFilter {
            group_ids: Some(vec![dm_a.group_id.as_slice().to_vec()]),
            ..event_filter(vec![EventKind::ConsentChanged])
        })
        .await?;
    let entity = hex::encode(dm_b.group_id);
    client
        .inner
        .set_consent_states(&[StoredConsentRecord::new(
            ConsentType::ConversationId,
            ConsentState::Denied,
            entity.clone(),
        )])
        .await?;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), reader.next()).await??,
        Some(ClientEvent::ConsentChanged { consent_changed }) if consent_changed.entity == entity
    ));
    reader.end().await?;
    other.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_accepts_unknown_conversation_id() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let unknown = xmtp_proto::types::GroupId::from([0xee; 16]);
    let reader = client
        .events(EventFilter {
            group_ids: Some(vec![unknown.as_slice().to_vec()]),
            ..event_filter(vec![EventKind::ConversationJoined])
        })
        .await?;
    reader.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_reports_storage_error_when_resolving_dm() {
    use xmtp_db::ConnectionExt;

    let mut settings = options();
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-event-filter-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    settings.storage.location = explicit_location(&path);
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = client
        .inner
        .find_or_create_dm(other.inbox_id().into_checked()?, None)
        .await?;
    client.inner.context.db().disconnect()?;
    let result = client
        .events(EventFilter {
            group_ids: Some(vec![dm.group_id.as_slice().to_vec()]),
            ..event_filter(vec![EventKind::ConversationJoined])
        })
        .await;
    client.inner.context.db().reconnect()?;
    assert!(result.is_err(), "storage error silently dropped the DM ID");
    other.end().await?;
    client.end().await?;
}
