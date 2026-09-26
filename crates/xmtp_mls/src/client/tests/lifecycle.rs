use super::*;
use crate::utils::test::set_registration_cursor_for_test as set_registration_cursor;

// verifies: EVENT-001, EVENT-010, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn registration_event_waits_for_visibility_and_fires_once() {
    use crate::utils::DefaultTestClientCreator;
    use crate::utils::test::{identity_setup, register_client};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_events::{EventFilter, EventKind};
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    let wallet = generate_local_wallet();
    let client = Client::builder(identity_setup(wallet.clone()))
        .temp_store()
        .await
        .api_client(DefaultTestClientCreator::create().build()?)
        .default_mls_store()?
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::IdentityRegistered]), Some(4));
    register_client(&client, wallet).await;
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::IdentityRegistered(registered)), ..
        }] if registered.inbox_id == client.inbox_id()
            && registered.installation_key == client.installation_id.to_vec()
    ));
    client
        .wait_for_registration_visible(Default::default())
        .await?;
    client
        .wait_for_registration_visible(Default::default())
        .await?;
    assert!(events.drain().is_empty());
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn dropping_the_last_client_handle_closes_app_events() {
    tester!(alix, disable_workers);
    let bus = alix.context.events().clone();
    let subscription = bus.subscribe_app(xmtp_events::EventFilter::new([
        xmtp_events::EventKind::ClientLockoutChanged,
    ]))?;
    drop(alix);
    assert!(subscription.is_closed());
    assert!(subscription.drain().is_empty());
}

// verifies: EVENT-001, EVENT-005, EVENT-010, EVENT-023
#[xmtp_common::test(unwrap_try = true)]
async fn created_and_welcomed_groups_emit_one_join_event() {
    tester!(alix, disable_workers);
    tester!(caro, disable_workers);
    tester!(bo, disable_workers);
    let created = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;
    let welcomed = bo
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;
    let group = alix.create_group(None, None)?;
    assert!(matches!(
        created.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == group.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Group
            && joined.origin == xmtp_events::JoinOrigin::Created
            && joined.adder_inbox_id.is_none()
    ));
    group.add_members(&[caro.inbox_id()]).await?;
    let caro_group = caro.sync_welcomes().await?.pop()?;
    group
        .update_admin_list(
            crate::groups::UpdateAdminListType::Add,
            caro.inbox_id().to_string(),
        )
        .await?;
    caro_group.sync().await?;
    caro_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    assert_eq!(bo_group.group_id, group.group_id);
    assert!(matches!(
        welcomed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == group.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Group
            && joined.origin == xmtp_events::JoinOrigin::Welcomed
            && joined.adder_inbox_id.as_deref() == Some(caro.inbox_id())
    ));
    bo.sync_welcomes().await?;
    assert!(welcomed.drain().is_empty());
    assert!(created.drain().is_empty());
}

// verifies: EVENT-001, EVENT-005
#[xmtp_common::test(unwrap_try = true)]
async fn created_and_welcomed_dms_emit_one_join_event() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let created = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;
    let welcomed = bo
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;

    let dm = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    assert!(matches!(
        created.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == dm.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Dm
            && joined.origin == xmtp_events::JoinOrigin::Created
            && joined.adder_inbox_id.is_none()
    ));

    let bo_dm = bo.sync_welcomes().await?.pop()?;
    assert_eq!(bo_dm.group_id, dm.group_id);
    assert!(matches!(
        welcomed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == dm.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Dm
            && joined.origin == xmtp_events::JoinOrigin::Welcomed
    ));
    bo.sync_welcomes().await?;
    assert!(welcomed.drain().is_empty());
    assert!(created.drain().is_empty());
}

// verifies: EVENT-007, EVENT-010
#[xmtp_common::test(unwrap_try = true)]
async fn local_delete_emits_once_after_the_row_is_deleted() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let message_id = group.prepare_message_for_later_publish(b"local delete", false, None)?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageDeleted]),
        Some(10),
    );
    assert_eq!(alix.delete_message(message_id.clone())?, 1);
    assert!(alix.context.db().get_group_message(&message_id)?.is_none());
    assert_eq!(alix.delete_message(message_id.clone())?, 0);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageDeleted(deleted)), ..
        }] if deleted.group_id == group.group_id.to_vec()
            && deleted.message_id == message_id
            && deleted.cause == xmtp_events::DeletionCause::DeletedLocally
    ));
}

