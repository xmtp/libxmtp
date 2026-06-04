use crate::error::GrpcBuilderError;
use http::Request;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{body::Body, client::GrpcService};
use tower::Service;
use url::Url;

use std::task::{Context, Poll};

/// HTTP/2 / TCP keep-alive parameters for the gRPC channel.
///
/// Defaults favour *fast* detection of a dead connection (~26s = interval + timeout),
/// which suits mobile clients and the default native path. A deployment on a high-latency
/// link can relax any of them via environment variables, without affecting other
/// consumers:
///
/// | env var                             | field                        | default |
/// |-------------------------------------|------------------------------|---------|
/// | `XMTP_GRPC_KEEPALIVE_INTERVAL_SECS` | `http2_keep_alive_interval`  | 16      |
/// | `XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS`  | `keep_alive_timeout`         | 10      |
/// | `XMTP_GRPC_TCP_KEEPALIVE_SECS`      | `tcp_keepalive` (0 disables) | 16      |
/// | `XMTP_GRPC_KEEPALIVE_WHILE_IDLE`    | `keep_alive_while_idle`      | true    |
///
/// Read once per process (servers set these before start), so it is not part of the TLS
/// endpoint cache key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeepaliveConfig {
    interval: Duration,
    timeout: Duration,
    tcp_keepalive: Option<Duration>,
    while_idle: bool,
}

impl KeepaliveConfig {
    const DEFAULT_INTERVAL: Duration = Duration::from_secs(16);
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
    const DEFAULT_TCP_KEEPALIVE: Duration = Duration::from_secs(16);

    fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from a generic lookup so the parsing is unit-testable without touching the
    /// process environment.
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let secs = |key: &str| {
            get(key)
                .and_then(|raw| raw.trim().parse::<u64>().ok())
                .map(Duration::from_secs)
        };
        Self {
            interval: secs("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS").unwrap_or(Self::DEFAULT_INTERVAL),
            timeout: secs("XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS").unwrap_or(Self::DEFAULT_TIMEOUT),
            // An explicit `0` disables TCP keep-alive; unset falls back to the default.
            tcp_keepalive: match secs("XMTP_GRPC_TCP_KEEPALIVE_SECS") {
                Some(d) if d.is_zero() => None,
                Some(d) => Some(d),
                None => Some(Self::DEFAULT_TCP_KEEPALIVE),
            },
            while_idle: get("XMTP_GRPC_KEEPALIVE_WHILE_IDLE")
                .and_then(|raw| parse_bool(raw.trim()))
                .unwrap_or(true),
        }
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Resolved once per process; servers set the env before start.
static KEEPALIVE: LazyLock<KeepaliveConfig> = LazyLock::new(KeepaliveConfig::from_env);

#[derive(Clone, Debug)]
pub struct NativeGrpcService {
    inner: Channel,
}

fn is_url_secure(url: &Url) -> bool {
    matches!(url.scheme(), "https" | "grpcs")
}

impl NativeGrpcService {
    pub fn new(host: url::Url, limit: Option<u64>) -> Result<Self, GrpcBuilderError> {
        let channel = match is_url_secure(&host) {
            true => create_tls_channel(host.into(), limit.unwrap_or(5000))?,
            false => apply_channel_options(
                Channel::from_shared(String::from(host))?,
                limit.unwrap_or(5000),
            )
            .connect_lazy(),
        };

        Ok(Self { inner: channel })
    }
}

impl Service<Request<Body>> for NativeGrpcService {
    type Response = <Channel as Service<Request<Body>>>::Response;
    type Error = <Channel as GrpcService<Body>>::Error;
    type Future = <Channel as GrpcService<Body>>::Future;

