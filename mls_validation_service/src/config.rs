use clap::Parser;

// Gather the command line arguments into a struct
#[derive(Parser, Debug)]
#[command(about = "MLS Validation Server")]
pub(crate) struct Args {
    // Port to run the server on
    #[arg(short, long, default_value_t = 50051)]
    pub(crate) port: u32,

    #[arg(long, default_value_t = 50052)]
    pub(crate) health_check_port: u32,

    // A path to a json file in the same format as chain_urls_default.json in the codebase.
    #[arg(long)]
    pub(crate) chain_urls: Option<String>,
}
