#![recursion_limit = "512"]

mod app;
mod args;
mod constants;
mod logger;
mod metrics;

use clap::Parser;
use color_eyre::eyre::Result;

use std::sync::{Arc, OnceLock};
use xmtp_mls::context::XmtpMlsLocalContext;

static FAIL_ON_ERROR: OnceLock<bool> = OnceLock::new();

/// Whether `--fail-on-error` was passed at the CLI. Returns `false` when
/// uninitialized (unit tests, early init paths).
pub fn fail_on_error() -> bool {
    FAIL_ON_ERROR.get().copied().unwrap_or(false)
}

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
    metrics::init_metrics(opts.metrics);
    let _ = FAIL_ON_ERROR.set(opts.fail_on_error);

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
