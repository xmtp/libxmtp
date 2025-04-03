#[allow(unused)]
pub const MAX_DB_POOL_SIZE: u32 = 25;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
#[cfg(target_arch = "wasm32")]
mod wasm {
    // Changing these values is a breaking change, unless a migration path is specified
    pub const VFS_NAME: &str = "opfs-libxmtp";
    pub const VFS_DIRECTORY: &str = ".opfs-libxmtp-metadata";
}
