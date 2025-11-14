//! Global Constants for xdbg
use std::sync::LazyLock;
use url::Url;
use xmtp_configuration::{GrpcUrlsDev, GrpcUrlsLocal, GrpcUrlsProduction, GrpcUrlsStaging};

pub static XMTP_PRODUCTION: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::NODE).unwrap());
pub static XMTP_DEV: LazyLock<Url> = LazyLock::new(|| Url::parse(GrpcUrlsDev::NODE).unwrap());
pub static XMTP_LOCAL: LazyLock<Url> = LazyLock::new(|| Url::parse(GrpcUrlsLocal::NODE).unwrap());

pub static XMTP_PRODUCTION_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::XMTPD).unwrap());
pub static XMTP_STAGING_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsStaging::XMTPD).unwrap());
pub static XMTP_DEV_D14N: LazyLock<Url> = LazyLock::new(|| Url::parse(GrpcUrlsDev::XMTPD).unwrap());
pub static XMTP_LOCAL_D14N: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsLocal::XMTPD).unwrap());

pub static XMTP_PRODUCTION_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsProduction::GATEWAY).unwrap());
pub static XMTP_STAGING_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsStaging::GATEWAY).unwrap());
pub static XMTP_DEV_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsDev::GATEWAY).unwrap());
pub static XMTP_LOCAL_GATEWAY: LazyLock<Url> =
    LazyLock::new(|| Url::parse(GrpcUrlsLocal::GATEWAY).unwrap());

pub const STORAGE_PREFIX: &str = "xdbg";
