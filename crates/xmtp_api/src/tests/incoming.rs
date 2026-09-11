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
    let topic = Topic::new_group_message([1; 16]);
    let expected = topic.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(1).returning(move |request| {
        assert_eq!(request.limit, 2);
        assert_eq!(request.queries[0].cursor.as_ref().unwrap().sequence_id, 5);
        Ok(wire::QueryResponse {
            envelopes: vec![row(&expected, 10), row(&expected, 90)],
            continuation: Some(wire::Continuation { has_more: true }),
        })
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

#[xmtp_common::test(unwrap_try = true)]
async fn ordered_query_reduces_the_page_before_exceeding_the_byte_limit() {
    let topic = Topic::new_group_message([1; 16]);
    let expected = topic.clone();
    let limit = row(&topic, 10).encoded_len();
    let mut calls = 0;
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(3).returning(move |request| {
        assert_eq!(request.limit, [4, 2, 1][calls]);
        calls += 1;
        Ok(wire::QueryResponse {
            envelopes: (1..=request.limit)
                .map(|index| row(&expected, u64::from(index) * 10))
                .collect(),
            continuation: Some(wire::Continuation { has_more: true }),
        })
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

#[xmtp_common::test(unwrap_try = true)]
async fn one_envelope_above_the_byte_limit_fails_without_skipping_it() {
    let topic = Topic::new_group_message([1; 16]);
    let expected = topic.clone();
    let mut mock = MockBackendClient::new();
    mock.expect_query().times(1).returning(move |_| {
        Ok(wire::QueryResponse {
            envelopes: vec![row(&expected, 10)],
            continuation: Some(wire::Continuation { has_more: false }),
        })
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

#[xmtp_common::test(unwrap_try = true)]
async fn newest_targets_keep_absent_topics_at_zero() {
    let present = Topic::new_group_message([1; 16]);
    let absent = Topic::new_group_message([2; 16]);
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
