use super::*;
use xmtp_proto::types::IncomingBatchLimits;

fn row(topic: &Topic, sequence: u64) -> wire::ServerEnvelope {
    let mut row = query_row(topic, sequence);
    row.meta.as_mut().unwrap().message_hash = Some(wire::MessageHash {
        hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
    });
    row
}

#[xmtp_common::test(unwrap_try = true)]
async fn ordered_query_returns_one_bounded_page_with_its_start_cursor() {
    let topic = Topic::new_group_message([1; 32]);
    let expected = topic.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_query_raw_envelopes()
        .times(1)
        .returning(move |request| {
            assert_eq!(request.limit, 2);
            assert_eq!(request.queries[0].cursor.as_ref().unwrap().sequence_id, 5);
            Ok(wire::QueryResponse {
                envelopes: vec![row(&expected, 10), row(&expected, 90)],
                continuation: Some(wire::Continuation { has_more: true }),
            }
            .into())
        });
    let page = wrapper(mock)
        .query_ordered_page(
            [(topic.clone(), Cursor(5))].into(),
            20,
            IncomingBatchLimits {
                max_rows: 2,
                max_bytes: 4096,
            },
        )
        .await?;
    assert!(page.has_more);
    assert_eq!(page.batches.len(), 1);
    assert_eq!(page.batches[0].after, Cursor(5));
    assert_eq!(page.batches[0].topic, topic);
    assert_eq!(page.batches[0].envelopes.len(), 2);
}

// verifies: PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn ordered_query_reduces_the_page_before_exceeding_the_byte_limit() {
    let topic = Topic::new_group_message([1; 32]);
    let expected = topic.clone();
    let limit = row(&topic, 10).encoded_len();
    let mut calls = 0;
    let mut mock = MockBackendClient::new();
    mock.expect_query_raw_envelopes()
        .times(3)
        .returning(move |request| {
            assert_eq!(request.limit, [4, 2, 1][calls]);
            calls += 1;
            Ok(wire::QueryResponse {
                envelopes: (1..=request.limit)
                    .map(|index| row(&expected, u64::from(index) * 10))
                    .collect(),
                continuation: Some(wire::Continuation { has_more: true }),
            }
            .into())
        });
    let page = wrapper(mock)
        .query_ordered_page(
            [(topic, Cursor(0))].into(),
            4,
            IncomingBatchLimits {
                max_rows: 4,
                max_bytes: limit,
            },
        )
        .await?;
    assert!(page.has_more);
    assert_eq!(page.batches[0].envelopes.len(), 1);
}

/// The Query decoder keeps unknown fields, and paging charges received bytes.
// verifies: PROC-002, PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn ordered_query_keeps_received_bytes_and_charges_their_size() {
    use xmtp_proto::{api::mock::MockNetworkClient, types::future_welcome_envelope};
    let topic = Topic::new_welcome_message([1; 32].into());
    let expected = topic.clone();
    let raw = |topic: &Topic, sequence: u64| {
        future_welcome_envelope(&row(topic, sequence).meta.unwrap(), &[9; 40])
    };
    let decoded = wire::ServerEnvelope::decode(raw(&topic, 10))?.encoded_len();
    // Two decoded envelopes fit `2 * decoded` bytes; two received envelopes do not.
    assert!(decoded < raw(&topic, 10).len() && raw(&topic, 10).len() <= 2 * decoded);
    let limits = [2 * decoded, decoded];
    let mut calls = 0;
    let mut mock = MockNetworkClient::new();
    mock.expect_request().times(4).returning(move |_, _, body| {
        let request = wire::QueryRequest::decode(body)?;
        assert_eq!(request.limit, [2, 1, 2, 1][calls]);
        calls += 1;
        let response = xmtp_proto::types::ReceivedQueryResponse {
            envelopes: (1..=u64::from(request.limit))
                .map(|i| raw(&expected, i * 10))
                .collect(),
            continuation: Some(wire::Continuation { has_more: true }),
        };
        Ok(http::Response::new(response.encode_to_vec().into()))
    });
    let api = ApiClientWrapper::new(xmtp_api_backend::BackendClient::new(mock), Retry::default());
    let read = |max_bytes| {
        api.query_ordered_page(
            [(topic.clone(), Cursor(0))].into(),
            2,
            IncomingBatchLimits {
                max_rows: 2,
                max_bytes,
            },
        )
    };
    let page = read(limits[0]).await?;
    let envelope = &page.batches[0].envelopes;
    assert_eq!(envelope.len(), 1);
    assert_eq!(envelope[0].raw(), &raw(&topic, 10));
    assert_eq!(envelope[0].meta(), row(&topic, 10).meta.as_ref());
    // One received envelope above the limit fails even though its decoded form fits.
    assert!(matches!(
        read(limits[1]).await,
        Err(ApiError::Envelope(
            xmtp_api_backend::envelope::EnvelopeError::Capacity
        ))
    ));
}

