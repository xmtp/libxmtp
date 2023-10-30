use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::{sql_key_store::SqlKeyStore, EncryptedMessageStore};

#[derive(Debug)]
pub struct XmtpOpenMlsProvider<'a> {
    crypto: RustCrypto,
    key_store: SqlKeyStore<'a>,
}

impl<'a> XmtpOpenMlsProvider<'a> {
    pub fn new(store: &'a EncryptedMessageStore) -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: SqlKeyStore::new(store),
        }
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
