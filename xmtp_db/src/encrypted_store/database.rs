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

use super::{ConnectionExt, TransactionGuard};

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

#[derive(Debug)]
pub enum PersistentOrMem<P, M> {
    Persistent(P),
    Mem(M),
}

// P and M must share connection & error types
impl<P, M> ConnectionExt for PersistentOrMem<P, M>
where
    P: ConnectionExt,
    M: ConnectionExt<Connection = P::Connection>,
{
    type Connection = P::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard, crate::ConnectionError> {
        match self {
            Self::Persistent(p) => p.start_transaction(),
            Self::Mem(m) => m.start_transaction(),
        }
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_read(fun),
            Self::Mem(m) => m.raw_query_read(fun),
        }
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_write(fun),
            Self::Mem(m) => m.raw_query_write(fun),
        }
    }

    fn is_in_transaction(&self) -> bool {
        match self {
            Self::Persistent(p) => p.is_in_transaction(),
            Self::Mem(m) => m.is_in_transaction(),
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
