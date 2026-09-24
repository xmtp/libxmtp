use crate::ConnectionExt;
use crate::MlsProviderExt;
use crate::TransactionalKeyStore;
use crate::sql_key_store::SqlKeyStoreError;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use openmls_traits::storage::CURRENT_VERSION;
use openmls_traits::storage::{Entity, StorageProvider};
use xmtp_common::{MaybeSend, MaybeSync};

/// Outcome of a [`XmtpMlsStorageProvider::transaction`] or
/// [`XmtpMlsStorageProvider::savepoint`] closure.
///
/// Returning `Ok(TransactionOutcome::Continue(value))` persists the transaction
/// and returns `Ok(value)` to the caller.
///
/// Returning `Ok(TransactionOutcome::Rollback)` rolls back the transaction
/// *without* recording a span error — the rollback was intentional.
///
/// Returning `Err(e)` rolls back the transaction *and* records `status=error`
/// on the enclosing `#[db_span]` / `#[rpc_span]` span — the error was real.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionOutcome<T> {
    /// Persist the transaction and return the enclosed value.
    Continue(T),
    /// Roll back the transaction without treating it as an error.
    Rollback,
}

impl<T> TransactionOutcome<T> {
    /// Unwrap the persisted value for call sites that never roll back.
    ///
    /// Panics if this is a `Rollback` (a bug at that call site).
    pub fn into_continued(self) -> T {
        match self {
            TransactionOutcome::Continue(v) => v,
            TransactionOutcome::Rollback => {
                unreachable!("transaction caller never returns TransactionOutcome::Rollback")
            }
        }
    }
}

/// Convenience super trait to constrain the storage provider to a
/// specific error type and version
/// This storage provider is likewise implemented on both &T and T references,
/// to allow creating a referenced or owned provider.
// constraining the error type here will avoid leaking
// the associated type parameter, so we don't need to define it on every function.
pub trait XmtpMlsStorageProvider:
    MaybeSend + MaybeSync + StorageProvider<CURRENT_VERSION, Error = SqlKeyStoreError>
{
    /// An Opaque Database connection type. Can be anything.
    type Connection: ConnectionExt;

    type TxQuery: TransactionalKeyStore;

    type DbQuery<'a>: crate::DbQuery
    where
        Self::Connection: 'a;

    fn db<'a>(&'a self) -> Self::DbQuery<'a>;

    /// Start a new transaction.
    ///
    /// The closure returns `Ok(TransactionOutcome::Continue(v))` to persist or
    /// `Ok(TransactionOutcome::Rollback)` to roll back without an error.
    /// Returning `Err(e)` also rolls back and propagates `e` as a real error.
    fn transaction<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error;

    /// Start a savepoint within a transaction.
    ///
    /// Must only be used when already in a transaction.
    // TODO: enforce that this is only used within transactions
    // otherwise we run into sqlite race conditions b/c this does not
    // use BEGIN IMMEDIATE.
    // we can ensure this by checking sqlite transaction depth.
    fn savepoint<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>,
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

    #[cfg(feature = "test-utils")]
    fn hash_all(&self) -> Result<Vec<u8>, SqlKeyStoreError>;
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
