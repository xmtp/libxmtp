#![allow(clippy::unwrap_used)]
use crate::{ApiClientWrapper, ApiError, PublishUnit, chunk::chunk_publish};
use prost::Message;
use rstest::rstest;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use xmtp_api_d14n::MockBackendClient;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::{ExponentialBackoff, Retry, RetryableError, time::Duration};
use xmtp_configuration::*;
use xmtp_mls_validation::{parse_envelope, test_utils::*};
use xmtp_proto::{
    api::{ApiClientError, grpc_status},
    backend_v1 as wire,
    types::{ApiIdentifier, Cursor, InstallationId, Topic, canonical_envelope},
    xmtp::identity::associations::IdentifierKind,
};

mod integration;

fn wrapper(mock: MockBackendClient) -> ApiClientWrapper<MockBackendClient> {
    let strategy = ExponentialBackoff::builder()
        .duration(Duration::ZERO)
        .max_jitter(Duration::ZERO)
        .build();
    ApiClientWrapper::new(
        mock,
        Retry::builder().with_strategy(strategy).retries(2).build(),
    )
}
fn status(code: tonic::Code) -> ApiClientError {
    ApiClientError::client(GrpcError::Status(tonic::Status::new(
        code,
        "arbitrary message",
    )))
}
fn meta(envelope: &wire::ClientEnvelope, sequence: u64) -> wire::EnvelopeMeta {
    let parsed = parse_envelope(envelope.clone()).unwrap();
    wire::EnvelopeMeta {
        cursor: Some(wire::Cursor {
            sequence_id: sequence,
        }),
        server_ns: 123,
        message_hash: Some(wire::MessageHash {
            hash: Some(wire::message_hash::Hash::Sha256(
                parsed.canonical.hash.to_vec(),
            )),
        }),
        topic: Some(wire::Topic {
            topic: parsed.topic.cloned_vec(),
        }),
        expiry_ns: 987,
        is_commit_or_proposal: parsed.is_commit_or_proposal,
    }
}
fn published(request: wire::PublishRequest) -> wire::PublishResponse {
    wire::PublishResponse {
        envelope_metas: request
            .envelopes
            .iter()
            .enumerate()
            .map(|(i, e)| meta(e, i as u64 + 1))
            .collect(),
    }
}
fn welcome(index: usize) -> wire::ClientEnvelope {
    let mut id = [0; BACKEND_INSTALLATION_ID_BYTES];
    id[..8].copy_from_slice(&(index as u64).to_be_bytes());
    inline_welcome_envelope(id)
}
fn query_row(topic: &Topic, sequence: u64) -> wire::ServerEnvelope {
    wire::ServerEnvelope {
        meta: Some(wire::EnvelopeMeta {
            topic: Some(wire::Topic {
                topic: topic.cloned_vec(),
            }),
            cursor: Some(wire::Cursor {
                sequence_id: sequence,
            }),
            ..Default::default()
        }),
        envelope: None,
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn publish_retries_identical_canonical_bytes_and_returns_metadata() {
    let envelope = welcome(1);
    let bytes = canonical_envelope(&envelope).bytes;
    let mut mock = MockBackendClient::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    mock.expect_publish().times(3).returning(move |request| {
        assert_eq!(request.envelopes[0].encode_to_vec(), bytes);
        if count.fetch_add(1, Ordering::SeqCst) < 2 {
            Err(status(tonic::Code::Unavailable))
        } else {
            Ok(published(request))
        }
    });
    let result = wrapper(mock)
        .publish_units(vec![PublishUnit::single(envelope.clone())?])
        .await?;
    assert_eq!(result, vec![meta(&envelope, 1)]);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[xmtp_common::test(unwrap_try = true)]
async fn publish_hash_mismatch_is_terminal() {
    let mut mock = MockBackendClient::new();
    mock.expect_publish().times(1).returning(|request| {
        let mut response = published(request);
        response.envelope_metas[0].message_hash = Some(wire::MessageHash {
            hash: Some(wire::message_hash::Hash::Sha256(vec![0; 32])),
        });
        Ok(response)
    });
    let error = wrapper(mock)
        .publish_units(vec![PublishUnit::single(welcome(1))?])
        .await
        .unwrap_err();
    assert!(matches!(error, ApiError::HashMismatch));
    assert!(!error.is_retryable());
}

fn size_status(code: tonic::Code) -> ApiClientError {
    if code != tonic::Code::InvalidArgument {
        return status(code);
    }
    let detail = wire::PublishError {
        index: None,
        reason: wire::publish_error::Reason::TooLarge as i32,
        message: "not used".into(),
    };
    let details = tonic_types::pb::Status {
        code: code as i32,
        message: "not used".into(),
        details: vec![prost_types::Any {
            type_url: "type.googleapis.com/xmtp.backend.v1.PublishError".into(),
            value: detail.encode_to_vec(),
        }],
    };
    ApiClientError::client(GrpcError::Status(tonic::Status::with_details(
        code,
        "not used",
        details.encode_to_vec().into(),
    )))
}

#[rstest]
#[case(tonic::Code::OutOfRange)]
#[case(tonic::Code::ResourceExhausted)]
#[case(tonic::Code::InvalidArgument)]
#[xmtp_common::test(unwrap_try = true)]
async fn publish_size_errors_split_between_atomic_units(#[case] code: tonic::Code) {
    let first = vec![
        group_message_envelope([1; 16], GroupMessageKind::Proposal, []),
        group_message_envelope([1; 16], GroupMessageKind::Commit, []),
    ];
    let second = vec![
        group_message_envelope([2; 16], GroupMessageKind::Proposal, []),
        group_message_envelope([2; 16], GroupMessageKind::Commit, []),
    ];
    let expected = [first.clone(), second.clone()];
    let mut mock = MockBackendClient::new();
    mock.expect_publish().times(3).returning(move |request| {
        if request.envelopes.len() > 2 {
            return Err(size_status(code));
        }
        assert!(expected.contains(&request.envelopes));
        Ok(published(request))
    });
    let result = wrapper(mock)
        .send_group_messages(vec![
            PublishUnit::new(first).unwrap(),
            PublishUnit::new(second).unwrap(),
        ])
        .await
        .unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].topic, result[1].topic);
    assert_eq!(result[2].topic, result[3].topic);
    assert_ne!(result[0].topic, result[2].topic);
}

#[rstest]
#[case(tonic::Code::OutOfRange)]
#[case(tonic::Code::InvalidArgument)]
#[xmtp_common::test(unwrap_try = true)]
async fn one_rejected_atomic_unit_stops_without_splitting(#[case] code: tonic::Code) {
    let mut mock = MockBackendClient::new();
    mock.expect_publish()
        .times(1)
        .returning(move |_| Err(size_status(code)));
    assert!(
        wrapper(mock)
            .publish_units(vec![
                PublishUnit::new(vec![welcome(1), welcome(2)]).unwrap()
            ])
            .await
            .is_err()
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn publish_chunks_measure_bytes_and_distinct_topics() {
    let mut large = welcome(1);
    let Some(wire::client_envelope::Payload::WelcomeMessage(message)) = large.payload.as_mut()
    else {
        unreachable!()
    };
    let Some(wire::welcome_message::Version::V1(message)) = message.version.as_mut() else {
        unreachable!()
    };
    message.data = vec![0; BACKEND_DEFAULT_MAX_ENVELOPE_BYTES];
    assert!(matches!(
        PublishUnit::single(large.clone()),
        Err(ApiError::EnvelopeTooLarge)
    ));
    let excess = large.encoded_len() - BACKEND_DEFAULT_MAX_ENVELOPE_BYTES;
    let Some(wire::client_envelope::Payload::WelcomeMessage(message)) = large.payload.as_mut()
    else {
        unreachable!()
    };
    let Some(wire::welcome_message::Version::V1(message)) = message.version.as_mut() else {
        unreachable!()
    };
    message.data.truncate(message.data.len() - excess);
    assert_eq!(large.encoded_len(), BACKEND_DEFAULT_MAX_ENVELOPE_BYTES);
    let count = BACKEND_DEFAULT_MAX_REQUEST_BYTES / BACKEND_DEFAULT_MAX_ENVELOPE_BYTES + 1;
    let units = (0..count)
        .map(|_| PublishUnit::single(large.clone()))
        .collect::<crate::Result<Vec<_>>>()?;
    let chunks = chunk_publish(&units)?;
    assert!(chunks.len() > 1);
    for chunk in chunks {
        assert!(crate::chunk::request(chunk).encoded_len() <= BACKEND_DEFAULT_MAX_REQUEST_BYTES);
    }
    assert!(matches!(
        PublishUnit::new(vec![large; count]),
        Err(ApiError::UnitTooLarge)
    ));
    let units = (0..=BACKEND_DEFAULT_MAX_PUBLISH_TOPICS)
        .map(|index| PublishUnit::single(welcome(index)))
        .collect::<crate::Result<Vec<_>>>()?;
    let chunks = chunk_publish(&units)?;
    assert_eq!(
        chunks.iter().map(|chunk| chunk.len()).collect::<Vec<_>>(),
        vec![BACKEND_DEFAULT_MAX_PUBLISH_TOPICS, 1]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn query_pages_three_times_with_independent_topic_cursors() {
    let a = Topic::new_group_message([1; 16]);
    let b = Topic::new_group_message([2; 16]);
    let requested = HashMap::from([(a.clone(), Cursor(0)), (b.clone(), Cursor(0))]);
    let mut expected = requested.clone();
    let mut page = 0;
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(3).returning(move |request| {
        let actual: HashMap<_, _> = request
            .queries
            .iter()
            .map(|query| {
                (
                    Topic::parse(&query.topic.as_ref().unwrap().topic).unwrap(),
                    Cursor(query.cursor.as_ref().unwrap().sequence_id),
                )
            })
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(request.limit, 2);
        let rows = match page {
            0 => vec![query_row(&a, 10), query_row(&b, 5)],
            1 => vec![query_row(&b, 8)],
            _ => vec![query_row(&a, 20)],
        };
        for row in &rows {
            let meta = row.meta.as_ref().unwrap();
            expected.insert(
                Topic::parse(&meta.topic.as_ref().unwrap().topic).unwrap(),
                Cursor(meta.cursor.as_ref().unwrap().sequence_id),
            );
        }
        page += 1;
        Ok(wire::QueryResponse {
            envelopes: rows,
            continuation: Some(wire::Continuation { has_more: page < 3 }),
        })
    });
    let rows = wrapper(mock).query_all(requested, 2).await?;
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .map(|row| row
                .meta
                .as_ref()
                .unwrap()
                .cursor
                .as_ref()
                .unwrap()
                .sequence_id)
            .collect::<Vec<_>>(),
        vec![10, 5, 8, 20]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn oversized_query_reduces_limit_before_splitting_topics() {
    let a = Topic::new_group_message([1; 16]);
    let b = Topic::new_group_message([2; 16]);
    let mut calls = Vec::new();
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(6).returning(move |request| {
        calls.push((request.limit, request.queries.len()));
        if request.limit > 1 || request.queries.len() > 1 {
            return Err(size_status(tonic::Code::OutOfRange));
        }
        assert_eq!(&calls[..4], &[(8, 2), (4, 2), (2, 2), (1, 2)]);
        Ok(wire::QueryResponse {
            envelopes: vec![],
            continuation: Some(wire::Continuation { has_more: false }),
        })
    });
    assert!(
        wrapper(mock)
            .query_all(HashMap::from([(a, Cursor(0)), (b, Cursor(0))]), 8)
            .await?
            .is_empty()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn query_rejects_has_more_without_progress() {
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(1).returning(|_| {
        Ok(wire::QueryResponse {
            envelopes: vec![],
            continuation: Some(wire::Continuation { has_more: true }),
        })
    });
    assert!(matches!(
        wrapper(mock)
            .query_all(
                HashMap::from([(Topic::new_group_message([1; 16]), Cursor(0))]),
                1
            )
            .await,
        Err(ApiError::InvalidResponse(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn key_packages_are_keyed_and_absence_is_explicit() {
    let fixture = key_package_envelope("ab".repeat(32), Default::default());
    let present: InstallationId = fixture.installation_id.as_slice().try_into()?;
    let absent: InstallationId = [99; 32].into();
    let envelope = fixture.envelope.clone();
    let topic = Topic::new_key_package(present);
    let mut mock = MockBackendClient::new();
    mock.expect_query_newest()
        .times(1)
        .returning(move |request| {
            assert!(request.include_full_envelope);
            assert_eq!(request.topics.len(), 2);
            Ok(wire::QueryNewestResponse {
                results: vec![wire::query_newest_response::Result {
                    topic: Some(wire::Topic {
                        topic: topic.cloned_vec(),
                    }),
                    meta: Some(meta(&envelope, 1)),
                    envelope: Some(envelope.clone()),
                }],
            })
        });
    let found = wrapper(mock)
        .fetch_key_packages(&[present, absent, present])
        .await?;
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[&present].as_ref().unwrap().key_package_tls_serialized,
        fixture.tls_bytes
    );
    assert_eq!(found[&absent], None);
}

#[rstest]
#[case(true, BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS)]
#[case(false, BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS)]
#[xmtp_common::test(unwrap_try = true)]
async fn newest_respects_each_topic_limit(#[case] full: bool, #[case] cap: usize) {
    let topics = (0..=cap)
        .map(|index| parse_envelope(welcome(index)).unwrap().topic)
        .collect();
    let mut mock = MockBackendClient::new();
    mock.expect_query_newest()
        .times(2)
        .returning(move |request| {
            assert_eq!(request.include_full_envelope, full);
            assert!(request.topics.len() <= cap);
            Ok(Default::default())
        });
    assert!(wrapper(mock).newest(topics, full).await.unwrap().is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn inbox_lookup_chunks_and_preserves_duplicates_and_absence() {
    let identifiers: Vec<_> = (0..=BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS)
        .map(|index| ApiIdentifier {
            identifier: format!("key{}", index % 3),
            identifier_kind: IdentifierKind::Passkey,
        })
        .collect();
    let expected: Vec<_> = identifiers
        .iter()
        .map(|id| {
            if id.identifier == "key1" {
                None
            } else {
                Some(id.identifier.clone())
            }
        })
        .collect();
    let mut mock = MockBackendClient::new();
    mock.expect_get_inbox_ids().times(2).returning(|request| {
        assert!(request.requests.len() <= BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS);
        Ok(wire::GetInboxIdsResponse {
            responses: request
                .requests
                .into_iter()
                .map(|id| wire::get_inbox_ids_response::Response {
                    inbox_id: if id.identifier == "key1" {
                        None
                    } else {
                        Some(id.identifier.clone())
                    },
                    identifier: id.identifier,
                    identifier_kind: id.identifier_kind,
                })
                .collect(),
        })
    });
    assert_eq!(wrapper(mock).get_inbox_ids(identifiers).await?, expected);
}

#[rstest]
#[case(0)]
#[case(999)]
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_lookup_rejects_unknown_response_kind(#[case] kind: i32) {
    let mut mock = MockBackendClient::new();
    mock.expect_get_inbox_ids().times(1).returning(move |_| {
        Ok(wire::GetInboxIdsResponse {
            responses: vec![wire::get_inbox_ids_response::Response {
                identifier: "key".into(),
                identifier_kind: kind,
                inbox_id: None,
            }],
        })
    });
    assert!(matches!(
        wrapper(mock)
            .get_inbox_ids(vec![ApiIdentifier {
                identifier: "key".into(),
                identifier_kind: IdentifierKind::Passkey
            }])
            .await,
        Err(ApiError::InvalidResponse(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn aborted_identity_publish_returns_conflict_without_retry() {
    let mut mock = MockBackendClient::new();
    mock.expect_publish()
        .times(1)
        .returning(|_| Err(status(tonic::Code::Aborted)));
    let update = xmtp_proto::xmtp::identity::associations::IdentityUpdate {
        inbox_id: "ab".repeat(32),
        ..Default::default()
    };
    assert!(matches!(
        wrapper(mock).publish_identity_update(update).await,
        Err(ApiError::IdentityUpdateConflict)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn get_does_not_retry_not_found() {
    let mut mock = MockBackendClient::new();
    mock.expect_get().times(1).returning(|request| {
        assert_eq!(request.sequence_id, 23);
        Err(status(tonic::Code::NotFound))
    });
    let error = wrapper(mock).get_envelope(23).await.unwrap_err();
    assert_eq!(grpc_status(&error).unwrap().code(), tonic::Code::NotFound);
}

#[xmtp_common::test(unwrap_try = true)]
fn group_decoder_keeps_payload_and_envelope_hashes_separate() {
    for kind in [
        GroupMessageKind::Application,
        GroupMessageKind::Proposal,
        GroupMessageKind::Commit,
    ] {
        let envelope = group_message_envelope(GROUP_ID, kind, [0xaa, 0xbb]);
        let Some(wire::client_envelope::Payload::GroupMessage(group)) = &envelope.payload else {
            unreachable!()
        };
        let metadata = meta(&envelope, 9);
        let decoded = xmtp_api_d14n::envelope::decode_group_message(wire::ServerEnvelope {
            meta: Some(metadata.clone()),
            envelope: Some(envelope.clone()),
        })?;
        assert_eq!(decoded.sequence_id(), 9);
        assert_eq!(decoded.timestamp(), metadata.server_ns as i64);
        assert_eq!(decoded.group_id.as_ref(), &GROUP_ID);
        assert_eq!(decoded.payload_hash, xmtp_common::sha256_array(&group.data));
        assert_eq!(
            decoded.envelope_hash,
            Some(canonical_envelope(&envelope).hash.to_vec())
        );
        assert_ne!(decoded.payload_hash, decoded.envelope_hash.unwrap());
        assert_eq!(decoded.expiry_ns, Some(metadata.expiry_ns));
        assert_eq!(decoded.sender_hmac, group.sender_hmac);
        assert_eq!(decoded.should_push, group.should_push);
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn welcome_decoder_retains_pointer_payload() {
    let mut envelope = welcome_pointer_envelope(INSTALLATION_ID);
    let Some(wire::client_envelope::Payload::WelcomeMessage(welcome)) = envelope.payload.as_mut()
    else {
        unreachable!()
    };
    let Some(wire::welcome_message::Version::WelcomePointer(pointer)) = welcome.version.as_mut()
    else {
        unreachable!()
    };
    pointer.wrapper_algorithm =
        xmtp_proto::xmtp::mls::message_contents::WelcomePointerWrapperAlgorithm::XwingMlkem768Draft6
            as i32;
    let metadata = meta(&envelope, 8);
    let decoded = xmtp_api_d14n::envelope::decode_welcome_message(wire::ServerEnvelope {
        meta: Some(metadata),
        envelope: Some(envelope),
    })?;
    let xmtp_proto::types::WelcomeMessageType::WelcomePointer(pointer) = decoded.variant else {
        panic!("expected welcome pointer")
    };
    assert_eq!(pointer.installation_key, INSTALLATION_ID);
    assert_eq!(pointer.welcome_pointer, vec![0x20, 0x21]);
    assert_eq!(pointer.hpke_public_key, vec![0x22, 0x23]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn query_splits_at_the_topic_limit() {
    let cursors = (0..=BACKEND_DEFAULT_MAX_QUERY_TOPICS)
        .map(|index| (parse_envelope(welcome(index)).unwrap().topic, Cursor(0)))
        .collect();
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(2).returning(|request| {
        assert!(request.queries.len() <= BACKEND_DEFAULT_MAX_QUERY_TOPICS);
        Ok(wire::QueryResponse {
            envelopes: vec![],
            continuation: Some(wire::Continuation { has_more: false }),
        })
    });
    let result = wrapper(mock)
        .query_all(cursors, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
        .await?;
    assert!(result.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn newest_size_errors_split_until_one_topic() {
    let mut mock = MockBackendClient::new();
    mock.expect_query_newest().times(3).returning(|request| {
        if request.topics.len() > 1 {
            Err(size_status(tonic::Code::ResourceExhausted))
        } else {
            Ok(Default::default())
        }
    });
    let result = wrapper(mock)
        .newest(
            vec![
                Topic::new_group_message([1; 16]),
                Topic::new_group_message([2; 16]),
            ],
            true,
        )
        .await?;
    assert!(result.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn signature_checks_chunk_and_keep_result_order() {
    let signatures: Vec<_> = (0..=BACKEND_DEFAULT_MAX_SCW_SIGNATURES)
        .map(
            |index| wire::verify_smart_contract_wallet_signatures_request::Signature {
                account_id: "eip155:1:0x0000000000000000000000000000000000000000".into(),
                block_number: Some(index as u64),
                hash: vec![0; 32],
                signature: vec![],
            },
        )
        .collect();
    let mut mock = MockBackendClient::new();
    mock.expect_verify_smart_contract_wallet_signatures()
        .times(2)
        .returning(|request| {
            assert!(request.signatures.len() <= BACKEND_DEFAULT_MAX_SCW_SIGNATURES);
            Ok(wire::VerifySmartContractWalletSignaturesResponse {
                responses: request
                    .signatures
                    .into_iter()
                    .map(|signature| {
                        wire::verify_smart_contract_wallet_signatures_response::Response {
                            block_number: signature.block_number,
                            is_valid: true,
                            error: None,
                        }
                    })
                    .collect(),
            })
        });
    let response = wrapper(mock)
        .verify_smart_contract_wallet_signatures(wire::VerifySmartContractWalletSignaturesRequest {
            signatures,
        })
        .await?;
    assert_eq!(
        response
            .responses
            .iter()
            .map(|response| response.block_number.unwrap())
            .collect::<Vec<_>>(),
        (0..=BACKEND_DEFAULT_MAX_SCW_SIGNATURES as u64).collect::<Vec<_>>()
    );
}

fn backoff_retry() -> Retry<ExponentialBackoff> {
    Retry::builder()
        .retries(2)
        .with_strategy(
            ExponentialBackoff::builder()
                .duration(Duration::from_millis(5))
                .max_jitter(Duration::ZERO)
                .build(),
        )
        .build()
}

#[xmtp_common::test(unwrap_try = true)]
async fn single_publish_retries_resource_exhausted_with_backoff() {
    let retry = backoff_retry();
    let budget = retry.retries();
    let mut mock = MockBackendClient::new();
    mock.expect_publish()
        .times(budget + 1)
        .returning(|request| {
            assert_eq!(request.envelopes.len(), 1);
            Err(status(tonic::Code::ResourceExhausted))
        });
    let client = ApiClientWrapper::new(mock, retry);
    let started = xmtp_common::time::Instant::now();
    let error = client
        .publish_units(vec![PublishUnit::single(welcome(1))?])
        .await
        .unwrap_err();
    assert!(started.elapsed() >= Duration::from_millis(5) * budget as u32);
    assert!(error.is_retryable());
    assert_eq!(
        grpc_status(&error).unwrap().code(),
        tonic::Code::ResourceExhausted
    );
    assert_eq!(grpc_status(&error).unwrap().message(), "arbitrary message");
}

#[xmtp_common::test(unwrap_try = true)]
async fn minimum_query_retries_resource_exhausted_with_backoff() {
    let retry = backoff_retry();
    let budget = retry.retries();
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(budget + 1).returning(|request| {
        assert_eq!(request.queries.len(), 1);
        assert_eq!(request.limit, 1);
        Err(status(tonic::Code::ResourceExhausted))
    });
    let client = ApiClientWrapper::new(mock, retry);
    let started = xmtp_common::time::Instant::now();
    let error = client
        .query_all(
            HashMap::from([(Topic::new_group_message([1; 16]), Cursor(0))]),
            1,
        )
        .await
        .unwrap_err();
    assert!(started.elapsed() >= Duration::from_millis(5) * budget as u32);
    assert!(error.is_retryable());
    assert_eq!(
        grpc_status(&error).unwrap().code(),
        tonic::Code::ResourceExhausted
    );
    assert_eq!(grpc_status(&error).unwrap().message(), "arbitrary message");
}

#[rstest]
#[case(tonic::Code::OutOfRange)]
#[case(tonic::Code::InvalidArgument)]
#[xmtp_common::test(unwrap_try = true)]
async fn minimum_query_size_errors_remain_terminal(#[case] code: tonic::Code) {
    let mut mock = MockBackendClient::new();
    mock.expect_query()
        .times(1)
        .returning(move |_| Err(size_status(code)));
    let error = wrapper(mock)
        .query_all(
            HashMap::from([(Topic::new_group_message([1; 16]), Cursor(0))]),
            1,
        )
        .await
        .unwrap_err();
    assert!(!error.is_retryable());
    assert_eq!(grpc_status(&error).unwrap().code(), code);
}

#[xmtp_common::test(unwrap_try = true)]
async fn invalid_input_returns_invalid_request_without_rpc() {
    let error = PublishUnit::new(vec![]).unwrap_err();
    assert!(matches!(
        error,
        ApiError::InvalidRequest("empty publish unit")
    ));
    assert!(!error.is_retryable());
    let client = wrapper(MockBackendClient::new());
    for limit in [0, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32 + 1] {
        let error = client.query_all(HashMap::new(), limit).await.unwrap_err();
        assert!(matches!(error, ApiError::InvalidRequest("query limit")));
        assert!(!error.is_retryable());
    }
}