// verifies: EVENT-010
#[xmtp_common::test(unwrap_try = true)]
async fn one_consent_batch_emits_only_the_final_state_for_an_entity() {
    tester!(alix, disable_workers);
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConsentChanged]),
        Some(10),
    );
    let denied = StoredConsentRecord::new(
        ConsentType::InboxId,
        ConsentState::Denied,
        "duplicate-consent".into(),
    );
    let allowed = StoredConsentRecord::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        "duplicate-consent".into(),
    );
    alix.set_consent_states(&[denied, allowed]).await?;
    assert_eq!(
        alix.context
            .db()
            .get_consent_record("duplicate-consent".into(), ConsentType::InboxId)?
            .map(|record| record.state),
        Some(ConsentState::Allowed)
    );
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConsentChanged(change)), ..
        }] if change.entity == "duplicate-consent"
            && change.state == xmtp_events::ConsentState::Allowed
    ));
}

// verifies: EVENT-001, EVENT-009
#[xmtp_common::test(unwrap_try = true)]
async fn app_consent_change_emits_once_and_duplicate_emits_none() {
    tester!(alix, disable_workers);
    let events = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConsentChanged,
        ]))?;
    let record = StoredConsentRecord::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        "one-consent-change".into(),
    );
    alix.set_consent_states(std::slice::from_ref(&record))
        .await?;
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConsentChanged(change)), ..
        }] if change.entity_kind == xmtp_events::ConsentEntityKind::Inbox
            && change.entity == record.entity
            && change.state == xmtp_events::ConsentState::Allowed
    ));

    let mut same_state = record;
    same_state.consented_at_ns += 1;
    alix.set_consent_states(&[same_state]).await?;
    assert!(events.drain().is_empty());
}

// verifies: EVENT-001, EVENT-009
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_consent_events_report_entity_and_states() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let entity = hex::encode(group.group_id);
    let events = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConsentChanged,
        ]))?;

    for (stored, expected) in [
        (ConsentState::Denied, xmtp_events::ConsentState::Denied),
        (ConsentState::Unknown, xmtp_events::ConsentState::Unknown),
    ] {
        let record = StoredConsentRecord::new(ConsentType::ConversationId, stored, entity.clone());
        alix.set_consent_states(&[record]).await?;
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(xmtp_events::ClientEvent::ConsentChanged(change)), ..
            }] if change.entity_kind == xmtp_events::ConsentEntityKind::Conversation
                && change.entity == entity
                && change.state == expected
        ));
    }
}

// verifies: CONS-041
#[xmtp_common::test(unwrap_try = true)]
async fn should_stream_consent() {
    let alix = Tester::builder().sync_worker().build().await;
    let bo = Tester::new().await;

    let receiver = alix.context.events().subscribe(
        xmtp_events::EventFilter::default().with_internal(|event| {
            matches!(
                event,
                crate::subscriptions::internal::InternalEvent::PreferencesChanged { .. }
            )
        }),
        Some(1024),
    );
    let stream = receiver.stream_consent_updates();
    futures::pin_mut!(stream);

    let group = alix
        .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
        .await
        .unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // first record is denied consent to the group.
    group.update_consent_state(ConsentState::Denied).unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // second is allowing consent for the group
    alix.set_consent_states(&[StoredConsentRecord {
        entity: hex::encode(group.group_id),
        state: ConsentState::Allowed,
        entity_type: ConsentType::ConversationId,
        consented_at_ns: now_ns(),
    }])
    .await
    .unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // third allowing consent for bo inbox id
    alix.set_consent_states(&[StoredConsentRecord {
        entity: bo.inbox_id().to_string(),
        entity_type: ConsentType::InboxId,
        state: ConsentState::Allowed,
        consented_at_ns: now_ns(),
    }])
    .await
    .unwrap();

    // First consent update from creating the group
    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Denied);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::InboxId);
    assert_eq!(item[0].entity, bo.inbox_id());
    assert_eq!(item[0].state, ConsentState::Allowed);
}

