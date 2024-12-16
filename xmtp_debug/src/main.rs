mod app;
mod args;
mod constants;
mod logger;

use clap::Parser;
use color_eyre::eyre::Result;

use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
use xmtp_mls::XmtpApi;

// pub type DbgClient = xmtp_mls::client::Client<GrpcClient>;
type DbgClient = xmtp_mls::client::Client<Box<dyn XmtpApi>>;

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let opts = args::AppOpts::parse();
    let mut logger = logger::Logger::from(&opts.log);
    logger.init()?;

    let app = app::App::new(opts)?;
    app.run().await?;

    Ok(())
}
