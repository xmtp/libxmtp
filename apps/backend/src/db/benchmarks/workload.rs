//! Closed-loop real Store workloads. Request generation and diagnostics are
//! outside timing; database calls and result materialization are inside it.

use super::{
    Config, Duration, Instant, Store, TestDatabase, TestResult, connect,
    corpus::{self, Topic},
    record, report,
};
use crate::db::{PendingEnvelope, TopicCursor};
use futures::future::try_join_all;
use serde_json::json;
use sqlx::Row;
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Read {
    NoCursor,
    Old,
    Backlog,
    Recent,
    Mixed,
    SparseRecent,
    Absent,
    CaughtUp,
    NewestFull,
    NewestMetadata,
    NewestAbsent,
}

impl Read {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::NoCursor => "query_no_cursor",
            Self::Old => "query_old",
            Self::Backlog => "query_backlog",
            Self::Recent => "query_recent",
            Self::Mixed => "query_mixed",
            Self::SparseRecent => "query_sparse_recent",
            Self::Absent => "query_absent",
            Self::CaughtUp => "query_caught_up",
            Self::NewestFull => "newest_full",
            Self::NewestMetadata => "newest_metadata",
            Self::NewestAbsent => "newest_absent",
        }
    }

    fn newest(self) -> bool {
        matches!(
            self,
            Self::NewestFull | Self::NewestMetadata | Self::NewestAbsent
        )
    }

    pub(super) fn floor(self, size: i64, ordinal: usize) -> i64 {
        match self {
            Self::NoCursor | Self::Absent => 0,
            Self::Old => size / 10,
            Self::Backlog => size * 9 / 10,
            Self::Mixed if ordinal.is_multiple_of(100) => 0,
            Self::Mixed if ordinal % 100 < 10 => size / 10,
            _ => size * 99 / 100,
        }
    }

    pub(super) fn eligible(self, topic: &Topic, size: i64) -> bool {
        match self {
            Self::SparseRecent | Self::CaughtUp | Self::Absent => true,
            value if value.newest() => true,
            Self::Mixed => topic.head > size * 99 / 100,
            value => topic.head > value.floor(size, 10),
        }
    }
}

pub(super) fn request(
    pool: &[&Topic],
    count: usize,
    iteration: usize,
    kind: Read,
    size: i64,
) -> Vec<TopicCursor> {
    (0..count)
        .map(|offset| {
            let topic = pool[(iteration * 7919 + offset) % pool.len()];
            TopicCursor {
                topic: if matches!(kind, Read::Absent | Read::NewestAbsent) {
                    corpus::topic(size + offset as i64 + (iteration % 10_000) as i64)
                } else {
                    topic.bytes.clone()
                },
                cursor: if kind == Read::CaughtUp {
                    topic.head
                } else {
                    kind.floor(size, iteration + offset)
                },
            }
        })
        .collect()
}

async fn read(
    store: &Store,
    queries: &[TopicCursor],
    kind: Read,
    limit: i64,
) -> TestResult<(usize, usize)> {
    let (rows, bytes) = if kind == Read::NewestMetadata || kind == Read::NewestAbsent {
        let topics: Vec<_> = queries.iter().map(|query| query.topic.clone()).collect();
        let rows = store.newest_metadata(&topics).await?;
        (std::hint::black_box(rows.len()), 0)
    } else {
        let envelopes = if kind == Read::NewestFull {
            let topics: Vec<_> = queries.iter().map(|query| query.topic.clone()).collect();
            store.newest_envelopes(&topics).await?
        } else {
            store.query(queries, limit).await?.envelopes
        };
        (
            envelopes.len(),
            envelopes
                .iter()
                .map(|envelope| envelope.payload.len())
                .sum(),
        )
    };
    match kind {
        Read::Absent | Read::CaughtUp | Read::NewestAbsent if rows != 0 => {
            return Err("expected a no-hit request".into());
        }
        Read::NewestFull | Read::NewestMetadata if rows != queries.len() => {
            return Err("newest omitted a seeded topic".into());
        }
        Read::NoCursor | Read::Old | Read::Backlog | Read::Recent | Read::Mixed if rows == 0 => {
            return Err("expected a hit request".into());
        }
        _ => {}
    }
    Ok((rows, bytes))
}

