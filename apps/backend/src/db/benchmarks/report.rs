//! Raw trial observations and reproducibility metadata; percentile computation
//! uses nearest ranks and keeps the underlying unsorted request samples.

use super::{Config, Store, TestResult};
use serde_json::{Value, json};
use sqlx::Row;
use std::process::Command;

fn system_line(path: &str, prefix: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text
            .lines()
            .find(|line| line.starts_with(prefix))
            .unwrap_or("not reported")
            .to_owned(),
        Err(error) => format!("unavailable: {error}"),
    }
}

pub(super) fn percentile(samples: &[f64], percent: usize) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)]
}

pub(super) fn measurement(samples: &[f64], seconds: f64, envelopes: usize, bytes: usize) -> Value {
    json!({"requests": samples.len(), "seconds": seconds,
        "requests_per_second": samples.len() as f64 / seconds,
        "envelopes": envelopes, "envelopes_per_second": envelopes as f64 / seconds,
        "payload_bytes": bytes, "p50_ms": percentile(samples, 50),
        "p95_ms": percentile(samples, 95), "p99_ms": percentile(samples, 99),
        "latency_ms": samples})
}

pub(super) async fn environment(store: &Store, config: &Config) -> TestResult<Value> {
    let mut settings = serde_json::Map::new();
    for row in sqlx::query(
        "SELECT name, setting, unit FROM pg_settings WHERE name = ANY($1::text[]) ORDER BY name",
    )
    .bind(vec![
        "server_version",
        "server_version_num",
        "shared_buffers",
        "effective_cache_size",
        "work_mem",
        "maintenance_work_mem",
        "fsync",
        "synchronous_commit",
        "full_page_writes",
        "wal_level",
        "wal_compression",
        "max_wal_size",
        "checkpoint_timeout",
        "random_page_cost",
        "seq_page_cost",
        "effective_io_concurrency",
        "jit",
        "autovacuum",
        "track_io_timing",
        "max_connections",
    ])
    .fetch_all(&store.primary)
    .await?
    {
        settings.insert(row.get("name"), json!({"value": row.get::<String, _>("setting"), "unit": row.get::<Option<String>, _>("unit")}));
    }
    for required in ["fsync", "synchronous_commit", "full_page_writes"] {
        if settings[required]["value"] != "on" {
            return Err(format!("baseline requires {required}=on").into());
        }
    }
    let migrations: Vec<_> = sqlx::query("SELECT version, encode(checksum, 'hex') AS checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&store.primary).await?.into_iter().map(|row| json!({"version": row.get::<i64, _>("version"), "checksum": row.get::<String, _>("checksum")})).collect();
    let revision = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    if !revision.status.success() {
        return Err("git rev-parse failed".into());
    }
    Ok(
        json!({"event": "environment", "format_version": 1, "config": config,
        "revision": String::from_utf8(revision.stdout)?.trim(), "postgres": settings, "migrations": migrations,
        "available_cpus": std::thread::available_parallelism()?.get(), "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS, "cpu": system_line("/proc/cpuinfo", "model name"),
        "memory": system_line("/proc/meminfo", "MemTotal:"),
        "cache": "warm working set; OS page cache not evicted", "load_model": "closed loop; no think time; no background application traffic"}),
    )
}

pub(super) async fn storage(store: &Store) -> TestResult<Value> {
    let row = sqlx::query("WITH RECURSIVE relations(oid) AS (SELECT 'envelopes'::regclass::oid UNION ALL SELECT inhrelid FROM pg_inherits JOIN relations ON inhparent = relations.oid) SELECT sum(pg_relation_size(oid))::bigint AS heap, sum(pg_indexes_size(oid))::bigint AS indexes, sum(pg_total_relation_size(oid))::bigint AS total FROM relations")
        .fetch_one(&store.primary).await?;
    let tables: Vec<_> = sqlx::query("SELECT relname, pg_total_relation_size(oid) AS bytes FROM pg_class WHERE relnamespace = 'public'::regnamespace AND relkind IN ('r', 'p') ORDER BY relname")
        .fetch_all(&store.primary).await?.into_iter().map(|table| json!({"name": table.get::<String, _>("relname"), "total_bytes": table.get::<i64, _>("bytes")})).collect();
    let database_bytes: i64 = sqlx::query_scalar("SELECT pg_database_size(current_database())")
        .fetch_one(&store.primary)
        .await?;
    Ok(
        json!({"heap_bytes": row.get::<i64, _>("heap"), "index_bytes": row.get::<i64, _>("indexes"), "total_bytes": row.get::<i64, _>("total"), "database_bytes": database_bytes, "tables": tables}),
    )
}
