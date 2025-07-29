//! Database configuration values

// Changing these values is a breaking change, unless a migration path is specified
pub const WASM_VFS_NAME: &str = "opfs-libxmtp";
pub const WASM_VFS_DIRECTORY: &str = ".opfs-libxmtp-metadata";
pub const MAX_DB_POOL_SIZE: u32 = 25;
