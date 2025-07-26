use crate::ConnectionExt;
use crate::MlsProviderExt;
use crate::sql_key_store::SqlKeyStoreError;
// use crate::sql_key_store::XmtpMlsTransactionProvider;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use openmls_traits::storage::CURRENT_VERSION;
use openmls_traits::storage::{Entity, StorageProvider};

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
    type Connection: ConnectionExt;

    type DbQuery<'a>: crate::DbQuery<&'a Self::Connection>
    where
        Self::Connection: 'a;

    fn db<'a>(&'a self) -> Self::DbQuery<'a>;

    fn transaction<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut <Self::Connection as ConnectionExt>::Connection) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error;

    fn _disable_lint_for_self<'a>(_: Self::DbQuery<'a>) {}

    fn read<V: Entity<CURRENT_VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Option<V>, SqlKeyStoreError>;

    fn read_list<V: Entity<CURRENT_VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error>;

    fn delete(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error>;

    fn write(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error>;
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
    type XmtpStorage = S;

    fn key_store(&self) -> &Self::XmtpStorage {
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
    type XmtpStorage = S;

    fn key_store(&self) -> &Self::XmtpStorage {
        self.mls_storage
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
        self.mls_storage
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
    type XmtpStorage = S;

    fn key_store(&self) -> &Self::XmtpStorage {
        self.mls_storage
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
        self.mls_storage
    }
}
