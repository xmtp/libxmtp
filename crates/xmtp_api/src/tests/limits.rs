//! P3-TST-002: client limit boundaries against the Docker backend.
//!
//! These tests use the same backend fixture as `integration`.
use super::*;
use futures::{StreamExt, TryStreamExt};
use std::collections::HashSet;
use xmtp_api_backend::TestClient;
use xmtp_proto::{
    api::HasStats,
    api_client::{ApiBuilder, XmtpBackendClient, XmtpMlsStreams, XmtpTestClient},
    types::TopicCursor,
};

fn backend() -> ApiClientWrapper<TestClient> {
    ApiClientWrapper::new(TestClient::create().build().unwrap(), Retry::default())
}

fn welcome_topics(count: usize) -> Vec<Topic> {
    let prefix = xmtp_common::rand_array::<BACKEND_INSTALLATION_ID_BYTES>();
    (0..count)
        .map(|index| {
            let mut id = prefix;
            id[..size_of::<usize>()].copy_from_slice(&index.to_be_bytes());
            Topic::new_welcome_message(id.into())
        })
        .collect()
}

fn welcome_for(topic: &Topic, data: Vec<u8>) -> wire::ClientEnvelope {
    let mut envelope = inline_welcome_envelope(topic.identifier());
    let Some(wire::client_envelope::Payload::WelcomeMessage(welcome)) = &mut envelope.payload
    else {
        unreachable!()
    };
    let Some(wire::welcome_message::Version::V1(welcome)) = &mut welcome.version else {
        unreachable!()
    };
    welcome.data = data;
    envelope
}

