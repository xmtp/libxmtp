use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn should_stream_consent() {
    let alix = Tester::builder().sync_worker().build().await;
    let bo = Tester::new().await;

    let receiver = alix.local_events.subscribe();
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

/// API-089: only the exact identity topic's serving head confirms registration.
#[xmtp_common::test(unwrap_try = true)]
#[rstest::rstest]
#[case(false)]
#[case(true)]
async fn registration_visibility_waits_for_serving_head(#[case] newer_head: bool) {
    use crate::client::VisibilityConfirmationOptions;
    use crate::identity::IdentityStrategy;
    use xmtp_api_backend::MockBackendClient;
    use xmtp_proto::backend_v1 as wire;
    use xmtp_proto::types::Topic;

    tester!(alix, disable_workers);
    let identity: StoredIdentity = alix.db().fetch(&())?.unwrap();
    let registration = identity.registration_cursor_sequence_id.unwrap() as u64;
    assert!(registration > 1);
    let topic = Topic::new_identity_update(hex::decode(alix.inbox_id())?);
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
        .default_mls_store()?
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    reader
        .wait_for_registration_visible(VisibilityConfirmationOptions { timeout_ms: 1_000 })
        .await?;
}

/// API-089: a response with a different metadata topic cannot confirm registration.
#[xmtp_common::test(unwrap_try = true)]
async fn registration_visibility_rejects_mismatched_metadata() {
    use crate::client::{ClientError, VisibilityConfirmationOptions};
    use crate::identity::IdentityStrategy;
    use xmtp_api_backend::MockBackendClient;
    use xmtp_proto::backend_v1 as wire;
    use xmtp_proto::types::Topic;

    tester!(alix, disable_workers);
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