/// Only the exact identity topic's serving head confirms registration.
#[rstest::rstest]
#[case(false)]
#[case(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn registration_visibility_waits_for_serving_head(#[case] newer_head: bool) {
    use crate::client::VisibilityConfirmationOptions;
    use crate::identity::IdentityStrategy;
    use xmtp_api_backend::MockBackendClient;
    use xmtp_proto::backend_v1 as wire;
    use xmtp_proto::types::Topic;

    tester!(alix, disable_workers);
    let registration = alix
        .context
        .db()
        .get_latest_sequence_id(&[alix.inbox_id()])
        .unwrap()[alix.inbox_id()] as u64;
    set_registration_cursor(&alix.context.db(), registration as i64);
    assert!(registration > 1);
    let topic = Topic::new_identity_update(hex::decode(alix.inbox_id()).unwrap());
    let mut calls = 0;
    let mut api = MockBackendClient::new();
    api.expect_query_newest()
        .times(3)
        .returning(move |request| {
            assert!(!request.include_full_envelope);
            assert_eq!(
                request.topics,
                vec![wire::Topic {
                    topic: topic.cloned_vec()
                }]
            );
            calls += 1;
            if calls == 1 {
                return Ok(wire::QueryNewestResponse::default());
            }
            let sequence_id = if calls == 2 {
                registration - 1
            } else {
                registration + u64::from(newer_head)
            };
            Ok(wire::QueryNewestResponse {
                results: vec![wire::query_newest_response::Result {
                    topic: Some(wire::Topic {
                        topic: topic.cloned_vec(),
                    }),
                    meta: Some(wire::EnvelopeMeta {
                        topic: Some(wire::Topic {
                            topic: topic.cloned_vec(),
                        }),
                        cursor: Some(wire::Cursor { sequence_id }),
                        message_hash: Some(wire::MessageHash {
                            hash: Some(wire::message_hash::Hash::Sha256(vec![1; 32])),
                        }),
                        ..Default::default()
                    }),
                    envelope: None,
                }],
            })
        });
    let reader = Client::builder(IdentityStrategy::CachedOnly)
        .store(alix.context.store().clone())
        .api_client(api)
        .with_scw_verifier(alix.context.scw_verifier())
        .default_mls_store()
        .unwrap()
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await
        .unwrap();
    let events = reader.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::IdentityRegistered]),
        Some(4),
    );
    reader
        .wait_for_registration_visible(VisibilityConfirmationOptions { timeout_ms: 1_000 })
        .await
        .unwrap();
    assert!(events.drain().is_empty());
}

/// A response with a different metadata topic cannot confirm registration.
#[xmtp_common::test(unwrap_try = true)]
async fn registration_visibility_rejects_mismatched_metadata() {
    use crate::client::{ClientError, VisibilityConfirmationOptions};
    use crate::identity::IdentityStrategy;
    use xmtp_api_backend::MockBackendClient;
    use xmtp_proto::backend_v1 as wire;
    use xmtp_proto::types::Topic;

    tester!(alix, disable_workers);
    set_registration_cursor(&alix.context.db(), 1);
    let topic = Topic::new_identity_update(hex::decode(alix.inbox_id())?);
    let mut other_topic = topic.cloned_vec();
    other_topic[1] ^= 1;
    let mut api = MockBackendClient::new();
    api.expect_query_newest()
        .times(1)
        .returning(move |request| {
            assert!(!request.include_full_envelope);
            assert_eq!(
                request.topics,
                vec![wire::Topic {
                    topic: topic.cloned_vec()
                }]
            );
            Ok(wire::QueryNewestResponse {
                results: vec![wire::query_newest_response::Result {
                    topic: Some(wire::Topic {
                        topic: topic.cloned_vec(),
                    }),
                    meta: Some(wire::EnvelopeMeta {
                        topic: Some(wire::Topic {
                            topic: other_topic.clone(),
                        }),
                        cursor: Some(wire::Cursor {
                            sequence_id: i64::MAX as u64,
                        }),
                        message_hash: Some(wire::MessageHash {
                            hash: Some(wire::message_hash::Hash::Sha256(vec![1; 32])),
                        }),
                        ..Default::default()
                    }),
                    envelope: None,
                }],
            })
        });
    let reader = Client::builder(IdentityStrategy::CachedOnly)
        .store(alix.context.store().clone())
        .api_client(api)
        .with_scw_verifier(alix.context.scw_verifier())
        .default_mls_store()?
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    assert!(matches!(
        reader
            .wait_for_registration_visible(VisibilityConfirmationOptions { timeout_ms: 1_000 })
            .await,
        Err(ClientError::Api(xmtp_api::ApiError::InvalidResponse(_)))
    ));
}

