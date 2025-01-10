use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::{db_connection::DbConnectionPrivate, sql_key_store::SqlKeyStore};
use std::sync::Arc;

pub type XmtpOpenMlsProvider = XmtpOpenMlsProviderPrivate<crate::storage::RawDbConnection>;

#[derive(Debug)]
pub struct XmtpOpenMlsProviderPrivate<C> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<C>,
}

impl<C> XmtpOpenMlsProviderPrivate<C> {
    pub fn new(conn: DbConnectionPrivate<C>) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
        }
    }

    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }

    pub fn conn_ref(&self) -> &DbConnectionPrivate<C> {
        self.key_store.conn_ref()
    }
}

impl<C> OpenMlsProvider for XmtpOpenMlsProviderPrivate<C>
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

impl<C> OpenMlsProvider for &XmtpOpenMlsProviderPrivate<C>
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
