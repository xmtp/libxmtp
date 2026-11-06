use super::*;
use xmtp_proto::types::IncomingBatchLimits;

const PAGE_ROWS: usize = 2;
const PAGE_COUNT: usize = 24;
const PAYLOAD_BYTES: usize = 2 * 1024;
const PAGE_BYTES: usize = 8 * 1024;

fn row(topic: &Topic, sequence: u64) -> wire::ServerEnvelope {
    let mut row = query_row(topic, sequence);
    row.meta.as_mut().unwrap().message_hash = Some(wire::MessageHash {
        hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
    });
    row.envelope = Some(wire::ClientEnvelope {
        payload: Some(wire::client_envelope::Payload::GroupMessage(
            wire::GroupMessage {
                data: vec![0xa5; PAYLOAD_BYTES],
                ..Default::default()
            },
        )),
    });
    row
}

#[derive(Debug, thiserror::Error)]
enum FoldError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("stop the fold")]
    Stop,
}

// Port of the whole-backlog allocation evidence: each callback sees only one page.
#[rstest]
#[case(None)]
#[case(Some(5))]
#[xmtp_common::test(unwrap_try = true)]
async fn fold_ordered_pages_hands_one_bounded_page_at_a_time(
    #[case] stop_at: Option<usize>,
) -> Result<(), FoldError> {
    let topic = Topic::new_group_message([7; 32]);
    let response_topic = topic.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let queried_pages = calls.clone();
    let expected_pages = stop_at.unwrap_or(PAGE_COUNT);
    let mut mock = MockBackendClient::new();
    mock.expect_query_raw_envelopes()
        .times(expected_pages)
        .returning(move |request| {
            let page = queried_pages.fetch_add(1, Ordering::SeqCst);
            assert_eq!(request.limit, PAGE_ROWS as u32);
            assert_eq!(request.queries.len(), 1);
            assert_eq!(
                request.queries[0].cursor.as_ref().unwrap().sequence_id,
                (page * PAGE_ROWS) as u64
            );
            Ok(wire::QueryResponse {
                envelopes: (1..=PAGE_ROWS)
                    .map(|offset| row(&response_topic, (page * PAGE_ROWS + offset) as u64))
                    .collect(),
                continuation: Some(wire::Continuation {
                    has_more: page + 1 < PAGE_COUNT,
                }),
            }
            .into())
        });

    let mut folded_pages = 0;
    let result = wrapper(mock)
        .fold_ordered_pages(
            [(topic.clone(), Cursor(0))].into(),
            IncomingBatchLimits {
                max_rows: PAGE_ROWS,
                max_bytes: PAGE_BYTES,
            },
            |batches, cursors| {
                assert_eq!(batches.len(), 1);
                let batch = &batches[0];
                assert_eq!(batch.topic, topic);
                assert_eq!(batch.after, Cursor((folded_pages * PAGE_ROWS) as u64));
                assert_eq!(batch.envelopes.len(), PAGE_ROWS);
                assert!(
                    batch
                        .envelopes
                        .iter()
                        .map(|envelope| envelope.raw().len())
                        .sum::<usize>()
                        <= PAGE_BYTES
                );
                folded_pages += 1;
                assert_eq!(cursors[&topic], Cursor((folded_pages * PAGE_ROWS) as u64));
                assert_eq!(calls.load(Ordering::SeqCst), folded_pages);
                if stop_at == Some(folded_pages) {
                    return Err(FoldError::Stop);
                }
                Ok(())
            },
        )
        .await;
    if stop_at.is_some() {
        assert!(matches!(result, Err(FoldError::Stop)));
    } else {
        result?;
    }
    assert_eq!(folded_pages, expected_pages);
    assert_eq!(calls.load(Ordering::SeqCst), expected_pages);
    Ok(())
}

#[rstest]
#[case(false)]
#[case(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn fold_ordered_pages_stops_a_removed_topic(#[case] remove_all: bool) -> crate::Result<()> {
    let a = Topic::new_group_message([1; 32]);
    let b = Topic::new_group_message([2; 32]);
    let response_a = a.clone();
    let response_b = b.clone();
    let mut queried_pages = 0;
    let mut mock = MockBackendClient::new();
    mock.expect_query_raw_envelopes()
        .times(if remove_all { 1 } else { 3 })
        .returning(move |request| {
            let positions: HashMap<_, _> = request
                .queries
                .iter()
                .map(|query| {
                    (
                        Topic::parse(&query.topic.as_ref().unwrap().topic).unwrap(),
                        query.cursor.as_ref().unwrap().sequence_id,
                    )
                })
                .collect();
            assert_eq!(positions[&response_a], queried_pages);
            let envelopes = if queried_pages == 0 {
                assert_eq!(positions[&response_b], 0);
                vec![row(&response_a, 1), row(&response_b, 10)]
            } else {
                assert_eq!(positions.len(), 1, "removed topic was requested again");
                vec![row(&response_a, queried_pages + 1)]
            };
            queried_pages += 1;
            Ok(wire::QueryResponse {
                envelopes,
                continuation: Some(wire::Continuation {
                    has_more: queried_pages < 3,
                }),
            }
            .into())
        });

    let mut folded_pages = 0;
    wrapper(mock)
        .fold_ordered_pages(
            [(a.clone(), Cursor(0)), (b.clone(), Cursor(0))].into(),
            IncomingBatchLimits {
                max_rows: PAGE_ROWS,
                max_bytes: PAGE_BYTES,
            },
            |batches, cursors| {
                folded_pages += 1;
                assert_eq!(cursors[&a], Cursor(folded_pages));
                if folded_pages == 1 {
                    assert_eq!(batches.len(), 2);
                    assert_eq!(cursors.remove(&b), Some(Cursor(10)));
                    if remove_all {
                        cursors.remove(&a);
                    }
                } else {
                    assert_eq!(batches.len(), 1);
                    assert!(!cursors.contains_key(&b));
                }
                Ok::<_, ApiError>(())
            },
        )
        .await?;
    assert_eq!(folded_pages, if remove_all { 1 } else { 3 });
    Ok(())
}
