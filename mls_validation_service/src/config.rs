use clap::Parser;
use std::num::NonZeroUsize;

// Gather the command line arguments into a struct
#[derive(Parser, Debug)]
#[command(about = "MLS Validation Server")]
pub(crate) struct Args {
    // Print Version
    #[arg(short, long)]
    pub(crate) version: bool,

    // Port to run the server on
    #[arg(short, long, default_value_t = 50051)]
    pub(crate) port: u32,

    #[arg(long, default_value_t = 50052)]
    pub(crate) health_check_port: u32,

    // A path to a json file in the same format as chain_urls_default.json in the codebase.
    #[arg(long)]
    pub(crate) chain_urls: Option<String>,

    // The size of the cache to use for the smart contract signature verifier.
    #[arg(long, default_value_t = NonZeroUsize::new(10000).expect("Set to positive number"))]
    pub(crate) cache_size: NonZeroUsize,
}
