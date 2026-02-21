use std::sync::{Arc, OnceLock};

use crate::{
    app::service_manager::ServiceManager,
    config::{AddNode, AppArgs, Node},
    network::Network,
    services::{self, Service, ToxiProxy},
    types::XmtpdNode,
    xmtpd_cli::XmtpdCli,
};
use chrono::{DateTime, Local, TimeZone};
use clap::Parser;
use color_eyre::eyre::{OptionExt, Result, eyre};
use futures::FutureExt;
use tokio::{runtime::EnterGuard, sync::Mutex};
use xmtp_api_d14n::d14n::FetchD14nCutover;
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

    pub async fn up(&self) -> Result<()> {
        info!("entered, starting net");
        let network = Network::new().await?;
        info!("starting service manager");
        let services = ServiceManager::start().await?;
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
        let cli = XmtpdCli::builder().toxiproxy(mgr.proxy.clone());
        let mut output = self.cli_output.lock().await;
        let gateway = mgr
            .gateway
            .external_url()
            .ok_or_eyre("no url for gateway")?;
        let mut xmtpd = XmtpdNode::new(gateway.as_str()).await?;
        cli.clone()
            .build()
            .register(&mgr, &mut *output, &xmtpd)
            .await?;
        cli.build().enable(&mut xmtpd, &mut *output).await?;
        if add.migrator {
            mgr.add_xmtpd_with_migrator(xmtpd.clone()).await?;
        } else {
            mgr.add_xmtpd(xmtpd.clone()).await?;
        }
        Ok(xmtpd)
    }

    pub async fn gateway_url(&self) -> Result<url::Url> {
        let mgr = ServiceManager::start().await?;
        mgr.gateway.external_url().ok_or_eyre("no url for gateway")
    }

    pub async fn info(&self) -> Result<()> {
        let network = Network::new().await?;
        network.list().await?;
        ServiceManager::print_port_allocations();
        Ok(())
    }
    /// Runs the command based on `Commands`
    pub async fn run(&self) -> Result<()> {
        let Some(ref cmd) = self.args.cmd else {
            return Ok(());
        };

        match cmd {
            crate::config::Commands::Up => self.up().await?,
            crate::config::Commands::Down => self.down().await?,
            crate::config::Commands::Delete => self.delete().await?,
            crate::config::Commands::Node(node) => match node {
                Node::Add(add) => {
                    let _ = self.add_node(add).await?;
                }
                _ => todo!(),
            },
            crate::config::Commands::Info(info) => self.info().await?,
            crate::config::Commands::Migrate(migrate) => {
                let cutover_ns = migrate.cutover_ns()?;
                info!("Setting d14n cutover to {} ns", cutover_ns);
                let mut mgr = ServiceManager::start().await?;
                mgr.reload_node_go(cutover_ns).await?;
            }
            crate::config::Commands::Cutover => {
                let mgr = ServiceManager::start().await?;
                let url = mgr
                    .node_go
                    .external_grpc_url()
                    .ok_or_eyre("node-go not running")?;
                let client = xmtp_api_grpc::GrpcClient::create(url.as_str(), false)
                    .map_err(|e| eyre!("{}", e))?;
                let response: FetchD14nCutoverResponse = FetchD14nCutover
                    .query(&client)
                    .await
                    .map_err(|e| eyre!("{}", e))?;
                let ts_ns = response.timestamp_ns;
                let secs = (ts_ns / 1_000_000_000) as i64;
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
        Ok(())
    }
}