// verifies: PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn one_envelope_above_the_byte_limit_fails_without_skipping_it() {
    let topic = Topic::new_group_message([1; 32]);
    let expected = topic.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_query_raw_envelopes()
        .times(1)
        .returning(move |_| {
            Ok(wire::QueryResponse {
                envelopes: vec![row(&expected, 10)],
                continuation: Some(wire::Continuation { has_more: false }),
            }
            .into())
        });
    assert!(matches!(
        wrapper(mock)
            .query_ordered_page(
                [(topic, Cursor(0))].into(),
                1,
                IncomingBatchLimits {
                    max_rows: 1,
                    max_bytes: 1
                }
            )
            .await,
        Err(ApiError::Envelope(
            xmtp_api_backend::envelope::EnvelopeError::Capacity
        ))
    ));
}

/// The backend fails a whole read that names a 17-byte group topic. Each
/// read helper refuses one before it sends. The mock expects no call, so a
/// request would panic.
#[xmtp_common::test(unwrap_try = true)]
async fn reads_naming_a_legacy_group_topic_are_refused_before_sending() {
    use xmtp_proto::types::GroupId;
    let api = wrapper(MockBackendClient::new());
    let legacy = Topic::new_group_message([1; 16]);
    let keyed = Topic::new_group_message([2; 32]);
    let refused = |result: &std::result::Result<(), ApiError>| {
        matches!(result, Err(ApiError::ProtoConversion(_)))
    };

    assert!(refused(
        &api.get_newest_message_metadata(&[GroupId::from([1; 16]), GroupId::from([2; 32])])
            .await
            .map(|_| ())
    ));
    assert!(refused(
        &api.newest_topic_cursors(vec![keyed.clone(), legacy.clone()])
            .await
            .map(|_| ())
    ));
    assert!(refused(
        &api.query_all(
            [(keyed.clone(), Cursor(0)), (legacy.clone(), Cursor(0))].into(),
            10
        )
        .await
        .map(|_| ())
    ));
    assert!(refused(
        &api.query_ordered_page(
            [(keyed, Cursor(0)), (legacy, Cursor(0))].into(),
            10,
            IncomingBatchLimits {
                max_rows: 1,
                max_bytes: 4096,
            },
        )
        .await
        .map(|_| ())
    ));
}

// verifies: PROC-015
#[xmtp_common::test(unwrap_try = true)]
async fn newest_targets_keep_absent_topics_at_zero() {
    let present = Topic::new_group_message([1; 32]);
    let absent = Topic::new_group_message([2; 32]);
    let expected = present.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_query_newest()
        .times(1)
        .returning(move |request| {
            assert!(!request.include_full_envelope);
            assert_eq!(request.topics.len(), 2);
            Ok(wire::QueryNewestResponse {
                results: vec![wire::query_newest_response::Result {
                    topic: Some(wire::Topic {
                        topic: expected.cloned_vec(),
                    }),
                    meta: row(&expected, 90).meta,
                    envelope: None,
                }],
            })
        });
    let targets = wrapper(mock)
        .newest_topic_cursors(vec![present.clone(), absent.clone()])
        .await?;
    assert_eq!(targets[&present], Cursor(90));
    assert_eq!(targets[&absent], Cursor(0));
}
