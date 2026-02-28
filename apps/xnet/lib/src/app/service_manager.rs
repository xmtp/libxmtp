//! Stateful service manager
//! network is hardcoded to XNET_NETWORK_NAME
use std::collections::HashMap;
use std::io::stdout;

use bollard::{
    Docker,
    models::ContainerCreateBody,
    query_parameters::{
        CreateContainerOptionsBuilder, ListContainersOptionsBuilder,
        RemoveContainerOptionsBuilder, StopContainerOptionsBuilder,
    },
};
use crate::{
    Config,
    config::NodeToml,
    constants::Xmtpd as XmtpdConst,
    network::{Network, XNET_NETWORK_NAME},
    services::{
        self, CoreDns, Gateway, Grafana, NodeGo, Otterscan, PgAdmin, Prometheus, ReplicationDb,
        Service, ToxiProxy, Traefik, TraefikConfig, Xmtpd, allocate_xmtpd_port,
        create_and_start_container,
    },
    types::XmtpdNode,
    xmtpd_cli::XmtpdCli,
};
use color_eyre::eyre::Result;
use futures::FutureExt;

/// Starts services in the right order
/// and keeps a list of running services
pub struct ServiceManager {
    pub coredns: CoreDns,
    pub traefik: Traefik,
    pub traefik_config: TraefikConfig,
    pub proxy: ToxiProxy,
    pub gateway: Gateway,
    pub node_go: NodeGo,
    services: Vec<Box<dyn Service>>,
    otterscan: Otterscan,
    prometheus: Prometheus,
    grafana: Grafana,
    pgadmin: PgAdmin,
    nodes: Vec<Xmtpd>,
}

impl ServiceManager {
    /// starts services if not already started
    /// if running connects to them.
    pub async fn start() -> Result<Self> {
        // Start ToxiProxy first (other services may register with it)
        let mut proxy = ToxiProxy::builder().build()?;
        proxy.start().await?;

        // Start Traefik first so we can get its IP for CoreDNS
        let mut traefik = Traefik::builder().build();
        traefik.start(&proxy).await?;

        // Get Traefik's IP address for CoreDNS container-facing DNS
        let traefik_ip = traefik.container_ip().await?;

        // Start CoreDNS with Traefik's IP
        let mut coredns = CoreDns::builder().traefik_ip(traefik_ip).build();
        coredns.start(&proxy).await?;

        // Create Traefik config manager (loads existing routes from file)
        let traefik_config = TraefikConfig::new(traefik.dynamic_config_path())?;

        // Start monitoring services (Prometheus + Grafana + PgAdmin) in parallel
        let mut prometheus = Prometheus::builder().build();
        let mut grafana = Grafana::builder().build();
        let mut pgadmin = PgAdmin::builder().build();
        let launch = vec![
            prometheus.start(&proxy).boxed(),
            grafana.start(&proxy).boxed(),
            pgadmin.start(&proxy).boxed(),
        ];
        futures::future::try_join_all(launch).await?;

        let mut services = Vec::new();
        info!("starting v3");
        let (node_go, svcs) = start_v3(&proxy).await?;
        services.extend(svcs);
        let dns_ip = coredns.container_ip().await?;
        let (gateway, anvil_external_rpc, svcs) = start_d14n(&proxy, dns_ip).await?;
        services.extend(svcs);

        let mut otterscan = Otterscan::builder()
            .anvil_host(anvil_external_rpc.to_string())
            .build();
        otterscan.start(&proxy).await?;

        let mut this = Self {
            node_go,
            coredns,
            traefik,
            traefik_config,
            proxy,
            gateway,
            services,
            otterscan,
            prometheus,
            grafana,
            pgadmin,
            nodes: Vec::new(),
        };

        let gateway_host = this.gateway.external_url().expect("just created gateway");
        let mut id = 0;
        // start any xmtpd nodes described in toml
        let config = Config::load()?;
        let cli = XmtpdCli::builder().toxiproxy(this.proxy.clone());
        let existing_proxies = this.proxy.list_proxies().await?;
        let mut output = stdout();
        for NodeToml {
            port,
            migrator,
            name,
            enable,
        } in config.xmtpd_nodes
        {
            id += XmtpdConst::NODE_ID_INCREMENT;
            let node_name = name.as_ref().unwrap_or(&format!("xnet-{}", id)).to_string();
            if existing_proxies.contains_key(&node_name) {
                info!("node {} already has proxy registered, skipping", node_name);
                continue;
            }
            if !enable {
                continue;
            }
            let node = XmtpdNode::builder();
            let node = if let Some(p) = port {
                node.port(p)
            } else {
                node.port(allocate_xmtpd_port()?)
            };
            let node = if let Some(n) = name {
                node.name(n)
            } else {
                node.name(format!("xnet-{}", id))
            };
            let node = node.node_id(id);
            let num_ids = id / XmtpdConst::NODE_ID_INCREMENT;
            let next_signer = &config.signers[num_ids as usize + 1];
            let mut node = node.signer(next_signer.clone()).build();
            cli.clone()
                .build()
                .register(&this, &mut output, &node)
                .await?;
            cli.clone().build().enable(&mut node, &mut output).await?;
            if migrator {
                this.add_xmtpd_with_migrator(node).await?;
            } else {
                this.add_xmtpd(node).await?;
            }
        }

        Ok(this)
    }

