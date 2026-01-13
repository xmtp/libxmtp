use std::sync::{Arc, OnceLock};

use crate::{
    app::service_manager::ServiceManager,
    config::{AddNode, AppArgs, Node},
    network::Network,
    services::{self, Service, ToxiProxy},
    types::XmtpdNode,
    xmtpd_cli::XmtpdCli,
};
use clap::Parser;
use color_eyre::eyre::{OptionExt, Result};
use futures::FutureExt;
use tokio::{runtime::EnterGuard, sync::Mutex};

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
            crate::config::Commands::Migrate(migrate) => todo!(),
        }
        Ok(())
    }
}
