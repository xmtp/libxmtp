use std::num::NonZeroUsize;

#[derive(Clone, Debug, Default)]
pub(crate) enum LogFormat {
    #[default]
    Text,
    Json,
}

impl LogFormat {
    fn parse(value: &str) -> Result<Self, lexopt::Error> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("invalid log format `{value}`; expected `text` or `json`").into()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) version: bool,
    pub(crate) port: u32,
    pub(crate) health_check_port: u32,
    pub(crate) chain_urls: Option<String>,
    pub(crate) cache_size: NonZeroUsize,
    pub(crate) log_format: LogFormat,
}

impl Args {
    pub(crate) fn parse() -> Result<Self, lexopt::Error> {
        use lexopt::prelude::*;

        let mut version = false;
        let mut port = 50051;
        let mut health_check_port = 50052;
        let mut chain_urls = None;
        let mut cache_size = NonZeroUsize::new(10000).expect("Set to positive number");
        let mut log_format = std::env::var("LOG_FORMAT")
            .ok()
            .map(|value| LogFormat::parse(&value))
            .transpose()?
            .unwrap_or_default();

        let mut parser = lexopt::Parser::from_env();
        while let Some(arg) = parser.next()? {
            match arg {
                Short('v') | Long("version") => version = true,
                Short('p') | Long("port") => port = parser.value()?.parse()?,
                Long("health-check-port") => health_check_port = parser.value()?.parse()?,
                Long("chain-urls") => chain_urls = Some(parser.value()?.string()?),
                Long("cache-size") => cache_size = parser.value()?.parse()?,
                Long("log-format") => {
                    let value = parser.value()?.string()?;
                    log_format = LogFormat::parse(&value)?;
                }
                Short('h') | Long("help") => {
                    print_help();
                    std::process::exit(0);
                }
                _ => return Err(arg.unexpected()),
            }
        }

        Ok(Self {
            version,
            port,
            health_check_port,
            chain_urls,
            cache_size,
            log_format,
        })
    }
}

fn print_help() {
    println!(
        "MLS Validation Server\n\nUsage: mls-validation-service [OPTIONS]\n\nOptions:\n  -v, --version                     Print version\n  -p, --port <PORT>                 Port to run the server on [default: 50051]\n      --health-check-port <PORT>    Health check port [default: 50052]\n      --chain-urls <PATH>           Path to chain URLs JSON file\n      --cache-size <SIZE>           Signature verifier cache size [default: 10000]\n      --log-format <FORMAT>         Log format: text or json [env: LOG_FORMAT] [default: text]\n  -h, --help                        Print help"
    );
}
