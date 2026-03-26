//! Prometheus metrics and CSV output for xdbg monitor mode.
//!
//! Metrics are opt-in at runtime: they activate only when `PUSHGATEWAY_URL` is set
//! in the environment. Without that env var every `record_*` and `push_metrics` call
//! is a silent no-op, keeping developer CLI usage clean.
//!
//! # Metric emission
//! Two parallel channels:
//! 1. **Prometheus PushGateway** — numeric gauges/counters pushed via HTTP after each
//!    operation.  Used by the ECS-deployed client monitor.
//! 2. **CSV stdout** — `kind,name,value,timestamp_ms,label=value;…` lines printed to
//!    stdout.  Compatible with log pipelines and easy to parse with `jq` / `awk`.

use prometheus::{CounterVec, Encoder, GaugeVec, Opts, Registry, TextEncoder};
use reqwest::Client;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Global singletons
// ---------------------------------------------------------------------------

static METRICS: OnceLock<Metrics> = OnceLock::new();
static PUSHGATEWAY_URL: OnceLock<String> = OnceLock::new();

// ---------------------------------------------------------------------------
// Metrics struct
// ---------------------------------------------------------------------------

pub struct Metrics {
    registry: Registry,
    latency: GaugeVec,
    member_count: GaugeVec,
    throughput: CounterVec,
    // Migration-specific metrics
    migration_latency: prometheus::Histogram,
    migration_success: prometheus::IntCounter,
    migration_failure: prometheus::IntCounter,
    // Content-parity metrics
    parity_pass: prometheus::IntCounterVec,
    parity_fail: prometheus::IntCounterVec,
    parity_missing: prometheus::IntCounterVec,
    parity_extra: prometheus::IntCounterVec,
    // Wallet continuity metrics (labelled by check_type)
    continuity_pass: prometheus::IntCounterVec,
    continuity_fail: prometheus::IntCounterVec,
    client: Client,
}

