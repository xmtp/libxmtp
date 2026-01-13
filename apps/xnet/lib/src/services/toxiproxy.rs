//! ToxiProxy container management for network fault injection testing.
//!
//! ToxiProxy is a proxy for simulating network conditions. It allows adding
//! latency, timeouts, bandwidth limits, and other network faults to connections.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU16, Ordering},
};

use async_trait::async_trait;
use bollard::{
    Docker,
    models::{Config as ContainerConfig, ContainerCreateBody},
    query_parameters::CreateContainerOptionsBuilder,
    secret::HostConfig,
};
use bon::Builder;
use color_eyre::eyre::{Result, eyre};
use map_macro::hash_map;
use tokio::time::{Duration, sleep};
use toxiproxy_rust::client::Client;
use toxiproxy_rust::proxy::{Proxy, ProxyPack};
use tracing::info;
use url::Url;

use crate::{
    Config,
    config::NodeToml,
    constants::{
        Anvil as AnvilConst, Gateway as GatewayConst, HistoryServer as HistoryServerConst,
        MlsDb as MlsDbConst, NodeGo as NodeGoConst, Redis as RedisConst,
        ReplicationDb as ReplicationDbConst, ToxiProxy as ToxiProxyConst, V3Db as V3DbConst,
        Validation as ValidationConst, Xmtpd as XmtpdConst,
    },
    network::XNET_NETWORK_NAME,
    services::{
        ContainerState, Service, create_and_start_container, ensure_container_running, expose,
        stop_container,
    },
};

/// Global port allocator for ToxiProxy proxy ports.
/// Allocates ports from the range 8100-8150.
static NEXT_STATIC_PORT: AtomicU16 = AtomicU16::new(ToxiProxyConst::STATIC_PORT_RANGE.0);

static NEXT_XMTPD_PORT: AtomicU16 = AtomicU16::new(ToxiProxyConst::XMTPD_PORT_RANGE.0);

/// Allocate the next available port from the ToxiProxy port range.
pub fn allocate_static_port() -> Result<u16> {
    let port = NEXT_STATIC_PORT.fetch_add(1, Ordering::SeqCst);
    if port >= ToxiProxyConst::STATIC_PORT_RANGE.1 {
        color_eyre::eyre::bail!(
            "ToxiProxy port range exhausted ({}..{})",
            ToxiProxyConst::STATIC_PORT_RANGE.0,
            ToxiProxyConst::STATIC_PORT_RANGE.1
        );
    }
    Ok(port)
}

/// Allocate the next available port from the ToxiProxy port range.
pub fn allocate_xmtpd_port() -> Result<u16> {
    let port = NEXT_XMTPD_PORT.fetch_add(1, Ordering::SeqCst);
    if port >= ToxiProxyConst::XMTPD_PORT_RANGE.1 {
        color_eyre::eyre::bail!(
            "ToxiProxy port range exhausted ({}..{})",
            ToxiProxyConst::XMTPD_PORT_RANGE.0,
            ToxiProxyConst::XMTPD_PORT_RANGE.1
        );
    }
    Ok(port)
}

/// Initialize port allocators based on existing ToxiProxy proxies.
/// Queries the proxy list and sets each allocator to max_used_port + 1.
async fn init_port_allocators(client: &Client) -> Result<()> {
    let proxies = client
        .all()
        .await
        .map_err(|e| eyre!("Failed to list proxies: {}", e))?;

    let mut max_static: Option<u16> = None;
    let mut max_xmtpd: Option<u16> = None;

    for proxy in proxies.values() {
        let listen = &proxy.proxy_pack.listen;
        // Format: "[::]:PORT" — extract port after last ':'
        let port: u16 = listen
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| eyre!("Could not parse port from listen address: {}", listen))?;

        if (ToxiProxyConst::STATIC_PORT_RANGE.0..ToxiProxyConst::STATIC_PORT_RANGE.1)
            .contains(&port)
        {
            max_static = Some(max_static.map_or(port, |m: u16| m.max(port)));
        } else if (ToxiProxyConst::XMTPD_PORT_RANGE.0..ToxiProxyConst::XMTPD_PORT_RANGE.1)
            .contains(&port)
        {
            max_xmtpd = Some(max_xmtpd.map_or(port, |m: u16| m.max(port)));
        }
    }

    let static_next = max_static.map_or(ToxiProxyConst::STATIC_PORT_RANGE.0, |m| m + 1);
    let xmtpd_next = max_xmtpd.map_or(ToxiProxyConst::XMTPD_PORT_RANGE.0, |m| m + 1);

    NEXT_STATIC_PORT.store(static_next, Ordering::SeqCst);
    NEXT_XMTPD_PORT.store(xmtpd_next, Ordering::SeqCst);

    info!(
        "Port allocators initialized: static={}, xmtpd={}",
        static_next, xmtpd_next
    );
    Ok(())
}

