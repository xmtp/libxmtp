//! Backend metric catalogue and bounded recording helpers.
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};
use std::{net::SocketAddr, time::Duration};

pub const LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
const UPKEEP_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}
pub struct MetricSpec {
    pub name: &'static str,
    pub kind: MetricType,
    pub help: &'static str,
}

/// Single source of truth for backend-owned metric names, types, and descriptions.
pub const CATALOGUE: &[MetricSpec] = &[
    MetricSpec {
        name: "xmtp_auth_rejections_total",
        kind: MetricType::Counter,
        help: "Authentication rejections by reason.",
    },
    MetricSpec {
        name: "xmtp_auth_jwks_refresh_total",
        kind: MetricType::Counter,
        help: "JWKS refresh attempts by result.",
    },
    MetricSpec {
        name: "xmtp_auth_keys",
        kind: MetricType::Gauge,
        help: "Loaded JWT verification keys.",
    },
    MetricSpec {
        name: "xmtp_operation_duration_seconds",
        kind: MetricType::Histogram,
        help: "Operation span duration by operation and status.",
    },
    MetricSpec {
        name: "xmtp_telemetry_export_failures_total",
        kind: MetricType::Counter,
        help: "Failed telemetry export batches.",
    },
    MetricSpec {
        name: "grpc_server_started_total",
        kind: MetricType::Counter,
        help: "gRPC requests started.",
    },
    MetricSpec {
        name: "grpc_server_handled_total",
        kind: MetricType::Counter,
        help: "gRPC requests completed.",
    },
    MetricSpec {
        name: "grpc_server_handling_seconds",
        kind: MetricType::Histogram,
        help: "gRPC response body lifetime.",
    },
    MetricSpec {
        name: "grpc_server_in_flight",
        kind: MetricType::Gauge,
        help: "gRPC requests in flight.",
    },
    MetricSpec {
        name: "grpc_server_request_bytes_total",
        kind: MetricType::Counter,
        help: "Consumed gRPC request body bytes.",
    },
    MetricSpec {
        name: "grpc_server_response_bytes_total",
        kind: MetricType::Counter,
        help: "Emitted gRPC response body bytes.",
    },
    MetricSpec {
        name: "xmtp_db_released_open_transactions_total",
        kind: MetricType::Counter,
        help: "Open transactions rolled back on pool release.",
    },
    MetricSpec {
        name: "xmtp_db_errors_total",
        kind: MetricType::Counter,
        help: "Database errors mapped to RPC statuses.",
    },
    MetricSpec {
        name: "xmtp_publish_envelopes_total",
        kind: MetricType::Counter,
        help: "Publish input positions by response origin.",
    },
    MetricSpec {
        name: "xmtp_publish_rejections_total",
        kind: MetricType::Counter,
        help: "Rejected publishes by validation reason.",
    },
    MetricSpec {
        name: "xmtp_scw_verifications_total",
        kind: MetricType::Counter,
        help: "Smart contract wallet verification results.",
    },
    MetricSpec {
        name: "xmtp_stream_sessions",
        kind: MetricType::Gauge,
        help: "Registered stream sessions.",
    },
    MetricSpec {
        name: "xmtp_stream_topics_registered",
        kind: MetricType::Gauge,
        help: "Registered stream topic interests.",
    },
    MetricSpec {
        name: "xmtp_stream_frames_sent_total",
        kind: MetricType::Counter,
        help: "Stream frames admitted to the outbound queue.",
    },
    MetricSpec {
        name: "xmtp_stream_frames_received_total",
        kind: MetricType::Counter,
        help: "Stream frames received.",
    },
    MetricSpec {
        name: "xmtp_stream_envelopes_sent_total",
        kind: MetricType::Counter,
        help: "Stream envelopes admitted by delivery phase.",
    },
    MetricSpec {
        name: "xmtp_stream_updates_total",
        kind: MetricType::Counter,
        help: "Stream interest update results.",
    },
    MetricSpec {
        name: "xmtp_stream_ended_total",
        kind: MetricType::Counter,
        help: "Stream sessions ended by reason.",
    },
    MetricSpec {
        name: "xmtp_stream_outbound_wait_seconds",
        kind: MetricType::Histogram,
        help: "Time waiting for outbound capacity.",
    },
    MetricSpec {
        name: "xmtp_stream_fetch_wait_seconds",
        kind: MetricType::Histogram,
        help: "Time waiting for a stream fetch permit.",
    },
    MetricSpec {
        name: "xmtp_tailer_polls_total",
        kind: MetricType::Counter,
        help: "Tailer poll results.",
    },
    MetricSpec {
        name: "xmtp_tailer_rows_total",
        kind: MetricType::Counter,
        help: "Tailer rows read by source.",
    },
    MetricSpec {
        name: "xmtp_tailer_gap_ranges",
        kind: MetricType::Gauge,
        help: "Unresolved tailer gap ranges.",
    },
    MetricSpec {
        name: "xmtp_tailer_restarts_total",
        kind: MetricType::Counter,
        help: "Tailer recovery generations started.",
    },
    MetricSpec {
        name: "xmtp_tailer_ready",
        kind: MetricType::Gauge,
        help: "Whether stream recovery is ready.",
    },
    MetricSpec {
        name: "xmtp_boundary_advances_total",
        kind: MetricType::Counter,
        help: "Allocation boundary advance results.",
    },
    MetricSpec {
        name: "xmtp_backend_ready",
        kind: MetricType::Gauge,
        help: "Whether the backend reports Serving.",
    },
    MetricSpec {
        name: "xmtp_backend_info",
        kind: MetricType::Gauge,
        help: "Backend build version.",
    },
];

