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

        registry
            .register(Box::new(latency.clone()))
            .expect("register latency");
        registry
            .register(Box::new(member_count.clone()))
            .expect("register member_count");
        registry
            .register(Box::new(throughput.clone()))
            .expect("register throughput");

        Metrics {
            registry,
            latency,
            member_count,
            throughput,
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

/// Async-push all current metrics to the PushGateway.
///
/// Fire-and-forget: the push runs in a detached `tokio::spawn` task so the
/// caller is not blocked.  No-op when metrics are inactive (no
/// `PUSHGATEWAY_URL`).  Push errors are reported via `tracing::warn`.
pub fn push_metrics(job: &'static str) {
    let Some(url) = PUSHGATEWAY_URL.get().cloned() else {
        return;
    };
    // Guard: only proceed if the metrics singleton is initialised.
    if METRICS.get().is_none() {
        return;
    }
    tokio::spawn(async move {
        METRICS.get().unwrap().push(job, &url).await;
    });
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
pub fn record_phase_metric(operation: &str, secs: f64, phase: &str, job: &'static str) {
    record_latency(operation, secs);
    record_throughput(operation);
    csv_metric("latency_seconds", operation, secs, &[("phase", phase)]);
    csv_metric("throughput_events", operation, 1.0, &[("phase", phase)]);
    push_metrics(job);
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
