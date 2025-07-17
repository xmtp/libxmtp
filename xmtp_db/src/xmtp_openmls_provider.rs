use crate::ConnectionExt;
use crate::MlsProviderExt;
use crate::sql_key_store::SqlKeyStoreError;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use openmls_traits::storage::CURRENT_VERSION;
use openmls_traits::storage::StorageProvider;

/// Convenience super trait to constrain the storage provider to a
/// specific error type and version
/// This storage provider is likewise implemented on both &T and T references,
/// to allow creating a referenced or owned provider.
// constraining the error type here will avoid leaking
// the associated type parameter, so we don't need to define it on every function.
pub trait XmtpMlsStorageProvider:
    StorageProvider<CURRENT_VERSION, Error = SqlKeyStoreError>
{
    /// An Opaque Database connection type. Can be anything.
    type Connection;
    type Storage<'a>
    where
        Self::Connection: 'a;
    fn conn(&self) -> &Self::Connection;

    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        for<'a> F: FnOnce(Self::Storage<'a>) -> Result<T, diesel::result::Error>,
        for<'a> Self::Connection: 'a,
        E: From<crate::ConnectionError> + std::error::Error;
}

pub struct XmtpOpenMlsProvider<S> {
    crypto: RustCrypto,
    mls_storage: S,
}

impl<S> XmtpOpenMlsProvider<S> {
    pub fn new(mls_storage: S) -> Self {
        Self {
            crypto: RustCrypto::default(),
            mls_storage,
        }
    }
}

impl<S> XmtpOpenMlsProvider<S> {
    pub fn new_crypto() -> RustCrypto {
        RustCrypto::default()
    }
}

impl<S> MlsProviderExt for XmtpOpenMlsProvider<S>
where
    S: XmtpMlsStorageProvider,
    <S as XmtpMlsStorageProvider>::Connection: ConnectionExt,
{
    type Storage = S;

    fn key_store(&self) -> &Self::Storage {
        &self.mls_storage
    }
}

impl<S> OpenMlsProvider for XmtpOpenMlsProvider<S>
where
    S: XmtpMlsStorageProvider,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = S;
    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.mls_storage
    }
}

pub struct XmtpOpenMlsProviderRef<'a, S> {
    crypto: RustCrypto,
    mls_storage: &'a S,
}

impl<'a, S> MlsProviderExt for XmtpOpenMlsProviderRef<'a, S>
where
    S: XmtpMlsStorageProvider,
    <S as XmtpMlsStorageProvider>::Connection: ConnectionExt,
{
    type Storage = S;

    fn key_store(&self) -> &Self::Storage {
        &self.mls_storage
    }
}

impl<'a, S> OpenMlsProvider for XmtpOpenMlsProviderRef<'a, S>
where
    S: XmtpMlsStorageProvider,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = S;
    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.mls_storage
    }
}

impl<'a, S> XmtpOpenMlsProviderRef<'a, S> {
    pub fn new(mls_storage: &'a S) -> Self {
        Self {
            crypto: RustCrypto::default(),
            mls_storage,
        }
    }
}

pub struct XmtpOpenMlsProviderRefMut<'a, S> {
    crypto: RustCrypto,
    mls_storage: &'a mut S,
}

impl<'a, S> XmtpOpenMlsProviderRefMut<'a, S> {
    pub fn new(mls_storage: &'a mut S) -> Self {
        Self {
            crypto: RustCrypto::default(),
            mls_storage,
        }
    }
}

impl<'a, S> MlsProviderExt for XmtpOpenMlsProviderRefMut<'a, S>
where
    S: XmtpMlsStorageProvider,
    <S as XmtpMlsStorageProvider>::Connection: ConnectionExt,
{
    type Storage = S;

    fn key_store(&self) -> &Self::Storage {
        &self.mls_storage
    }
}

impl<'a, S> OpenMlsProvider for XmtpOpenMlsProviderRefMut<'a, S>
where
    S: XmtpMlsStorageProvider,
{
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = S;
    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn storage(&self) -> &Self::StorageProvider {
        &self.mls_storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls_memory_storage::MemoryStorage;
    /*
        fn create_provider_with_reference() {
            let storage = MemoryStorage::default();
            XmtpOpenMlsProvider::new(storage)
        }
    */
}
