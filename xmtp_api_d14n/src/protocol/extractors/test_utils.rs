mod envelope_builder;
pub use envelope_builder::*;

use openmls::prelude::OpenMlsProvider;
use openmls_rust_crypto::{MemoryStorage, RustCrypto};

#[derive(Clone, Default)]
pub struct MemProvider {
    storage: MemoryStorage,
    crypto: RustCrypto,
}

impl OpenMlsProvider for MemProvider {
    type CryptoProvider = RustCrypto;

    type RandProvider = RustCrypto;

    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}
