//! Correctness gates for the disposable partition experiment, independent of latency.

use super::*;
use crate::db::{
    TopicCursor,
    benchmarks::{corpus, experiments::PageQuery, workload},
};
use futures::future::try_join_all;
use sqlx::postgres::PgPoolOptions;

async fn fixture() -> TestResult<(TestDatabase, Store, Store)> {
    let database = TestDatabase::new()?;
    let owner = super::super::connect(&database, 2, 30_000).await?;
    install(&owner).await?;
    Partitions::new(owner.primary.clone()).ensure(1).await?;
    let primary = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::raw_sql("SET ROLE xmtp_bench_writer")
                    .execute(&mut *connection)
                    .await?;
                crate::db::configure(connection, "30000ms").await
            })
        })
        .after_release(|connection, _| Box::pin(crate::db::release(connection)))
        .connect(database.url())
        .await?;
    let writer = Store {
        read: primary.clone(),
        primary,
        partitions: Some(Arc::new(Partitions::new(owner.primary.clone()))),
        page_query: PageQuery::Lateral,
    };
    Ok((database, owner, writer))
}

async fn write(store: &Store, nonce: i64) -> TestResult<StoredMeta> {
    let mut pending = vec![workload::pending(corpus::topic(0), nonce, 0)];
    store.find_duplicates(&mut pending).await?;
    Ok(store
        .commit_publish(&mut pending, None, 30_000)
        .await?
        .remove(0))
}

/// Schema selection must never implicitly enable partitions on its unpartitioned control.
#[xmtp_common::test(unwrap_try = true)]
fn query_variants_do_not_change_schema_selection() {
    for (layout, partitioned) in [
        (Layout::Unpartitioned, false),
        (Layout::SequenceRanges, true),
    ] {
        assert_eq!(layout.is_partitioned(), partitioned);
    }
}

/// Half-open million-ID ranges cover every positive bigint, including its last partial range.
#[xmtp_common::test(unwrap_try = true)]
fn bounds_cover_edges_without_overflow() {
    assert_eq!(bounds(1)?, (1, Some(1_000_001)));
    assert_eq!(bounds(1_000_000)?, (1, Some(1_000_001)));
    assert_eq!(bounds(1_000_001)?, (1_000_001, Some(2_000_001)));
    assert_eq!(bounds(i64::MAX)?, (9_223_372_036_854_000_001, None));
    assert!(bounds(0).is_err());
    assert!(bounds(-1).is_err());
}

/// A retry after crossing a range returns the original ID; direct SQL cannot bypass global deduplication.
#[xmtp_common::test(unwrap_try = true)]
async fn uniqueness_survives_range_crossing_and_direct_child_writes() {
    let (_database, owner, writer) = fixture().await?;
    let first = write(&writer, 1).await?;
    let partitions = writer.partitions.as_ref().unwrap();
    partitions.ensure(1_000_001).await?;
    sqlx::query("SELECT setval('envelope_sequence', 1000000, true)")
        .execute(&owner.primary)
        .await?;
    let second = write(&writer, 2).await?;
    assert_eq!(second.sequence_id, 1_000_001);
    let duplicate = write(&writer, 1).await?;
    assert_eq!(duplicate.sequence_id, first.sequence_id);
    for target in ["envelopes", "envelopes_p1"] {
        let result = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "INSERT INTO public.{target}
            SELECT 1000002, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal,
            payload, push_eligible, sender_hmac FROM public.envelopes WHERE sequence_id = 1"
        )))
        .execute(&writer.primary)
        .await;
        assert_eq!(
            result
                .unwrap_err()
                .as_database_error()
                .unwrap()
                .code()
                .as_deref(),
            Some("23505")
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelope_identity")
        .fetch_one(&owner.primary)
        .await?;
    assert_eq!(count, 2);
    close(&writer).await;
    owner.primary.close().await;
}

/// Restricted writers cannot forge/remove registry state or change keys, even through child tables.
#[xmtp_common::test(unwrap_try = true)]
async fn writer_permissions_protect_registry_and_envelope_identity() {
    let (_database, owner, writer) = fixture().await?;
    write(&writer, 1).await?;
    let user: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&writer.primary)
        .await?;
    assert_eq!(user, "xmtp_bench_writer");
    for statement in [
        "DELETE FROM envelope_identity",
        "UPDATE envelope_identity SET sequence_id = 2",
        "INSERT INTO envelope_identity SELECT * FROM envelope_identity",
        "TRUNCATE envelope_identity",
        "UPDATE envelopes SET topic = '\\x01'",
        "UPDATE envelopes_p0 SET sequence_id = 2",
        "ALTER TABLE envelopes_p0 DISABLE TRIGGER USER",
        "SET session_replication_role = replica",
    ] {
        let error = sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&writer.primary)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("42501"),
            "{statement}"
        );
    }
    close(&writer).await;
    owner.primary.close().await;
}