/// Configuration for a proxy to be created.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Name of the proxy (e.g., "node-go", "anvil")
    pub name: String,
    /// Listen port inside the ToxiProxy container
    pub listen_port: u16,
    /// Upstream service address (e.g., "xnet-anvil:8545")
    pub upstream: String,
}

impl ProxyConfig {
    pub fn new(name: impl Into<String>, listen_port: u16, upstream: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            listen_port,
            upstream: upstream.into(),
        }
    }

    /// Convert to a ProxyPack for use with the toxiproxy_rust client.
    pub async fn into_proxy_pack(self) -> ProxyPack {
        let listen = format!("[::]:{}", self.listen_port);
        ProxyPack::new(self.name, listen, self.upstream).await
    }
}

fn default_toxiproxy_port() -> u16 {
    let conf = Config::load_unchecked();
    if conf.use_standard_ports {
        8474
    } else {
        ToxiProxyConst::API_PORT
    }
}

/// Manages a ToxiProxy Docker container for network fault injection testing.
#[derive(Builder, Clone)]
#[builder(on(String, into), derive(Debug))]
#[builder(finish_fn(vis = "", name = build_internal))]
pub struct ToxiProxy {
    /// The ToxiProxy image
    #[builder(default = ToxiProxyConst::IMAGE.to_string())]
    image: String,
    #[builder(default = ToxiProxyConst::VERSION.to_string())]
    version: String,

    /// Host port for the ToxiProxy API
    #[builder(default = default_toxiproxy_port())]
    api_port: u16,

    /// Docker client (initialized on start)
    #[builder(skip)]
    docker: Option<Docker>,

    /// Container ID once started
    #[builder(skip)]
    container_id: Option<String>,

    /// ToxiProxy client for managing proxies (initialized after container starts)
    #[builder(skip)]
    client: Option<Client>,
}

impl<S: toxi_proxy_builder::IsComplete> ToxiProxyBuilder<S> {
    pub fn build(self) -> Result<ToxiProxy> {
        // Delegate to `build_internal()` to get the instance of user.
        let mut this = self.build_internal();
        let config = Config::load()?;
        if let Some(version) = config.toxiproxy.version {
            this.version = version;
        }
        if let Some(image) = config.toxiproxy.image {
            this.image = image;
        }
        if let Some(port) = config.toxiproxy_port {
            this.api_port = port;
        }
        Ok(this)
    }
}

impl std::fmt::Debug for ToxiProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToxiProxy")
            .field("image", &self.image)
            .field("api_port", &self.api_port)
            .field("container_id", &self.container_id)
            .field("docker", &self.docker.as_ref().map(|_| "Docker"))
            .field("client", &self.client.as_ref().map(|_| "Client"))
            .finish()
    }
}

