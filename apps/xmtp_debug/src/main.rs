#![recursion_limit = "512"]

mod app;
mod args;
mod constants;
mod logger;
mod metrics;

use clap::Parser;
use color_eyre::eyre::{Result, eyre};

use std::sync::{Arc, OnceLock};
use xmtp_mls::context::XmtpMlsLocalContext;

use crate::args::AppOpts;

static FAIL_FAST: OnceLock<bool> = OnceLock::new();

/// Whether `--fail-fast` was passed at the CLI. Returns `false` when
/// uninitialized (unit tests, early init paths).
pub fn fail_fast() -> bool {
    FAIL_FAST.get().copied().unwrap_or(false)
}

pub type MlsContext =
    Arc<XmtpMlsLocalContext<DbgClientApi, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;
type DbgClientApi = xmtp_mls::XmtpApiClient;
type DbgClient = xmtp_mls::client::Client<MlsContext>;

const XDBG_ID_NONCE: u64 = 1;

pub static CONF: OnceLock<Arc<AppOpts>> = OnceLock::new();

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let opts = config()?;
    let mut logger = logger::Logger::from(&opts.log);
    logger.init()?;
    metrics::init_metrics(opts.metrics);
    let _ = FAIL_FAST.set(opts.fail_fast);

    if opts.version {
        info!("Version: {0}", get_version());
        return Ok(());
    }

    let app = app::App::new();
    app.run().await?;

    Ok(())
}

pub fn get_version() -> String {
    format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("VERGEN_GIT_SHA"))
}

pub fn config() -> Result<&'static AppOpts> {
    if CONF.get().is_none() {
        let opts = args::AppOpts::parse();
        CONF.set(Arc::new(opts))
            .map_err(|_| eyre!("config already initialized"))?;
    }
    CONF.get()
        .map(|v| v.as_ref())
        .ok_or_else(|| eyre!("Config not initialized"))
}

pub fn config_unchecked() -> &'static AppOpts {
    CONF.get().map(|v| v.as_ref()).expect("unchecked config")
}
