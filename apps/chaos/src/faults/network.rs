//! Network faults for the worktree's isolated Toxiproxy service.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tonic_health::pb::{
    HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
};
use xmtp_common::time::{Duration, timeout};
use xmtp_proto::{backend_v1 as api, types::Topic};

/// One proxy per installation, plus the process that shares a database.
pub const PROXY_CAPACITY: usize = 19;
// These container ports match dev/worktree-env and dev/docker/compose.yml.
const PROXY_PORTS: [u16; PROXY_CAPACITY] = [
    6003, 6005, 6007, 6012, 6014, 6019, 6021, 6023, 6025, 6030, 6035, 6037, 6039, 6041, 6043, 6047,
    6052, 6054, 6056,
];
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const HEALTH_RPC_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTH_RESPONSE_BYTES: usize = 1024;
const CONTROL_RESPONSE_BYTES: usize = 64 * 1024;
const HEALTH_TOPIC_GROUP_ID: [u8; 16] = [0; 16];
const COMPOSE_TIMEOUT: Duration = Duration::from_secs(30);
const PROXY_PREFIX: &str = "xchaos-";
const UPSTREAM: &str = "backend:5050";
const LATENCY_MS: u32 = 400;
const JITTER_MS: u32 = 200;
const BANDWIDTH_KB: u32 = 8;
const LIMIT_BYTES: u32 = 1024;
const SLICE_BYTES: u32 = 32;
const SLICE_VARIATION: u32 = 16;
const SLICE_DELAY_US: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkFault {
    Disconnect,
    Latency,
    Bandwidth,
    Timeout,
    LimitData,
    ResetPeer,
    Slicer,
    BackendPause,
}

