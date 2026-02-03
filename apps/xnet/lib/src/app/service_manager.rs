//! Stateful service manager
//! network is hardcoded to XNET_NETWORK_NAME
use crate::{
    network::Network,
    services::{
        self, CoreDns, Gateway, Otterscan, ReplicationDb, Service, ToxiProxy, Traefik,
        TraefikConfig, Xmtpd,
    },
    types::XmtpdNode,
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
    services: Vec<Box<dyn Service>>,
    otterscan: Otterscan,
    nodes: Vec<Xmtpd>,
}

impl ServiceManager {
    /// starts services if not already started
    /// if running connects to them.
    pub async fn start() -> Result<Self> {
        // Start DNS and reverse proxy infrastructure first
        let mut coredns = CoreDns::builder().build();
        let mut traefik = Traefik::builder().build();

        // CoreDNS doesn't use ToxiProxy, but Traefik expects it (even though it doesn't register)
        let mut proxy = ToxiProxy::builder().build();
        proxy.start().await?;

        // Start networking infrastructure
        coredns.start(&proxy).await?;
        traefik.start(&proxy).await?;

        // Create Traefik config manager (loads existing routes from file)
        let traefik_config = TraefikConfig::new(traefik.dynamic_config_path())?;

        let mut services = Vec::new();
        info!("starting v3");
        services.extend(start_v3(&proxy).await?);
        let (gateway, anvil_external_rpc, svcs) = start_d14n(&proxy).await?;
        services.extend(svcs);

        let mut otterscan = Otterscan::builder()
            .anvil_host(anvil_external_rpc.to_string())
            .build();
        otterscan.start(&proxy).await?;

        Ok(Self {
            coredns,
            traefik,
            traefik_config,
            proxy,
            gateway,
            services,
            otterscan,
            nodes: Vec::new(),
        })
    }

    pub fn print_port_allocations() {
        ToxiProxy::print_port_allocations();
    }

    pub async fn stop(&mut self) -> Result<()> {
        for service in &mut self.services {
            service.stop().await?;
        }
        Ok(())
    }

    pub async fn add_xmtpd(&mut self, node: XmtpdNode) -> Result<()> {
        let mut xmtpd = Xmtpd::builder().node(node).build();
        xmtpd.start(&self.proxy).await?;

        // Register with Traefik for unified addressing
        if let Some(hostname) = <Xmtpd as Service>::hostname(&xmtpd) {
            if let Some(toxi_port) = xmtpd.proxy_port() {
                self.traefik_config.add_route(hostname, toxi_port)?;
            }
        }

        self.nodes.push(xmtpd);
        Ok(())
    }
}

async fn start_d14n(proxy: &ToxiProxy) -> Result<(Gateway, url::Url, Vec<Box<dyn Service>>)> {
    let mut anvil = services::Anvil::builder().build();
    let mut redis = services::Redis::builder().build();

    let launch = vec![anvil.start(&proxy).boxed(), redis.start(&proxy).boxed()];
    futures::future::try_join_all(launch).await?;
    let mut gateway = services::Gateway::builder()
        .redis_host(redis.internal_proxy_host()?)
        .anvil_host(anvil.internal_proxy_host()?)
        .build();
    gateway.start(proxy).await?;
    let anvil_external_rpc = anvil.external_rpc_url().unwrap_or_else(|| anvil.rpc_url());
    Ok((
        gateway,
        anvil_external_rpc,
        vec![Box::new(anvil) as _, Box::new(redis) as _],
    ))
}

async fn start_v3(proxy: &ToxiProxy) -> Result<Vec<Box<dyn Service>>> {
    let mut validation = services::Validation::builder().build();
    let mut mls_db = services::MlsDb::builder().build();
    let mut v3_db = services::V3Db::builder().build();
    // history is both but OK to start with v3 stuff to follow docker-compose
    let mut history = services::HistoryServer::builder().build();
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
        .build();
    node_go.start(proxy).await?;

    Ok(vec![
        Box::new(validation) as _,
        Box::new(mls_db) as _,
        Box::new(v3_db) as _,
        Box::new(node_go) as _,
        Box::new(history) as _,
    ])
}
