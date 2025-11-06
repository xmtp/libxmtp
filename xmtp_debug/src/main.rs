mod app;
mod args;
mod constants;
mod logger;
mod queries;

use clap::Parser;
use color_eyre::eyre::Result;

use std::sync::Arc;
use xmtp_mls::context::XmtpMlsLocalContext;

pub type MlsContext =
    Arc<XmtpMlsLocalContext<DbgClientApi, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;
type DbgClientApi = xmtp_mls::XmtpApiClient;
type DbgClient = xmtp_mls::client::Client<MlsContext>;

const XDBG_ID_NONCE: u64 = 1;

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let opts = args::AppOpts::parse();
    let mut logger = logger::Logger::from(&opts.log);
    logger.init()?;

    if opts.version {
        info!("Version: {0}", get_version());
        return Ok(());
    }

    let app = app::App::new(opts)?;
    app.run().await?;

    Ok(())
}

pub fn get_version() -> String {
    format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("VERGEN_GIT_SHA"))
}