impl NetworkFault {
    fn toxic(self) -> Option<(&'static str, Value)> {
        Some(match self {
            Self::Latency => (
                "latency",
                json!({"latency": LATENCY_MS, "jitter": JITTER_MS}),
            ),
            Self::Bandwidth => ("bandwidth", json!({"rate": BANDWIDTH_KB})),
            Self::Timeout => ("timeout", json!({"timeout": 0})),
            Self::LimitData => ("limit_data", json!({"bytes": LIMIT_BYTES})),
            Self::ResetPeer => ("reset_peer", json!({"timeout": 0})),
            Self::Slicer => (
                "slicer",
                json!({
                    "average_size": SLICE_BYTES,
                    "size_variation": SLICE_VARIATION,
                    "delay": SLICE_DELAY_US,
                }),
            ),
            Self::Disconnect | Self::BackendPause => return None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("invalid network proxy slot or count: {0}")]
    InvalidSlot(usize),
    #[error("worktree environment is missing {0}; run the chaos recipe")]
    MissingEnvironment(String),
    #[error("invalid proxy URL: {0}")]
    InvalidUrl(String),
    #[error("Toxiproxy control failed: {0}")]
    Control(#[from] reqwest::Error),
    #[error("Toxiproxy health response exceeds {CONTROL_RESPONSE_BYTES} bytes")]
    OversizeControlResponse,
    #[error("invalid Toxiproxy health response: {0}")]
    InvalidControlResponse(#[from] serde_json::Error),
    #[error("proxy {proxy} is not healthy: {detail}")]
    ProxyHealth { proxy: String, detail: &'static str },
    #[error("cannot connect to health service at {endpoint}: {source}")]
    HealthTransport {
        endpoint: String,
        #[source]
        source: tonic::transport::Error,
    },
    #[error("health request failed at {endpoint}: {source}")]
    HealthRpc {
        endpoint: String,
        #[source]
        source: tonic::Status,
    },
    #[error("health service at {endpoint} is not serving (status {status})")]
    NotServing { endpoint: String, status: i32 },
    // The upstream helper returns String, rather than a typed error.
    #[error("Toxiproxy discovery failed: {0}")]
    Discovery(String),
    #[error("another chaos run owns proxies in this worktree: {0}")]
    Occupied(String),
    #[error("network control timed out")]
    Timeout,
    #[error("backend control failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("docker compose {action} backend exited with {status}")]
    Compose {
        action: &'static str,
        status: std::process::ExitStatus,
    },
    #[error("network cleanup failed: {0:?}")]
    Cleanup(Vec<Self>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRecord {
    pub slot: usize,
    pub name: String,
    pub endpoint: String,
    pub listen: String,
    pub upstream: String,
}

#[derive(Deserialize)]
struct HealthProxy {
    name: String,
    listen: String,
    upstream: String,
    enabled: bool,
    toxics: Vec<Value>,
}

/// The caller must clear faults before a checkpoint and call shutdown on exit.
/// Only one fault window can own a slot, or the backend, at a time.
pub struct NetworkController {
    client: Client,
    api: String,
    backend: String,
    proxies: Vec<ProxyRecord>,
    backend_paused: AtomicBool,
    compose_project: String,
    compose_file: PathBuf,
}

impl NetworkController {
    pub async fn create(slot_count: usize) -> Result<Self, NetworkError> {
        if slot_count == 0 || slot_count > PROXY_CAPACITY {
            return Err(NetworkError::InvalidSlot(slot_count));
        }
        let api = environment("XMTP_TOXIPROXY_API")?;
        let backend = environment("XMTP_BACKEND_URL")?;
        let compose_project = environment("XMTP_COMPOSE_PROJECT")?;
        let client = xmtp_common::http::client_builder()
            .timeout(CONTROL_TIMEOUT)
            .build()?;
        let existing = timeout(CONTROL_TIMEOUT, xmtp_common::toxiproxy().all())
            .await
            .map_err(|_| NetworkError::Timeout)?
            .map_err(NetworkError::Discovery)?;
        if let Some(name) = existing.keys().find(|name| name.starts_with(PROXY_PREFIX)) {
            return Err(NetworkError::Occupied(name.clone()));
        }
        let mut proxies = Vec::with_capacity(slot_count);
        for (slot, port) in PROXY_PORTS.iter().enumerate().take(slot_count) {
            let endpoint = environment(&format!("XMTP_CHAOS_PROXY_{slot}_URL"))?;
            let parsed = reqwest::Url::parse(&endpoint)
                .map_err(|_| NetworkError::InvalidUrl(endpoint.clone()))?;
            if parsed.scheme() != "http"
                || parsed.host_str() != Some("127.0.0.1")
                || parsed.port().is_none()
            {
                return Err(NetworkError::InvalidUrl(endpoint));
            }
            proxies.push(ProxyRecord {
                slot,
                name: format!("{PROXY_PREFIX}{slot}"),
                endpoint,
                listen: format!("0.0.0.0:{port}"),
                upstream: UPSTREAM.into(),
            });
        }
        let controller = Self {
            client,
            api: api.trim_end_matches('/').into(),
            backend,
            proxies,
            backend_paused: AtomicBool::new(false),
            compose_project,
            compose_file: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../dev/docker/compose.yml"),
        };
        for proxy in &controller.proxies {
            if let Err(error) = controller
                .request(
                    Method::POST,
                    "proxies",
                    Some(json!({
                        "name": proxy.name,
                        "listen": proxy.listen,
                        "upstream": proxy.upstream,
                        "enabled": true,
                    })),
                )
                .await
            {
                // A lost response can follow a successful creation. Delete every
                // reserved name, including the request whose response was lost.
                return match controller.shutdown().await {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(NetworkError::Cleanup(vec![error, cleanup])),
                };
            }
        }
        Ok(controller)
    }

    pub fn records(&self) -> &[ProxyRecord] {
        &self.proxies
    }

    pub fn endpoint(&self, slot: usize) -> Result<String, NetworkError> {
        Ok(self.proxy(slot)?.endpoint.clone())
    }

    /// Check infrastructure after faults are cleared and before an SDK verdict.
    /// The direct backend probe is independent of every managed fault proxy.
    pub async fn check_health(&self) -> Result<(), NetworkError> {
        timeout(HEALTH_TIMEOUT, async {
            let channel = check_service_health(&self.backend).await?;
            // Aggregate health describes the server lifecycle. This read also
            // checks database access, without creating or publishing data.
            let mut query =
                tonic::client::Grpc::new(channel).max_decoding_message_size(HEALTH_RESPONSE_BYTES);
            query
                .ready()
                .await
                .map_err(|source| NetworkError::HealthTransport {
                    endpoint: self.backend.clone(),
                    source,
                })?;
            query
                .unary::<api::QueryNewestRequest, api::QueryNewestResponse, _>(
                    tonic::Request::new(api::QueryNewestRequest {
                        topics: vec![api::Topic {
                            topic: AsRef::<[u8]>::as_ref(&Topic::new_group_message(
                                HEALTH_TOPIC_GROUP_ID,
                            ))
                            .to_vec(),
                        }],
                        include_full_envelope: false,
                    }),
                    tonic::codegen::http::uri::PathAndQuery::from_static(
                        "/xmtp.backend.v1.QueryService/QueryNewest",
                    ),
                    tonic_prost::ProstCodec::default(),
                )
                .await
                .map_err(|source| NetworkError::HealthRpc {
                    endpoint: self.backend.clone(),
                    source,
                })?;
            let mut response = self
                .client
                .get(format!("{}/proxies", self.api))
                .send()
                .await?
                .error_for_status()?;
            if response
                .content_length()
                .is_some_and(|size| size > CONTROL_RESPONSE_BYTES as u64)
            {
                return Err(NetworkError::OversizeControlResponse);
            }
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > CONTROL_RESPONSE_BYTES.saturating_sub(body.len()) {
                    return Err(NetworkError::OversizeControlResponse);
                }
                body.extend_from_slice(&chunk);
            }
            let managed: HashMap<String, HealthProxy> = serde_json::from_slice(&body)?;
            for expected in &self.proxies {
                let actual =
                    managed
                        .get(&expected.name)
                        .ok_or_else(|| NetworkError::ProxyHealth {
                            proxy: expected.name.clone(),
                            detail: "missing proxy",
                        })?;
                let detail = if actual.name != expected.name
                    || !same_listener(&actual.listen, &expected.listen)
                    || actual.upstream != expected.upstream
                {
                    Some("route changed")
                } else if !actual.enabled {
                    Some("proxy disabled")
                } else if !actual.toxics.is_empty() {
                    Some("faults remain active")
                } else {
                    None
                };
                if let Some(detail) = detail {
                    return Err(NetworkError::ProxyHealth {
                        proxy: expected.name.clone(),
                        detail,
                    });
                }
            }
            futures::future::try_join_all(
                self.proxies
                    .iter()
                    .map(|proxy| check_service_health(&proxy.endpoint)),
            )
            .await?;
            Ok(())
        })
        .await
        .map_err(|_| NetworkError::Timeout)?
    }

    pub async fn apply(&self, slot: usize, fault: NetworkFault) -> Result<(), NetworkError> {
        let proxy = self.proxy(slot)?;
        match fault {
            NetworkFault::BackendPause => {
                // Mark it before the call: cancellation or a lost response must
                // still cause an unpause during cleanup.
                self.backend_paused.store(true, Ordering::SeqCst);
                self.compose("pause").await
            }
            NetworkFault::Disconnect => self.set_enabled(proxy, false).await,
            _ => {
                if let Some((kind, attributes)) = fault.toxic() {
                    for direction in ["upstream", "downstream"] {
                        self.request(
                            Method::POST,
                            &format!("proxies/{}/toxics", proxy.name),
                            Some(json!({
                                "name": format!("chaos-{direction}"),
                                "type": kind,
                                "stream": direction,
                                "toxicity": 1.0,
                                "attributes": attributes,
                            })),
                        )
                        .await?;
                    }
                }
                Ok(())
            }
        }
    }

    pub async fn clear_fault(&self, slot: usize, fault: NetworkFault) -> Result<(), NetworkError> {
        if fault == NetworkFault::BackendPause {
            self.resume_backend().await
        } else {
            self.clear(slot).await
        }
    }

    pub async fn clear(&self, slot: usize) -> Result<(), NetworkError> {
        let proxy = self.proxy(slot)?;
        let mut errors = Vec::new();
        for direction in ["upstream", "downstream"] {
            collect(
                &mut errors,
                self.request(
                    Method::DELETE,
                    &format!("proxies/{}/toxics/chaos-{direction}", proxy.name),
                    None,
                )
                .await,
            );
        }
        collect(&mut errors, self.set_enabled(proxy, true).await);
        cleanup_result(errors)
    }

    pub async fn clear_all(&self) -> Result<(), NetworkError> {
        let mut errors = Vec::new();
        collect(&mut errors, self.resume_backend().await);
        for slot in 0..self.proxies.len() {
            collect(&mut errors, self.clear(slot).await);
        }
        cleanup_result(errors)
    }

    pub async fn shutdown(&self) -> Result<(), NetworkError> {
        let mut errors = Vec::new();
        collect(&mut errors, self.resume_backend().await);
        for proxy in &self.proxies {
            collect(
                &mut errors,
                self.request(Method::DELETE, &format!("proxies/{}", proxy.name), None)
                    .await,
            );
        }
        cleanup_result(errors)
    }

    fn proxy(&self, slot: usize) -> Result<&ProxyRecord, NetworkError> {
        self.proxies
            .get(slot)
            .ok_or(NetworkError::InvalidSlot(slot))
    }

    async fn set_enabled(&self, proxy: &ProxyRecord, enabled: bool) -> Result<(), NetworkError> {
        self.request(
            Method::POST,
            &format!("proxies/{}", proxy.name),
            Some(json!({"enabled": enabled})),
        )
        .await
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<(), NetworkError> {
        let deleting = method == Method::DELETE;
        let mut request = self.client.request(method, format!("{}/{path}", self.api));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        if deleting && response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        response.error_for_status()?;
        Ok(())
    }

    async fn resume_backend(&self) -> Result<(), NetworkError> {
        if self.backend_paused.load(Ordering::SeqCst) {
            self.compose("unpause").await?;
            self.backend_paused.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    async fn compose(&self, action: &'static str) -> Result<(), NetworkError> {
        let mut command = tokio::process::Command::new("docker");
        command
            .arg("compose")
            .arg("-f")
            .arg(&self.compose_file)
            .arg("-p")
            .arg(&self.compose_project)
            .arg(action)
            .arg("backend")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let status = timeout(COMPOSE_TIMEOUT, command.status())
            .await
            .map_err(|_| NetworkError::Timeout)??;
        if !status.success() {
            return Err(NetworkError::Compose { action, status });
        }
        Ok(())
    }
}

fn environment(name: &str) -> Result<String, NetworkError> {
    std::env::var(name).map_err(|_| NetworkError::MissingEnvironment(name.into()))
}

fn same_listener(actual: &str, expected: &str) -> bool {
    let (Ok(actual), Ok(expected)) = (
        actual.parse::<std::net::SocketAddr>(),
        expected.parse::<std::net::SocketAddr>(),
    ) else {
        return false;
    };
    // Toxiproxy reports a requested IPv4 wildcard as an IPv6 wildcard.
    // The route probe still checks that the published IPv4 endpoint works.
    actual.port() == expected.port()
        && (actual.ip() == expected.ip()
            || (actual.ip().is_unspecified() && expected.ip().is_unspecified()))
}

async fn check_service_health(endpoint: &str) -> Result<tonic::transport::Channel, NetworkError> {
    let transport_error = |source| NetworkError::HealthTransport {
        endpoint: endpoint.into(),
        source,
    };
    let channel = tonic::transport::Endpoint::from_shared(endpoint.to_owned())
        .map_err(transport_error)?
        .connect_timeout(HEALTH_CONNECT_TIMEOUT)
        .timeout(HEALTH_RPC_TIMEOUT)
        .connect()
        .await
        .map_err(transport_error)?;
    let response = HealthClient::new(channel.clone())
        .max_decoding_message_size(HEALTH_RESPONSE_BYTES)
        .check(HealthCheckRequest {
            service: String::new(),
        })
        .await
        .map_err(|source| NetworkError::HealthRpc {
            endpoint: endpoint.into(),
            source,
        })?
        .into_inner();
    if response.status != ServingStatus::Serving as i32 {
        return Err(NetworkError::NotServing {
            endpoint: endpoint.into(),
            status: response.status,
        });
    }
    Ok(channel)
}

fn collect(errors: &mut Vec<NetworkError>, result: Result<(), NetworkError>) {
    if let Err(error) = result {
        errors.push(error);
    }
}

fn cleanup_result(errors: Vec<NetworkError>) -> Result<(), NetworkError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(NetworkError::Cleanup(errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicUsize};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct MockServer {
        endpoint: String,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    struct QueryProbe {
        unavailable: bool,
        reads: Arc<AtomicUsize>,
    }

    #[tonic::async_trait]
    impl api::query_service_server::QueryService for QueryProbe {
        async fn query(
            &self,
            _: tonic::Request<api::QueryRequest>,
        ) -> Result<tonic::Response<api::QueryResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("not part of the probe"))
        }

        async fn query_newest(
            &self,
            request: tonic::Request<api::QueryNewestRequest>,
        ) -> Result<tonic::Response<api::QueryNewestResponse>, tonic::Status> {
            let request = request.into_inner();
            assert_eq!(request.topics.len(), 1);
            assert!(!request.include_full_envelope);
            assert!(Topic::parse(&request.topics[0].topic).is_ok());
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.unavailable {
                Err(tonic::Status::unavailable("database unavailable"))
            } else {
                Ok(tonic::Response::new(api::QueryNewestResponse::default()))
            }
        }
    }

    async fn mock_backend(serving: bool, unavailable: bool) -> (MockServer, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (reporter, health) = tonic_health::server::health_reporter();
        reporter
            .set_service_status(
                "",
                if serving {
                    tonic_health::ServingStatus::Serving
                } else {
                    tonic_health::ServingStatus::NotServing
                },
            )
            .await;
        let reads = Arc::new(AtomicUsize::new(0));
        let probe = QueryProbe {
            unavailable,
            reads: reads.clone(),
        };
        let incoming = futures::stream::unfold(listener, |listener| async {
            let (stream, _) = listener.accept().await.unwrap();
            Some((Ok::<_, std::io::Error>(stream), listener))
        });
        let task = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(health)
                .add_service(api::query_service_server::QueryServiceServer::new(probe))
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });
        (MockServer { endpoint, task }, reads)
    }

    async fn mock_control(body: Vec<u8>) -> MockServer {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            assert!(request.starts_with(b"GET /proxies "));
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            // A size rejection can close the socket before the body arrives.
            let _ = stream.write_all(&body).await;
        });
        MockServer { endpoint, task }
    }

    fn valid_proxy() -> Value {
        json!({"xchaos-0": {
            "name":"xchaos-0", "listen":"0.0.0.0:6003",
            "upstream":UPSTREAM, "enabled":true, "toxics":[],
        }})
    }

    fn test_controller(backend: &str, control: &str, proxy: &str) -> NetworkController {
        NetworkController {
            client: xmtp_common::http::client_builder()
                .timeout(CONTROL_TIMEOUT)
                .build()
                .unwrap(),
            api: control.into(),
            backend: backend.into(),
            proxies: vec![ProxyRecord {
                slot: 0,
                name: "xchaos-0".into(),
                endpoint: proxy.into(),
                listen: "0.0.0.0:6003".into(),
                upstream: UPSTREAM.into(),
            }],
            backend_paused: AtomicBool::new(false),
            compose_project: "unused".into(),
            compose_file: PathBuf::new(),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_checks_direct_database_and_proxy_route() {
        let (direct, reads) = mock_backend(true, false).await;
        let (proxy, proxy_reads) = mock_backend(true, false).await;
        let control = mock_control(serde_json::to_vec(&valid_proxy())?).await;
        test_controller(&direct.endpoint, &control.endpoint, &proxy.endpoint)
            .check_health()
            .await?;
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(proxy_reads.load(Ordering::SeqCst), 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_accepts_toxiproxy_normalized_wildcard_listener() {
        let (direct, _) = mock_backend(true, false).await;
        // This is the control response shape observed from Toxiproxy 2.12.0.
        let normalized = json!({"xchaos-0": {
            "name":"xchaos-0", "listen":"[::]:6003",
            "upstream":"backend:5050", "enabled":true, "Logger":{}, "toxics":[],
        }});
        let control = mock_control(serde_json::to_vec(&normalized)?).await;
        test_controller(&direct.endpoint, &control.endpoint, &direct.endpoint)
            .check_health()
            .await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_rejects_unavailable_backend_despite_serving_health() {
        let (direct, reads) = mock_backend(true, true).await;
        let controller = test_controller(&direct.endpoint, "http://unused", "http://unused");
        assert!(
            matches!(controller.check_health().await, Err(NetworkError::HealthRpc { source, .. })
            if source.code() == tonic::Code::Unavailable)
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_rejects_not_serving_backend() {
        let (direct, reads) = mock_backend(false, false).await;
        let controller = test_controller(&direct.endpoint, "http://unused", "http://unused");
        assert!(matches!(
            controller.check_health().await,
            Err(NetworkError::NotServing { .. })
        ));
        assert_eq!(reads.load(Ordering::SeqCst), 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_rejects_missing_disabled_changed_or_faulted_proxy() {
        let (direct, _) = mock_backend(true, false).await;
        let mut cases = vec![json!({})];
        for (field, value) in [
            ("enabled", json!(false)),
            ("upstream", json!("wrong:5050")),
            ("listen", json!("[::]:6005")),
            ("listen", json!("127.0.0.1:6003")),
            ("toxics", json!([{"type":"timeout"}])),
        ] {
            let mut proxy = valid_proxy();
            proxy["xchaos-0"][field] = value;
            cases.push(proxy);
        }
        for body in cases {
            let control = mock_control(serde_json::to_vec(&body)?).await;
            let controller = test_controller(&direct.endpoint, &control.endpoint, "http://unused");
            assert!(matches!(
                controller.check_health().await,
                Err(NetworkError::ProxyHealth { .. })
            ));
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_rejects_broken_proxy_route() {
        let (direct, _) = mock_backend(true, false).await;
        let (proxy, _) = mock_backend(false, false).await;
        let control = mock_control(serde_json::to_vec(&valid_proxy())?).await;
        let controller = test_controller(&direct.endpoint, &control.endpoint, &proxy.endpoint);
        assert!(
            matches!(controller.check_health().await, Err(NetworkError::NotServing { endpoint, .. })
            if endpoint == proxy.endpoint)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn health_caps_control_response() {
        let (direct, _) = mock_backend(true, false).await;
        let control = mock_control(vec![b' '; CONTROL_RESPONSE_BYTES + 1]).await;
        let controller = test_controller(&direct.endpoint, &control.endpoint, "http://unused");
        assert!(matches!(
            controller.check_health().await,
            Err(NetworkError::OversizeControlResponse)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn cleanup_continues_after_a_control_failure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let api = format!("http://{}", listener.local_addr()?);
        let controller = NetworkController {
            client: xmtp_common::http::client_builder()
                .timeout(CONTROL_TIMEOUT)
                .build()?,
            api,
            backend: "http://unused".into(),
            proxies: vec![ProxyRecord {
                slot: 0,
                name: "xchaos-0".into(),
                endpoint: "http://127.0.0.1:6003".into(),
                listen: "0.0.0.0:6003".into(),
                upstream: UPSTREAM.into(),
            }],
            backend_paused: AtomicBool::new(false),
            compose_project: "unused".into(),
            compose_file: PathBuf::new(),
        };
        let server = async {
            let mut requests = Vec::new();
            for status in ["500 Internal Server Error", "404 Not Found", "200 OK"] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                // Read through the header so a fragmented request is valid.
                while !request.ends_with(b"\r\n\r\n") {
                    request.push(stream.read_u8().await.unwrap());
                }
                let request = String::from_utf8(request).unwrap();
                let content_length = request
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or_default();
                let mut body = vec![0; content_length];
                stream.read_exact(&mut body).await.unwrap();
                requests.push(request);
                let response =
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
            }
            requests
        };
        let (result, requests) = timeout(CONTROL_TIMEOUT, async {
            tokio::join!(controller.clear(0), server)
        })
        .await?;
        assert!(matches!(result, Err(NetworkError::Cleanup(errors)) if errors.len() == 1));
        assert!(requests[0].starts_with("DELETE /proxies/xchaos-0/toxics/chaos-upstream "));
        assert!(requests[1].starts_with("DELETE /proxies/xchaos-0/toxics/chaos-downstream "));
        assert!(requests[2].starts_with("POST /proxies/xchaos-0 "));
    }
}
