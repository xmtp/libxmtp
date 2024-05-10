use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::{db_connection::DbConnection, sql_key_store::SqlKeyStore};

#[derive(Debug)]
pub struct XmtpOpenMlsProvider {
    crypto: RustCrypto,
    key_store: SqlKeyStore<'static>,
}

pub struct XmtpOpenMlsProviderRef<'a> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<'a>,
}

impl<'a> XmtpOpenMlsProviderRef<'a> {
    pub fn new(conn: &'a DbConnection) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::with_ref(conn),
        }
    }

    pub(crate) fn conn(&'a self) -> &'a DbConnection {
        &self.key_store.conn()
    }
}

/*
impl Clone for XmtpOpenMlsProvider {
    fn clone(&self) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: self.key_store.clone(),
        }
    }
}
*/

impl XmtpOpenMlsProvider {
    pub fn new(conn: DbConnection) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
        }
    }

    pub(crate) fn conn(&self) -> DbConnection {
        self.key_store.conn()
    }

    //TODO:insipx prob a better way to accomplish this
    pub(crate) fn conn_ref(&self) -> &DbConnection {
        self.key_store.conn_ref()
    }
}

impl OpenMlsProvider for XmtpOpenMlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type KeyStoreProvider = SqlKeyStore<'static>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn key_store(&self) -> &Self::KeyStoreProvider {
        &self.key_store
    }
}

impl<'a> OpenMlsProvider for &'a XmtpOpenMlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type KeyStoreProvider = SqlKeyStore<'static>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn key_store(&self) -> &Self::KeyStoreProvider {
        &self.key_store
    }
}

impl<'a> OpenMlsProvider for XmtpOpenMlsProviderRef<'a> {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type KeyStoreProvider = SqlKeyStore<'a>;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn key_store(&self) -> &Self::KeyStoreProvider {
        &self.key_store
    }
}

impl<'a, 'b: 'a> OpenMlsProvider for &'a XmtpOpenMlsProviderRef<'b> {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type KeyStoreProvider = SqlKeyStore<'a>;

    fn crypto(&self) -> &'a Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &'a Self::RandProvider {
        &self.crypto
    }

    fn key_store(&self) -> &'a Self::KeyStoreProvider {
        &self.key_store
    }
}