impl ToxiProxy {
    /// Start the ToxiProxy container.
    ///
    /// If a container with the same name already exists, it will be reused.
    /// The container exposes all ports in the range 8100-8150 for proxy use.
    pub async fn start(&mut self) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let container_id = match ensure_container_running(&docker, ToxiProxyConst::CONTAINER_NAME)
            .await?
        {
            ContainerState::Exists(id) => id,
            ContainerState::NotFound => {
                let options =
                    CreateContainerOptionsBuilder::default().name(ToxiProxyConst::CONTAINER_NAME);

                let Self {
                    image,
                    version,
                    api_port,
                    ..
                } = &self;

                let mut port_bindings = HashMap::new();
                port_bindings.extend(hash_map! {
                    "8474/tcp".to_string() => expose(*api_port),
                });
                let config = Config::load()?;
                // expose "standard" ports if enabled

                if config.use_standard_ports {
                    let map = hash_map! {
                        format!("{}/tcp", AnvilConst::PORT)           => expose(AnvilConst::PORT),
                        format!("{}/tcp", RedisConst::PORT)           => expose(RedisConst::PORT),
                        format!("{}/tcp", ReplicationDbConst::PORT)   => expose(ReplicationDbConst::PORT),
                        format!("{}/tcp", V3DbConst::PORT)            => expose(V3DbConst::PORT),
                        format!("{}/tcp", MlsDbConst::PORT)           => expose(MlsDbConst::PORT),
                        format!("{}/tcp", HistoryServerConst::PORT)   => expose(HistoryServerConst::PORT),
                        format!("{}/tcp", ValidationConst::PORT)      => expose(ValidationConst::PORT),
                        format!("{}/tcp", NodeGoConst::API_PORT)      => expose(NodeGoConst::API_PORT),
                        format!("{}/tcp", GatewayConst::PORT)         => expose(GatewayConst::PORT),
                        format!("{}/tcp", ToxiProxyConst::API_PORT)   => expose(ToxiProxyConst::API_PORT),
                        format!("{}/tcp", XmtpdConst::GRPC_PORT)      => expose(XmtpdConst::GRPC_PORT),
                    };
                    port_bindings.extend(map);
                } else if let Some(p) = config.v3_port {
                    port_bindings.extend(hash_map! {
                        format!("{}/tcp", NodeGoConst::API_PORT) => expose(p)
                    });
                }

                // expose any possible static xmtpd nodes
                for NodeToml { port, .. } in config.xmtpd_nodes {
                    if let Some(p) = port {
                        port_bindings.insert(format!("{p}/tcp"), expose(p));
                    }
                }
                // Expose all ports in the range for dynamic proxy allocation
                // for xmtpd nodes / static services
                for port in ToxiProxyConst::XMTPD_PORT_RANGE.0..ToxiProxyConst::XMTPD_PORT_RANGE.1 {
                    port_bindings.insert(format!("{port}/tcp"), expose(port));
                }
                for port in ToxiProxyConst::STATIC_PORT_RANGE.0..ToxiProxyConst::STATIC_PORT_RANGE.1
                {
                    port_bindings.insert(format!("{port}/tcp"), expose(port));
                }

                let config = ContainerCreateBody {
                    image: Some(format!("{image}:{version}")),
                    cmd: Some(vec!["-host=0.0.0.0".to_string()]),
                    host_config: Some(HostConfig {
                        port_bindings: Some(port_bindings),
                        network_mode: Some(XNET_NETWORK_NAME.to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                create_and_start_container(&docker, ToxiProxyConst::CONTAINER_NAME, options, config)
                    .await?
            }
        };

        self.docker = Some(docker);
        self.container_id = Some(container_id);

        let client = Client::new(format!("127.0.0.1:{}", self.api_port));
        // Wait for ToxiProxy API to be ready and create client
        self.wait_for_ready(&client).await?;
        // Sync port allocators with any proxies already registered from a previous run
        init_port_allocators(&client).await?;
        self.client = Some(client);

        Ok(())
    }

    /// Wait for ToxiProxy API to be ready.
    async fn wait_for_ready(&self, client: &Client) -> Result<()> {
        for _ in 0..30 {
            if client.is_running().await {
                info!("ToxiProxy API is ready");
                sleep(Duration::from_millis(250)).await;
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }

        color_eyre::eyre::bail!("ToxiProxy failed to become ready within 15 seconds")
    }

    /// Stop the ToxiProxy container.
    pub async fn stop(&mut self) -> Result<()> {
        if let (Some(docker), Some(id)) = (&self.docker, &self.container_id) {
            stop_container(docker, id, ToxiProxyConst::CONTAINER_NAME).await?;
        }
        Ok(())
    }

    /// Register a static service with ToxiProxy.
    ///
    /// Allocates a port from the proxy port range and creates a proxy
    /// that forwards traffic to the upstream service.
    ///
    /// Returns the allocated port that external clients should connect to.
    pub async fn register(
        &self,
        name: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Result<u16> {
        let name = name.into();
        let upstream = upstream.into();
        let port = allocate_static_port()?;
        self.register_at(name, upstream, port).await?;
        Ok(port)
    }

    /// Register a service with ToxiProxy at a port.
    ///
    /// Allocates a port from the proxy port range and creates a proxy
    /// that forwards traffic to the upstream service.
    ///
    /// Returns the allocated port that external clients should connect to.
    pub async fn register_at(
        &self,
        name: impl Into<String>,
        upstream: impl Into<String>,
        port: impl Into<u16>,
    ) -> Result<()> {
        let name = name.into();
        let upstream = upstream.into();
        let port: u16 = port.into();

        // Check if proxy already exists (e.g. container reused from previous run)
        if let Ok(existing) = self.find_proxy(&name).await {
            let listen = &existing.proxy_pack.listen;
            info!(
                "proxy '{}' already registered (listen={}), skipping",
                name, listen
            );
            return Ok(());
        }

        let config = ProxyConfig::new(&name, port, &upstream);
        self.add_proxy(config).await?;

        info!(
            "registered service '{}' -> {} on local port {}",
            name, upstream, port
        );
        Ok(())
    }

    /// Add a proxy to ToxiProxy.
    pub async fn add_proxy(&self, config: ProxyConfig) -> Result<Proxy> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("ToxiProxy not started"))?;

        debug!(
            "adding proxy '{}': [::]:{}  -> {}",
            config.name, config.listen_port, config.upstream
        );

        let proxy_pack = config.into_proxy_pack().await;
        let mut proxies = client
            .populate(vec![proxy_pack])
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to add proxy: {}", e))?;

        proxies
            .pop()
            .ok_or_else(|| color_eyre::eyre::eyre!("No proxy returned from populate"))
    }

    /// Get a proxy by name.
    pub async fn find_proxy(&self, name: &str) -> Result<Proxy> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("ToxiProxy not started"))?;

        client
            .find_proxy(name)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to find proxy '{}': {}", name, e))
    }

    pub async fn list_proxies(&self) -> Result<HashMap<String, Proxy>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("ToxiProxy not started"))?;

