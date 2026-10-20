//! Requirements for benchmark correctness, independent of performance thresholds.

use super::workload::{Read, request};
use super::*;
use sqlx::Row;
use std::collections::HashSet;

fn config() -> Config {
    serde_json::from_str(include_str!("../../../benchmarks/baseline.json")).unwrap()
}

/// Explicit configuration rejects unknown fields rather than silently running a different experiment.
#[xmtp_common::test(unwrap_try = true)]
fn configuration_rejects_unknown_and_missing_fields() {
    let mut value = serde_json::to_value(config())?;
    value["typo"] = json!(10);
    assert!(serde_json::from_value::<Config>(value.clone()).is_err());
    value.as_object_mut().unwrap().remove("typo");
    value.as_object_mut().unwrap().remove("read_ms");
    assert!(serde_json::from_value::<Config>(value).is_err());
}

/// Zero durations, impossible growth budgets, and invalid database sizes must fail before database creation.
#[xmtp_common::test(unwrap_try = true)]
fn configuration_rejects_unmeasurable_trials() {
    config().validate()?;
    for change in ["duration", "size", "budget", "topics", "path"] {
        let mut config = config();
        match change {
            "duration" => config.read_ms = 0,
            "size" => config.sizes = vec![0],
            "budget" => {
                config.writers = vec![64];
                config.batch_sizes = vec![100];
            }
            "topics" => config.topic_counts = vec![1_001],
            _ => config.output = PathBuf::new(),
        }
        assert!(config.validate().is_err(), "{change}");
    }
}

/// Cursor age is a global sequence position: no cursor, 10%, 90%, and 99% of the corpus.
#[xmtp_common::test(unwrap_try = true)]
fn cursor_ages_and_mixed_weights_are_explicit() {
    assert_eq!(Read::NoCursor.floor(100_000, 0), 0);
    assert_eq!(Read::Old.floor(100_000, 0), 10_000);
    assert_eq!(Read::Backlog.floor(100_000, 0), 90_000);
    assert_eq!(Read::Recent.floor(100_000, 0), 99_000);
    let floors: Vec<_> = (0..100)
        .map(|ordinal| Read::Mixed.floor(100_000, ordinal))
        .collect();
    assert_eq!(floors.iter().filter(|&&value| value == 99_000).count(), 90);
    assert_eq!(floors.iter().filter(|&&value| value == 10_000).count(), 9);
    assert_eq!(floors.iter().filter(|&&value| value == 0).count(), 1);
}

/// Requests rotate through distinct topic sets; caught-up and missing-topic reads are separate no-hit workloads.
#[xmtp_common::test(unwrap_try = true)]
fn requests_rotate_without_duplicate_topics() {
    let topics: Vec<_> = (0..100)
        .map(|identifier| corpus::Topic {
            bytes: corpus::topic(identifier),
            head: identifier + 1,
        })
        .collect();
    let pool: Vec<_> = topics.iter().collect();
    let first = request(&pool, 10, 0, Read::CaughtUp, 1_000);
    let second = request(&pool, 10, 1, Read::CaughtUp, 1_000);
    assert_ne!(first[0].topic, second[0].topic);
    assert_eq!(
        first
            .iter()
            .map(|query| &query.topic)
            .collect::<HashSet<_>>()
            .len(),
        10
    );
    assert!(first.iter().all(|query| {
        topics
            .iter()
            .any(|topic| topic.bytes == query.topic && topic.head == query.cursor)
    }));
    let missing = request(&pool, 1_000, 0, Read::Absent, 1_000);
    assert_eq!(
        missing
            .iter()
            .map(|query| &query.topic)
            .collect::<HashSet<_>>()
            .len(),
        1_000
    );
    assert!(
        missing
            .iter()
            .all(|query| !topics.iter().any(|topic| topic.bytes == query.topic))
    );
}

/// Hit-only scenarios never time empty reads; sparse-recent requests intentionally retain inactive topics.
#[xmtp_common::test(unwrap_try = true)]
fn recent_hits_exclude_dormant_topics() {
    let dormant = corpus::Topic {
        bytes: corpus::topic(0),
        head: 50_000,
    };
    assert!(!Read::Recent.eligible(&dormant, 100_000));
    assert!(Read::SparseRecent.eligible(&dormant, 100_000));
    assert!(Read::NoCursor.eligible(&dormant, 100_000));
}

/// Nearest-rank percentiles and request/envelope throughput use raw samples and measured wall time.
#[xmtp_common::test(unwrap_try = true)]
fn statistics_preserve_samples_and_units() {
    let samples = [4.0, 1.0, 3.0, 2.0];
    let measurement = report::measurement(&samples, 2.0, 40, 4_096);
    assert_eq!(measurement["p50_ms"], 2.0);
    assert_eq!(measurement["p95_ms"], 4.0);
    assert_eq!(measurement["p99_ms"], 4.0);
    assert_eq!(measurement["requests_per_second"], 2.0);
    assert_eq!(measurement["envelopes_per_second"], 20.0);
    assert_eq!(measurement["latency_ms"], json!(samples));
}

