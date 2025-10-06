//! Global Constants for xdbg
use tempfile::TempDir;

use std::sync::LazyLock;
use url::Url;

pub static XMTP_PRODUCTION: LazyLock<Url> = LazyLock::new(|| Url::parse("").unwrap());
pub static XMTP_DEV: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://grpc.dev.xmtp.network:443").unwrap());
pub static XMTP_LOCAL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("http://localhost:5556").unwrap());

pub static XMTP_PRODUCTION_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://grpc.testnet.xmtp.network:443").unwrap());
pub static XMTP_STAGING_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://grpc.testnet-staging.xmtp.network:443").unwrap());
pub static XMTP_DEV_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://grpc.testnet-dev.xmtp.network:443").unwrap());
pub static XMTP_LOCAL_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse("http://localhost:5050").unwrap());

pub static XMTP_PRODUCTION_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://payer.testnet.xmtp.network:443").unwrap());
pub static XMTP_STAGING_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://payer.testnet-staging.xmtp.network:443").unwrap());
pub static XMTP_DEV_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://payer.testnet-dev.xmtp.network:443").unwrap());
pub static XMTP_LOCAL_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse("http://localhost:5052").unwrap());

pub static TMPDIR: LazyLock<TempDir> = LazyLock::<TempDir>::new(|| TempDir::new().unwrap());
pub const STORAGE_PREFIX: &str = "xdbg";
