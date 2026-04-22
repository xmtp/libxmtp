//! Global Constants for xdbg
use std::sync::LazyLock;
use tempfile::TempDir;
use url::Url;
use xmtp_configuration::{GrpcUrlsDev, GrpcUrlsLocal, GrpcUrlsProduction};

pub static XMTP_PRODUCTION: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::NODE).unwrap());
pub static XMTP_DEV: LazyLock<Url> = LazyLock::new(|| Url::parse(GrpcUrlsDev::NODE).unwrap());
pub static XMTP_LOCAL: LazyLock<Url> = LazyLock::new(|| Url::parse(GrpcUrlsLocal::NODE).unwrap());

pub static XMTP_PRODUCTION_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::XMTPD).unwrap());
pub static XMTP_LOCAL_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsLocal::XMTPD).unwrap());

pub static XMTP_PRODUCTION_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::GATEWAY).unwrap());
pub static XMTP_LOCAL_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsLocal::GATEWAY).unwrap());

pub static XMTP_PRODUCTION_PERF_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::PERF_GATEWAY).unwrap());
pub static XMTP_LOCAL_PERF_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsLocal::PERF_GATEWAY).unwrap());

pub static TMPDIR: LazyLock<TempDir> = LazyLock::<TempDir>::new(|| TempDir::new().unwrap());
pub const STORAGE_PREFIX: &str = "xdbg";
