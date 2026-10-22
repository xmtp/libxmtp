//! Correctness requirements for bounded scans, sparse fallbacks, and shared index controls.

use super::*;
use crate::db::benchmarks::{connect, corpus, partition, workload};
use crate::test_support::TestDatabase;

fn broad(topic: Vec<u8>, cursor: i64) -> Vec<TopicCursor> {
    let mut queries = (0..MIN_WINDOW_TOPICS)
        .map(|index| TopicCursor {
            topic: corpus::topic(100_000 + index as i64),
            cursor,
        })
        .collect::<Vec<_>>();
    queries[0].topic = topic;
    queries
}

/// Small and heterogeneous cursor requests retain the exact production SQL rather than paying for a speculative scan.
#[xmtp_common::test(unwrap_try = true)]
fn bounded_dispatch_preserves_small_and_mixed_controls() {
    let mut queries = broad(corpus::topic(0), 0);
    assert!(PageQuery::Bounded.uses_window(&queries));
    assert!(PageQuery::Bounded.sql(&queries).contains("scan_window"));
    queries[1].cursor = 1;
    assert_eq!(
        PageQuery::Bounded.sql(&queries),
        PageQuery::Lateral.sql(&queries)
    );
    queries.truncate(1);
    assert_eq!(
        PageQuery::Bounded.sql(&queries),
        PageQuery::Lateral.sql(&queries)
    );
    assert!(!PageQuery::Bounded.uses_window(&[]));
}

/// Dense windows and sparse matches beyond the scan budget must produce the same global prefix and exact has_more, including bigint's last ID.
#[xmtp_common::test(unwrap_try = true)]
async fn bounded_pages_match_reference_before_and_after_sparse_fallback() {
    let database = TestDatabase::new()?;
    let mut store = connect(&database, 1, 30_000).await?;
    sqlx::query("INSERT INTO envelopes (sequence_id, topic, server_ns, message_hash,
        is_commit_or_proposal, payload, push_eligible)
        SELECT id, CASE WHEN id = 15001 THEN $2::bytea WHEN mod(id,2) = 0 THEN $1::bytea ELSE $3::bytea END,
            id, sha256(int8send(id)), false, int8send(id), false FROM generate_series(1,20000::bigint) id")
        .bind(corpus::topic(0)).bind(corpus::topic(1)).bind(corpus::topic(2))
        .execute(&store.primary).await?;
    sqlx::query("INSERT INTO envelopes (sequence_id, topic, server_ns, message_hash,
        is_commit_or_proposal, payload, push_eligible) VALUES ($1, $2, 1, sha256(int8send($1)), false, int8send($1), false)")
        .bind(i64::MAX).bind(corpus::topic(1)).execute(&store.primary).await?;
    sqlx::query(
        "INSERT INTO topic_watermark SELECT topic, max(sequence_id) FROM envelopes GROUP BY topic",
    )
    .execute(&store.primary)
    .await?;
    store.page_query = PageQuery::Bounded;
    for (topic, cursor) in [
        (0, 0),
        (1, 0),
        (0, 19_998),
        (1, i64::MAX - 1),
        (1, i64::MAX),
        (999, 0),
    ] {
        let queries = broad(corpus::topic(topic), cursor);
        assert!(store.page_query.uses_window(&queries));
        let topics = queries
            .iter()
            .map(|query| query.topic.clone())
            .collect::<Vec<_>>();
        let cursors = queries.iter().map(|query| query.cursor).collect::<Vec<_>>();
        for limit in [1_i64, 100, 1_000] {
            let expected: Vec<i64> = sqlx::query_scalar("SELECT sequence_id FROM envelopes e
                JOIN unnest($1::bytea[], $2::bigint[]) wanted(topic,cursor) ON e.topic = wanted.topic
                WHERE sequence_id > cursor ORDER BY sequence_id LIMIT $3")
                .bind(&topics).bind(&cursors).bind(limit + 1).fetch_all(&store.primary).await?;
            let page = store.query(&queries, limit).await?;
            assert_eq!(
                page.has_more,
                expected.len() > limit as usize,
                "{topic}/{cursor}/{limit}"
            );
            assert_eq!(
                page.envelopes
                    .iter()
                    .map(|row| row.sequence_id)
                    .collect::<Vec<_>>(),
                expected
                    .into_iter()
                    .take(limit as usize)
                    .collect::<Vec<_>>(),
                "{topic}/{cursor}/{limit}"
            );
        }
    }
    store.primary.close().await;
}

/// Covering indexes must preserve newest metadata and real publishes; physical write copies retain the measured profile, not baseline indexes.
#[xmtp_common::test(unwrap_try = true)]
async fn covering_profile_preserves_newest_publish_and_physical_copies() {
    let database = TestDatabase::new()?;
    let store = connect(&database, 1, 30_000).await?;
    let catalog = corpus::seed(&store, 1_000, 42).await?;
    let topics = catalog
        .iter()
        .map(|topic| topic.bytes.clone())
        .collect::<Vec<_>>();
    let mut before = store.newest_metadata(&topics).await?;
    IndexProfile::MetadataCovering.install(&store).await?;
    let mut after = store.newest_metadata(&topics).await?;
    before.sort_by_key(|row| row.sequence_id);
    after.sort_by_key(|row| row.sequence_id);
    assert_eq!(
        before
            .iter()
            .map(|row| (row.sequence_id, &row.message_hash))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|row| (row.sequence_id, &row.message_hash))
            .collect::<Vec<_>>()
    );
    store.primary.close().await;
    let mut copy = database.copy()?;
    let writer = connect(&copy, 1, 30_000).await?;
    let definition: String =
        sqlx::query_scalar("SELECT pg_get_indexdef('envelopes_topic_sequence'::regclass)")
            .fetch_one(&writer.primary)
            .await?;
    assert!(
        definition.contains("INCLUDE (server_ns, expiry_ns, message_hash, is_commit_or_proposal)")
    );
    let mut pending = vec![workload::pending(topics[0].clone(), 99_999_999, 0)];
    writer.find_duplicates(&mut pending).await?;
    let published = writer.commit_publish(&mut pending, None, 30_000).await?;
    let newest = writer.newest_metadata(&[topics[0].clone()]).await?;
    assert_eq!(newest[0].sequence_id, published[0].sequence_id);
    assert_eq!(newest[0].message_hash, published[0].message_hash);
    partition::close(&writer).await;
    copy.remove()?;
}
