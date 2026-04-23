//! Stateful service manager
//! network is hardcoded to XNET_NETWORK_NAME
use std::collections::HashMap;

use crate::{
    Config,
    network::{Network, XNET_NETWORK_NAME},
    node_provisioner::NodeProvisioner,
    services::{
        self, CoreDns, Gateway, Grafana, NodeGo, Otterscan, PgAdmin, Prometheus, ReplicationDb,
        Service, ToxiProxy, Traefik, TraefikConfig, Xmtpd, create_and_start_container,
    },
    types::XmtpdNode,
};
use bollard::{
    Docker,
    models::ContainerCreateBody,
    query_parameters::{
        CreateContainerOptionsBuilder, ListContainersOptionsBuilder, RemoveContainerOptionsBuilder,
        StopContainerOptionsBuilder,
    },
};
use color_eyre::eyre::Result;
use futures::FutureExt;

/// Starts services in the right order
/// and keeps a list of running services
pub struct ServiceManager {
    // Always required (infrastructure)
    pub coredns: CoreDns,
    pub traefik: Traefik,
    pub traefik_config: TraefikConfig,
    pub proxy: ToxiProxy,

    // V3 stack (None when enable_v3 = false)
    pub node_go: Option<NodeGo>,
    v3_services: Vec<Box<dyn Service>>,

    // D14n stack (None when enable_d14n = false)
    pub gateway: Option<Gateway>,
    d14n_services: Vec<Box<dyn Service>>,

    // Shared — starts when either V3 or D14n is enabled
    anvil_rpc_url: Option<url::Url>,
    shared_services: Vec<Box<dyn Service>>,

    // Monitoring (None when enable_monitoring = false)
    prometheus: Option<Prometheus>,
    grafana: Option<Grafana>,
    pgadmin: Option<PgAdmin>,
    otterscan: Option<Otterscan>,

    nodes: Vec<Xmtpd>,
}

impl ServiceManager {
    pub fn anvil_rpc_url(&self) -> Option<&url::Url> {
        self.anvil_rpc_url.as_ref()
    }

    /// starts services if not already started
    /// if running connects to them.
    pub async fn start() -> Result<Self> {
        Self::start_paused(false).await
    }