/// Make an envelope with the exact encoded size, including protobuf lengths.
fn sized_welcome(topic: &Topic, bytes: usize) -> wire::ClientEnvelope {
    let mut data_bytes = bytes;
    loop {
        let envelope = welcome_for(topic, vec![0x42; data_bytes]);
        match envelope.encoded_len().cmp(&bytes) {
            std::cmp::Ordering::Equal => return envelope,
            std::cmp::Ordering::Greater => data_bytes -= envelope.encoded_len() - bytes,
            std::cmp::Ordering::Less => data_bytes += bytes - envelope.encoded_len(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Read {
    Query,
    NewestMetadata,
    NewestFull,
}

/// P3-TST-002, P3-API-007: topic limits split reads without losing results.
#[rstest]
#[case::query(Read::Query, BACKEND_DEFAULT_MAX_QUERY_TOPICS)]
#[case::newest_metadata(Read::NewestMetadata, BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS)]
#[case::newest_full(Read::NewestFull, BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS)]
#[xmtp_common::test(unwrap_try = true)]
async fn read_topic_boundaries(#[case] read: Read, #[case] cap: usize) {
    for count in [cap, cap + 1] {
        let api = backend();
        let topics = welcome_topics(count);
        let rows = match read {
            Read::Query => {
                // Empty topics isolate topic chunking from the server's row clamp.
                let rows = api
                    .query_all(
                        topics.iter().cloned().map(|t| (t, Cursor(0))).collect(),
                        BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32,
                    )
                    .await
                    .unwrap();
                assert!(rows.is_empty());
                api.api_client.mls_stats().query.get_count()
            }
            Read::NewestMetadata | Read::NewestFull => {
                let units = topics
                    .iter()
                    .map(|topic| PublishUnit::single(welcome_for(topic, vec![1])))
                    .collect::<crate::Result<Vec<_>>>()
                    .unwrap();
                api.publish_units(units).await.unwrap();
                let full = matches!(read, Read::NewestFull);
                let results = api.newest(topics.clone(), full).await.unwrap();
                let returned: HashSet<_> = results
                    .iter()
                    .map(|row| Topic::parse(&row.topic.as_ref().unwrap().topic).unwrap())
                    .collect();
                assert_eq!(returned, topics.into_iter().collect());
                assert_eq!(results.len(), count);
                assert!(results.iter().all(|row| row.envelope.is_some() == full));
                api.api_client.mls_stats().query_newest.get_count()
            }
        };
        assert_eq!(rows, count.div_ceil(cap));
    }
}

/// P3-TST-002, P3-API-003: distinct publish topics split at the configured cap.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn publish_topic_boundary(#[case] extra: usize) {
    let api = backend();
    let cap = BACKEND_DEFAULT_MAX_PUBLISH_TOPICS;
    let topics = welcome_topics(cap + extra);
    let units = topics
        .iter()
        .map(|topic| PublishUnit::single(welcome_for(topic, vec![1])))
        .collect::<crate::Result<Vec<_>>>()
        .unwrap();
    let metas = api.publish_units(units).await.unwrap();
    assert_eq!(metas.len(), topics.len());
    for (meta, topic) in metas.iter().zip(topics) {
        assert_eq!(meta.topic.as_ref().unwrap().topic, topic.cloned_vec());
    }
    assert_eq!(
        api.api_client.mls_stats().publish.get_count(),
        (cap + extra).div_ceil(cap)
    );
}

/// P3-TST-002, API-031: envelope count alone does not split a publish.
#[rstest]
#[case(BACKEND_DEFAULT_MAX_PUBLISH_TOPICS)]
#[xmtp_common::test(unwrap_try = true)]
async fn publish_envelope_count_has_no_separate_cap(#[case] topic_cap: usize) {
    let api = backend();
    let topic = welcome_topics(1).remove(0);
    for count in [topic_cap, topic_cap + 1] {
        let units = (0..count)
            .map(|index| PublishUnit::single(welcome_for(&topic, index.to_be_bytes().to_vec())))
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();
        let before = api.api_client.mls_stats().publish.get_count();
        let metas = api.publish_units(units).await.unwrap();
        let unique: HashSet<_> = metas
            .iter()
            .map(|meta| meta.cursor.as_ref().unwrap().sequence_id)
            .collect();
        assert_eq!(unique.len(), count);
        assert_eq!(api.api_client.mls_stats().publish.get_count() - before, 1);
    }
}

/// P3-TST-002, P3-API-003: measured request bytes split at the exact byte cap.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn publish_byte_boundary(#[case] extra: usize) {
    let api = backend();
    let target = BACKEND_DEFAULT_MAX_REQUEST_BYTES + extra;
    let topic = welcome_topics(1).remove(0);
    let large = sized_welcome(&topic, BACKEND_DEFAULT_MAX_ENVELOPE_BYTES);
    let field_bytes = wire::PublishRequest {
        envelopes: vec![large.clone()],
    }
    .encoded_len();
    let count = target / field_bytes;
    let mut envelopes = vec![large; count];
    let remainder = target - count * field_bytes;
    // The final repeated field includes its tag and length delimiter.
    let mut last_bytes = remainder;
    loop {
        let last = sized_welcome(&topic, last_bytes);
        let measured = wire::PublishRequest {
            envelopes: vec![last.clone()],
        }
        .encoded_len();
        if measured == remainder {
            envelopes.push(last);
            break;
        }
        last_bytes -= measured - remainder;
    }
    assert_eq!(
        wire::PublishRequest {
            envelopes: envelopes.clone()
        }
        .encoded_len(),
        target
    );
    let units = envelopes
        .into_iter()
        .map(PublishUnit::single)
        .collect::<crate::Result<Vec<_>>>()
        .unwrap();
    let expected = units.len();
    let metas = api.publish_units(units).await.unwrap();
    assert_eq!(metas.len(), expected);
    assert_eq!(api.api_client.mls_stats().publish.get_count(), 1 + extra);
}

/// P3-TST-002: envelope bytes are accepted at the cap and rejected once above it.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn envelope_byte_boundary(#[case] extra: usize) {
    let api = backend();
    let topic = welcome_topics(1).remove(0);
    let envelope = sized_welcome(&topic, BACKEND_DEFAULT_MAX_ENVELOPE_BYTES + extra);
    // The thin client exposes the server rejection. The wrapper rejects locally.
    let result = api
        .api_client
        .publish(wire::PublishRequest {
            envelopes: vec![envelope.clone()],
        })
        .await;
    if extra == 0 {
        assert_eq!(result.unwrap().envelope_metas.len(), 1);
        assert!(PublishUnit::single(envelope).is_ok());
    } else {
        assert_eq!(
            grpc_status(&result.unwrap_err()).unwrap().code(),
            tonic::Code::InvalidArgument
        );
        assert!(matches!(
            PublishUnit::single(envelope),
            Err(ApiError::EnvelopeTooLarge)
        ));
    }
    assert_eq!(api.api_client.mls_stats().publish.get_count(), 1);
}

/// P3-TST-002, P3-API-012: lookup chunks retain all duplicate positions.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_identifier_boundary(#[case] extra: usize) {
    let api = backend();
    let cap = BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS;
    let identifier = ApiIdentifier {
        identifier: xmtp_common::rand_account_address(),
        identifier_kind: IdentifierKind::Ethereum,
    };
    let ids = api
        .get_inbox_ids(vec![identifier; cap + extra])
        .await
        .unwrap();
    assert_eq!(ids, vec![None; cap + extra]);
    assert_eq!(
        api.api_client.identity_stats().get_inbox_ids.get_count(),
        (cap + extra).div_ceil(cap)
    );
}