/// A severed registration wait ends before its deadline.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn registration_visibility_deadline_bounds_a_severed_connection() {
    use crate::client::VisibilityConfirmationOptions;
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;
    toxiproxy_test(async || {
        tester!(alix, proxy, disable_workers);
        alix.wait_for_registration_visible(VisibilityConfirmationOptions::default())
            .await
            .unwrap();
        set_registration_cursor(&alix.context.db(), 1);
        let outcome = AssertUnwindSafe(async {
            alix.for_each_proxy(async |proxy| proxy.disable().await.unwrap())
                .await;
            let started = xmtp_common::time::Instant::now();
            let result = xmtp_common::time::timeout(
                Duration::from_secs(2),
                alix.wait_for_registration_visible(VisibilityConfirmationOptions {
                    timeout_ms: 250,
                }),
            )
            .await;
            (result, started.elapsed())
        })
        .catch_unwind()
        .await;
        alix.for_each_proxy(async |proxy| proxy.enable().await.unwrap())
            .await;
        let (result, elapsed) = outcome.unwrap();
        assert!(elapsed < Duration::from_secs(2));
        assert!(
            result.unwrap().is_err(),
            "a severed registration read must fail"
        );
    })
    .await;
}

#[xmtp_common::timeout(Duration::from_secs(100))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
/// One conversation stream resumes from durable receipt after a black hole.
async fn should_reconnect() {
    use crate::subscriptions::incoming::{IncomingConnection, IncomingCoordinator, IncomingScope};
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;
    use xmtp_db::incoming_envelope::{NetworkEntityKind, QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::Topic;

    // Nextest starts a fresh process, so these values precede the first client.
    unsafe {
        std::env::set_var("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS", "5");
        std::env::set_var("XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS", "5");
    }
    toxiproxy_test(async || {
        tester!(alix, proxy, disable_workers);
        tester!(bo, disable_workers);
        let start_new_convo = || async {
            bo.create_group_with_members(&[alix.inbox_id().to_string()], None, None)
                .await
                .unwrap()
        };
        let stream = alix.stream_conversations(None, false).await.unwrap();
        futures::pin_mut!(stream);
        let lease =
            IncomingCoordinator::for_context(&alix.context).acquire(IncomingScope::Topics(vec![
                Topic::new_welcome_message(alix.installation_public_key()),
            ]));
        let topic = StreamTopic {
            entity_id: alix.installation_public_key().to_vec(),
            kind: NetworkEntityKind::Welcome,
        };
        let wait = Duration::from_secs(20);

        let initial = start_new_convo().await;
        let delivered = xmtp_common::time::timeout(wait, stream.try_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(delivered.group_id, initial.group_id);
        let connected = xmtp_common::wait_for_some(|| async {
            let status = lease.snapshot();
            (status.connection == IncomingConnection::Connected).then_some(status)
        })
        .await
        .expect("the initial receiver did not connect");
        let before = alix.context.db().topic_progress(&topic).unwrap();
        assert!(before.processed.0 > 0);
        assert_eq!(before.received, before.processed);

        alix.for_each_proxy(async |p| {
            p.with_timeout("downstream".into(), 60_000, 1.0).await;
        })
        .await;
        let outage = AssertUnwindSafe(async {
            let missed = start_new_convo().await;
            assert!(
                xmtp_common::wait_for_some(|| async {
                    (lease.snapshot().connection != IncomingConnection::Connected).then_some(())
                })
                .await
                .is_some(),
                "the receiver did not detect the black hole"
            );
            let pending = alix.context.db().topic_progress(&topic).unwrap();
            assert_eq!(pending.received, before.received);
            assert_eq!(pending.processed, before.processed);
            missed
        })
        .catch_unwind()
        .await;
        // Restore the test's proxy before reporting any outage assertion failure.
        alix.for_each_proxy(async |p| {
            p.delete_all_toxics().await.unwrap();
        })
        .await;
        let missed = outage.unwrap();
        let delivered = xmtp_common::time::timeout(wait, stream.try_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(delivered.group_id, missed.group_id);
        assert!(
            xmtp_common::wait_for_some(|| async {
                let status = lease.snapshot();
                (status.connection == IncomingConnection::Connected
                    && status.connection_generation > connected.connection_generation)
                    .then_some(())
            })
            .await
            .is_some(),
            "the original receiver did not reconnect"
        );
        let recovered = alix.context.db().topic_progress(&topic).unwrap();
        assert!(recovered.processed > before.processed);
        assert_eq!(recovered.received, recovered.processed);

        let fresh = start_new_convo().await;
        let delivered = xmtp_common::time::timeout(wait, stream.try_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(delivered.group_id, fresh.group_id);
        let after = alix.context.db().topic_progress(&topic).unwrap();
        assert!(after.processed > recovered.processed);
        assert_eq!(after.received, after.processed);
    })
    .await
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_stops_workers() {
    tester!(client);
    assert!(
        client.workers.is_running(),
        "worker supervisor must be running before close"
    );

    client.close().await?;

    assert!(
        !client.workers.is_running(),
        "supervisor handle should be taken after close"
    );
    assert!(
        client.context.is_closed(),
        "context closed flag must be set after close"
    );
    assert!(
        client.context.cancellation_token().is_cancelled(),
        "cancellation token must be cancelled after close"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_keeps_worker_subscription_open_for_queued_facts() {
    use crate::subscriptions::internal::InternalEvent;
    use crate::worker::WorkerKind;
    use std::sync::Arc;
    use tokio::sync::Notify;
    use xmtp_events::EventWriter;

    tester!(alix, persistent_db);
    let subscription = alix
        .client
        .workers
        .subscription_for_test(WorkerKind::TaskRunner)
        .expect("TaskRunner has a subscription");
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    *crate::worker::test_hooks::PAUSE_NEXT_SPAWN.lock() = Some((
        alix.installation_id.to_vec(),
        entered.clone(),
        release.clone(),
    ));
    alix.client.reconnect_db()?;
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), entered.notified()).await?;
    xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!subscription.is_closed());

    let (consumed, received) = tokio::sync::oneshot::channel();
    *crate::worker::tasks::test_hooks::TASK_SCHEDULED_CONSUMED
        .lock()
        .unwrap() = Some((alix.installation_id.to_vec(), consumed));
    alix.context
        .events()
        .emit(None, Some(InternalEvent::TaskScheduled));
    release.notify_one();
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), received).await??;
    assert!(!subscription.is_closed());
}

// verifies: EVENT-056
#[xmtp_common::test(unwrap_try = true)]
async fn clients_sharing_a_database_receive_only_their_own_events() {
    use xmtp_events::{EventFilter, EventKind};

    tester!(alix, disable_workers);
    let second = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let first_events = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(10));
    let second_events = second
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(10));

    alix.set_consent_states(&[StoredConsentRecord::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        "from-first-client".into(),
    )])
    .await?;
    assert_eq!(first_events.drain().len(), 1);
    assert!(second_events.drain().is_empty());

    second
        .set_consent_states(&[StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Denied,
            "from-second-client".into(),
        )])
        .await?;
    assert!(first_events.drain().is_empty());
    assert_eq!(second_events.drain().len(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_cleans_up_workers_when_delivery_release_fails() {
    use crate::worker::WorkerKind;

    tester!(alix);
    let subscription = alix
        .client
        .workers
        .subscription_for_test(WorkerKind::TaskRunner)
        .expect("TaskRunner has a subscription");
    *crate::context::FAIL_NEXT_DELIVERY_RELEASE.lock() = Some(alix.installation_id.to_vec());

    assert!(alix.close().await.is_err());
    assert!(!alix.client.workers.is_running());
    assert!(subscription.is_closed());
    assert!(!alix.context.shutdown_complete());
    assert!(alix.close().await.is_ok());
    assert!(alix.context.shutdown_complete());
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_is_idempotent() {
    tester!(client);
    client.close().await?;
    // second call must return Ok(()) without panic
    client.close().await?;
}

// persistent_db: ephemeral in-memory stores no-op on disconnect, so the
// pool-released assertion only meaningfully tests against a real SQLite
// file. Skipped on WASM where file-backed test stores aren't wired in.
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn close_disconnects_db() {
    use diesel::RunQueryDsl;
    use diesel::sql_query;

    tester!(client, persistent_db);
    client.close().await?;

    let conn = client.context.store().conn();
    let result = conn.raw_query(|c| sql_query("SELECT 1").execute(c));
    assert!(
        result.is_err(),
        "raw_query after close should surface a ConnectionError; got Ok"
    );
}

#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn close_cancels_callback_stream() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    tester!(client);

    let closed_flag = Arc::new(AtomicBool::new(false));
    let flag_for_cb = closed_flag.clone();

    let _handle = crate::client::Client::stream_conversations_with_callback(
        Arc::new((*client).clone()),
        None,
        move |_| {},
        move || {
            flag_for_cb.store(true, Ordering::SeqCst);
        },
        false,
    );

    client.close().await?;

    xmtp_common::time::timeout(std::time::Duration::from_secs(1), async {
        while !closed_flag.load(Ordering::SeqCst) {
            xmtp_common::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("on_close must fire within 1s of Client::close");
}

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_after_close_errors() {
    tester!(client);
    client.close().await?;

    let err = client
        .reconnect_db()
        .expect_err("reconnect_db after close must fail");
    assert!(
        matches!(err, crate::client::ClientError::AlreadyClosed),
        "expected ClientError::AlreadyClosed, got {err:?}"
    );
}

// verifies: IDENT-072
#[xmtp_common::test(unwrap_try = true)]
async fn register_identity_waits_until_visible() {
    use crate::utils::test::{identity_setup, register_client};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use xmtp_api_backend::MockBackendClient;
    use xmtp_proto::backend_v1 as wire;
    let published = Arc::new(Mutex::new(Vec::<wire::ServerEnvelope>::new()));
    let visible = Arc::new(AtomicBool::new(false));
    let queried = Arc::new(AtomicBool::new(false));
    let mut api = MockBackendClient::new();
    api.expect_get_inbox_ids().returning(|request| {
        Ok(wire::GetInboxIdsResponse {
            responses: request
                .requests
                .into_iter()
                .map(|r| wire::get_inbox_ids_response::Response {
                    identifier: r.identifier,
                    identifier_kind: r.identifier_kind,
                    inbox_id: None,
                })
                .collect(),
        })
    });
    api.expect_query().returning({
        let published = published.clone();
        move |request| {
            Ok(wire::QueryResponse {
                envelopes: published
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|e| {
                        request
                            .queries
                            .iter()
                            .any(|q| q.topic == e.meta.as_ref().unwrap().topic)
                    })
                    .cloned()
                    .collect(),
                continuation: Some(Default::default()),
            })
        }
    });
    api.expect_publish().times(..=2).returning({
        let published = published.clone();
        move |request| {
            let mut published = published.lock().unwrap();
            let mut metas = Vec::new();
            for envelope in request.envelopes {
                let parsed = xmtp_mls_validation::parse_envelope(envelope.clone()).unwrap();
                let meta = wire::EnvelopeMeta {
                    topic: Some(wire::Topic {
                        topic: parsed.topic.cloned_vec(),
                    }),
                    cursor: Some(wire::Cursor {
                        sequence_id: published.len() as u64 + 1,
                    }),
                    message_hash: Some(wire::MessageHash {
                        hash: Some(wire::message_hash::Hash::Sha256(
                            parsed.canonical.hash.to_vec(),
                        )),
                    }),
                    ..Default::default()
                };
                published.push(wire::ServerEnvelope {
                    meta: Some(meta.clone()),
                    envelope: Some(envelope),
                });
                metas.push(meta);
            }
            Ok(wire::PublishResponse {
                envelope_metas: metas,
            })
        }
    });
    api.expect_query_newest().returning({
        let visible = visible.clone();
        let queried = queried.clone();
        let published = published.clone();
        move |request| {
            assert!(!request.include_full_envelope);
            assert_eq!(
                request.topics,
                vec![
                    published
                        .lock()
                        .unwrap()
                        .last()
                        .unwrap()
                        .meta
                        .as_ref()
                        .unwrap()
                        .topic
                        .clone()
                        .unwrap()
                ]
            );
            queried.store(true, Ordering::Release);
            Ok(registration_head(
                request,
                if visible.load(Ordering::Acquire) {
                    2
                } else {
                    1
                },
            ))
        }
    });
    let wallet = generate_local_wallet();
    let client = Client::builder(identity_setup(wallet.clone()))
        .temp_store()
        .await
        .api_client(api)
        .with_scw_verifier(
            xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier::new(true),
        )
        .default_mls_store()?
        .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::default()))
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::IdentityRegistered]),
        Some(4),
    );
    let registration = register_client(&client, wallet);
    futures::pin_mut!(registration);
    assert!(
        xmtp_common::time::timeout(Duration::from_millis(200), &mut registration)
            .await
            .is_err()
    );
    assert!(queried.load(Ordering::Acquire));
    assert!(!client.is_registration_visible()?);
    assert!(events.drain().is_empty());
    visible.store(true, Ordering::Release);
    registration.await;
    assert_eq!(published.lock().unwrap().len(), 2);
    assert!(client.is_registration_visible()?);
    assert_eq!(events.drain().len(), 1);
    client.ensure_registration_visible().await?;
    assert!(events.drain().is_empty());
}

