//! Database configuration values

// Changing these values is a breaking change, unless a migration path is specified
pub const WASM_VFS_NAME: &str = "opfs-libxmtp";
/// VFS Directory for OPFS
pub const WASM_VFS_DIRECTORY: &str = ".opfs-libxmtp-metadata";
/// Max Size the Database Pool is allowed to grow to
pub const MAX_DB_POOL_SIZE: u32 = 12;
/// max time the database will wait to acquire a lock for a table
pub const BUSY_TIMEOUT: i32 = 5_000;
/// Minimum amount of connections to keep open & idle to the database in the pool
pub const MIN_DB_POOL_SIZE: u32 = 5;
