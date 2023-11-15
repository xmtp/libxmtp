use std::cell::RefCell;

use diesel::Connection;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::{sql_key_store::SqlKeyStore, DbConnection};

#[derive(Debug)]
pub struct XmtpOpenMlsProvider<'a> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<'a>,
}

impl<'a> XmtpOpenMlsProvider<'a> {
    pub fn new(conn: &'a mut DbConnection) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(conn),
        }
    }

    pub(crate) fn conn(&self) -> &RefCell<&'a mut DbConnection> {
        self.key_store.conn()
    }

    /// Start a new database transaction with the OpenMLS Provider from XMTP
    /// # Arguments
    /// `fun`: Scoped closure providing a MLSProvider to carry out the transaction
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let connection = EncryptedMessageStore::new_unencrypted(StorageOptions::default());
    /// XmtpOpenMlsProvider::transaction(conn, |provider| {
    ///     // do some operations requiring provider
    ///     // access the connection with .conn()
    ///     provider.conn().borrow_mut()
    /// })
    /// ```
    pub fn transaction<T, F, E>(connection: &mut DbConnection, fun: F) -> Result<T, E>
    where
        F: FnOnce(XmtpOpenMlsProvider) -> Result<T, E>,
        E: From<diesel::result::Error>,
    {
        connection.transaction(|conn| {
            let provider = XmtpOpenMlsProvider::new(conn);
            fun(provider)
        })
    }
}

impl<'a> OpenMlsProvider for XmtpOpenMlsProvider<'a> {
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
