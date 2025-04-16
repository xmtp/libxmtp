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

use crate::StorageError;

use super::{ConnectionError, ConnectionExt, TransactionGuard};

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod wasm_exports {
    pub type RawDbConnection = diesel::prelude::SqliteConnection;
    pub type DefaultDatabase = super::wasm::WasmDb;

    pub(super) type DefaultConnectionInner =
        super::PersistentOrMem<super::wasm::WasmDbConnection, super::wasm::WasmDbConnection>;

    pub type DefaultConnection = std::sync::Arc<DefaultConnectionInner>;
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub mod native_exports {
    pub type DefaultDatabase = super::native::NativeDb;
    pub use super::native::EncryptedConnection;
    /// The 'inner' default connection
    /// 'DefaultConnection' is preferred here
    pub(super) type DefaultConnectionInner = super::PersistentOrMem<
        super::native::NativeDbConnection,
        super::native::EphemeralDbConnection,
    >;

    // the native module already defines this
    // pub type RawDbConnection = native::RawDbConnection;
    pub type DefaultConnection = std::sync::Arc<DefaultConnectionInner>;
}

#[derive(Debug)]
pub enum PersistentOrMem<P, M> {
    Persistent(P),
    Mem(M),
}

impl<P, M> ConnectionExt for PersistentOrMem<P, M>
where
    P: ConnectionExt,
    M: ConnectionExt<Connection = P::Connection>,
{
    type Connection = P::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        match self {
            Self::Persistent(p) => p.start_transaction(),
            Self::Mem(m) => m.start_transaction(),
        }
    }

    fn raw_query_read<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<ConnectionError>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_read(fun),
            Self::Mem(m) => m.raw_query_read(fun),
        }
    }

    fn raw_query_write<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<ConnectionError>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_write(fun),
            Self::Mem(m) => m.raw_query_write(fun),
        }
    }
}
