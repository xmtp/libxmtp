#[cfg(feature = "sqlite")]
use crate::ConnectionExt;
use crate::MlsProviderExt;
use crate::SqlKeyStoreError;
use crate::TransactionalKeyStore;
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

/// Postgres-backend helper bundling "an async closure over `&mut TxQ` whose returned
/// future is `Send`" into one method-friendly trait bound.
///
/// The async `transaction`/`savepoint` impls await the closure's future *inside*
/// their own `Send` future, so that future must be `Send`. Naming it to bound it
/// (`<F as AsyncFnOnce<_>>::CallOnceFuture: Send`) needs nightly (`async_fn_traits`,
/// `unboxed_closures`), AND a *concrete* impl cannot carry a
/// `for<'a> …::CallOnceFuture: Send` predicate on a **method** without tripping
/// E0276 (a projection under the `for<'a>` binder is not normalized). So the HRTB
/// lives here, on a blanket impl over a **generic** closure `F`, where it
/// normalizes cleanly; the provider method then bounds only
/// `F: TxFn<Self::TxQuery, T, E>` — a plain bound a concrete impl can satisfy.
#[cfg(all(feature = "sqlx", not(feature = "sqlite")))]
pub trait TxFn<TxQ, T, E>: MaybeSend {
    fn run(
        self,
        tx: &mut TxQ,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend;
}

#[cfg(all(feature = "sqlx", not(feature = "sqlite")))]
impl<TxQ, T, E, F> TxFn<TxQ, T, E> for F
where
    F: MaybeSend
        + for<'a> AsyncFnOnce<
            (&'a mut TxQ,),
            Output = Result<TransactionOutcome<T>, E>,
            CallOnceFuture: MaybeSend,
        >,
{
    fn run(
        self,
        tx: &mut TxQ,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend {
        self(tx)
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
    // On the SQLite backend the connection is a diesel `ConnectionExt`; on the
    // Postgres backend (sqlx) there is no `ConnectionExt`, so the bound is dropped.
    #[cfg(feature = "sqlite")]
    type Connection: ConnectionExt;
    #[cfg(not(feature = "sqlite"))]
    type Connection;

    // `MaybeSend` so a `&mut TxQuery` can be captured by the (Send) boxed
    // transaction/savepoint closure future for spawned stream/worker tasks.
    type TxQuery: TransactionalKeyStore + MaybeSend;

    type DbQuery<'a>: crate::DbQuery
    where
        Self::Connection: 'a;

    fn db<'a>(&'a self) -> Self::DbQuery<'a>;

    /// Start a new transaction.
    ///
    /// The closure returns `Ok(TransactionOutcome::Continue(v))` to persist or
    /// `Ok(TransactionOutcome::Rollback)` to roll back without an error.
    /// Returning `Err(e)` also rolls back and propagates `e` as a real error.
    // The SQLite backend rolls back via diesel's `RollbackTransaction` sentinel, so
    // the caller's error must be `From<diesel::result::Error>`. The Postgres backend
    // drives a real sqlx transaction and needs no such bound.
    // `async fn` on both backends: the sqlx impl genuinely awaits; the SQLite impl
    // has a synchronous body under this async signature, returning an already-ready
    // future. Callers `.await` on both — consistent with the `Query*` traits, and no
    // thread-hopping bridge on the Postgres side.
    // Returns `impl Future + MaybeSend` rather than being a bare `async fn`:
    // async-fn-in-trait does NOT imply the returned future is `Send`, and these
    // futures flow into spawned stream/worker tasks that require `Send`. This is
    // the same shape the `Query*`/`Store`/`Fetch` traits use.
    #[cfg(feature = "sqlite")]
    fn transaction<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend
    where
        T: MaybeSend,
        // Plain `AsyncFnOnce` sugar, with no named `CallOnceFuture` bound: on the
        // SQLite backend the body runs synchronously and is driven to completion in
        // one poll, so it never suspends and needs no Send-across-await bound — it
        // compiles on stable. (The Postgres variant below does need that bound.)
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E> + MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error;
    #[cfg(not(feature = "sqlite"))]
    fn transaction<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend
    where
        T: MaybeSend,
        // `f`'s returned future is awaited inside this (Send) future, so it must
        // be Send. That HRTB `CallOnceFuture: Send` bound lives on `TxFn`'s
        // blanket impl (over a generic closure); here the method carries only the
        // plain `TxFn` bound, which a concrete impl can satisfy without E0276.
        // The `AsyncFnOnce` sugar pins the closure's `conn` param to
        // `Self::TxQuery` so callers' `async |conn| …` infer it (the bare `TxFn`
        // bound alone leaves `conn` ambiguous); `TxFn` provides the Send `run()`.
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + TxFn<Self::TxQuery, T, E>,
        E: From<crate::ConnectionError> + std::error::Error + MaybeSend;

    /// Start a savepoint within a transaction.
    ///
    /// Must only be used when already in a transaction.
    // TODO: enforce that this is only used within transactions
    // otherwise we run into sqlite race conditions b/c this does not
    // use BEGIN IMMEDIATE.
    // we can ensure this by checking sqlite transaction depth.
    #[cfg(feature = "sqlite")]
    fn savepoint<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend
    where
        T: MaybeSend,
        // Plain `AsyncFnOnce` sugar, with no named `CallOnceFuture` bound: on the
        // SQLite backend the body runs synchronously and is driven to completion in
        // one poll, so it never suspends and needs no Send-across-await bound — it
        // compiles on stable. (The Postgres variant below does need that bound.)
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E> + MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error;
    #[cfg(not(feature = "sqlite"))]
    fn savepoint<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend
    where
        T: MaybeSend,
        // Same as `transaction`: the plain `TxFn` bound; the `CallOnceFuture: Send`
        // HRTB lives on `TxFn`'s blanket impl.
        // The `AsyncFnOnce` sugar pins the closure's `conn` param to
        // `Self::TxQuery` so callers' `async |conn| …` infer it (the bare `TxFn`
        // bound alone leaves `conn` ambiguous); `TxFn` provides the Send `run()`.
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>
            + TxFn<Self::TxQuery, T, E>,
        E: From<crate::ConnectionError> + std::error::Error + MaybeSend;

    fn _disable_lint_for_self<'a>(_: Self::DbQuery<'a>) {}

    // Typed accessors for libxmtp's own stored values. These REPLACE a former
    // generic byte-KV (`read`/`write`/`delete` over a `(label, key)` pair): the
    // caller set is closed and compile-time known, so each value gets a named
    // operation, and the storage LAYOUT is the backend's business — the SQLite
    // impl keeps its `openmls_key_value` KV (encoding the key with a label
    // internally), while the Postgres impl hits a purpose-built table. No label
    // reaches a caller, so an unhandled value is a compile error, not a runtime
    // fallback that could strand data.

    /// Store a group's commit-log signer key. `signer_key` is stored verbatim.
    fn set_commit_log_signer_key(
        &self,
        group_id: &[u8],
        signer_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + MaybeSend;

    /// Load a group's commit-log signer key, if present.
    fn commit_log_signer_key<V: Entity<CURRENT_VERSION> + MaybeSend>(
        &self,
        group_id: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + MaybeSend;

    /// Store a key-package reference (`public_key -> hash_ref`, stored verbatim).
    /// Keyed by the TLS-serialized init key or the post-quantum public key.
    fn set_key_package_reference(
        &self,
        public_key: &[u8],
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + MaybeSend;

    /// Load the key-package reference for a public key, if present.
    fn key_package_reference<V: Entity<CURRENT_VERSION> + MaybeSend>(
        &self,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + MaybeSend;

    /// Delete the key-package reference for a public key.
    fn delete_key_package_reference(
        &self,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + MaybeSend;

    /// Store a key-package wrapper private key (`hash_ref -> private_key`).
    fn set_key_package_wrapper_key(
        &self,
        hash_ref: &[u8],
        private_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + MaybeSend;

    /// Load the wrapper private key for a key-package hash ref, if present.
    fn key_package_wrapper_key<V: Entity<CURRENT_VERSION> + MaybeSend>(
        &self,
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + MaybeSend;

    /// Delete the wrapper private key for a key-package hash ref.
    fn delete_key_package_wrapper_key(
        &self,
        hash_ref: &[u8],
    ) -> impl std::future::Future<Output = Result<(), SqlKeyStoreError>> + MaybeSend;

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
