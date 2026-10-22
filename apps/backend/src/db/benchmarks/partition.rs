//! Disposable sequence-range schema experiment used only by benchmark/test builds.
//! `install` applies the versioned fixture migration. `Partitions` validates and
//! provisions ranges after publish rollback; `connect` separates restricted writers
//! from the privileged provisioner. Production migrations and startup are unchanged.
//! `Layout` selects the schema; paging and index experiments are independent.

use super::{Store, TestDatabase, TestResult};
use crate::{
    db::{PendingEnvelope, StoredMeta},
    error::{AdmissionError, Error},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use std::sync::Arc;
use xmtp_common::time::{Duration, Instant, timeout};

#[cfg(test)]
mod tests;

pub(crate) const RANGE_SIZE: i64 = 1_000_000;
const PROVISION_LOCK: i32 = 4;
const MAX_RETRIES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Layout {
    Unpartitioned,
    SequenceRanges,
}

impl Layout {
    pub(super) fn is_partitioned(self) -> bool {
        self == Self::SequenceRanges
    }
}

pub(crate) struct Partitions {
    provisioner: PgPool,
}

/// Return half-open sequence bounds without overflowing at bigint's maximum.
fn bounds(sequence_id: i64) -> Result<(i64, Option<i64>), Error> {
    if sequence_id <= 0 {
        return Err(Error::Invariant("partition sequence ID must be positive"));
    }
    let lower = (sequence_id - 1) / RANGE_SIZE * RANGE_SIZE + 1;
    Ok((lower, lower.checked_add(RANGE_SIZE)))
}

fn partition_name(lower: i64) -> String {
    format!("envelopes_p{}", (lower - 1) / RANGE_SIZE)
}

impl Partitions {
    pub(super) fn new(provisioner: PgPool) -> Self {
        Self { provisioner }
    }

    /// Validate actual parent membership, bounds, and enabled inherited triggers.
    /// Never treat a coincidentally matching relation name as a valid partition.
    async fn exists(connection: &mut PgConnection, sequence_id: i64) -> Result<bool, Error> {
        let (lower, upper) = bounds(sequence_id)?;
        let expected = match upper {
            Some(upper) => format!("FOR VALUES FROM ('{lower}') TO ('{upper}')"),
            None => format!("FOR VALUES FROM ('{lower}') TO (MAXVALUE)"),
        };
        let definition = sqlx::query_scalar::<_, String>(
            "SELECT pg_get_expr(child.relpartbound, child.oid) FROM pg_inherits i
             JOIN pg_class child ON child.oid = i.inhrelid
             WHERE i.inhparent = 'public.envelopes'::regclass
               AND child.relnamespace = 'public'::regnamespace AND child.relname = $1
               AND (SELECT count(*) FROM pg_index ci JOIN pg_inherits ii ON ii.inhrelid = ci.indexrelid
                    JOIN pg_class pi ON pi.oid = ii.inhparent
                    WHERE ci.indrelid = child.oid AND ci.indisvalid
                    AND pi.relname IN ('envelopes_pkey', 'envelopes_topic_sequence', 'envelopes_push_eligible')) = 3
               AND (SELECT count(*) FROM pg_trigger t WHERE t.tgrelid = child.oid
                    AND t.tgname IN ('register_envelope_identity', 'immutable_envelope_key')
                    AND t.tgparentid <> 0 AND t.tgenabled = 'O') = 2",
        )
        .bind(partition_name(lower))
        .fetch_optional(connection)
        .await?;
        match definition {
            Some(actual) if actual != expected => {
                Err(Error::Invariant("incorrect envelope partition bounds"))
            }
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    pub(in crate::db) async fn missing(
        &self,
        connection: &mut PgConnection,
        ids: &[i64],
    ) -> Result<Vec<i64>, Error> {
        let ranges = ids
            .iter()
            .map(|id| bounds(*id).map(|(lower, _)| lower))
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        let mut missing = Vec::new();
        for lower in ranges {
            if !Self::exists(connection, lower).await? {
                missing.push(lower);
            }
        }
        Ok(missing)
    }

    /// Serialize catalogue validation and ATTACH on a separate owner connection.
    /// The caller must have rolled back the entire publish transaction first.
    pub(super) async fn ensure(&self, sequence_id: i64) -> Result<(), Error> {
        let (lower, upper) = bounds(sequence_id)?;
        let mut tx = self.provisioner.begin().await?;
        sqlx::raw_sql("SET LOCAL lock_timeout = '5s'; SET LOCAL statement_timeout = '30s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(0, $1)")
            .bind(PROVISION_LOCK)
            .execute(&mut *tx)
            .await?;
        if Self::exists(&mut tx, sequence_id).await? {
            tx.commit().await?;
            return Ok(());
        }
        let name = partition_name(lower);
        let upper_bound = upper.map_or("MAXVALUE".to_owned(), |value| value.to_string());
        let check = upper.map_or(format!("sequence_id >= {lower}"), |upper| {
            format!("sequence_id >= {lower} AND sequence_id < {upper}")
        });
        let ddl = format!(
            "CREATE TABLE public.{name} (LIKE public.envelopes INCLUDING ALL);
             ALTER TABLE public.{name} ADD CONSTRAINT {name}_bound CHECK ({check});
             ALTER TABLE public.envelopes ATTACH PARTITION public.{name} FOR VALUES FROM ({lower}) TO ({upper_bound});
             GRANT SELECT, INSERT, DELETE ON public.{name} TO xmtp_bench_writer;
             GRANT UPDATE (server_ns, expiry_ns, is_commit_or_proposal, payload, push_eligible, sender_hmac)
                 ON public.{name} TO xmtp_bench_writer;"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(ddl))
            .execute(&mut *tx)
            .await?;
        if !Self::exists(&mut tx, sequence_id).await? {
            return Err(Error::Invariant("provisioned partition failed validation"));
        }
        tx.commit().await?;
        Ok(())
    }

    /// Retry complete publishes under one deadline, after explicitly releasing locks.
    pub(in crate::db) async fn publish(
        &self,
        store: &Store,
        pending: &mut [PendingEnvelope],
        parse_error: Option<(usize, AdmissionError)>,
        max_duration_ms: u64,
    ) -> Result<Vec<StoredMeta>, Error> {
        let duration = Duration::from_millis(max_duration_ms);
        let started = Instant::now();
        timeout(duration, async {
            for _ in 0..MAX_RETRIES {
                let remaining = duration.saturating_sub(started.elapsed()).as_millis();
                if remaining == 0 {
                    return Err(Error::PartitionPublishDeadline);
                }
                match store
                    .commit_publish_once(pending, parse_error.clone(), remaining as u64)
                    .await
                {
                    Err(Error::MissingEnvelopePartitions(ranges)) => {
                        for lower in ranges {
                            self.ensure(lower).await?;
                        }
                        for item in pending.iter_mut() {
                            item.duplicate = None;
                        }
                    }
                    result => return result,
                }
            }
            Err(Error::PartitionPublishDeadline)
        })
        .await
        .map_err(|_| Error::PartitionPublishDeadline)?
    }
}

pub(super) async fn install(store: &Store) -> TestResult {
    let mut tx = store.primary.begin().await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(include_str!(
        "../../../benchmarks/migrations/20261021000000_sequence_ranges.sql"
    )))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn connect(
    database: &TestDatabase,
    connections: u32,
    timeout_ms: u64,
    layout: Layout,
) -> TestResult<Store> {
    if !layout.is_partitioned() {
        return super::connect(database, connections, timeout_ms).await;
    }
    let provisioner = super::connect(database, 1, super::REQUEST_TIMEOUT_MS)
        .await?
        .primary;
    let mut url = url::Url::parse(database.url())?;
    url.set_username("xmtp_bench_writer")
        .map_err(|_| "benchmark URL cannot contain a writer username")?;
    url.set_password(None)
        .map_err(|_| "benchmark URL cannot remove administrative credentials")?;
    let mut config: crate::config::Config =
        toml::from_str(&format!("[database]\nurl = {:?}", url.as_str()))?;
    config.database.max_connections = connections;
    config.database.max_statement_timeout_ms = timeout_ms;
    let primary = crate::db::connect_pool(url.as_str(), &config).await?;
    let restricted: bool = sqlx::query_scalar(
        "SELECT current_user = 'xmtp_bench_writer'
        AND session_user = current_user",
    )
    .fetch_one(&primary)
    .await?;
    if !restricted {
        return Err(
            "partition benchmark must authenticate directly as its restricted writer".into(),
        );
    }
    Ok(Store {
        read: primary.clone(),
        primary,
        partitions: Some(Arc::new(Partitions::new(provisioner))),
        page_query: super::experiments::PageQuery::Lateral,
    })
}

/// Measure first-use DDL and precreated controls separately from steady-state trials.
/// Each fresh fixture has 1,000 envelopes and sparse IDs to isolate rollover cost.
pub(super) async fn rollovers(
    config: &super::Config,
    output: &mut impl std::io::Write,
) -> TestResult {
    for edge in [1_000_001_i64, 10_000_001] {
        for precreated in [false, true] {
            for &writers in &config.writers {
                for trial in 0..config.trials {
                    let mut database = TestDatabase::new()?;
                    let owner = super::connect(&database, 1, super::SEED_TIMEOUT_MS).await?;
                    install(&owner).await?;
                    let partitions = Partitions::new(owner.primary.clone());
                    partitions.ensure(1).await?;
                    super::corpus::seed(&owner, 1_000, config.seed).await?;
                    if precreated {
                        partitions.ensure(edge).await?;
                    }
                    sqlx::query("SELECT setval('envelope_sequence', $1, true)")
                        .bind(edge - 1)
                        .execute(&owner.primary)
                        .await?;
                    owner.primary.close().await;
                    let store = connect(
                        &database,
                        writers as u32,
                        super::REQUEST_TIMEOUT_MS,
                        config.layout,
                    )
                    .await?;
                    let connections = futures::future::try_join_all(
                        (0..writers).map(|_| store.primary.acquire()),
                    )
                    .await?;
                    drop(connections);
                    let before_wal: String =
                        sqlx::query_scalar("SELECT pg_current_wal_insert_lsn()::text")
                            .fetch_one(&store.primary)
                            .await?;
                    let start = Instant::now();
                    let samples = futures::future::try_join_all((0..writers).map(|worker| {
                        let store = &store;
                        async move {
                            let mut pending = vec![super::workload::pending(
                                super::corpus::topic(worker as i64),
                                -(worker as i64) - 1,
                                0,
                            )];
                            let started = Instant::now();
                            store.find_duplicates(&mut pending).await?;
                            let inserted = store
                                .commit_publish(&mut pending, None, super::REQUEST_TIMEOUT_MS)
                                .await?;
                            if inserted.len() != 1 || inserted[0].sequence_id < edge {
                                return Err("rollover did not publish in the target range".into());
                            }
                            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                                started.elapsed().as_secs_f64() * 1_000.0,
                            )
                        }
                    }))
                    .await?;
                    let seconds = start.elapsed().as_secs_f64();
                    let wal_bytes: i64 = sqlx::query_scalar(
                        "SELECT pg_wal_lsn_diff(pg_current_wal_insert_lsn(), $1::pg_lsn)::bigint",
                    )
                    .bind(before_wal)
                    .fetch_one(&store.primary)
                    .await?;
                    super::record(
                        output,
                        serde_json::json!({"event": "rollover", "edge": edge, "precreated": precreated,
                        "seed_rows": 1000, "writers": writers, "trial": trial, "wal_bytes": wal_bytes,
                        "measurement": super::report::measurement(&samples, seconds, writers, writers * 1024)}),
                    )?;
                    close(&store).await;
                    database.remove()?;
                }
            }
        }
    }
    Ok(())
}

pub(super) async fn close(store: &Store) {
    store.primary.close().await;
    if let Some(partitions) = &store.partitions {
        partitions.provisioner.close().await;
    }
}

/// Resolve global identities before touching the exact payload partition.
pub(in crate::db) async fn duplicates(
    connection: &mut PgConnection,
    pending: &mut [PendingEnvelope],
) -> Result<(), Error> {
    let topics: Vec<_> = pending.iter().map(|item| item.topic.clone()).collect();
    let hashes: Vec<_> = pending
        .iter()
        .map(|item| item.message_hash.to_vec())
        .collect();
    let rows = sqlx::query("SELECT wanted.ordinality, e.sequence_id, e.topic, e.server_ns,
        e.expiry_ns, e.message_hash, e.is_commit_or_proposal
        FROM unnest($1::bytea[], $2::bytea[]) WITH ORDINALITY AS wanted(topic, hash, ordinality)
        JOIN public.envelope_identity identity ON identity.topic = wanted.topic AND identity.message_hash = wanted.hash
        JOIN public.envelopes e ON e.sequence_id = identity.sequence_id
        ORDER BY wanted.ordinality")
        .bind(topics).bind(hashes).fetch_all(connection).await?;
    for row in rows {
        let ordinal: i64 = row.try_get("ordinality")?;
        let item = usize::try_from(ordinal - 1)
            .ok()
            .and_then(|index| pending.get_mut(index))
            .ok_or(Error::Invariant("unknown duplicate ordinal"))?;
        item.duplicate = Some(StoredMeta {
            sequence_id: row.try_get("sequence_id")?,
            topic: row.try_get("topic")?,
            server_ns: row.try_get("server_ns")?,
            expiry_ns: row.try_get("expiry_ns")?,
            message_hash: row.try_get("message_hash")?,
            is_commit_or_proposal: row.try_get("is_commit_or_proposal")?,
        });
    }
    Ok(())
}
