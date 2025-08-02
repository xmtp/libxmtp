#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native;

use diesel::SqliteConnection;
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

use super::ConnectionExt;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm_exports {
    pub type RawDbConnection = diesel::prelude::SqliteConnection;
    pub type DefaultDatabase = super::wasm::WasmDb;
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native_exports {
    pub type DefaultDatabase = super::native::NativeDb;
    pub use super::native::EncryptedConnection;
}

mod instrumentation;

#[derive(Debug)]
pub enum PersistentOrMem<P, M> {
    Persistent(P),
    Mem(M),
}

// P and M must share connection & error types
impl<P, M> ConnectionExt for PersistentOrMem<P, M>
where
    P: ConnectionExt,
    M: ConnectionExt,
{
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_read(fun),
            Self::Mem(m) => m.raw_query_read(fun),
        }
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_write(fun),
            Self::Mem(m) => m.raw_query_write(fun),
        }
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        match self {
            Self::Persistent(p) => p.disconnect(),
            Self::Mem(m) => m.disconnect(),
        }
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        match self {
            Self::Persistent(p) => p.reconnect(),
            Self::Mem(m) => m.reconnect(),
        }
    }
}