fn registration_head(
    request: xmtp_proto::backend_v1::QueryNewestRequest,
    sequence_id: u64,
) -> xmtp_proto::backend_v1::QueryNewestResponse {
    use xmtp_proto::backend_v1 as wire;
    wire::QueryNewestResponse {
        results: request
            .topics
            .into_iter()
            .map(|topic| wire::query_newest_response::Result {
                topic: Some(topic.clone()),
                meta: Some(wire::EnvelopeMeta {
                    topic: Some(topic),
                    cursor: Some(wire::Cursor { sequence_id }),
                    message_hash: Some(wire::MessageHash {
                        hash: Some(wire::message_hash::Hash::Sha256(vec![1; 32])),
                    }),
                    ..Default::default()
                }),
                envelope: None,
            })
            .collect(),
    }
}

// verifies: IDENT-072
#[xmtp_common::test(unwrap_try = true)]
async fn resumed_registration_waits_until_visible() {
    registration_recovery(false).await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn confirmed_registration_does_not_require_network() {
    tester!(alix, disable_workers);
    alix.context.server_configuration().block_connection(
        crate::server_configuration::BlockedConnection::BackendMismatch {
            stored: "old.example".to_string(),
            received: "new.example".to_string(),
        },
    );
    alix.ensure_registration_visible().await?;
    alix.wait_for_registration_visible(Default::default())
        .await?;
    assert!(alix.is_registration_visible()?);

    set_registration_cursor(&alix.context.db(), 1);
    assert!(matches!(
        alix.ensure_registration_visible().await,
        Err(crate::client::ClientError::BackendMismatch { .. })
    ));
    let stored: StoredIdentity = alix.context.db().fetch(&())?.unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, Some(1));
}

