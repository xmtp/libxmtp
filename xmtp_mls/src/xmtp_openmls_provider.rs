use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;

use crate::storage::in_memory_key_store::InMemoryKeyStore;

#[derive(Default, Debug)]
pub struct XmtpOpenMlsProvider {
    crypto: RustCrypto,
    key_store: InMemoryKeyStore,
}

impl OpenMlsProvider for XmtpOpenMlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type KeyStoreProvider = InMemoryKeyStore;

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