    fn poll_ready(&mut self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        <Channel as Service<Request<Body>>>::poll_ready(&mut self.inner, ctx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        <Channel as Service<Request<Body>>>::call(&mut self.inner, request)
    }
}

pub(crate) fn apply_channel_options(endpoint: Endpoint, limit: u64) -> Endpoint {
    let keepalive = *KEEPALIVE;
    endpoint
        // Purpose: This setting controls the size of the initial connection-level flow control window for HTTP/2, which is the underlying protocol for gRPC.
        // Functionality: Flow control in HTTP/2 manages how much data can be in flight on the network. Setting the initial connection window size to (1 << 31) - 1 (the maximum possible value for a 32-bit integer, which is 2,147,483,647 bytes) essentially allows the client to receive a very large amount of data from the server before needing to acknowledge receipt and permit more data to be sent. This can be particularly useful in high-latency networks or when transferring large amounts of data.
        // Impact: Increasing the window size can improve throughput by allowing more data to be in transit at a time, but it may also increase memory usage and can potentially lead to inefficient use of bandwidth if the network is unreliable.
        .initial_connection_window_size(Some((1 << 31) - 1))
        // Purpose: Configures whether the client should send keep-alive pings to the server when the connection is idle.
        // Functionality: When set to true, this option ensures that periodic pings are sent on an idle connection to keep it alive and detect if the server is still responsive.
        // Impact: This helps maintain active connections, particularly through NATs, load balancers, and other middleboxes that might drop idle connections. It helps ensure that the connection is promptly usable when new requests need to be sent.
        .keep_alive_while_idle(keepalive.while_idle)
        // Purpose: Sets the maximum amount of time the client will wait for a connection to be established.
        // Functionality: If a connection cannot be established within the specified duration, the attempt is aborted and an error is returned.
        // Impact: This setting prevents the client from waiting indefinitely for a connection to be established, which is crucial in scenarios where rapid failure detection is necessary to maintain responsiveness or to quickly fallback to alternative services or retry logic.
        .connect_timeout(Duration::from_secs(10))
        // Purpose: Configures the TCP keep-alive interval for the socket connection.
        // Functionality: This setting tells the operating system to send TCP keep-alive probes periodically when no data has been transferred over the connection within the specified interval.
        // Impact: Similar to the gRPC-level keep-alive, this helps keep the connection alive at the TCP layer and detect broken connections. It's particularly useful for detecting half-open connections and ensuring that resources are not wasted on unresponsive peers.
        .tcp_keepalive(keepalive.tcp_keepalive)
        // Purpose: Sets a maximum duration for the client to wait for a response to a request.
        // Functionality: If a response is not received within the specified timeout, the request is canceled and an error is returned.
        // Impact: This is critical for bounding the wait time for operations, which can enhance the predictability and reliability of client interactions by avoiding indefinitely hanging requests.
        .timeout(Duration::from_secs(120))
        // Purpose: Specifies how long the client will wait for a response to a keep-alive ping before considering the connection dead.
        // Functionality: If a ping response is not received within this duration, the connection is presumed to be lost and is closed.
        // Impact: This setting is crucial for quickly detecting unresponsive connections and freeing up resources associated with them. It ensures that the client has up-to-date information on the status of connections and can react accordingly.
        //
        // Values are sourced from `KeepaliveConfig` (env-overridable). The defaults are
        // intentionally aggressive (~26s detection = interval + timeout), which suits mobile
        // and the default native path. A high-latency deployment such as herald — whose
        // cross-cloud Fly->AWS round-trip can transiently starve a PONG past a tight deadline
        // and trigger spurious `UNAVAILABLE` (status 14) disconnects (herald-lite #70) —
        // relaxes them via environment variables without affecting other consumers.
        .keep_alive_timeout(keepalive.timeout)
        .http2_keep_alive_interval(keepalive.interval)
        .rate_limit(limit, Duration::from_secs(60))
}

/// Cache of fully-built TLS endpoints, keyed by `(host, rate_limit)`.
///
/// Building the endpoint runs `ClientTlsConfig::with_enabled_roots()`, which
/// makes tonic call `rustls_native_certs::load_native_certs()` and parse the
/// whole OS trust store into a rustls `ClientConfig` on *every* call. On macOS
/// that read is serialized through Security.framework (~40ms each), so callers
/// that create many clients (e.g. loading 100 identities, each building an
/// api + sync channel) paid it hundreds of times.
///
/// The built `Endpoint` owns an `Arc<ClientConfig>` with the parsed roots, so
/// caching it and calling `connect_lazy()` on a clone pays that cost once per
/// host while still handing every client its own connection (`connect_lazy`
/// builds a fresh `Channel`). The cache key includes `limit` because it feeds
/// the endpoint's rate-limit option.
static TLS_ENDPOINTS: LazyLock<Mutex<HashMap<(String, u64), Endpoint>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tracing::instrument(level = "trace", skip_all)]
pub fn create_tls_channel(address: String, limit: u64) -> Result<Channel, GrpcBuilderError> {
    // Hold the lock across the (one-time, per-key) build so concurrent callers
    // for the same host load native certs exactly once instead of racing.
    let mut endpoints = TLS_ENDPOINTS.lock().unwrap_or_else(|e| e.into_inner());
    let endpoint = match endpoints.get(&(address.clone(), limit)) {
        Some(endpoint) => endpoint.clone(),
        None => {
            let endpoint = apply_channel_options(Channel::from_shared(address.clone())?, limit)
                .tls_config(ClientTlsConfig::new().with_enabled_roots())?;
            endpoints.insert((address, limit), endpoint.clone());
            endpoint
        }
    };
    Ok(endpoint.connect_lazy())
}

#[cfg(test)]
mod keepalive_tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn defaults_when_env_absent() {
        let cfg = KeepaliveConfig::from_lookup(|_| None);
        assert_eq!(cfg.interval, Duration::from_secs(16));
        assert_eq!(cfg.timeout, Duration::from_secs(10));
        assert_eq!(cfg.tcp_keepalive, Some(Duration::from_secs(16)));
        assert!(cfg.while_idle);
    }

    #[test]
    fn env_overrides_are_applied() {
        let cfg = KeepaliveConfig::from_lookup(lookup(&[
            ("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS", "30"),
            ("XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS", "45"),
            ("XMTP_GRPC_TCP_KEEPALIVE_SECS", "30"),
            ("XMTP_GRPC_KEEPALIVE_WHILE_IDLE", "false"),
        ]));
        assert_eq!(cfg.interval, Duration::from_secs(30));
        assert_eq!(cfg.timeout, Duration::from_secs(45));
        assert_eq!(cfg.tcp_keepalive, Some(Duration::from_secs(30)));
        assert!(!cfg.while_idle);
    }

    #[test]
    fn zero_tcp_keepalive_disables_it() {
        let cfg = KeepaliveConfig::from_lookup(lookup(&[("XMTP_GRPC_TCP_KEEPALIVE_SECS", "0")]));
        assert_eq!(cfg.tcp_keepalive, None);
    }

    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let cfg = KeepaliveConfig::from_lookup(lookup(&[
            ("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS", "not-a-number"),
            ("XMTP_GRPC_KEEPALIVE_WHILE_IDLE", "maybe"),
        ]));
        assert_eq!(cfg.interval, Duration::from_secs(16));
        assert!(cfg.while_idle);
    }
}
