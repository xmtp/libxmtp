mod app;
mod args;
mod constants;
mod logger;

use clap::Parser;
use color_eyre::eyre::Result;

use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;

pub type DbgClient = xmtp_mls::client::Client<GrpcClient>;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let opts = args::AppOpts::parse();
    logger::Logger::from(&opts.log).init()?;

    let app = app::App::new(opts)?;
    app.run().await?;

    Ok(())
}
