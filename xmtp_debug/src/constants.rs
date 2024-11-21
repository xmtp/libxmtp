//! Global Constants for xdbg
use tempfile::TempDir;

use std::sync::LazyLock;
use url::Url;

pub static XMTP_PRODUCTION: LazyLock<Url> = LazyLock::new(|| Url::parse("").unwrap());
pub static XMTP_DEV: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://grpc.dev.xmtp.network:443").unwrap());
pub static XMTP_LOCAL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("http://localhost:5556").unwrap());
pub static TMPDIR: LazyLock<TempDir> = LazyLock::<TempDir>::new(|| TempDir::new().unwrap());
pub const STORAGE_PREFIX: &str = "xdbg";