/// P3-TST-002, P3-API-007: SCW signature chunks preserve one result per input.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn scw_signature_boundary(#[case] extra: usize) {
    use xmtp_cryptography::ethereum::{
        address_from_pubkey, public_key_uncompressed, sign_recoverable, zeroizing_private_key,
    };
    let api = backend();
    let cap = BACKEND_DEFAULT_MAX_SCW_SIGNATURES;
    let key = xmtp_common::rand_array::<32>();
    let public = public_key_uncompressed(zeroizing_private_key(&key).unwrap()).unwrap();
    let address = address_from_pubkey(&public).unwrap();
    let hash = xmtp_common::rand_array::<32>();
    let signed = sign_recoverable(&hash, zeroizing_private_key(&key).unwrap(), false).unwrap();
    let signature = wire::verify_smart_contract_wallet_signatures_request::Signature {
        account_id: format!("eip155:31337:{address}"),
        block_number: None,
        hash: hash.to_vec(),
        signature: signed.to_vec(),
    };
    let response = api
        .verify_smart_contract_wallet_signatures(wire::VerifySmartContractWalletSignaturesRequest {
            signatures: vec![signature; cap + extra],
        })
        .await
        .unwrap();
    assert_eq!(response.responses.len(), cap + extra);
    assert!(response.responses.iter().all(|result| result.is_valid));
    assert_eq!(
        api.api_client
            .identity_stats()
            .verify_smart_contract_wallet_signatures
            .get_count(),
        (cap + extra).div_ceil(cap)
    );
}

/// P3-TST-002, P3-API-006: the server clamps rows and paging returns each row once.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn query_row_clamp_boundary(#[case] extra: usize) {
    let api = backend();
    let topics = welcome_topics(2);
    let count = BACKEND_DEFAULT_MAX_QUERY_LIMIT + extra;
    let envelopes: Vec<_> = (0..count)
        .map(|index| welcome_for(&topics[index % topics.len()], index.to_be_bytes().to_vec()))
        .collect();
    let units = envelopes
        .iter()
        .cloned()
        .map(PublishUnit::single)
        .collect::<crate::Result<Vec<_>>>()
        .unwrap();
    let metas = api.publish_units(units).await.unwrap();
    let expected: HashSet<_> = metas
        .iter()
        .map(|meta| meta.cursor.as_ref().unwrap().sequence_id)
        .collect();
    let initial: TopicCursor = topics.into_iter().map(|topic| (topic, Cursor(0))).collect();
    let mut cursors = initial.clone();
    let mut seen = HashSet::new();
    let mut server_cap = None;
    loop {
        let response = api
            .api_client
            .query(wire::QueryRequest {
                queries: cursors
                    .iter()
                    .map(|(topic, cursor)| wire::TopicQuery {
                        topic: Some(wire::Topic {
                            topic: topic.cloned_vec(),
                        }),
                        cursor: Some((*cursor).into()),
                    })
                    .collect(),
                limit: (BACKEND_DEFAULT_MAX_QUERY_LIMIT + extra) as u32,
            })
            .await
            .unwrap();
        let cap = *server_cap.get_or_insert(response.envelopes.len());
        assert!(
            cap > 0 && cap < BACKEND_DEFAULT_MAX_QUERY_LIMIT,
            "the test stack must clamp a saturated response"
        );
        assert!(response.envelopes.len() <= cap);
        for row in response.envelopes {
            let meta = row.meta.unwrap();
            let sequence = meta.cursor.unwrap().sequence_id;
            assert!(seen.insert(sequence), "paging returned a duplicate");
            *cursors
                .get_mut(&Topic::parse(&meta.topic.unwrap().topic).unwrap())
                .unwrap() = Cursor(sequence);
        }
        let has_more = response.continuation.unwrap().has_more;
        assert_eq!(has_more, seen.len() < expected.len());
        if !has_more {
            break;
        }
    }
    assert_eq!(seen, expected);
    let before = api.api_client.mls_stats().query.get_count();
    let rows = api
        .query_all(initial, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
        .await
        .unwrap();
    let actual: HashSet<_> = rows
        .iter()
        .map(|row| {
            row.meta
                .as_ref()
                .unwrap()
                .cursor
                .as_ref()
                .unwrap()
                .sequence_id
        })
        .collect();
    assert_eq!(rows.len(), expected.len());
    assert_eq!(actual, expected);
    assert_eq!(
        api.api_client.mls_stats().query.get_count() - before,
        count.div_ceil(server_cap.unwrap())
    );
}

