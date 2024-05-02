use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::{db_connection::DbConnection, sql_key_store::SqlKeyStore};

#[derive(Debug)]
pub struct XmtpOpenMlsProvider<'a> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<'a>,
}

impl<'a> XmtpOpenMlsProvider<'a> {
    pub fn new(conn: &'a DbConnection<'a>) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
        }
    }

    pub(crate) fn conn(&self) -> &DbConnection<'a> {
        self.key_store.conn()
    }
}

impl<'a> OpenMlsProvider for XmtpOpenMlsProvider<'a> {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = SqlKeyStore<'a>;

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
