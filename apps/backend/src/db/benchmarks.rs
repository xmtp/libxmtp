//! Manual database benchmarks for schema experiments, using the backend's real
//! Store reads and admitted-publish path. `baseline` loads explicit JSON settings,
//! seeds disposable migrated databases, and writes raw JSONL observations.
//! Corpus construction, workloads, and reporting live in sibling modules.

mod corpus;
mod report;
mod tests;
mod workload;

use super::Store;
use crate::{
    config::Config as BackendConfig,
    test_support::{TestDatabase, TestResult},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::PathBuf,
};
use xmtp_common::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    sizes: Vec<i64>,
    topic_counts: Vec<usize>,
    writers: Vec<usize>,
    batch_sizes: Vec<usize>,
    trials: usize,
    read_ms: u64,
    write_ms: u64,
    warmup_requests: usize,
    write_growth_percent: usize,
    page_limit: i64,
    seed: i64,
    output: PathBuf,
}

impl Config {
    fn validate(&self) -> TestResult {
        if self.sizes.is_empty()
            || self
                .sizes
                .iter()
                .any(|size| !(1_000..=1_000_000_000).contains(size))
            || self.topic_counts.is_empty()
            || self
                .topic_counts
                .iter()
                .any(|count| !(1..=1_000).contains(count))
            || self.writers.is_empty()
            || self.writers.iter().any(|count| !(1..=64).contains(count))
            || self.batch_sizes.is_empty()
            || self
                .batch_sizes
                .iter()
                .any(|count| !(1..=100).contains(count))
            || self.trials == 0
            || self.read_ms == 0
            || self.write_ms == 0
            || self.warmup_requests == 0
            || !(1..=10).contains(&self.write_growth_percent)
            || !(1..=1_000).contains(&self.page_limit)
            || !(0..=1_000_000).contains(&self.seed)
            || self.output.as_os_str().is_empty()
        {
            return Err("invalid benchmark config: see benchmarks/README.md for bounds".into());
        }
        for size in &self.sizes {
            let budget = *size as usize * self.write_growth_percent / 100;
            if self.writers.iter().any(|writers| {
                self.batch_sizes
                    .iter()
                    .any(|batch| 2 * writers * batch > budget)
            }) {
                return Err(
                    "write growth budget must accommodate warmup and measurement per writer".into(),
                );
            }
        }
        Ok(())
    }
}

fn record(output: &mut impl Write, value: serde_json::Value) -> TestResult {
    serde_json::to_writer(&mut *output, &value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

async fn connect(database: &TestDatabase, connections: u32) -> TestResult<Store> {
    let mut config: BackendConfig =
        toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
    config.database.max_connections = connections;
    config.database.max_statement_timeout_ms = 120_000;
    Ok(Store::connect(&config).await?)
}

/// Run explicitly, never in ordinary tests. DATABASE_URL is an admin connection
/// to a dedicated benchmark PostgreSQL instance; it is never written to output.
#[ignore = "manual performance experiment; use just backend bench CONFIG"]
#[xmtp_common::test(
    unwrap_try = true,
    flavor = "multi_thread",
    worker_threads = 8,
    disable_logging = true
)]
async fn baseline() {
    let config: Config =
        serde_json::from_slice(&std::fs::read(std::env::var("XMTP_BENCH_CONFIG")?)?)?;
    config.validate()?;
    let mut output = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config.output)?,
    );
    let started = Instant::now();
    let mut metadata = TestDatabase::new()?;
    let store = connect(&metadata, 1).await?;
    record(&mut output, report::environment(&store, &config).await?)?;
    store.primary.close().await;
    metadata.remove()?;
    for &size in &config.sizes {
        let mut database = TestDatabase::new()?;
        let store = connect(&database, 1).await?;
        let seeded = Instant::now();
        let catalog = corpus::seed(&store, size, config.seed).await?;
        record(
            &mut output,
            json!({"event": "corpus", "rows": size,
            "seed_seconds": seeded.elapsed().as_secs_f64(), "topics": catalog.len(),
            "storage": report::storage(&store).await?}),
        )?;
        workload::reads(&store, &catalog, size, &config, &mut output).await?;
        store.primary.close().await;
        workload::writes(&database, size, &config, &mut output).await?;
        database.remove()?;
    }
    record(
        &mut output,
        json!({"event": "complete", "seconds": started.elapsed().as_secs_f64()}),
    )?;
}