/// The bulk corpus preserves hashes, payload sizes, watermarks, sequence allocation, and real query behavior.
#[xmtp_common::test(unwrap_try = true)]
async fn bulk_seed_matches_production_storage_and_read_semantics() {
    let mut database = TestDatabase::new()?;
    let store = connect(&database, 1).await?;
    let catalog = corpus::seed(&store, 1_000, 42).await?;
    assert_eq!(catalog.len(), 100);
    let storage = report::storage(&store).await?;
    assert!(storage["heap_bytes"].as_i64().unwrap() > 0);
    assert!(storage["index_bytes"].as_i64().unwrap() > 0);
    assert!(
        storage["database_bytes"].as_i64().unwrap() >= storage["total_bytes"].as_i64().unwrap()
    );
    let row = sqlx::query("SELECT count(*) AS rows, sum(octet_length(payload))::bigint AS bytes, bool_and(message_hash = sha256(payload)) AS hashes FROM envelopes")
        .fetch_one(&store.primary).await?;
    assert_eq!(row.get::<i64, _>("rows"), 1_000);
    assert_eq!(row.get::<i64, _>("bytes"), 947_200);
    assert!(row.get::<bool, _>("hashes"));
    let invalid: i64 = sqlx::query_scalar("SELECT count(*) FROM topic_watermark w WHERE last_sequence_id <> (SELECT max(sequence_id) FROM envelopes e WHERE e.topic = w.topic)").fetch_one(&store.primary).await?;
    assert_eq!(invalid, 0);
    for kind in [
        Read::NoCursor,
        Read::Old,
        Read::Backlog,
        Read::Recent,
        Read::Mixed,
        Read::SparseRecent,
        Read::CaughtUp,
        Read::Absent,
    ] {
        let pool: Vec<_> = catalog
            .iter()
            .filter(|topic| kind.eligible(topic, 1_000))
            .collect();
        let queries = request(&pool, pool.len().min(10), 0, kind, 1_000);
        let topics: Vec<_> = queries.iter().map(|query| query.topic.clone()).collect();
        let cursors: Vec<_> = queries.iter().map(|query| query.cursor).collect();
        let expected: Vec<i64> = sqlx::query_scalar("SELECT e.sequence_id FROM envelopes e JOIN unnest($1::bytea[], $2::bigint[]) r(topic, cursor) ON e.topic = r.topic WHERE e.sequence_id > r.cursor ORDER BY e.sequence_id")
            .bind(topics).bind(cursors).fetch_all(&store.primary).await?;
        let page = store.query(&queries, 5).await?;
        assert_eq!(
            page.envelopes
                .iter()
                .map(|row| row.sequence_id)
                .collect::<Vec<_>>(),
            expected.iter().copied().take(5).collect::<Vec<_>>(),
            "{kind:?}"
        );
        assert_eq!(page.has_more, expected.len() > 5, "{kind:?}");
    }
    let topics: Vec<_> = catalog.iter().map(|topic| topic.bytes.clone()).collect();
    let newest = store.newest_envelopes(&topics).await?;
    let metadata = store.newest_metadata(&topics).await?;
    assert_eq!(newest.len(), catalog.len());
    assert_eq!(metadata.len(), catalog.len());
    for row in metadata {
        assert!(
            catalog
                .iter()
                .any(|topic| topic.bytes == row.topic && topic.head == row.sequence_id)
        );
    }
    for row in newest {
        assert!(
            catalog
                .iter()
                .any(|topic| topic.bytes == row.topic && topic.head == row.sequence_id)
        );
    }
    let mut pending = [workload::pending(catalog[0].bytes.clone(), -1, 0)];
    store.find_duplicates(&mut pending).await?;
    let receipts = store.commit_publish(&mut pending, None, 5_000).await?;
    assert_eq!(receipts[0].sequence_id, 1_001);
    store.find_duplicates(&mut pending).await?;
    assert!(pending[0].duplicate.is_some());
    store.primary.close().await;
    database.remove()?;
}

/// A dormant half of the topic population and a hot 1% make recent-vs-old cursor tests meaningful.
#[xmtp_common::test(unwrap_try = true)]
async fn corpus_has_skew_and_dormant_history() {
    let mut database = TestDatabase::new()?;
    let store = connect(&database, 1).await?;
    let catalog = corpus::seed(&store, 100_000, 42).await?;
    assert_eq!(catalog.len(), 10_000);
    assert_eq!(
        catalog.iter().filter(|topic| topic.head <= 50_000).count(),
        5_000
    );
    let hot: Vec<_> = (0..100).map(corpus::topic).collect();
    let hot_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM envelopes WHERE topic = ANY($1::bytea[])")
            .bind(hot)
            .fetch_one(&store.primary)
            .await?;
    assert!((40_000..55_000).contains(&hot_rows));
    assert!(catalog.iter().any(|topic| topic.head > 99_000));
    store.primary.close().await;
    database.remove()?;
}

/// Database copies preserve the physical starting corpus but never mutate or delete their source fixture.
#[xmtp_common::test(unwrap_try = true)]
async fn copied_trials_are_isolated_and_removed_templates_fail() {
    let mut database = TestDatabase::new()?;
    let store = connect(&database, 1).await?;
    corpus::seed(&store, 1_000, 42).await?;
    store.primary.close().await;
    let mut copy = database.copy()?;
    let copy_store = connect(&copy, 1).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM envelopes")
            .fetch_one(&copy_store.primary)
            .await?,
        1_000
    );
    sqlx::query("DELETE FROM envelopes")
        .execute(&copy_store.primary)
        .await?;
    copy_store.primary.close().await;
    copy.remove()?;
    let store = connect(&database, 1).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM envelopes")
            .fetch_one(&store.primary)
            .await?,
        1_000
    );
    store.primary.close().await;
    database.remove()?;
    assert!(database.copy().is_err());
}
