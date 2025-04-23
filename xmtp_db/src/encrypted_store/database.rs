#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use native::*;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use wasm::*;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use wasm_exports::*;

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use native_exports::*;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm_exports {
    pub type RawDbConnection = diesel::prelude::SqliteConnection;
    pub type DefaultDatabase = super::wasm::WasmDb;
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native_exports {
    pub type DefaultDatabase = super::native::NativeDb;
    pub use super::native::EncryptedConnection;
    // the native module already defines this
    // pub type RawDbConnection = native::RawDbConnection;
}
