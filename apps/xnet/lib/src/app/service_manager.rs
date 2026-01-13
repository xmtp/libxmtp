//! Stateful service manager
//! network is hardcoded to XNET_NETWORK_NAME
use std::io::stdout;

use crate::{
    Config,
    config::NodeToml,
    constants::Xmtpd as XmtpdConst,
    network::Network,
    services::{
        self, CoreDns, Gateway, NodeGo, Otterscan, ReplicationDb, Service, ToxiProxy, Traefik,
        TraefikConfig, Xmtpd, allocate_xmtpd_port,
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

        let mut services = Vec::new();
        info!("starting v3");
        let (node_go, svcs) = start_v3(&proxy).await?;
        services.extend(svcs);
        let (gateway, anvil_external_rpc, svcs) = start_d14n(&proxy).await?;
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

    async fn internal_add_xmtpd(&mut self, mut xmtpd: Xmtpd) -> Result<()> {
        xmtpd.start(&self.proxy).await?;

        // Register with Traefik for unified addressing
        if let Some(hostname) = <Xmtpd as Service>::hostname(&xmtpd)
            && let Some(toxi_port) = xmtpd.proxy_port()
        {
            self.traefik_config.add_route(hostname, toxi_port)?;
        }

        self.nodes.push(xmtpd);
        Ok(())
    }
}

async fn start_d14n(proxy: &ToxiProxy) -> Result<(Gateway, url::Url, Vec<Box<dyn Service>>)> {
    let mut anvil = services::Anvil::builder().build()?;
    let mut redis = services::Redis::builder().build();

    let launch = vec![anvil.start(proxy).boxed(), redis.start(proxy).boxed()];
    futures::future::try_join_all(launch).await?;
    let mut gateway = services::Gateway::builder()
        .redis_host(redis.internal_proxy_host()?)
        .anvil_host(anvil.internal_proxy_host()?)
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
