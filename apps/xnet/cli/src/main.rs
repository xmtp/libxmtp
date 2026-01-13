//! xnet CLI - XMTP Network Testing Framework
//!
//! A CLI tool for managing Docker containers for XMTP testing.

use clap::Parser;
use color_eyre::eyre::Result;
use xnet::{Config, app::App, config::AppArgs, get_version};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let args = AppArgs::parse();
    tracing_subscriber::fmt().init();
    if args.version {
        tracing::info!("Version: {}", get_version());
        return Ok(());
    }
    let _ = Config::load()?;
    let app = App::new(args)?;
    app.run().await?;
    Ok(())
}