    /// Like [`start`](Self::start) but also pauses broadcaster contracts when
    /// `cli_paused` is true (before node provisioning).
    pub async fn start_paused(cli_paused: bool) -> Result<Self> {
        let config = Config::load()?;

        // Phase 1: Infrastructure (always)
        let mut proxy = ToxiProxy::builder().build()?;
        proxy.start().await?;

        let mut traefik = Traefik::builder()
            .maybe_http_port(config.traefik_port)
            .build();
        traefik.start(&proxy).await?;

        let traefik_ip = traefik.container_ip().await?;

        let mut coredns = CoreDns::builder().traefik_ip(traefik_ip).build();
        coredns.start(&proxy).await?;

        let traefik_config = TraefikConfig::new(traefik.dynamic_config_path())?;
        if !config.extra_traefik_routes.is_empty() {
            traefik_config.set_extra_routes(config.extra_traefik_routes.clone())?;
        }

        // Phase 2: Monitoring — Prometheus + Grafana (conditional)
        // PgAdmin is started AFTER xmtpd nodes so it can discover their
        // ReplicationDb containers via Docker labels (xnet.pgadmin=true).
        let (prometheus, grafana, pgadmin) = if config.enable_monitoring {
            let mut p = Prometheus::builder().build();
            let mut g = Grafana::builder().build();
            let pa = PgAdmin::builder().build();
            let launch = vec![p.start(&proxy).boxed(), g.start(&proxy).boxed()];
            futures::future::try_join_all(launch).await?;
            (Some(p), Some(g), Some(pa))
        } else {
            (None, None, None)
        };

        // Phase 3: Shared services — Anvil, Validation, History
        // These are needed by both V3 and D14n stacks
        let need_shared = config.enable_v3 || config.enable_d14n;
        let (anvil_rpc_url, anvil_proxy_host, shared_services) = if need_shared {
            let shared = start_shared_services(&proxy).await?;
            (
                Some(shared.anvil_rpc_url),
                Some(shared.anvil_proxy_host),
                shared.services,
            )
        } else {
            (None, None, vec![])
        };

        // Phase 4: V3 (conditional)
        let (node_go, v3_services) = if config.enable_v3 {
            info!("starting v3");
            let (ng, svcs) = start_v3(&proxy).await?;
            (Some(ng), svcs)
        } else {
            (None, vec![])
        };

        // Phase 5: D14n — Redis + Gateway (conditional)
        let dns_ip = coredns.container_ip().await?;
        let (gateway, d14n_services) = if config.enable_d14n {
            let host = anvil_proxy_host
                .as_ref()
                .expect("anvil must be running when d14n is enabled");
            let anvil_rpc = anvil_rpc_url.as_ref().unwrap();
            let (gw, svcs) = start_d14n(&proxy, dns_ip.clone(), host.clone(), anvil_rpc).await?;
            (Some(gw), svcs)
        } else {
            (None, vec![])
        };

        // Phase 6: Otterscan (needs monitoring AND shared services)
        let otterscan = if config.enable_monitoring && need_shared {
            let anvil_rpc = anvil_rpc_url.as_ref().unwrap();
            let anvil_host_for_browser = match &config.address_mode {
                crate::config::AddressMode::RemoteDomain(domain) => {
                    anvil_rpc.to_string().replace("localhost", domain)
                }
                crate::config::AddressMode::Local => anvil_rpc.to_string(),
            };
            let mut ot = Otterscan::builder()
                .anvil_host(anvil_host_for_browser)
                .build();
            ot.start(&proxy).await?;
            Some(ot)
        } else {
            None
        };

        // Phase 6b: Pause broadcasters if configured (must happen before Phase 7 node provisioning)
        if config.paused || cli_paused {
            if let Some(ref rpc) = anvil_rpc_url {
                crate::contracts::set_broadcasters_paused(
                    rpc.as_str(),
                    crate::constants::Anvil::ADMIN_KEY,
                    true,
                )
                .await?;
                info!("broadcaster contracts paused");
            }
        }

        let mut this = Self {
            node_go,
            coredns,
            traefik,
            traefik_config,
            proxy,
            gateway,
            v3_services,
            d14n_services,
            shared_services,
            anvil_rpc_url,
            otterscan,
            prometheus,
            grafana,
            pgadmin,
            nodes: Vec::new(),
        };

        // Phase 7: XMTPD nodes from config (requires D14n)
        if config.enable_d14n {
            let existing_proxies = this.proxy.list_proxies().await?;
            for node_toml in &config.xmtpd_nodes {
                if !node_toml.enable {
                    continue;
                }
                // Skip nodes whose proxy already exists (already provisioned in a previous run).
                // Only applies to named nodes — unnamed nodes get their name from the gateway ID.
                if let Some(ref name) = node_toml.name
                    && existing_proxies.contains_key(name)
                {
                    info!("node {} already has proxy registered, skipping", name);
                    continue;
                }

                NodeProvisioner::builder()
                    .migrator(node_toml.migrator)
                    .use_standard_port(node_toml.use_standard_port)
                    .maybe_name(node_toml.name.clone())
                    .maybe_port(node_toml.port)
                    .build()
                    .provision(&mut this)
                    .await?;
            }
        }

        // Start PgAdmin AFTER xmtpd nodes so it can discover their
        // ReplicationDb containers via Docker labels (xnet.pgadmin=true).
        // Dependency chain: ReplicationDb (labels) → PgAdmin (scans labels)
        if let Some(ref mut pa) = this.pgadmin {
            pa.start(&this.proxy).await?;
        }

        Ok(this)
    }

    pub fn print_port_allocations() {
        ToxiProxy::print_port_allocations();
    }

    pub async fn stop(&mut self) -> Result<()> {
        // Stop node-go before its database dependencies
        if let Some(ref mut ng) = self.node_go {
            ng.stop().await?;
        }
        // Stop V3 services (databases)
        for service in &mut self.v3_services {
            service.stop().await?;
        }

        // Stop XMTPD nodes (before their dependencies)
        for node in &mut self.nodes {
            node.stop().await?;
        }

        // Stop D14n services
        for service in &mut self.d14n_services {
            service.stop().await?;
        }
        if let Some(ref mut gw) = self.gateway {
            gw.stop().await?;
        }

        // Stop shared services (Anvil, Validation, History)
        for service in &mut self.shared_services {
            service.stop().await?;
        }

        // Stop monitoring
        if let Some(ref mut ot) = self.otterscan {
            ot.stop().await?;
        }
        if let Some(ref mut pa) = self.pgadmin {
            pa.stop().await?;
        }
        if let Some(ref mut g) = self.grafana {
            g.stop().await?;
        }
        if let Some(ref mut p) = self.prometheus {
            p.stop().await?;
        }

        // Infrastructure (always)
        self.coredns.stop().await?;
        self.traefik.stop().await?;
        self.proxy.stop().await?;
        Ok(())
    }

