//! Global Constants for xdbg
use std::sync::LazyLock;
use tempfile::TempDir;
pub static TMPDIR: LazyLock<TempDir> = LazyLock::<TempDir>::new(|| TempDir::new().unwrap());
pub const STORAGE_PREFIX: &str = "xdbg";
