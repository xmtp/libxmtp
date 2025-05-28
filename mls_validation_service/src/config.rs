use std::num::NonZeroUsize;

// Gather the command line arguments into a struct
#[derive(Debug)]
pub(crate) struct Args {
    // Print Version
    pub(crate) version: bool,

    // Port to run the server on
    pub(crate) port: u32,

    pub(crate) health_check_port: u32,

    // A path to a json file in the same format as chain_urls_default.json in the codebase.
    pub(crate) chain_urls: Option<String>,

    // The size of the cache to use for the smart contract signature verifier.
    pub(crate) cache_size: NonZeroUsize,
}

impl Args {
    pub(crate) fn parse() -> Result<Self, lexopt::Error> {
        use lexopt::prelude::*;

        let mut version = false;
        let mut port = 50051;
        let mut health_check_port = 50052;
        let mut chain_urls = None;
        let mut cache_size = NonZeroUsize::new(10000).expect("Set to positive number");

        let mut parser = lexopt::Parser::from_env();
        while let Some(arg) = parser.next()? {
            match arg {
                Short('v') | Long("version") => {
                    version = true;
                }
                Short('p') | Long("port") => {
                    port = parser.value()?.parse()?;
                }
                Long("health-check-port") => {
                    health_check_port = parser.value()?.parse()?;
                }
                Long("chain-urls") => {
                    chain_urls = Some(parser.value()?.string()?);
                }
                Long("cache-size") => {
                    let size: usize = parser.value()?.parse()?;
                    cache_size =
                        NonZeroUsize::new(size).ok_or("cache-size must be a positive number")?;
                }
                Long("help") => {
                    println!("MLS Validation Server

USAGE:
    mls-validation-service [OPTIONS]

OPTIONS:
    -v, --version                Print version information
    -p, --port <PORT>            Port to run the server on [default: 50051]
        --health-check-port <PORT>  Port for health check [default: 50052]
        --chain-urls <PATH>      Path to a json file with chain URLs
        --cache-size <SIZE>      Size of the cache for smart contract signature verifier [default: 10000]
        --help                   Print help information");
                    std::process::exit(0);
                }
                _ => return Err(arg.unexpected()),
            }
        }

        Ok(Args {
            version,
            port,
            health_check_port,
            chain_urls,
            cache_size,
        })
    }

    #[cfg(test)]
    pub(crate) fn parse_from<I>(args: I) -> Result<Self, lexopt::Error>
    where
        I: IntoIterator,
        I::Item: Into<std::ffi::OsString>,
    {
        use lexopt::prelude::*;

        let mut version = false;
        let mut port = 50051;
        let mut health_check_port = 50052;
        let mut chain_urls = None;
        let mut cache_size = NonZeroUsize::new(10000).expect("Set to positive number");

        let mut parser = lexopt::Parser::from_iter(args);
        while let Some(arg) = parser.next()? {
            match arg {
                Short('v') | Long("version") => {
                    version = true;
                }
                Short('p') | Long("port") => {
                    port = parser.value()?.parse()?;
                }
                Long("health-check-port") => {
                    health_check_port = parser.value()?.parse()?;
                }
                Long("chain-urls") => {
                    chain_urls = Some(parser.value()?.string()?);
                }
                Long("cache-size") => {
                    let size: usize = parser.value()?.parse()?;
                    cache_size =
                        NonZeroUsize::new(size).ok_or("cache-size must be a positive number")?;
                }
                Long("help") => {
                    println!("MLS Validation Server

USAGE:
    mls-validation-service [OPTIONS]

OPTIONS:
    -v, --version                Print version information
    -p, --port <PORT>            Port to run the server on [default: 50051]
        --health-check-port <PORT>  Port for health check [default: 50052]
        --chain-urls <PATH>      Path to a json file with chain URLs
        --cache-size <SIZE>      Size of the cache for smart contract signature verifier [default: 10000]
        --help                   Print help information");
                    std::process::exit(0);
                }
                _ => return Err(arg.unexpected()),
            }
        }

        Ok(Args {
            version,
            port,
            health_check_port,
            chain_urls,
            cache_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_args() {
        let args = Args::parse_from(&["test"]).unwrap();
        assert_eq!(args.version, false);
        assert_eq!(args.port, 50051);
        assert_eq!(args.health_check_port, 50052);
        assert_eq!(args.chain_urls, None);
        assert_eq!(args.cache_size.get(), 10000);
    }

    #[test]
    fn test_version_flag() {
        let args = Args::parse_from(&["test", "--version"]).unwrap();
        assert_eq!(args.version, true);
    }

    #[test]
    fn test_port_flag() {
        let args = Args::parse_from(&["test", "--port", "8080"]).unwrap();
        assert_eq!(args.port, 8080);
    }

    #[test]
    fn test_short_flags() {
        let args = Args::parse_from(&["test", "-v", "-p", "9000"]).unwrap();
        assert_eq!(args.version, true);
        assert_eq!(args.port, 9000);
    }
}