    pub async fn add_xmtpd(&mut self, node: XmtpdNode) -> Result<()> {
        let dns_ip = self.coredns.container_ip().await?;
        let xmtpd = Xmtpd::builder().node(node).dns_server(dns_ip).build()?;
        self.internal_add_xmtpd(xmtpd).await
    }

    pub async fn add_xmtpd_with_migrator(&mut self, node: XmtpdNode) -> Result<()> {
        let node_go = self.node_go.as_ref().ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Cannot add migrator node: V3 stack is disabled (enable_v3 required)"
            )
        })?;
        let dns_ip = self.coredns.container_ip().await?;
        let xmtpd = Xmtpd::builder()
            .node(node)
            .migrator(true)
            .node_go(node_go.clone())
            .dns_server(dns_ip)
            .build()?;
        self.internal_add_xmtpd(xmtpd).await
    }

    pub async fn reload_node_go(&mut self, d14n_cutover_ns: i64) -> Result<()> {
        let node_go = self.node_go.as_mut().ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Cannot reload node-go: V3 stack is disabled (enable_v3 required)"
            )
        })?;
        node_go.reload(d14n_cutover_ns, &self.proxy).await
    }

    async fn internal_add_xmtpd(&mut self, mut xmtpd: Xmtpd) -> Result<()> {
        let rpc = self.anvil_rpc_url.as_ref().ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Cannot add xmtpd node: Anvil is not running (enable_v3 or enable_d14n required)"
            )
        })?;
        let node = xmtpd.node();
        for address in [
            node.address(),
            node.payer_address(),
            node.migration_payer_address(),
        ] {
            crate::wallet_funding::fund_wallet(rpc.as_str(), address, None).await?;
        }

        xmtpd.start(&self.proxy).await?;

        if let Some(hostname) = <Xmtpd as Service>::hostname(&xmtpd)
            && let Some(toxi_port) = xmtpd.proxy_port()
        {
            self.traefik_config.add_route(hostname, toxi_port)?;
        }

        self.nodes.push(xmtpd);

        if let Some(ref mut prometheus) = self.prometheus {
            prometheus.update_targets(&self.nodes)?;
        }

        // Rescan Docker labels to pick up the new node's ReplicationDb.
        // PgAdmin discovers databases via xnet.pgadmin=true container labels,
        // so it doesn't need to know about Xmtpd directly.
        if let Some(ref mut pgadmin) = self.pgadmin {
            pgadmin.discover_databases().await?;
        }

        Ok(())
    }

    /// Remove all migrator nodes and restart them without migrator flags.
    ///
    /// Uses Docker as the source of truth since the CLI is stateless.
    /// Inspects running containers for `XMTPD_MIGRATION_SERVER_ENABLE=true`,
    /// then removes and recreates them without any `XMTPD_MIGRATION_*` env vars.
    pub async fn remove_migrators(&mut self) -> Result<()> {
        let docker = Docker::connect_with_socket_defaults()?;

        let mut filters = HashMap::new();
        filters.insert("network".to_string(), vec![XNET_NETWORK_NAME.to_string()]);
        let options = ListContainersOptionsBuilder::default()
            .filters(&filters)
            .build();
        let containers = docker.list_containers(Some(options)).await?;

        let mut count = 0;
        for container in &containers {
            let names = container.names.as_deref().unwrap_or(&[]);
            // Docker prepends "/" to container names; xmtpd nodes are named xnet-{id}
            let is_xmtpd = names.iter().any(|n| {
                let trimmed = n.trim_start_matches('/');
                trimmed.starts_with("xnet-") && trimmed[5..].chars().all(|c| c.is_ascii_digit())
            });
            if !is_xmtpd {
                continue;
            }

            let id = container.id.as_deref().unwrap_or_default();
            let info = docker.inspect_container(id, None).await?;
            let env = info.config.as_ref().and_then(|c| c.env.as_ref());

            let is_migrator = env
                .map(|vars| {
                    vars.iter()
                        .any(|v| v == "XMTPD_MIGRATION_SERVER_ENABLE=true")
                })
                .unwrap_or(false);

            if !is_migrator {
                continue;
            }

            let container_name = names[0].trim_start_matches('/').to_string();
            info!("Reloading migrator container: {}", container_name);

            // Extract config from existing container for recreation
            let image = info
                .config
                .as_ref()
                .and_then(|c| c.image.clone())
                .unwrap_or_default();
            let cmd = info.config.as_ref().and_then(|c| c.cmd.clone());
            let filtered_env: Vec<String> = env
                .map(|vars| {
                    vars.iter()
                        .filter(|v| !v.starts_with("XMTPD_MIGRATION_"))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let host_config = info.host_config.clone();

            // Stop and remove the container
            let stop_opts = StopContainerOptionsBuilder::default().t(10).build();
            let _ = docker.stop_container(id, Some(stop_opts)).await;
            let remove_opts = RemoveContainerOptionsBuilder::default().force(true).build();
            docker.remove_container(id, Some(remove_opts)).await?;

            // Recreate without migrator env vars
            let create_options = CreateContainerOptionsBuilder::default()
                .name(&container_name)
                .platform("linux/amd64");
            let config = ContainerCreateBody {
                image: Some(image),
                cmd,
                env: Some(filtered_env),
                host_config,
                ..Default::default()
            };
            create_and_start_container(&docker, &container_name, create_options, config).await?;
            info!("Restarted {} without migrator mode", container_name);
            count += 1;
        }

        info!("Reloaded {} migrator node(s)", count);
        Ok(())
    }
}

struct SharedServices {
    services: Vec<Box<dyn Service>>,
    anvil_rpc_url: url::Url,
    anvil_proxy_host: String,
}

/// Start shared services needed by both V3 and D14n: Anvil, Validation, History
async fn start_shared_services(proxy: &ToxiProxy) -> Result<SharedServices> {
    let mut anvil = services::Anvil::builder().build()?;
    anvil.start(proxy).await?;
    let rpc = anvil.external_rpc_url().unwrap_or_else(|| anvil.rpc_url());
    let anvil_proxy_host = anvil.internal_proxy_host()?;

    let mut validation = services::Validation::builder().build()?;
    let mut history = services::HistoryServer::builder().build()?;
    let launch = vec![
        validation.start(proxy).boxed(),
        history.start(proxy).boxed(),
    ];
    futures::future::try_join_all(launch).await?;

    Ok(SharedServices {
        services: vec![
            Box::new(anvil) as _,
            Box::new(validation) as _,
            Box::new(history) as _,
        ],
        anvil_rpc_url: rpc,
        anvil_proxy_host,
    })
}

async fn start_d14n(
    proxy: &ToxiProxy,
    dns_ip: String,
    anvil_proxy_host: String,
    anvil_rpc: &url::Url,
) -> Result<(Gateway, Vec<Box<dyn Service>>)> {
    use crate::constants::Gateway as GatewayConst;
    use alloy::signers::local::PrivateKeySigner;

    let mut redis = services::Redis::builder().build();
    redis.start(proxy).await?;

    // Fund the gateway wallet before starting it
    let gateway_key: PrivateKeySigner = GatewayConst::PRIVATE_KEY.parse()?;
    let gateway_address = gateway_key.address();
    crate::wallet_funding::fund_wallet(anvil_rpc.as_str(), gateway_address, None).await?;

    let mut gateway = services::Gateway::builder()
        .redis_host(redis.internal_proxy_host()?)
        .anvil_host(anvil_proxy_host)
        .dns_server(dns_ip)
        .build()?;
    gateway.start(proxy).await?;

    Ok((gateway, vec![Box::new(redis) as _]))
}

async fn start_v3(proxy: &ToxiProxy) -> Result<(NodeGo, Vec<Box<dyn Service>>)> {
    let mut mls_db = services::MlsDb::builder().build();
    let mut v3_db = services::V3Db::builder().build();
    let launch = vec![mls_db.start(proxy).boxed(), v3_db.start(proxy).boxed()];
    futures::future::try_join_all(launch).await?;
    let mut node_go = services::NodeGo::builder()
        .store_db_host(v3_db.internal_proxy_host()?)
        .mls_store_db_host(mls_db.internal_proxy_host()?)
        .build()?;
    node_go.start(proxy).await?;

    Ok((node_go, vec![Box::new(mls_db) as _, Box::new(v3_db) as _]))
}