    pub fn print_port_allocations() {
        ToxiProxy::print_port_allocations();
    }

    pub async fn stop(&mut self) -> Result<()> {
        for service in &mut self.services {
            service.stop().await?;
        }
        self.gateway.stop().await?;
        self.node_go.stop().await?;
        self.otterscan.stop().await?;
        self.pgadmin.stop().await?;
        self.grafana.stop().await?;
        self.prometheus.stop().await?;
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
        let dns_ip = self.coredns.container_ip().await?;
        let xmtpd = Xmtpd::builder()
            .node(node)
            .migrator(true)
            .node_go(self.node_go.clone())
            .dns_server(dns_ip)
            .build()?;
        self.internal_add_xmtpd(xmtpd).await
    }

    pub async fn reload_node_go(&mut self, d14n_cutover_ns: i64) -> Result<()> {
        self.node_go.reload(d14n_cutover_ns, &self.proxy).await
    }

    async fn internal_add_xmtpd(&mut self, mut xmtpd: Xmtpd) -> Result<()> {
        xmtpd.start(&self.proxy).await?;

        // Register with Traefik for unified addressing
        if let Some(hostname) = <Xmtpd as Service>::hostname(&xmtpd)
            && let Some(toxi_port) = xmtpd.proxy_port()
        {
            self.traefik_config.add_route(hostname, toxi_port)?;
        }

        self.nodes.push(xmtpd);

        // Update Prometheus scrape targets with the new node
        self.prometheus.update_targets(&self.nodes)?;

        // Update PgAdmin servers.json with the new node's replication DB
        self.pgadmin.update_servers(&self.nodes)?;

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
            let names = container
                .names
                .as_ref()
                .map(|n| n.as_slice())
                .unwrap_or(&[]);
            // Docker prepends "/" to container names; xmtpd nodes are named xnet-{id}
            let is_xmtpd = names.iter().any(|n| {
                let trimmed = n.trim_start_matches('/');
                trimmed.starts_with("xnet-")
                    && trimmed[5..].chars().all(|c| c.is_ascii_digit())
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
            let remove_opts = RemoveContainerOptionsBuilder::default()
                .force(true)
                .build();
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

async fn start_d14n(
    proxy: &ToxiProxy,
    dns_ip: String,
) -> Result<(Gateway, url::Url, Vec<Box<dyn Service>>)> {
    let mut anvil = services::Anvil::builder().build()?;
    let mut redis = services::Redis::builder().build();

    let launch = vec![anvil.start(proxy).boxed(), redis.start(proxy).boxed()];
    futures::future::try_join_all(launch).await?;
    let mut gateway = services::Gateway::builder()
        .redis_host(redis.internal_proxy_host()?)
        .anvil_host(anvil.internal_proxy_host()?)
        .dns_server(dns_ip)
        .build()?;
    gateway.start(proxy).await?;
    let anvil_external_rpc = anvil.external_rpc_url().unwrap_or_else(|| anvil.rpc_url());
    Ok((
        gateway,
        anvil_external_rpc,
        vec![Box::new(anvil) as _, Box::new(redis) as _],
    ))
}

async fn start_v3(proxy: &ToxiProxy) -> Result<(NodeGo, Vec<Box<dyn Service>>)> {
    let mut validation = services::Validation::builder().build()?;
    let mut mls_db = services::MlsDb::builder().build();
    let mut v3_db = services::V3Db::builder().build();
    // history is both but OK to start with v3 stuff to follow docker-compose
    let mut history = services::HistoryServer::builder().build()?;
    // dependencies
    let launch = vec![
        validation.start(proxy).boxed(),
        mls_db.start(proxy).boxed(),
        v3_db.start(proxy).boxed(),
        history.start(proxy).boxed(),
    ];
    futures::future::try_join_all(launch).await?;
    let mut node_go = services::NodeGo::builder()
        .store_db_host(v3_db.internal_proxy_host()?)
        .mls_store_db_host(mls_db.internal_proxy_host()?)
        .mls_validation_address(validation.internal_proxy_host()?)
        .build()?;
    node_go.start(proxy).await?;

    Ok((
        node_go,
        vec![
            Box::new(validation) as _,
            Box::new(mls_db) as _,
            Box::new(v3_db) as _,
            Box::new(history) as _,
        ],
    ))
}