impl Metrics {
    fn new() -> Self {
        let registry = Registry::new();

        let latency = GaugeVec::new(
            Opts::new(
                "xdbg_operation_latency_seconds",
                "Latency of xdbg operations in seconds",
            ),
            &["operation_type"],
        )
        .expect("valid gauge");

        let member_count = GaugeVec::new(
            Opts::new(
                "xdbg_group_add_member_count",
                "Number of members added to a group",
            ),
            &["operation_type"],
        )
        .expect("valid gauge");

        let throughput = CounterVec::new(
            Opts::new("xdbg_messages_sent_total", "Total number of messages sent"),
            &["operation_type"],
        )
        .expect("valid counter");

        // Migration-specific metrics
        let migration_latency = prometheus::Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "xdbg_migration_latency_seconds",
                "V3→V4 migration latency in seconds (time for message to appear on V4 after V3 write)",
            )
            .buckets(vec![1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0]),
        )
        .expect("valid histogram");

        let migration_success = prometheus::IntCounter::new(
            "xdbg_migration_success_total",
            "Total successful V3→V4 migration round-trips",
        )
        .expect("valid counter");

        let migration_failure = prometheus::IntCounter::new(
            "xdbg_migration_failure_total",
            "Total failed V3→V4 migration round-trips (timeout or error)",
        )
        .expect("valid counter");

        // Content-parity metrics (labelled by data_type: group_messages, identity_updates, etc.)
        let parity_pass = prometheus::IntCounterVec::new(
            Opts::new(
                "xdbg_parity_pass_total",
                "Content-parity checks that passed",
            ),
            &["data_type"],
        )
        .expect("valid counter");

        let parity_fail = prometheus::IntCounterVec::new(
            Opts::new(
                "xdbg_parity_fail_total",
                "Content-parity checks that failed",
            ),
            &["data_type"],
        )
        .expect("valid counter");

        let parity_missing = prometheus::IntCounterVec::new(
            Opts::new(
                "xdbg_parity_missing_total",
                "V3 payloads missing from V4 after migration",
            ),
            &["data_type"],
        )
        .expect("valid counter");

        let parity_extra = prometheus::IntCounterVec::new(
            Opts::new(
                "xdbg_parity_extra_total",
                "Unexpected extra envelopes on V4 beyond V3 baseline",
            ),
            &["data_type"],
        )
        .expect("valid counter");

        let continuity_pass = prometheus::IntCounterVec::new(
            Opts::new(
                "xdbg_continuity_pass_total",
                "Wallet continuity checks that passed, by check type",
            ),
            &["check_type"],
        )
        .expect("valid counter");

        let continuity_fail = prometheus::IntCounterVec::new(
            Opts::new(
                "xdbg_continuity_fail_total",
                "Wallet continuity checks that failed, by check type",
            ),
            &["check_type"],
        )
        .expect("valid counter");

        registry
            .register(Box::new(latency.clone()))
            .expect("register latency");
        registry
            .register(Box::new(member_count.clone()))
            .expect("register member_count");
        registry
            .register(Box::new(throughput.clone()))
            .expect("register throughput");
        registry
            .register(Box::new(migration_latency.clone()))
            .expect("register migration_latency");
        registry
            .register(Box::new(migration_success.clone()))
            .expect("register migration_success");
        registry
            .register(Box::new(migration_failure.clone()))
            .expect("register migration_failure");
        registry
            .register(Box::new(parity_pass.clone()))
            .expect("register parity_pass");
        registry
            .register(Box::new(parity_fail.clone()))
            .expect("register parity_fail");
        registry
            .register(Box::new(parity_missing.clone()))
            .expect("register parity_missing");
        registry
            .register(Box::new(parity_extra.clone()))
            .expect("register parity_extra");
        registry
            .register(Box::new(continuity_pass.clone()))
            .expect("register continuity_pass");
        registry
            .register(Box::new(continuity_fail.clone()))
            .expect("register continuity_fail");

        Metrics {
            registry,
            latency,
            member_count,
            throughput,
            migration_latency,
            migration_success,
            migration_failure,
            parity_pass,
            parity_fail,
            parity_missing,
            parity_extra,
            continuity_pass,
            continuity_fail,
            client: Client::new(),
        }
    }

    fn set_latency(&self, operation_type: &str, seconds: f64) {
        self.latency
            .with_label_values(&[operation_type])
            .set(seconds);
    }

    fn set_member_count(&self, operation_type: &str, count: f64) {
        self.member_count
            .with_label_values(&[operation_type])
            .set(count);
    }

    fn inc_throughput(&self, operation_type: &str) {
        self.throughput.with_label_values(&[operation_type]).inc();
    }

    async fn push(&self, job: &str, push_url: &str) {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let mf = self.registry.gather();
        encoder.encode(&mf, &mut buffer).expect("encode metrics");
        let body = String::from_utf8(buffer).expect("utf8 conversion");

        let url = format!("{}/metrics/job/{}", push_url.trim_end_matches('/'), job);

        match self.client.post(&url).body(body).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    tracing::warn!(status = %resp.status(), "metrics push failed");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "metrics push error");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialise the metrics subsystem.
///
/// Reads `PUSHGATEWAY_URL` from the environment.  If the variable is absent the
/// subsystem stays uninitialised and all subsequent `record_*` / `push_metrics`
/// calls are silent no-ops — safe for developer CLI usage.
pub fn init_metrics() {
    if let Ok(url) = std::env::var("PUSHGATEWAY_URL") {
        PUSHGATEWAY_URL.get_or_init(|| url);
        METRICS.get_or_init(Metrics::new);
        tracing::info!(
            pushgateway = PUSHGATEWAY_URL.get().map(|s| s.as_str()).unwrap_or(""),
            "metrics subsystem initialised"
        );
    }
}

/// Record an operation latency (seconds).  No-op when metrics are inactive.
pub fn record_latency(operation_type: &str, seconds: f64) {
    if let Some(m) = METRICS.get() {
        m.set_latency(operation_type, seconds);
    }
}

/// Record a member-count observation.  No-op when metrics are inactive.
pub fn record_member_count(operation_type: &str, count: f64) {
    if let Some(m) = METRICS.get() {
        m.set_member_count(operation_type, count);
    }
}

/// Increment the throughput counter for `operation_type`.  No-op when metrics are inactive.
pub fn record_throughput(operation_type: &str) {
    if let Some(m) = METRICS.get() {
        m.inc_throughput(operation_type);
    }
}

/// Record a migration latency observation (seconds). No-op when metrics are inactive.
pub fn record_migration_latency(seconds: f64) {
    if let Some(m) = METRICS.get() {
        m.migration_latency.observe(seconds);
        csv_metric(
            "xdbg_migration_latency_seconds",
            "migration",
            seconds,
            &[("phase", "v3_to_v4")],
        );
    }
}