/// P3-TST-002, P3-STR-010: static topic chunks deliver all requested topics.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn static_topic_boundary(#[case] extra: usize) {
    let api = backend();
    let topics = welcome_topics(BACKEND_DEFAULT_MAX_STATIC_TOPICS + extra);
    let units = topics
        .iter()
        .map(|topic| PublishUnit::single(welcome_for(topic, vec![1])))
        .collect::<crate::Result<Vec<_>>>()
        .unwrap();
    api.publish_units(units).await.unwrap();
    let cursors = topics
        .iter()
        .cloned()
        .map(|topic| (topic, Cursor(0)))
        .collect();
    let subscription = api
        .api_client
        .subscribe_welcome_messages_with_cursors(&cursors)
        .await
        .unwrap();
    let rows = xmtp_common::time::timeout(
        Duration::from_secs(60),
        subscription.take(topics.len()).try_collect::<Vec<_>>(),
    )
    .await
    .unwrap()
    .unwrap();
    let sequences: HashSet<_> = rows.iter().map(|row| row.sequence_id()).collect();
    assert_eq!(sequences.len(), topics.len());
    let returned: HashSet<_> = rows
        .iter()
        .map(|row| Topic::new_welcome_message(row.as_v1().unwrap().installation_key))
        .collect();
    assert_eq!(returned, topics.into_iter().collect());
    // ApiStats counts logical subscriptions; the adapter's mock test counts wires.
    assert_eq!(api.api_client.mls_stats().subscribe_static.get_count(), 1);
}

/// P3-TST-002: a full identity log rejects one more update without a retry.
#[rstest]
#[case(BACKEND_DEFAULT_MAX_IDENTITY_ENTRIES)]
#[xmtp_common::test(unwrap_try = true)]
async fn identity_entry_boundary(#[case] cap: usize) {
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::{
        builder::SignatureRequestBuilder,
        test_utils::{WalletTestExt, add_wallet_signature},
    };

    let api = backend();
    let mut wallet = generate_local_wallet();
    let inbox = wallet.get_inbox_id(0);
    let mut request = SignatureRequestBuilder::new(&inbox)
        .create_inbox(wallet.identifier(), 0)
        .build();
    add_wallet_signature(&mut request, &wallet).await;
    api.publish_identity_update(request.build_identity_update().unwrap())
        .await
        .unwrap();
    let mut last = Cursor(0);
    for entries in 1..=cap {
        // A new recovery address makes each signature distinct. The member set stays small.
        let next_wallet = generate_local_wallet();
        let mut request = SignatureRequestBuilder::new(&inbox)
            .change_recovery_address(wallet.identifier().into(), next_wallet.identifier())
            .build();
        add_wallet_signature(&mut request, &wallet).await;
        let before = api.api_client.mls_stats().publish.get_count();
        let result = api
            .publish_identity_update(request.build_identity_update().unwrap())
            .await;
        assert_eq!(api.api_client.mls_stats().publish.get_count() - before, 1);
        if entries < cap {
            let cursor = result.unwrap();
            assert!(cursor > last);
            last = cursor;
            wallet = next_wallet;
        } else {
            assert_eq!(
                grpc_status(&result.unwrap_err()).unwrap().code(),
                tonic::Code::InvalidArgument
            );
        }
    }
    let history = api
        .get_identity_updates_v2(vec![crate::GetIdentityUpdatesV2Filter {
            inbox_id: inbox.clone(),
            sequence_id: None,
        }])
        .await
        .unwrap();
    assert_eq!(history[&inbox].len(), cap);
    assert_eq!(
        history[&inbox]
            .last()
            .unwrap()
            .meta
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id,
        last.0
    );
}