        client.all().await.map_err(|e| eyre!(e))
    }

    /// Reset all proxies (remove all toxics).
    pub async fn reset(&self) -> Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("ToxiProxy not started"))?;

        client
            .reset()
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to reset proxies: {}", e))?;
        info!("ToxiProxy reset complete");

        Ok(())
    }

    /// Log current port allocator state.
    pub fn print_port_allocations() {
        let static_next = NEXT_STATIC_PORT.load(Ordering::SeqCst);
        let xmtpd_next = NEXT_XMTPD_PORT.load(Ordering::SeqCst);
        let static_used = static_next - ToxiProxyConst::STATIC_PORT_RANGE.0;
        let xmtpd_used = xmtpd_next - ToxiProxyConst::XMTPD_PORT_RANGE.0;

        info!(
            "ToxiProxy port allocations: static {}/{} (range {}..{}, next {}), xmtpd {}/{} (range {}..{}, next {})",
            static_used,
            ToxiProxyConst::STATIC_PORT_RANGE.1 - ToxiProxyConst::STATIC_PORT_RANGE.0,
            ToxiProxyConst::STATIC_PORT_RANGE.0,
            ToxiProxyConst::STATIC_PORT_RANGE.1,
            static_next,
            xmtpd_used,
            ToxiProxyConst::XMTPD_PORT_RANGE.1 - ToxiProxyConst::XMTPD_PORT_RANGE.0,
            ToxiProxyConst::XMTPD_PORT_RANGE.0,
            ToxiProxyConst::XMTPD_PORT_RANGE.1,
            xmtpd_next,
        );
    }

    /// Get the toxiproxy_rust Client for advanced operations.
    pub fn client(&self) -> Option<&Client> {
        self.client.as_ref()
    }

    /// ToxiProxy API URL for use within the docker network.
    pub fn api_url(&self) -> Url {
        Url::parse(&format!(
            "http://{}:{}",
            ToxiProxyConst::CONTAINER_NAME,
            ToxiProxyConst::API_PORT
        ))
        .expect("valid URL")
    }

    /// ToxiProxy API URL for external access (from host machine).
    pub fn external_api_url(&self) -> Url {
        Url::parse(&format!("http://localhost:{}", self.api_port)).expect("valid URL")
    }

    /// Check if ToxiProxy is running.
    pub fn is_running(&self) -> bool {
        self.container_id.is_some()
    }
}

#[async_trait]
impl Service for ToxiProxy {
    /// Start ToxiProxy. The `_toxiproxy` parameter is ignored since ToxiProxy
    /// doesn't register with itself.
    async fn start(&mut self, _toxiproxy: &ToxiProxy) -> Result<()> {
        ToxiProxy::start(self).await
    }

    async fn stop(&mut self) -> Result<()> {
        ToxiProxy::stop(self).await
    }

    fn is_running(&self) -> bool {
        ToxiProxy::is_running(self)
    }

    fn url(&self) -> Url {
        self.api_url()
    }

    fn external_url(&self) -> Url {
        self.external_api_url()
    }

    fn name(&self) -> String {
        "toxiproxy".to_string()
    }

    fn port(&self) -> u16 {
        ToxiProxyConst::API_PORT
    }
}
