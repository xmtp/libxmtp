use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::{db_connection::DbConnectionPrivate, sql_key_store::SqlKeyStore};

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

    pub(crate) fn conn_ref(&self) -> &DbConnectionPrivate<C> {
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