/// Explain the exact production SQL with both prepared-plan policies. Store's
/// normal auto policy remains unchanged during timing; diagnostics use one pool connection.
async fn plans(
    store: &Store,
    queries: &[TopicCursor],
    kind: Read,
    limit: i64,
) -> TestResult<serde_json::Value> {
    let sql = match kind {
        Read::NewestFull => include_str!("../read/newest.sql"),
        Read::NewestMetadata | Read::NewestAbsent => include_str!("../read/newest_metadata.sql"),
        _ => include_str!("../read/page.sql"),
    };
    let statement = format!("EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT TEXT) {sql}");
    let topics: Vec<_> = queries.iter().map(|query| query.topic.clone()).collect();
    let cursors: Vec<_> = queries.iter().map(|query| query.cursor).collect();
    let mut connection = store.primary.acquire().await?;
    let mut result = serde_json::Map::new();
    for policy in ["force_custom_plan", "force_generic_plan"] {
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(policy)
            .execute(&mut *connection)
            .await?;
        let query = sqlx::query(sqlx::AssertSqlSafe(statement.as_str())).bind(&topics);
        let query = if kind.newest() {
            query
        } else {
            query.bind(&cursors).bind(limit)
        };
        let lines: Vec<String> = query
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .map(|row| row.get(0))
            .collect();
        result.insert(policy.into(), json!(lines));
    }
    sqlx::query("RESET plan_cache_mode")
        .execute(&mut *connection)
        .await?;
    Ok(result.into())
}

