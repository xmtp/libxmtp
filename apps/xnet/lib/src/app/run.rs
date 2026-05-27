use std::sync::{Arc, OnceLock};

use crate::{
    app::service_manager::ServiceManager,
    config::{AddNode, AppArgs, Node},
    network::Network,
    node_provisioner::NodeProvisioner,
    types::XmtpdNode,
};
use chrono::{DateTime, Local, TimeZone};
use clap::Parser;
use color_eyre::eyre::{OptionExt, Result, eyre};
use tokio::sync::Mutex;
use xmtp_api_d14n::d14n::{FetchD14nCutover, GetNodes};
use xmtp_proto::{prelude::Query, xmtp::migration::api::v1::FetchD14nCutoverResponse};

pub use crate::config::Config;

pub struct App {
    pub args: AppArgs,
    cli_output: Arc<Mutex<Vec<u8>>>,
}

static ARGS: OnceLock<AppArgs> = OnceLock::new();

/// Actions functions are split into fns to make it easier
/// to use from both cli and gui
impl App {
    pub fn new(args: AppArgs) -> Result<Self> {
        // let rt = tokio::runtime::Builder::new_current_thread()
        //     .thread_name("xnet")
        //     .enable_time()
        //     .enable_io()
        //     .build()?;
        Ok(Self {
            args,
            cli_output: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn parse() -> Result<Self> {
        if ARGS.get().is_none() {
            ARGS.set(AppArgs::parse()).expect("checked for init");
        }
        let args = ARGS.get().expect("app args must already be set");
        Self::new(args.clone())
    }

    pub async fn up(&self, cli_paused: bool) -> Result<()> {
        let _network = Network::new().await?;
        let _mgr = ServiceManager::start_paused(cli_paused).await?;
        Ok(())
    }

    pub async fn down(&self) -> Result<()> {
        let network = Network::new().await?;
        network.down().await?;
        Ok(())
    }

    pub async fn delete(&self) -> Result<()> {
        let network = Network::new().await?;
        network.delete_all().await?;
        Ok(())
    }

    pub async fn add_node(&self, add: &AddNode) -> Result<XmtpdNode> {
        let mut mgr = ServiceManager::start().await?;

        let node = NodeProvisioner::builder()
            .migrator(add.migrator)
            .use_standard_port(add.use_standard_port)
            .build()
            .provision(&mut mgr)
            .await?;

        Ok(node)
    }

    pub async fn gateway_url(&self) -> Result<url::Url> {
        let mgr = ServiceManager::start().await?;
        mgr.gateway
            .as_ref()
            .ok_or_eyre("Gateway not running (enable_d14n required)")?
            .external_url()
            .ok_or_eyre("no url for gateway")
    }

    pub async fn addresses(&self) -> Result<()> {
        use ascii_table::AsciiTable;

        let mgr = ServiceManager::start().await?;
        let gateway_url = mgr
            .gateway
            .as_ref()
            .ok_or_eyre("Gateway not running")?
            .external_url()
            .ok_or_eyre("gateway not running")?;
        let grpc =
            xmtp_api_grpc::GrpcClient::create(gateway_url.clone()).map_err(|e| eyre!("{}", e))?;
        let response = GetNodes::builder()
            .build()
            .unwrap()
            .query(&grpc)
            .await
            .map_err(|e| eyre!("{}", e))?;

        let config = Config::load()?;
        let mut nodes: Vec<_> = response.nodes.into_iter().collect();
        nodes.sort_by_key(|(id, _)| *id);

        let mut table = AsciiTable::default();
        table.column(0).set_header("ID");
        table.column(1).set_header("Name");
        table.column(2).set_header("Signer");
        table.column(3).set_header("Payer");
        table.column(4).set_header("Migration Payer");
        table.column(5).set_header("URL");

        // Gateway row
        use crate::constants::Gateway as GatewayConst;
        use alloy::signers::local::PrivateKeySigner;
        let gateway_key: PrivateKeySigner = GatewayConst::PRIVATE_KEY.parse()?;
        let mut rows: Vec<Vec<String>> = vec![vec![
            "-".into(),
            GatewayConst::CONTAINER_NAME.into(),
            gateway_key.address().to_string(),
            "-".into(),
            "-".into(),
            gateway_url.to_string(),
        ]];

        // Node rows
        rows.extend(nodes.iter().map(|(id, url)| {
            vec![
                id.to_string(),
                format!("xnet-{}", id),
                config.address_for_node(*id).to_string(),
                config.payer_address_for_node(*id).to_string(),
                config.migration_payer_address_for_node(*id).to_string(),
                url.to_string(),
            ]
        }));

        table.println(rows);
        Ok(())
    }

    pub async fn activate_d14n(&self) -> Result<()> {
        use crate::constants::Anvil as AnvilConst;
        let mut mgr = ServiceManager::start().await?;
        let rpc = mgr.anvil_rpc_url().ok_or_eyre("Anvil not running")?;
        crate::contracts::set_broadcasters_paused(rpc.as_str(), AnvilConst::ADMIN_KEY, false)
            .await?;
        mgr.remove_migrators().await?;
        Ok(())
    }

    pub async fn info(&self) -> Result<()> {
        use ascii_table::AsciiTable;

        let network = Network::new().await?;
        network.list().await?;
        ServiceManager::print_port_allocations();

        let mgr = ServiceManager::start().await?;
        let rpc = mgr.anvil_rpc_url().ok_or_eyre("Anvil not running")?;
        let statuses = crate::contracts::get_broadcaster_pause_status(rpc.as_str()).await?;

        println!();
        let mut table = AsciiTable::default();
        table.column(0).set_header("Contract");
        table.column(1).set_header("Status");

        let rows: Vec<Vec<String>> = statuses
            .iter()
            .map(|(target, paused)| {
                vec![
                    target.to_string(),
                    if *paused {
                        "Paused".to_string()
                    } else {
                        "Active".to_string()
                    },
                ]
            })
            .collect();

        table.println(rows);
        Ok(())
    }
    /// Runs the command based on `Commands`
    pub async fn run(&self) -> Result<()> {
        let Some(ref cmd) = self.args.cmd else {
            return Ok(());
        };

        match cmd {
            crate::config::Commands::Up(up) => self.up(up.paused).await?,
            crate::config::Commands::Down => self.down().await?,
            crate::config::Commands::Delete => self.delete().await?,
            crate::config::Commands::Node(node) => match node {
                Node::Add(add) => {
                    let _ = self.add_node(add).await?;
                }
            },
            crate::config::Commands::ActivateD14n => self.activate_d14n().await?,
            crate::config::Commands::Info(info) => self.info().await?,
            crate::config::Commands::Migrate(migrate) => {
                let cutover_ns = migrate.cutover_ns()?;
                info!("Setting d14n cutover to {} ns", cutover_ns);
                let mut mgr = ServiceManager::start().await?;
                mgr.reload_node_go(cutover_ns).await?;
            }
            crate::config::Commands::Addresses => self.addresses().await?,
            crate::config::Commands::Cutover(cutover) => {
                let url = match &cutover.grpc_url {
                    Some(u) => u.clone(),
                    None => {
                        let mgr = ServiceManager::start().await?;
                        mgr.node_go
                            .as_ref()
                            .ok_or_eyre("node-go not running (enable_v3 required)")?
                            .external_grpc_url()
                            .ok_or_eyre(
                                "node-go has no external gRPC URL (ToxiProxy port not configured)",
                            )?
                    }
                };
                let client = xmtp_api_grpc::GrpcClient::create(url).map_err(|e| eyre!("{}", e))?;
                let response: FetchD14nCutoverResponse = FetchD14nCutover
                    .query(&client)
                    .await
                    .map_err(|e| eyre!("{}", e))?;
                let ts_ns = response.timestamp_ns;
                let secs = (ts_ns / 1_000_000_000) as i64;
                if cutover.unix {
                    println!("{}", secs);
                } else {
                    let nanos = (ts_ns % 1_000_000_000) as u32;
                    let dt: DateTime<Local> = Local
                        .timestamp_opt(secs, nanos)
                        .single()
                        .ok_or_eyre("invalid timestamp")?;
                    println!(
                        "d14n cutover: {} ({} ns)",
                        dt.format("%Y-%m-%d %H:%M:%S %Z"),
                        ts_ns
                    );
                }
            }
        }
        Ok(())
    }
}
