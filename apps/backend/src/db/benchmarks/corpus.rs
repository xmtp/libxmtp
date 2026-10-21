//! Deterministic bulk corpus: skewed active traffic, dormant topics, and opaque
//! non-compressible payloads. The SQL retains every production index and constraint.

use super::{Store, TestResult};
use sqlx::Row;
use std::collections::BTreeMap;

const SEED_BATCH: i64 = 50_000;
const MAX_TOPICS: i64 = 10_000;
const PAYLOAD_SIZES: [usize; 10] = [256, 256, 256, 256, 256, 1024, 1024, 1024, 1024, 4096];

#[derive(Clone, Debug)]
pub(super) struct Topic {
    pub bytes: Vec<u8>,
    pub head: i64,
}

pub(super) fn topic(identifier: i64) -> Vec<u8> {
    [
        vec![0],
        xmtp_common::sha256_array(&identifier.to_be_bytes())[..16].to_vec(),
    ]
    .concat()
}

fn payload(identifier: i64, size: usize) -> Vec<u8> {
    (0..size.div_ceil(32))
        .flat_map(|block| {
            xmtp_common::sha256_array(
                &[identifier.to_be_bytes(), (block as i64).to_be_bytes()].concat(),
            )
        })
        .take(size)
        .collect()
}

/// Bulk-load in bounded transactions, then establish sequence/watermark state
/// and vacuum/analyze before timing. No durability or index shortcuts are used.
pub(super) async fn seed(store: &Store, rows: i64, seed: i64) -> TestResult<Vec<Topic>> {
    let topics = (rows / 10).min(MAX_TOPICS);
    let hot = (topics / 100).max(1);
    let active = topics / 2;
    let payloads: Vec<_> = (0..100)
        .map(|index| payload(seed * 100 + index, PAYLOAD_SIZES[index as usize % 10]))
        .collect();
    sqlx::query(
        "CREATE TABLE benchmark_payload (payload_id bigint PRIMARY KEY, payload bytea NOT NULL)",
    )
    .execute(&store.primary)
    .await?;
    sqlx::query("INSERT INTO benchmark_payload SELECT ordinality - 1, payload FROM unnest($1::bytea[]) WITH ORDINALITY AS p(payload, ordinality)")
        .bind(&payloads).execute(&store.primary).await?;
    let mut heads = BTreeMap::<Vec<u8>, i64>::new();
    for first in (1..=rows).step_by(SEED_BATCH as usize) {
        let batch_heads = sqlx::query(sqlx::AssertSqlSafe(include_str!("seed.sql")))
            .bind(first)
            .bind((first + SEED_BATCH - 1).min(rows))
            .bind(rows)
            .bind(topics)
            .bind(hot)
            .bind(active)
            .bind(seed)
            .fetch_all(&store.primary)
            .await?;
        heads.extend(
            batch_heads
                .into_iter()
                .map(|row| (row.get::<Vec<u8>, _>("topic"), row.get::<i64, _>("head"))),
        );
    }
    sqlx::query("DROP TABLE benchmark_payload")
        .execute(&store.primary)
        .await?;
    let (topics, heads): (Vec<_>, Vec<_>) = heads.into_iter().unzip();
    sqlx::query("INSERT INTO topic_watermark SELECT * FROM unnest($1::bytea[], $2::bigint[])")
        .bind(topics)
        .bind(heads)
        .execute(&store.primary)
        .await?;
    sqlx::query("SELECT setval('envelope_sequence', $1, true)")
        .bind(rows)
        .execute(&store.primary)
        .await?;
    sqlx::query("UPDATE allocation_boundary SET closed_sequence_id = $1 WHERE singleton")
        .bind(rows)
        .execute(&store.primary)
        .await?;
    let has_registry: bool =
        sqlx::query_scalar("SELECT to_regclass('public.envelope_identity') IS NOT NULL")
            .fetch_one(&store.primary)
            .await?;
    if has_registry {
        sqlx::raw_sql("VACUUM (ANALYZE) envelope_identity")
            .execute(&store.primary)
            .await?;
        let registered: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM envelope_identity identity
            JOIN envelopes e USING (sequence_id, topic, message_hash)",
        )
        .fetch_one(&store.primary)
        .await?;
        if registered != rows {
            return Err("seeded registry does not match envelope identities".into());
        }
    }
    for statement in [
        "VACUUM (ANALYZE) envelopes",
        "VACUUM (ANALYZE) topic_watermark",
        "CHECKPOINT",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&store.primary)
            .await?;
    }
    let actual: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&store.primary)
        .await?;
    if actual != rows {
        return Err("seeded row count does not match requested size".into());
    }
    catalog(store).await
}

pub(super) async fn catalog(store: &Store) -> TestResult<Vec<Topic>> {
    Ok(
        sqlx::query("SELECT topic, last_sequence_id FROM topic_watermark ORDER BY topic")
            .fetch_all(&store.primary)
            .await?
            .into_iter()
            .map(|row| Topic {
                bytes: row.get("topic"),
                head: row.get("last_sequence_id"),
            })
            .collect(),
    )
}
