//! Network faults for the worktree's isolated Toxiproxy service.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use xmtp_common::time::{Duration, timeout};

/// One proxy per installation, plus the process that shares a database.
pub const PROXY_CAPACITY: usize = 19;
// These container ports match dev/worktree-env and dev/docker/compose.yml.
const PROXY_PORTS: [u16; PROXY_CAPACITY] = [
    6003, 6005, 6007, 6012, 6014, 6019, 6021, 6023, 6025, 6030, 6035, 6037, 6039, 6041, 6043, 6047,
    6052, 6054, 6056,
];
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
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

/// The caller must clear faults before a checkpoint and call shutdown on exit.
/// Only one fault window can own a slot, or the backend, at a time.
pub struct NetworkController {
    client: Client,
    api: String,
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[xmtp_common::test(unwrap_try = true)]
    async fn cleanup_continues_after_a_control_failure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let api = format!("http://{}", listener.local_addr()?);
        let controller = NetworkController {
            client: xmtp_common::http::client_builder()
                .timeout(CONTROL_TIMEOUT)
                .build()?,
            api,
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
