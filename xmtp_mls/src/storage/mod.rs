pub(super) mod encrypted_store;
mod errors;
pub mod serialization;
pub mod sql_key_store;

pub use encrypted_store::*;
pub use errors::*;

/// Initialize the SQLite WebAssembly Library
#[cfg(target_arch = "wasm32")]
pub async fn init_sqlite() {
    diesel_wasm_sqlite::init_sqlite().await;
}
#[cfg(not(target_arch = "wasm32"))]
pub async fn init_sqlite() {}