/// Install before logging so the first closed span has a recorder.
pub fn install(
    listen: &str,
) -> Result<metrics_exporter_prometheus::PrometheusHandle, Box<dyn std::error::Error + Send + Sync>>
{
    let builder = recorder_builder()?;
    if listen.is_empty() {
        let handle = builder.install_recorder()?;
        let upkeep = handle.clone();
        tokio::spawn(async move {
            loop {
                xmtp_common::time::sleep(UPKEEP_INTERVAL).await;
                upkeep.run_upkeep();
            }
        });
        Ok(handle)
    } else {
        let address: SocketAddr = listen.parse()?;
        let (recorder, exporter) =
            builder
                .with_http_listener(address)
                .build()
                .map_err(|error| {
                    format!("could not bind telemetry.metrics_listen {address}: {error}")
                })?;
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder)?;
        tokio::spawn(exporter);
        Ok(handle)
    }
}

pub(crate) fn recorder_builder()
-> Result<PrometheusBuilder, metrics_exporter_prometheus::BuildError> {
    PrometheusBuilder::new()
        .set_buckets_for_metric(Matcher::Suffix("_seconds".into()), LATENCY_BUCKETS)
}

/// Register descriptions once, independently of whether any series exists yet.
pub fn describe() {
    for spec in CATALOGUE {
        match spec.kind {
            MetricType::Counter => metrics::describe_counter!(spec.name, spec.help),
            MetricType::Gauge => metrics::describe_gauge!(spec.name, spec.help),
            MetricType::Histogram => metrics::describe_histogram!(spec.name, spec.help),
        }
    }
}

pub fn ready(serving: bool) {
    gauge!("xmtp_backend_ready").set(f64::from(serving));
}
pub fn info(version: &'static str) {
    gauge!("xmtp_backend_info", "version" => version).set(1.0);
}
pub(crate) fn released_open_transaction() {
    counter!("xmtp_db_released_open_transactions_total").increment(1);
}

