use std::{sync::Arc, collections::BTreeMap};
use sqlx::PgPool;
use tokio::{sync::Notify, task::JoinHandle};
use tonic::Status;
use xmtp_common::time::{Duration, Instant, sleep};
use crate::{config::{Config, FETCH_BUFFER_BYTES}, db::{self, stream::Candidate}, error::Error};
use super::{Registry, ENVELOPE_OVERHEAD};

const GAP_BATCH: usize = 128;
const GAP_ROWS: i64 = 64;
const FORWARD_ROWS: i64 = 1_024;

struct Worker(JoinHandle<Result<(), Error>>);
struct Recovery { boundary: i64, connection: sqlx::PgConnection, last_attempt: Instant }
impl Drop for Worker { fn drop(&mut self) { self.0.abort(); } }

/// Supervise independent primary maintenance and read-only tailing. Fail all old
/// sessions before replacing recovery state after any worker failure.
pub(super) async fn start(primary: PgPool, read: PgPool, registry: Arc<Registry>, config: &Config) -> Result<JoinHandle<()>, Error> {
    let interval = Duration::from_millis(config.streams.poll_interval_ms);
    let wait = config.publishing.max_barrier_wait_ms;
    let max_gaps = config.streams.max_gap_ranges;
    let statement_ms = config.database.max_statement_timeout_ms;
    let first = bootstrap(&primary, &read, wait, interval, statement_ms).await?;
    registry.ready();
    Ok(tokio::spawn(async move {
        let mut initial = first;
        loop {
            let maintenance = Arc::new(Notify::new());
            let mut boundary = Worker(tokio::spawn(maintain(primary.clone(), maintenance.clone(), interval, wait, initial.last_attempt)));
            let tailer = run(initial.connection, &registry, maintenance, initial.boundary, max_gaps, interval);
            let failure = tokio::select! {
                result = tailer => result.err().unwrap_or_else(|| Status::unavailable("tailer stopped")),
                _ = &mut boundary.0 => Status::unavailable("boundary maintenance failed"),
            };
            registry.fail_all(failure);
            drop(boundary);
            loop {
                sleep(interval).await;
                if let Ok(boundary) = bootstrap(&primary, &read, wait, interval, statement_ms).await { initial = boundary; break; }
            }
            registry.ready();
        }
    }))
}

/// Establish a new closed boundary and wait for its visibility on the selected
/// database. No subscriptions may start during this wait.
async fn bootstrap(primary: &PgPool, read: &PgPool, wait: u64, interval: Duration, statement_ms: u64) -> Result<Recovery, Error> {
    let (boundary, last_attempt) = loop {
        let attempt = Instant::now();
        if let Some(boundary) = db::boundary::advance(primary, wait).await? { break (boundary, attempt); }
        sleep(interval).await;
    };
    let mut connection = db::dedicated_read(read, statement_ms).await?;
    loop {
        let mut tx = db::stream::snapshot_connection(&mut connection).await?;
        let visible = db::stream::boundary(&mut tx).await?;
        tx.commit().await?;
        if visible >= boundary { return Ok(Recovery { boundary, connection, last_attempt }); }
        sleep(interval).await;
    }
}

/// Coalesce maintenance demand with Notify's one stored permit. A lock timeout
/// keeps the request pending; other database failures end this worker.
async fn maintain(primary: PgPool, requested: Arc<Notify>, interval: Duration, wait: u64, mut last: Instant) -> Result<(), Error> {
    loop {
        requested.notified().await;
        loop {
            sleep(interval.saturating_sub(last.elapsed())).await;
            last = Instant::now();
            if db::boundary::advance(&primary, wait).await?.is_some() { break; }
        }
    }
}

/// Own the selected database connection and all gap state. Dispatch only after
/// a successful snapshot, and never discard unknown gaps to meet capacity.
async fn run(mut connection: sqlx::PgConnection, registry: &Registry, maintenance: Arc<Notify>, initial: i64, max_gaps: usize, interval: Duration) -> Result<(), Status> {
    let mut forward = initial;
    let mut gaps = Vec::new();
    loop {
        let (next_forward, next_gaps, rows, immediate, boundary) = poll(&mut connection, forward, &gaps).await
            .map_err(|_| Status::unavailable("tailer database read failed"))?;
        if next_gaps.len() > max_gaps { return Err(Status::resource_exhausted("tailer gap capacity exceeded")); }
        registry.dispatch(rows);
        forward = next_forward;
        gaps = next_gaps;
        if gaps.iter().any(|&(_, high)| high > boundary) { maintenance.notify_one(); }
        if !immediate { sleep(interval).await; }
    }
}