/// Key immutability is enforced by triggers as well as privileges, including privileged maintenance mistakes.
#[xmtp_common::test(unwrap_try = true)]
async fn parent_child_and_registry_reject_key_updates() {
    let (_database, owner, writer) = fixture().await?;
    write(&writer, 1).await?;
    for table in ["envelopes", "envelopes_p0", "envelope_identity"] {
        let error = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET sequence_id = 2"
        )))
        .execute(&owner.primary)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
    close(&writer).await;
    owner.primary.close().await;
}

/// Failed envelope writes roll registration back; deferred FKs also reject orphan registrations at commit.
#[xmtp_common::test(unwrap_try = true)]
async fn registry_has_no_rows_after_failed_insert_or_orphan_commit() {
    let (_database, owner, writer) = fixture().await?;
    let error = sqlx::query("INSERT INTO envelopes (sequence_id, topic, server_ns, message_hash,
        is_commit_or_proposal, payload, push_eligible) VALUES (1, '\\x01', -1, sha256('a'), false, '', false)")
        .execute(&writer.primary).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let mut tx = owner.primary.begin().await?;
    sqlx::query("INSERT INTO envelope_identity VALUES ('\\x01', sha256('a'), 1)")
        .execute(&mut *tx)
        .await?;
    assert_eq!(
        tx.commit()
            .await
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .as_deref(),
        Some("23503")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelope_identity")
        .fetch_one(&owner.primary)
        .await?;
    assert_eq!(count, 0);
    close(&writer).await;
    owner.primary.close().await;
}

/// Pruning removes identity state atomically, so a legitimately pruned message may be published again.
#[xmtp_common::test(unwrap_try = true)]
async fn pruning_cascades_and_preserves_republish_behavior() {
    let (_database, owner, writer) = fixture().await?;
    let first = write(&writer, 1).await?;
    let registered: i64 = sqlx::query_scalar("SELECT count(*) FROM envelope_identity")
        .fetch_one(&owner.primary)
        .await?;
    assert_eq!(registered, 1);
    sqlx::query("DELETE FROM envelopes WHERE sequence_id = $1")
        .bind(first.sequence_id)
        .execute(&writer.primary)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelope_identity")
        .fetch_one(&owner.primary)
        .await?;
    assert_eq!(count, 0);
    assert!(write(&writer, 1).await?.sequence_id > first.sequence_id);
    close(&writer).await;
    owner.primary.close().await;
}

/// Missing ranges cause rollback/provision/retry, including multi-row batches crossing an edge and sequence gaps.
#[xmtp_common::test(unwrap_try = true)]
async fn automatic_provisioning_retries_whole_cross_boundary_batch() {
    let (_database, owner, writer) = fixture().await?;
    sqlx::query("SELECT setval('envelope_sequence', 999999, true)")
        .execute(&owner.primary)
        .await?;
    let mut pending = (0..2)
        .map(|index| workload::pending(corpus::topic(index as i64), index as i64, index))
        .collect::<Vec<_>>();
    let rows = writer.commit_publish(&mut pending, None, 30_000).await?;
    assert_eq!(
        rows.iter().map(|row| row.sequence_id).collect::<Vec<_>>(),
        vec![1_000_002, 1_000_003]
    );
    assert_eq!(
        rows.iter().map(|row| row.topic.clone()).collect::<Vec<_>>(),
        pending
            .iter()
            .map(|item| item.topic.clone())
            .collect::<Vec<_>>()
    );
    let page = writer
        .query(
            &pending
                .iter()
                .map(|item| TopicCursor {
                    topic: item.topic.clone(),
                    cursor: 0,
                })
                .collect::<Vec<_>>(),
            1,
        )
        .await?;
    assert_eq!(page.envelopes.len(), 1);
    assert!(page.has_more);
    let topics = pending
        .iter()
        .map(|item| item.topic.clone())
        .collect::<Vec<_>>();
    assert_eq!(writer.newest_envelopes(&topics).await?.len(), 2);
    assert_eq!(writer.newest_metadata(&topics).await?.len(), 2);
    sqlx::query("SELECT setval('envelope_sequence', 10000000, true)")
        .execute(&owner.primary)
        .await?;
    assert!(write(&writer, 100).await?.sequence_id > 10_000_001);
    close(&writer).await;
    owner.primary.close().await;
}

/// Concurrent processes may discover the same missing range; provisioning and duplicate retries remain atomic.
#[xmtp_common::test(unwrap_try = true)]
async fn independent_provisioners_serialize_and_restart_from_catalogue() {
    let (_database, owner, writer) = fixture().await?;
    let other = Partitions::new(owner.primary.clone());
    try_join_all([
        writer.partitions.as_ref().unwrap().ensure(1_000_001),
        other.ensure(1_000_001),
    ])
    .await?;
    let mut connection = owner.primary.acquire().await?;
    assert!(Partitions::exists(&mut connection, 1_000_001).await?);
    drop(connection);
    close(&writer).await;
    owner.primary.close().await;
}

/// Incorrect bounds or a name collision must fail loudly, not be cached as a usable partition.
#[xmtp_common::test(unwrap_try = true)]
async fn provisioning_rejects_incorrect_bounds_and_name_collisions() {
    let (_database, owner, writer) = fixture().await?;
    sqlx::raw_sql(
        "CREATE TABLE envelopes_p1 PARTITION OF envelopes FOR VALUES FROM (1000001) TO (1500001)",
    )
    .execute(&owner.primary)
    .await?;
    assert!(
        writer
            .partitions
            .as_ref()
            .unwrap()
            .ensure(1_000_001)
            .await
            .is_err()
    );
    sqlx::raw_sql("CREATE TABLE envelopes_p2 (dummy bigint)")
        .execute(&owner.primary)
        .await?;
    assert!(
        writer
            .partitions
            .as_ref()
            .unwrap()
            .ensure(2_000_001)
            .await
            .is_err()
    );
    close(&writer).await;
    owner.primary.close().await;
}

/// Concurrent duplicate publishes at a missing boundary all return one globally unique committed ID.
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_duplicates_at_rollover_return_original_metadata() {
    let (_database, owner, writer) = fixture().await?;
    sqlx::query("SELECT setval('envelope_sequence', 1000000, true)")
        .execute(&owner.primary)
        .await?;
    let rows = try_join_all((0..8).map(|_| write(&writer, 1))).await?;
    assert!(
        rows.iter()
            .all(|row| row.sequence_id == rows[0].sequence_id)
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelope_identity")
        .fetch_one(&owner.primary)
        .await?;
    assert_eq!(count, 1);
    close(&writer).await;
    owner.primary.close().await;
}

/// A duplicate committed after preflight must skip allocation under partitioned publish locks.
/// Renaming the sequence makes even a zero-count allocation query fail.
#[xmtp_common::test(unwrap_try = true)]
async fn locked_duplicates_skip_sequence_allocation() {
    let (_database, owner, writer) = fixture().await?;
    let mut pending = vec![workload::pending(corpus::topic(0), 1, 0)];
    writer.find_duplicates(&mut pending).await?;
    assert!(pending[0].duplicate.is_none());
    let committed = write(&owner, 1).await?;
    sqlx::query("ALTER SEQUENCE public.envelope_sequence RENAME TO unused_envelope_sequence")
        .execute(&owner.primary)
        .await?;
    let rows = writer.commit_publish(&mut pending, None, 30_000).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].sequence_id, committed.sequence_id);
    assert!(pending[0].duplicate.is_some());
    close(&writer).await;
    owner.primary.close().await;
}