/// RPC labels come only from this fixed route table.
#[derive(Clone, Copy)]
pub(crate) struct RpcLabels {
    pub service: &'static str,
    pub method: &'static str,
    kind: &'static str,
    health: bool,
}
impl RpcLabels {
    pub(crate) fn from_path(path: &str) -> Self {
        let (service, method) = match path {
            "/xmtp.backend.v1.QueryService/Query" => ("xmtp.backend.v1.QueryService", "Query"),
            "/xmtp.backend.v1.QueryService/QueryNewest" => {
                ("xmtp.backend.v1.QueryService", "QueryNewest")
            }
            "/xmtp.backend.v1.QueryService/Get" => ("xmtp.backend.v1.QueryService", "Get"),
            "/xmtp.backend.v1.PublishService/Publish" => {
                ("xmtp.backend.v1.PublishService", "Publish")
            }
            "/xmtp.backend.v1.SubscriptionService/Subscribe" => {
                ("xmtp.backend.v1.SubscriptionService", "Subscribe")
            }
            "/xmtp.backend.v1.SubscriptionService/SubscribeStatic" => {
                ("xmtp.backend.v1.SubscriptionService", "SubscribeStatic")
            }
            "/xmtp.backend.v1.IdentityService/GetInboxIds" => {
                ("xmtp.backend.v1.IdentityService", "GetInboxIds")
            }
            "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures" => (
                "xmtp.backend.v1.IdentityService",
                "VerifySmartContractWalletSignatures",
            ),
            "/grpc.health.v1.Health/Check" => ("grpc.health.v1.Health", "Check"),
            "/grpc.health.v1.Health/Watch" => ("grpc.health.v1.Health", "Watch"),
            "/grpc.health.v1.Health/List" => ("grpc.health.v1.Health", "List"),
            _ => ("unknown", "unknown"),
        };
        let kind = match method {
            "Subscribe" => "bidi_stream",
            "SubscribeStatic" => "server_stream",
            _ => "unary",
        };
        Self {
            service,
            method,
            kind,
            health: service == "grpc.health.v1.Health",
        }
    }
    fn labels(self) -> [(&'static str, &'static str); 3] {
        [
            ("grpc_type", self.kind),
            ("grpc_service", self.service),
            ("grpc_method", self.method),
        ]
    }
}
pub(crate) fn grpc_started(rpc: RpcLabels) {
    if rpc.health {
        return;
    }
    counter!("grpc_server_started_total", &rpc.labels()).increment(1);
    gauge!("grpc_server_in_flight", &rpc.labels()).increment(1.0);
}
pub(crate) fn grpc_completed(
    rpc: RpcLabels,
    code: tonic::Code,
    elapsed: Duration,
    request_bytes: u64,
    response_bytes: u64,
) {
    if rpc.health {
        return;
    }
    let mut completed = rpc.labels().to_vec();
    completed.push(("grpc_code", grpc_code(code)));
    counter!("grpc_server_handled_total", &completed).increment(1);
    histogram!("grpc_server_handling_seconds", &completed).record(elapsed.as_secs_f64());
    counter!("grpc_server_request_bytes_total", &rpc.labels()).increment(request_bytes);
    counter!("grpc_server_response_bytes_total", &rpc.labels()).increment(response_bytes);
    gauge!("grpc_server_in_flight", &rpc.labels()).decrement(1.0);
}
/// Match go-grpc-prometheus status labels byte for byte.
pub(crate) fn grpc_code(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "OK",
        tonic::Code::Cancelled => "Cancelled",
        tonic::Code::Unknown => "Unknown",
        tonic::Code::InvalidArgument => "InvalidArgument",
        tonic::Code::DeadlineExceeded => "DeadlineExceeded",
        tonic::Code::NotFound => "NotFound",
        tonic::Code::AlreadyExists => "AlreadyExists",
        tonic::Code::PermissionDenied => "PermissionDenied",
        tonic::Code::ResourceExhausted => "ResourceExhausted",
        tonic::Code::FailedPrecondition => "FailedPrecondition",
        tonic::Code::Aborted => "Aborted",
        tonic::Code::OutOfRange => "OutOfRange",
        tonic::Code::Unimplemented => "Unimplemented",
        tonic::Code::Internal => "Internal",
        tonic::Code::Unavailable => "Unavailable",
        tonic::Code::DataLoss => "DataLoss",
        tonic::Code::Unauthenticated => "Unauthenticated",
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PublishOutcome {
    Stored,
    Duplicate,
    Rejected,
}
impl PublishOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Duplicate => "duplicate",
            Self::Rejected => "rejected",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum DbErrorKind {
    Timeout,
    Invariant,
    Connection,
    Other,
}
impl DbErrorKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Invariant => "invariant",
            Self::Connection => "connection",
            Self::Other => "other",
        }
    }
}
pub(crate) fn publish_outcome(outcome: PublishOutcome, n: usize) {
    if n > 0 {
        counter!("xmtp_publish_envelopes_total", "outcome" => outcome.as_str()).increment(n as u64);
    }
}

/// Count decoded input positions even when the handler future is cancelled.
pub(crate) struct PublishAttempt(usize);
impl PublishAttempt {
    pub(crate) fn new(positions: usize) -> Self {
        Self(positions)
    }

    /// Record response origins only when the full response fits the transport limit.
    pub(crate) fn succeeded(mut self, stored: usize, duplicate: usize) {
        self.0 = 0;
        publish_outcome(PublishOutcome::Stored, stored);
        publish_outcome(PublishOutcome::Duplicate, duplicate);
    }
}
impl Drop for PublishAttempt {
    fn drop(&mut self) {
        publish_outcome(PublishOutcome::Rejected, self.0);
    }
}
pub(crate) fn publish_rejected(reason: crate::api::publish_error::Reason) {
    counter!("xmtp_publish_rejections_total", "reason" => reason.as_str_name()).increment(1);
}
pub(crate) fn db_error(kind: DbErrorKind) {
    counter!("xmtp_db_errors_total", "kind" => kind.as_str()).increment(1);
}

#[derive(Clone, Copy)]
pub(crate) enum VerificationResult {
    Valid,
    Invalid,
    Error,
}
impl VerificationResult {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }
}
pub(crate) fn scw_verified(result: VerificationResult) {
    counter!("xmtp_scw_verifications_total", "result" => result.as_str()).increment(1);
}
#[derive(Clone, Copy)]
pub(crate) enum BoundaryResult {
    Ok,
    LockTimeout,
    Error,
}
impl BoundaryResult {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::LockTimeout => "lock_timeout",
            Self::Error => "error",
        }
    }
}
pub(crate) fn boundary_advanced(result: BoundaryResult) {
    counter!("xmtp_boundary_advances_total", "result" => result.as_str()).increment(1);
}