// verifies: IDENT-072
#[xmtp_common::test(unwrap_try = true)]
async fn cursor_cleared_only_after_visible() {
    registration_recovery(true).await;
}

async fn registration_recovery(fail_first: bool) {
    use crate::identity::IdentityStrategy;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use xmtp_api_backend::MockBackendClient;
    tester!(alix, disable_workers);
    let signature = alix
        .identity_updates()
        .associate_identity(generate_local_wallet().identifier())
        .await
        .unwrap();
    // A stored registration resumes without using the supplied signature.
    set_registration_cursor(&alix.context.db(), 2);
    let visible = Arc::new(AtomicBool::new(false));
    let queried = Arc::new(AtomicBool::new(false));
    let mut api = MockBackendClient::new();
    api.expect_query_newest().returning({
        let visible = visible.clone();
        let queried = queried.clone();
        move |request| {
            assert!(!request.include_full_envelope);
            queried.store(true, Ordering::Release);
            if fail_first && !visible.load(Ordering::Acquire) {
                return Ok(Default::default());
            }
            Ok(registration_head(
                request,
                if visible.load(Ordering::Acquire) {
                    2
                } else {
                    1
                },
            ))
        }
    });
    let reader = Client::builder(IdentityStrategy::CachedOnly)
        .store(alix.context.store().clone())
        .api_client(api)
        .with_scw_verifier(alix.context.scw_verifier())
        .default_mls_store()
        .unwrap()
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await
        .unwrap();
    assert!(!reader.is_registration_visible().unwrap());
    if fail_first {
        assert!(
            reader
                .wait_for_registration_visible(crate::VisibilityConfirmationOptions {
                    timeout_ms: 100
                })
                .await
                .is_err()
        );
    } else {
        assert!(
            xmtp_common::time::timeout(
                Duration::from_millis(200),
                reader.register_identity(signature.clone())
            )
            .await
            .is_err()
        );
    }
    assert!(queried.load(Ordering::Acquire));
    let stored: StoredIdentity = reader.context.db().fetch(&()).unwrap().unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, Some(2));
    visible.store(true, Ordering::Release);
    reader.register_identity(signature).await.unwrap();
    assert!(reader.is_registration_visible().unwrap());
    reader.ensure_registration_visible().await.unwrap();
}
