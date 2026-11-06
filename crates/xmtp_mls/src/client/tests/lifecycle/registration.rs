//! Client lifecycle tests: registration.

use super::*;

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
    api.expect_query_raw_envelopes().returning({
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
            }
            .into())
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