#[derive(Clone, Copy)]
pub(crate) enum StreamKind {
    Bidi,
    Static,
}
impl StreamKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bidi => "bidi",
            Self::Static => "static",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Frame {
    Started,
    Applied,
    Messages,
    Keepalive,
    Ping,
    Pong,
    Update,
}
impl Frame {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Applied => "applied",
            Self::Messages => "messages",
            Self::Keepalive => "keepalive",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Update => "update",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum DeliveryPhase {
    CatchUp,
    Live,
}
impl DeliveryPhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CatchUp => "catch_up",
            Self::Live => "live",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum UpdateOutcome {
    Applied,
    Invalid,
    RateLimited,
}
impl UpdateOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Invalid => "invalid",
            Self::RateLimited => "rate_limited",
        }
    }
}
pub(crate) fn stream_registered(kind: StreamKind) {
    gauge!("xmtp_stream_sessions", "kind" => kind.as_str()).increment(1.0);
}
pub(crate) fn stream_deregistered(kind: StreamKind, topics: usize) {
    gauge!("xmtp_stream_sessions", "kind" => kind.as_str()).decrement(1.0);
    gauge!("xmtp_stream_topics_registered").decrement(topics as f64);
}
pub(crate) fn stream_topic_registered() {
    gauge!("xmtp_stream_topics_registered").increment(1.0);
}
pub(crate) fn stream_topic_removed() {
    gauge!("xmtp_stream_topics_registered").decrement(1.0);
}
pub(crate) fn stream_frame_sent(frame: Frame) {
    counter!("xmtp_stream_frames_sent_total", "frame" => frame.as_str()).increment(1);
}
pub(crate) fn stream_frame_received(frame: Frame) {
    counter!("xmtp_stream_frames_received_total", "frame" => frame.as_str()).increment(1);
}
pub(crate) fn stream_envelopes_sent(phase: DeliveryPhase, count: usize) {
    counter!("xmtp_stream_envelopes_sent_total", "phase" => phase.as_str()).increment(count as u64);
}
pub(crate) fn stream_updated(outcome: UpdateOutcome) {
    counter!("xmtp_stream_updates_total", "outcome" => outcome.as_str()).increment(1);
}
pub(crate) fn stream_ended(reason: crate::stream::StreamEnd) {
    counter!("xmtp_stream_ended_total", "reason" => reason.as_str()).increment(1);
}
pub(crate) fn stream_outbound_waited(wait: Duration) {
    if !wait.is_zero() {
        histogram!("xmtp_stream_outbound_wait_seconds").record(wait.as_secs_f64());
    }
}
pub(crate) struct FetchWait(xmtp_common::time::Instant);
impl FetchWait {
    pub(crate) fn start() -> Self {
        Self(xmtp_common::time::Instant::now())
    }
}
impl Drop for FetchWait {
    fn drop(&mut self) {
        histogram!("xmtp_stream_fetch_wait_seconds").record(self.0.elapsed().as_secs_f64());
    }
}
pub(crate) fn tailer_ready(ready: bool) {
    gauge!("xmtp_tailer_ready").set(f64::from(ready));
}
pub(crate) fn tailer_restarted() {
    counter!("xmtp_tailer_restarts_total").increment(1);
    tailer_ready(false);
    gauge!("xmtp_tailer_gap_ranges").set(0.0);
}
pub(crate) fn tailer_polled(ok: bool) {
    counter!("xmtp_tailer_polls_total", "result" => if ok { "ok" } else { "error" }).increment(1);
}
pub(crate) fn tailer_rows_read(forward: usize, gap: usize) {
    counter!("xmtp_tailer_rows_total", "source" => "forward").increment(forward as u64);
    counter!("xmtp_tailer_rows_total", "source" => "gap").increment(gap as u64);
}
pub(crate) fn tailer_gaps_observed(gaps: usize) {
    gauge!("xmtp_tailer_gap_ranges").set(gaps as f64);
}

#[cfg(test)]
mod tests;

/// Record only fixed authentication reason labels.
pub(crate) fn auth_rejection(reason: crate::auth::verify::Rejection) {
    counter!("xmtp_auth_rejections_total", "reason" => reason.label()).increment(1);
}
pub(crate) fn auth_jwks_refresh(success: bool) {
    counter!("xmtp_auth_jwks_refresh_total", "result" => if success { "ok" } else { "error" })
        .increment(1);
}
pub(crate) fn auth_keys(count: usize) {
    gauge!("xmtp_auth_keys").set(count as f64);
}