/// Production and prefiltered multi-range pages, empty/caught-up reads, and newest match an independent reference.
#[xmtp_common::test(unwrap_try = true)]
async fn multi_range_reads_preserve_reference_pages_and_newest() {
    let (_database, owner, mut writer) = fixture().await?;
    for base in [0_i64, 1_000_000, 2_000_000] {
        writer.partitions.as_ref().unwrap().ensure(base + 1).await?;
        sqlx::query("INSERT INTO envelopes (sequence_id, topic, server_ns, message_hash,
            is_commit_or_proposal, payload, push_eligible)
            SELECT $1 + id, topic, id, sha256(int8send($1 + id)), false, int8send($1 + id), false
            FROM generate_series(1,20) id JOIN unnest($2::bytea[]) WITH ORDINALITY AS topics(topic, ordinal)
                ON ordinal = mod(id, 3) + 1")
            .bind(base).bind(vec![corpus::topic(0), corpus::topic(1), corpus::topic(2)])
            .execute(&owner.primary).await?;
    }
    sqlx::query(
        "INSERT INTO topic_watermark SELECT topic, max(sequence_id) FROM envelopes GROUP BY topic",
    )
    .execute(&owner.primary)
    .await?;
    for strategy in [
        PageQuery::Lateral,
        PageQuery::Watermarks,
        PageQuery::Bounded,
    ] {
        writer.page_query = strategy;
        for (floor, other) in [
            (0, 0),
            (1_000_010, 500_005),
            (2_000_010, 1_000_005),
            (3_000_000, 1_500_000),
            (3_000_000, 3_000_000),
        ] {
            let mut queries = vec![
                TopicCursor {
                    topic: corpus::topic(0),
                    cursor: floor,
                },
                TopicCursor {
                    topic: corpus::topic(1),
                    cursor: other,
                },
                TopicCursor {
                    topic: corpus::topic(999),
                    cursor: 0,
                },
            ];
            queries.extend((1_000..1_256).map(|identifier| TopicCursor {
                topic: corpus::topic(identifier),
                cursor: floor,
            }));
            let topics = queries
                .iter()
                .map(|query| query.topic.clone())
                .collect::<Vec<_>>();
            let cursors = queries.iter().map(|query| query.cursor).collect::<Vec<_>>();
            for limit in [1_i64, 10, 100] {
                let expected: Vec<i64> = sqlx::query_scalar("SELECT sequence_id FROM envelopes e
                JOIN unnest($1::bytea[], $2::bigint[]) wanted(topic, cursor) ON e.topic = wanted.topic
                WHERE sequence_id > cursor ORDER BY sequence_id LIMIT $3")
                .bind(&topics).bind(&cursors).bind(limit + 1).fetch_all(&owner.primary).await?;
                let page = writer.query(&queries, limit).await?;
                assert_eq!(page.has_more, expected.len() > limit as usize);
                assert_eq!(
                    page.envelopes
                        .iter()
                        .map(|row| row.sequence_id)
                        .collect::<Vec<_>>(),
                    expected
                        .into_iter()
                        .take(limit as usize)
                        .collect::<Vec<_>>()
                );
            }
            let newest = writer.newest_envelopes(&topics).await?;
            let metadata = writer.newest_metadata(&topics).await?;
            assert_eq!(newest.len(), 2);
            assert_eq!(metadata.len(), 2);
            assert!(newest.iter().all(|row| row.sequence_id > 2_000_000));
        }
    }
    close(&writer).await;
    owner.primary.close().await;
}

/// Provisioning waits count against the original request deadline and cancellation releases publish locks.
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_ddl_respects_total_publish_deadline() {
    let (_database, owner, writer) = fixture().await?;
    sqlx::query("SELECT setval('envelope_sequence', 1000000, true)")
        .execute(&owner.primary)
        .await?;
    let mut blocker = owner.primary.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(0, $1)")
        .bind(PROVISION_LOCK)
        .execute(&mut *blocker)
        .await?;
    let mut pending = vec![workload::pending(corpus::topic(0), 1, 0)];
    assert!(matches!(
        writer.commit_publish(&mut pending, None, 100).await,
        Err(Error::PartitionPublishDeadline)
    ));
    blocker.rollback().await?;
    assert!(write(&writer, 1).await?.sequence_id > 1_000_001);
    close(&writer).await;
    owner.primary.close().await;
}

/// The final partial range supports the maximum sequence ID without arithmetic overflow.
#[xmtp_common::test(unwrap_try = true)]
async fn maximum_sequence_id_can_be_stored() {
    let (_database, owner, writer) = fixture().await?;
    writer.partitions.as_ref().unwrap().ensure(i64::MAX).await?;
    sqlx::query("SELECT setval('envelope_sequence', $1, false)")
        .bind(i64::MAX)
        .execute(&owner.primary)
        .await?;
    assert_eq!(write(&writer, 1).await?.sequence_id, i64::MAX);
    close(&writer).await;
    owner.primary.close().await;
}

/// Bulk seeding retains real registry enforcement and refuses to transform a nonempty production-shaped database.
#[xmtp_common::test(unwrap_try = true)]
async fn seeding_builds_complete_registry_and_install_refuses_existing_data() {
    let (_database, owner, writer) = fixture().await?;
    corpus::seed(&owner, 1_000, 42).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM envelope_identity identity
        JOIN envelopes e USING (sequence_id, topic, message_hash)",
    )
    .fetch_one(&owner.primary)
    .await?;
    assert_eq!(count, 1_000);
    close(&writer).await;
    owner.primary.close().await;
    let database = TestDatabase::new()?;
    let owner = super::super::connect(&database, 1, 30_000).await?;
    write(&owner, 1).await?;
    assert!(install(&owner).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&owner.primary)
        .await?;
    assert_eq!(count, 1);
    owner.primary.close().await;
}