/// Increment migration success counter. No-op when metrics are inactive.
pub fn record_migration_success() {
    if let Some(m) = METRICS.get() {
        m.migration_success.inc();
    }
}

/// Increment migration failure counter. No-op when metrics are inactive.
pub fn record_migration_failure() {
    if let Some(m) = METRICS.get() {
        m.migration_failure.inc();
    }
}

/// Record a content-parity pass for the given data type. No-op when metrics are inactive.
pub fn record_parity_pass(data_type: &str) {
    if let Some(m) = METRICS.get() {
        m.parity_pass.with_label_values(&[data_type]).inc();
    }
}

/// Record a content-parity failure for the given data type. No-op when metrics are inactive.
pub fn record_parity_fail(data_type: &str) {
    if let Some(m) = METRICS.get() {
        m.parity_fail.with_label_values(&[data_type]).inc();
    }
}

/// Record missing V3 payloads on V4 (count). No-op when metrics are inactive.
pub fn record_parity_missing(data_type: &str, count: u64) {
    if let Some(m) = METRICS.get() {
        m.parity_missing
            .with_label_values(&[data_type])
            .inc_by(count);
    }
}

/// Record unexpected extra envelopes on V4 (count). No-op when metrics are inactive.
pub fn record_parity_extra(data_type: &str, count: u64) {
    if let Some(m) = METRICS.get() {
        m.parity_extra.with_label_values(&[data_type]).inc_by(count);
    }
}

/// Increment wallet-continuity pass counter for `check_type`. No-op when metrics are inactive.
pub fn record_continuity_pass(check_type: &str) {
    if let Some(m) = METRICS.get() {
        m.continuity_pass.with_label_values(&[check_type]).inc();
    }
}

/// Increment wallet-continuity fail counter for `check_type`. No-op when metrics are inactive.
pub fn record_continuity_fail(check_type: &str) {
    if let Some(m) = METRICS.get() {
        m.continuity_fail.with_label_values(&[check_type]).inc();
    }
}

/// Push all current metrics to the PushGateway, awaiting completion.
///
/// The push is awaited inline so that short-lived xdbg subprocess invocations
/// (entrypoint.sh calls xdbg once per step) finish the HTTP POST before the
/// process exits.  No-op when metrics are inactive (no `PUSHGATEWAY_URL`).
/// Push errors are reported via `tracing::warn`.
pub async fn push_metrics(job: &str) {
    let Some(url) = PUSHGATEWAY_URL.get() else {
        return;
    };
    let Some(m) = METRICS.get() else {
        return;
    };
    m.push(job, url).await;
}

/// Emit the canonical per-phase metric bundle in one call:
/// `record_latency` + `record_throughput` + 2× `csv_metric` + `push_metrics`.
///
/// Use this for any timed operation that follows the standard emit pattern.
/// For operations with extra CSV labels (e.g. a `success` flag) keep the
/// individual calls explicit alongside this one.
///
/// - `operation`: short operation name, e.g. `"identity_register"`
/// - `secs`: elapsed time in **seconds**
/// - `phase`: value for the `phase=` CSV label, e.g. `"register"`
/// - `job`: PushGateway job name (`"xdbg_debug"` or `"xdbg_test"`)
pub async fn record_phase_metric(operation: &str, secs: f64, phase: &str, job: &str) {
    record_latency(operation, secs);
    record_throughput(operation);
    csv_metric("latency_seconds", operation, secs, &[("phase", phase)]);
    csv_metric("throughput_events", operation, 1.0, &[("phase", phase)]);
    push_metrics(job).await;
}

// ---------------------------------------------------------------------------
// CSV metric output
// ---------------------------------------------------------------------------

/// Print a CSV metric line to stdout.
///
/// Format: `kind,name,value,timestamp_ms,label1=v1;label2=v2`
///
/// This output is distinct from structured logs and can be filtered with
/// standard Unix tools or piped into a metrics aggregation pipeline.
pub fn csv_metric(kind: &str, name: &str, value: f64, labels: &[(&str, &str)]) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let labels_str = if labels.is_empty() {
        String::new()
    } else {
        labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";")
    };
    println!("{},{},{:.6},{},{}", kind, name, value, ts, labels_str);
}
