#[cfg(not(target_arch = "wasm32"))]
use crate::storage::native::NativeDb;
#[cfg(target_arch = "wasm32")]
use crate::storage::wasm::WasmDb;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use std::marker::PhantomData;

use crate::storage::{db_connection::DbConnectionPrivate, sql_key_store::SqlKeyStore};
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
pub type XmtpOpenMlsProvider = XmtpOpenMlsProviderPrivate<WasmDb, crate::storage::RawDbConnection>;
#[cfg(not(target_arch = "wasm32"))]
pub type XmtpOpenMlsProvider =
    XmtpOpenMlsProviderPrivate<NativeDb, crate::storage::RawDbConnection>;

#[derive(Debug)]
pub struct XmtpOpenMlsProviderPrivate<Db, C> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<C>,
    _phantom: PhantomData<Db>,
}

impl<Db, C> XmtpOpenMlsProviderPrivate<Db, C> {
    pub fn new(conn: DbConnectionPrivate<C>) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
            _phantom: PhantomData,
        }
    }

    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }

    pub fn conn_ref(&self) -> &DbConnectionPrivate<C> {
        self.key_store.conn_ref()
    }
}

impl<Db, C> OpenMlsProvider for XmtpOpenMlsProviderPrivate<Db, C>
where
    C: diesel::Connection<Backend = crate::storage::Sqlite> + diesel::connection::LoadConnection,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqlKeyStore<C>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.key_store
    }
}

impl<Db, C> OpenMlsProvider for &XmtpOpenMlsProviderPrivate<Db, C>
where
    C: diesel::Connection<Backend = crate::storage::Sqlite> + diesel::connection::LoadConnection,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqlKeyStore<C>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.key_store
    }
}

/// This trait would be useful elsewhere too,
/// but it would be a large refactor. For now, it is used in `ProcessMessageFuture`
/// to accept either a Reference or Owned type.
/// `as_ref` is a hack to convert back to a concrete type.
/// In the future, we can replace function arguments with generics for MlsProviderExt

pub trait MlsProviderExt: OpenMlsProvider {
    type DbConnection;
    fn conn_ref(&self) -> &Self::DbConnection;
}

impl<C> MlsProviderExt for XmtpOpenMlsProviderPrivate<C> {
    type DbConnection = DbConnectionPrivate<C>;
    fn conn_ref(&self) -> &Self::DbConnection {
        XmtpOpenMlsProviderPrivate::<C>::conn_ref(self)
    }
}

impl<T> MlsProviderExt for &T where T: MlsProviderExt + OpenMlsProvider {
    type DbConnection = <T as MlsProviderExt>::DbConnection;

    fn conn_ref(&self) ->  &Self::DbConnection {
        T::conn_ref(&*self)
    }
}

impl<T> MlsProviderExt for Arc<T> where T: MlsProviderExt + OpenMlsProvider {
    type DbConnection = <T as MlsProviderExt>::DbConnection;

    fn conn_ref(&self) -> &Self::DbConnection {
        T::conn_ref(&*self)
    }
}
