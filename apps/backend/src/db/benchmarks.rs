//! Manual database benchmarks for schema experiments, using the backend's real
//! Store reads and admitted-publish path. `baseline` loads explicit JSON settings,
//! seeds disposable migrated databases, and writes raw JSONL observations.
//! Workload dimensions must be unique before any database is created.
//! Corpus construction, workloads, and reporting live in sibling modules.
//! `partition` installs the approved experimental schema only in disposable fixtures;
//! its writer and provisioning paths are excluded from production builds.
//! `experiments` selects paging algorithms and index profiles independently.
//! Query controls share each seeded corpus; covering-index builds are separately
//! timed, and write_sizes limits costly physical write copies to explicit sizes.

mod corpus;
pub(super) mod experiments;
pub(super) mod partition;
mod report;
mod tests;
mod workload;

const REQUEST_TIMEOUT_MS: u64 = 120_000;
const SEED_TIMEOUT_MS: u64 = 900_000;

use super::Store;
use crate::test_support::{TestDatabase, TestResult, config_for};
use experiments::{IndexProfile, PageQuery};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashSet,
    fs::OpenOptions,
    hash::Hash,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use xmtp_common::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    layout: partition::Layout,
    index_profiles: Vec<IndexProfile>,
    read_strategies: Vec<PageQuery>,
    write_sizes: Vec<i64>,
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
        if has_duplicates(&self.sizes)
            || has_duplicates(&self.topic_counts)
            || has_duplicates(&self.writers)
            || has_duplicates(&self.batch_sizes)
            || has_duplicates(&self.index_profiles)
            || has_duplicates(&self.read_strategies)
            || has_duplicates(&self.write_sizes)
        {
            return Err("benchmark dimensions must not contain duplicate values".into());
        }
        if !matches!(
            self.index_profiles.as_slice(),
            [IndexProfile::Baseline] | [IndexProfile::Baseline, IndexProfile::MetadataCovering]
        ) || (self.layout.is_partitioned() && self.index_profiles.len() != 1)
            || self.read_strategies.is_empty()
            || self
                .write_sizes
                .iter()
                .any(|size| !self.sizes.contains(size))
            || self.sizes.is_empty()
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
        for size in &self.write_sizes {
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

/// Dimension values identify trial groups, so repeated values cannot be measured twice.
fn has_duplicates<Value: Eq + Hash>(values: &[Value]) -> bool {
    values.iter().collect::<HashSet<_>>().len() != values.len()
}

fn record(output: &mut impl Write, value: serde_json::Value) -> TestResult {
    serde_json::to_writer(&mut *output, &value)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

fn repository_path(path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

async fn connect(database: &TestDatabase, connections: u32, timeout_ms: u64) -> TestResult<Store> {
    Ok(Store::connect(&connection_config(database.url(), connections, timeout_ms)?).await?)
}

/// Validate benchmark pool settings without shortening the requested statement budget.
fn connection_config(
    url: &str,
    connections: u32,
    timeout_ms: u64,
) -> TestResult<crate::config::ValidatedConfig> {
    let mut config = config_for(url);
    config.database.max_connections = connections;
    config.database.max_statement_timeout_ms = timeout_ms;
    config.publishing.max_publish_duration_ms = timeout_ms.saturating_add(1);
    Ok(config.validate()?)
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
    let config: Config = serde_json::from_slice(&std::fs::read(repository_path(std::env::var(
        "XMTP_BENCH_CONFIG",
    )?))?)?;
    config.validate()?;
    let mut output = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(repository_path(&config.output))?,
    );
    let started = Instant::now();
    let mut metadata = TestDatabase::new()?;
    let store = connect(&metadata, 1, REQUEST_TIMEOUT_MS).await?;
    record(&mut output, report::environment(&store, &config).await?)?;
    store.primary.close().await;
    metadata.remove()?;
    for &size in &config.sizes {
        let mut database = TestDatabase::new()?;
        let store = connect(&database, 1, SEED_TIMEOUT_MS).await?;
        let setup = Instant::now();
        if config.layout.is_partitioned() {
            partition::install(&store).await?;
            let partitions = partition::Partitions::new(store.primary.clone());
            let last = size + size * config.write_growth_percent as i64 / 100;
            for first in (1..=last).step_by(partition::RANGE_SIZE as usize) {
                partitions.ensure(first).await?;
            }
        }
        record(
            &mut output,
            json!({"event": "schema_setup", "rows": size,
            "layout": config.layout, "seconds": setup.elapsed().as_secs_f64()}),
        )?;
        let seeded = Instant::now();
        let catalog = corpus::seed(&store, size, config.seed).await?;
        let seed_seconds = seeded.elapsed().as_secs_f64();
        store.primary.close().await;
        for &index_profile in &config.index_profiles {
            let owner = connect(&database, 1, SEED_TIMEOUT_MS).await?;
            let setup = Instant::now();
            index_profile.install(&owner).await?;
            record(
                &mut output,
                json!({"event": "index_setup", "rows": size,
                "index_profile": index_profile, "seconds": setup.elapsed().as_secs_f64()}),
            )?;
            record(
                &mut output,
                json!({"event": "corpus", "rows": size,
                "index_profile": index_profile, "seed_seconds": seed_seconds,
                "topics": catalog.len(), "storage": report::storage(&owner).await?}),
            )?;
            owner.primary.close().await;
            let store = partition::connect(&database, 1, REQUEST_TIMEOUT_MS, config.layout).await?;
            workload::reads(&store, &catalog, size, &config, index_profile, &mut output).await?;
            partition::close(&store).await;
            if config.write_sizes.contains(&size) {
                workload::writes(&database, size, &config, index_profile, &mut output).await?;
            }
        }
        database.remove()?;
    }
    if !config.write_sizes.is_empty() && config.layout.is_partitioned() {
        partition::rollovers(&config, &mut output).await?;
    }
    record(
        &mut output,
        json!({"event": "complete", "seconds": started.elapsed().as_secs_f64()}),
    )?;
}