pub(super) async fn reads(
    store: &Store,
    catalog: &[Topic],
    size: i64,
    config: &Config,
    output: &mut impl Write,
) -> TestResult {
    let mut cases = Vec::new();
    for &count in &config.topic_counts {
        for kind in [
            Read::NoCursor,
            Read::Old,
            Read::Backlog,
            Read::Recent,
            Read::Mixed,
            Read::SparseRecent,
            Read::Absent,
            Read::CaughtUp,
            Read::NewestFull,
            Read::NewestMetadata,
            Read::NewestAbsent,
        ] {
            let pool: Vec<_> = catalog
                .iter()
                .filter(|topic| kind.eligible(topic, size))
                .collect();
            let reason = if count > pool.len() && !matches!(kind, Read::Absent | Read::NewestAbsent)
            {
                Some("not enough distinct eligible topics")
            } else if kind == Read::NewestFull
                && count > xmtp_configuration::BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS
            {
                Some("exceeds production full-envelope topic limit")
            } else {
                None
            };
            if let Some(reason) = reason {
                record(
                    output,
                    json!({"event": "skipped", "rows": size, "case": kind.name(), "topics": count, "eligible_topics": pool.len(), "reason": reason}),
                )?;
            } else {
                cases.push((count, kind, pool));
            }
        }
    }
    for trial in 0..config.trials {
        for offset in 0..cases.len() {
            let (count, kind, pool) = &cases[(offset + trial * 7) % cases.len()];
            for iteration in 0..config.warmup_requests {
                read(
                    store,
                    &request(pool, *count, iteration, *kind, size),
                    *kind,
                    config.page_limit,
                )
                .await?;
            }
            let mut samples = Vec::new();
            let mut rows = 0;
            let mut bytes = 0;
            let mut empty = 0;
            let start = Instant::now();
            while samples.is_empty() || start.elapsed() < Duration::from_millis(config.read_ms) {
                let queries = request(
                    pool,
                    *count,
                    samples.len() + config.warmup_requests,
                    *kind,
                    size,
                );
                let request_start = Instant::now();
                let (returned, payload) = read(store, &queries, *kind, config.page_limit).await?;
                samples.push(request_start.elapsed().as_secs_f64() * 1_000.0);
                rows += returned;
                bytes += payload;
                empty += usize::from(returned == 0);
            }
            record(
                output,
                json!({"event": "read", "rows": size, "case": kind.name(), "topics": count,
                "eligible_topics": pool.len(), "trial": trial, "empty_requests": empty,
                "measurement": report::measurement(&samples, start.elapsed().as_secs_f64(), rows, bytes)}),
            )?;
            if trial == 0 {
                record(
                    output,
                    json!({"event": "plan", "rows": size, "case": kind.name(), "topics": count,
                    "plans": plans(store, &request(pool, *count, 0, *kind, size), *kind, config.page_limit).await?}),
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn pending(topic: Vec<u8>, nonce: i64, index: usize) -> PendingEnvelope {
    let mut payload: Vec<_> = (0_i64..32)
        .flat_map(|block| xmtp_common::sha256_array(&block.to_be_bytes()))
        .collect();
    payload[..8].copy_from_slice(&nonce.to_be_bytes());
    PendingEnvelope {
        message_hash: xmtp_common::sha256_array(&payload),
        topic,
        payload,
        index,
        is_commit_or_proposal: false,
        identity: None,
        duplicate: None,
        validation: Ok(None),
        retention_ns: Some(7_776_000_000_000_000),
        push_eligible: true,
        sender_hmac: Some(vec![1; 32]),
    }
}

async fn publish(store: &Store, pending: &mut [PendingEnvelope]) -> TestResult {
    store.find_duplicates(pending).await?;
    let metas = store.commit_publish(pending, None, 120_000).await?;
    if metas.len() != pending.len() || pending.iter().any(|item| item.duplicate.is_some()) {
        return Err("insert workload unexpectedly deduplicated a message".into());
    }
    Ok(())
}

pub(super) async fn writes(
    database: &TestDatabase,
    size: i64,
    config: &Config,
    output: &mut impl Write,
) -> TestResult {
    for trial in 0..config.trials {
        for &writers in &config.writers {
            for &batch in &config.batch_sizes {
                for single_topic in [trial % 2 == 0, trial % 2 != 0] {
                    let mut copy = database.copy()?;
                    let store = connect(&copy, writers as u32).await?;
                    let topics = corpus::catalog(&store).await?;
                    let connections =
                        try_join_all((0..writers).map(|_| store.primary.acquire())).await?;
                    drop(connections);
                    let budget = size as usize * config.write_growth_percent / 100;
                    let operations = budget / batch - writers;
                    for worker in 0..writers {
                        let mut warmup: Vec<_> = (0..batch)
                            .map(|index| {
                                pending(
                                    topics[0].bytes.clone(),
                                    -((worker * batch + index + 1) as i64),
                                    index,
                                )
                            })
                            .collect();
                        publish(&store, &mut warmup).await?;
                    }
                    let before_wal: String =
                        sqlx::query_scalar("SELECT pg_current_wal_insert_lsn()::text")
                            .fetch_one(&store.primary)
                            .await?;
                    let start = Instant::now();
                    let results = try_join_all((0..writers).map(|worker| {
                        let store = &store;
                        let topics = &topics;
                        async move {
                            let mut samples = Vec::new();
                            let quota =
                                operations / writers + usize::from(worker < operations % writers);
                            for operation in 0..quota {
                                if !samples.is_empty()
                                    && start.elapsed() >= Duration::from_millis(config.write_ms)
                                {
                                    break;
                                }
                                let mut request: Vec<_> = (0..batch)
                                    .map(|index| {
                                        let nonce =
                                            ((worker * operations + operation) * batch + index + 1)
                                                as i64;
                                        let topic = if single_topic {
                                            0
                                        } else {
                                            nonce as usize % topics.len()
                                        };
                                        pending(topics[topic].bytes.clone(), nonce, index)
                                    })
                                    .collect();
                                let request_start = Instant::now();
                                publish(store, &mut request).await?;
                                samples.push(request_start.elapsed().as_secs_f64() * 1_000.0);
                            }
                            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(samples)
                        }
                    }))
                    .await?;
                    let seconds = start.elapsed().as_secs_f64();
                    let samples: Vec<_> = results.into_iter().flatten().collect();
                    let wal_bytes: i64 = sqlx::query_scalar("SELECT pg_wal_lsn_diff(pg_current_wal_insert_lsn(), $1::text::pg_lsn)::bigint")
                        .bind(before_wal).fetch_one(&store.primary).await?;
                    let end_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
                        .fetch_one(&store.primary)
                        .await?;
                    if end_rows != size + (writers * batch + samples.len() * batch) as i64 {
                        return Err("write row count did not match acknowledged inserts".into());
                    }
                    record(
                        output,
                        json!({"event": "write", "rows": size, "case": if single_topic { "insert_single_topic" } else { "insert_many_topics" },
                        "topics": if single_topic { 1 } else { topics.len() }, "writers": writers, "batch_size": batch, "trial": trial,
                        "warmup_rows": writers * batch, "end_rows": end_rows, "growth_cap_reached": samples.len() == operations,
                        "wal_bytes": wal_bytes, "measurement": report::measurement(&samples, seconds, samples.len() * batch, samples.len() * batch * 1024)}),
                    )?;
                    store.primary.close().await;
                    copy.remove()?;
                }
            }
        }
    }
    Ok(())
}