struct PollPage {
    selected: Vec<Candidate>,
    bytes: usize,
    gaps: BTreeMap<i64, i64>,
}

impl PollPage {
    fn select(&mut self, candidate: Candidate) -> bool {
        let size = candidate.payload_bytes as usize + ENVELOPE_OVERHEAD;
        if self.bytes + size > FETCH_BUFFER_BYTES { return false; }
        self.bytes += size;
        self.selected.push(candidate);
        true
    }
}

type PollResult = (i64, Vec<(i64, i64)>, Vec<db::StoredEnvelope>, bool, i64);

/// Probe earlier gaps before newer rows in one snapshot. Unread pages prevent
/// forward dispatch; only exhausted, boundary-covered absence can be retired.
async fn poll(connection: &mut sqlx::PgConnection, forward: i64, gaps: &[(i64, i64)]) -> Result<PollResult, Error> {
    let started = Instant::now();
    let mut tx = db::stream::snapshot_connection(connection).await?;
    let boundary = db::stream::boundary(&mut tx).await?;
    let mut page = PollPage { selected: Vec::new(), bytes: 0, gaps: gaps.iter().copied().collect() };
    let mut immediate = false;
    let mut complete = true;
    'gaps: for batch in gaps.chunks(GAP_BATCH) {
        let candidates = db::stream::gaps(&mut tx, batch, GAP_ROWS).await?;
        for &(low, high) in batch {
            let candidates: Vec<_> = candidates.iter().filter(|candidate| candidate.sequence_id >= low && candidate.sequence_id <= high).collect();
            let mut consumed = true;
            for candidate in &candidates {
                if !page.select((*candidate).clone()) { consumed = false; complete = false; immediate = true; break; }
                subtract(&mut page.gaps, candidate.sequence_id, candidate.sequence_id);
            }
            if candidates.len() < GAP_ROWS as usize && consumed {
                subtract(&mut page.gaps, low, high.min(boundary));
            } else { complete = false; immediate = true; break 'gaps; }
        }
        if page.bytes >= FETCH_BUFFER_BYTES.saturating_sub(ENVELOPE_OVERHEAD) { complete = false; immediate = true; break; }
    }
    let mut next_forward = forward;
    if complete {
        let candidates = db::stream::forward(&mut tx, forward, FORWARD_ROWS).await?;
        immediate |= candidates.len() == FORWARD_ROWS as usize;
        for candidate in candidates {
            let id = candidate.sequence_id;
            if !page.select(candidate) { immediate = true; break; }
            if id > next_forward + 1 { page.gaps.insert(next_forward + 1, id - 1); }
            next_forward = id;
        }
    }
    let ids: Vec<_> = page.selected.iter().map(|candidate| candidate.sequence_id).collect();
    let rows = if ids.is_empty() { Vec::new() } else { db::stream::payloads(&mut tx, &ids).await? };
    if rows.len() != ids.len() { return Err(Error::Invariant("tailer candidate disappeared in snapshot")); }
    tx.commit().await?;
    tracing::trace!(rows = rows.len(), gaps = page.gaps.len(), duration_ms = started.elapsed().as_millis() as u64, "tailer snapshot complete");
    Ok((next_forward, page.gaps.into_iter().collect(), rows, immediate, boundary))
}

/// Remove a proved interval while preserving every unknown endpoint. Ranges
/// remain disjoint; no allocation is represented by a separate tracking object.
fn subtract(ranges: &mut BTreeMap<i64, i64>, low: i64, high: i64) {
    if high < low { return; }
    let first = ranges.range(..=low).next_back().map_or(low, |(&start, _)| start);
    let overlaps: Vec<_> = ranges.range(first..=high).filter(|(_, end)| **end >= low)
        .map(|(&start, &end)| (start, end)).collect();
    for (start, end) in overlaps {
        ranges.remove(&start);
        if start < low { ranges.insert(start, low - 1); }
        if end > high { ranges.insert(high + 1, end); }
    }
}