/// P3-TST-002: response bytes succeed at the cap and surface the next byte once.
#[rstest]
#[case::at_limit(0)]
#[case::one_past(1)]
#[xmtp_common::test(unwrap_try = true)]
async fn response_byte_boundary(#[case] extra: usize) {
    let api = backend();
    let target = BACKEND_DEFAULT_MAX_RESPONSE_BYTES + extra;
    let count = BACKEND_DEFAULT_MAX_RESPONSE_BYTES / BACKEND_DEFAULT_MAX_ENVELOPE_BYTES;
    let mut topics = welcome_topics(count);
    let envelopes: Vec<_> = topics[..count - 1]
        .iter()
        .map(|topic| sized_welcome(topic, BACKEND_DEFAULT_MAX_ENVELOPE_BYTES))
        .collect();
    let units = envelopes
        .iter()
        .cloned()
        .map(PublishUnit::single)
        .collect::<crate::Result<Vec<_>>>()
        .unwrap();
    let metas = api.publish_units(units).await.unwrap();
    let mut expected = wire::QueryResponse {
        envelopes: envelopes
            .into_iter()
            .zip(&metas)
            .map(|(envelope, meta)| wire::ServerEnvelope {
                envelope: Some(envelope),
                meta: Some(meta.clone()),
            })
            .collect(),
        continuation: Some(wire::Continuation { has_more: false }),
    };
    let mut predicted_meta = metas.last().unwrap().clone();
    let mut envelope_bytes = target - expected.encoded_len();
    const METADATA_WIDTH_ATTEMPTS: usize = 10;
    for _ in 0..METADATA_WIDTH_ATTEMPTS {
        let topic = topics.last().unwrap();
        predicted_meta.topic = Some(wire::Topic {
            topic: topic.cloned_vec(),
        });
        predicted_meta.cursor.as_mut().unwrap().sequence_id += 1;
        let final_envelope = loop {
            let envelope = sized_welcome(topic, envelope_bytes);
            expected.envelopes.push(wire::ServerEnvelope {
                envelope: Some(envelope.clone()),
                meta: Some(predicted_meta.clone()),
            });
            let measured = expected.encoded_len();
            expected.envelopes.pop();
            if measured == target {
                break envelope;
            }
            if measured > target {
                envelope_bytes -= measured - target;
            } else {
                envelope_bytes += target - measured;
            }
        };
        let final_meta = api
            .publish_units(vec![PublishUnit::single(final_envelope.clone()).unwrap()])
            .await
            .unwrap()
            .remove(0);
        expected.envelopes.push(wire::ServerEnvelope {
            envelope: Some(final_envelope),
            meta: Some(final_meta.clone()),
        });
        if expected.encoded_len() == target {
            break;
        }
        // Concurrent publishes can change the sequence ID's encoded width.
        // Use a fresh final topic so a sizing correction adds no queried row.
        expected.envelopes.pop();
        predicted_meta = final_meta;
        *topics.last_mut().unwrap() = welcome_topics(1).remove(0);
    }
    assert_eq!(expected.encoded_len(), target);
    let before = api.api_client.mls_stats().query.get_count();
    let result = api
        .api_client
        .query(wire::QueryRequest {
            queries: topics
                .into_iter()
                .map(|topic| wire::TopicQuery {
                    topic: Some(wire::Topic {
                        topic: topic.cloned_vec(),
                    }),
                    cursor: None,
                })
                .collect(),
            limit: BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32,
        })
        .await;
    if extra == 0 {
        let response = result.unwrap();
        assert_eq!(response.encoded_len(), target);
        assert_eq!(response.envelopes.len(), expected.envelopes.len());
        let returned: HashSet<_> = response
            .envelopes
            .iter()
            .map(|row| {
                row.meta
                    .as_ref()
                    .unwrap()
                    .cursor
                    .as_ref()
                    .unwrap()
                    .sequence_id
            })
            .collect();
        let expected: HashSet<_> = expected
            .envelopes
            .iter()
            .map(|row| {
                row.meta
                    .as_ref()
                    .unwrap()
                    .cursor
                    .as_ref()
                    .unwrap()
                    .sequence_id
            })
            .collect();
        assert_eq!(returned, expected);
        assert!(!response.continuation.unwrap().has_more);
    } else {
        assert!(matches!(
            grpc_status(&result.unwrap_err()).unwrap().code(),
            tonic::Code::OutOfRange | tonic::Code::ResourceExhausted
        ));
    }
    assert_eq!(api.api_client.mls_stats().query.get_count() - before, 1);
}

#[cfg(not(target_arch = "wasm32"))]
mod native;
