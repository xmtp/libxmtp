#[cfg(feature = "sync")]
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

/// Async-track helper bundling "an async closure over `&mut TxQ` whose returned
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
#[cfg(all(feature = "async", not(feature = "sync")))]
pub trait TxFn<TxQ, T, E>: MaybeSend {
    fn run(
        self,
        tx: &mut TxQ,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend;
}

#[cfg(all(feature = "async", not(feature = "sync")))]
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
    // On the sync track the connection is a diesel `ConnectionExt`; on the async
    // track (sqlx/Postgres) there is no `ConnectionExt`, so the bound is dropped.
    #[cfg(feature = "sync")]
    type Connection: ConnectionExt;
    #[cfg(not(feature = "sync"))]
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
    // The sync track rolls back via diesel's `RollbackTransaction` sentinel, so
    // the caller's error must be `From<diesel::result::Error>`. The async track
    // drives a real sqlx transaction and needs no such bound.
    // `async fn` on both tracks: the async (sqlx) impl genuinely awaits; the sync
    // (SQLite) impl has a synchronous body under this async signature, returning a
    // ready future. Callers `.await` on both tracks — consistent with the `Query*`
    // traits, and no thread-hopping bridge on the async side.
    // Returns `impl Future + MaybeSend` rather than being a bare `async fn`:
    // async-fn-in-trait does NOT imply the returned future is `Send`, and these
    // futures flow into spawned stream/worker tasks that require `Send`. This is
    // the same shape the `Query*`/`Store`/`Fetch` traits use.
    #[cfg(feature = "sync")]
    fn transaction<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend
    where
        T: MaybeSend,
        // Boxed closure (not `AsyncFnOnce`) so its returned future can be named
        // and bounded `Send` on stable Rust: the async track awaits it inside the
        // (Send) transaction future. `AsyncFnOnce::CallOnceFuture` is unnameable
        // on stable, so callers pass `|conn| Box::pin(async move { .. })`.
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E> + MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error;
    #[cfg(not(feature = "sync"))]
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
    #[cfg(feature = "sync")]
    fn savepoint<T, E, F>(
        &self,
        f: F,
    ) -> impl std::future::Future<Output = Result<TransactionOutcome<T>, E>> + MaybeSend
    where
        T: MaybeSend,
        // Boxed closure (not `AsyncFnOnce`) so its returned future can be named
        // and bounded `Send` on stable Rust: the async track awaits it inside the
        // (Send) transaction future. `AsyncFnOnce::CallOnceFuture` is unnameable
        // on stable, so callers pass `|conn| Box::pin(async move { .. })`.
        F: AsyncFnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E> + MaybeSend,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error;
    #[cfg(not(feature = "sync"))]
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

    // Generic byte KV accessors (key-package references, the commit-log signer
    // key, ...). Maybe-async: the sync (SQLite) impl bodies are synchronous under
    // an `async fn` (ready futures), while the async (Postgres) impl awaits real
    // sqlx against `openmls_key_value`. The `+ MaybeSend` future flows through
    // libxmtp's Send worker tasks, so it must be Send on the async track.
    fn read<V: Entity<CURRENT_VERSION> + MaybeSend>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<V>, SqlKeyStoreError>> + MaybeSend;

    fn read_list<V: Entity<CURRENT_VERSION> + MaybeSend>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> impl std::future::Future<
        Output = Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error>,
    > + MaybeSend;

    fn delete(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> impl std::future::Future<
        Output = Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error>,
    > + MaybeSend;

    fn write(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> impl std::future::Future<
        Output = Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error>,
    > + MaybeSend;

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
