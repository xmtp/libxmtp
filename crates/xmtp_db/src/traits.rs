#[cfg(feature = "sqlite")]
use crate::ConnectionExt;
use crate::StorageError;
use crate::association_state::QueryAssociationStateCache;
use crate::d14n_migration_cutover::QueryMigrationCutover;
use crate::icebox::QueryIcebox;
use crate::message_deletion::QueryMessageDeletion;
use crate::pending_remove::QueryPendingRemove;
use crate::prelude::*;
use crate::readd_status::QueryReaddStatus;
use xmtp_common::{MaybeSend, MaybeSync};

/// Get an MLS Key store in the context of a transaction
/// this must only be used within transactions.
// The trait is backend-agnostic; the automock's `Store` names the diesel-backed
// `MockSqlKeyStore` (`#[maybe_async]`), so mock generation is gated to the
// `sqlite` backend.
#[cfg_attr(all(feature = "sqlite", any(feature = "test-utils", test)), mockall::automock(type Store = crate::sql_key_store::mock::MockSqlKeyStore;))]
pub trait TransactionalKeyStore {
    type Store<'a>: XmtpMlsStorageProvider
    where
        Self: 'a;

    fn key_store<'a>(&'a mut self) -> Self::Store<'a>;
}

// `Store`/`StoreOrIgnore`/`Fetch` use the same hand-written AFIT shape as the
// `Query*` traits (def returns `impl Future + MaybeSend`, impls are `async fn`):
// the diesel (SQLite) impls have a synchronous body and so return an already-ready
// future, while the sqlx/Postgres impls genuinely await. Callers `.await` on both.
/// Inserts a model to the underlying data store, erroring if it already exists
pub trait Store<StorageConnection> {
    type Output;
    fn store(
        &self,
        into: &StorageConnection,
    ) -> impl std::future::Future<Output = Result<Self::Output, StorageError>> + MaybeSend;
}

/// Inserts a model to the underlying data store, silent no-op on unique constraint violations
pub trait StoreOrIgnore<StorageConnection> {
    type Output;
    fn store_or_ignore(
        &self,
        into: &StorageConnection,
    ) -> impl std::future::Future<Output = Result<Self::Output, StorageError>> + MaybeSend;
}

/// Fetches a model from the underlying data store, returning None if it does not exist
pub trait Fetch<Model> {
    type Key;
    fn fetch(
        &self,
        key: &Self::Key,
    ) -> impl std::future::Future<Output = Result<Option<Model>, StorageError>> + MaybeSend;
}

/// Fetches all instances of `Model` from the data store.
/// Returns an empty list if no items are found or an error if the fetch fails.
pub trait FetchList<Model> {
    fn fetch_list(&self) -> Result<Vec<Model>, StorageError>;
}

/// Fetches a filtered list of `Model` instances matching the specified key.
/// Logs an error and returns an empty list if no items are found or if an error occurs.
///
/// # Parameters
/// - `key`: The key used to filter the items in the data store.
pub trait FetchListWithKey<Model> {
    type Key;
    fn fetch_list_with_key(&self, keys: &[Self::Key]) -> Result<Vec<Model>, StorageError>;
}

/// Deletes a model from the underlying data store
pub trait Delete<Model> {
    type Key;
    fn delete(&self, key: Self::Key) -> Result<usize, StorageError>;
}

#[cfg(feature = "sqlite")]
pub trait IntoConnection {
    type Connection: ConnectionExt;
    fn into_connection(self) -> Self::Connection;
}

/// The parts of `DbQuery` that only one storage backend can provide.
///
/// `ReadOnly` (`PRAGMA query_only`), `Pragmas` (`busy_timeout`, `cipher_log_level`)
/// and `ConnectionExt` (`raw_query` over a `&mut SqliteConnection`) are all
/// SQLite-shaped, so they are supertraits of `DbQuery` on the `sqlite` backend
/// only. On the Postgres backend this collapses to an empty blanket-implemented trait,
/// which keeps the `DbQuery` list below single-copy instead of cfg-forking it.
#[cfg(feature = "sqlite")]
pub trait BackendSpecific: ReadOnly + Pragmas + crate::ConnectionExt {}
#[cfg(feature = "sqlite")]
impl<T: ?Sized> BackendSpecific for T where T: ReadOnly + Pragmas + crate::ConnectionExt {}

/// The Postgres backend's analog of `ConnectionExt`: hands out a pooled (or
/// transaction-pinned) `PgConn` to run sqlx against.
///
/// It is a supertrait of `DbQuery` (via `BackendSpecific`), so the generic
/// `Store`/`Fetch`/`StoreOrIgnore` impls — written for any
/// `C: PgConnectionProvider` — are reachable through a `&impl DbQuery`, exactly
/// as the diesel macros reach theirs through `C: ConnectionExt`. Without this the
/// impls would be concrete on `PgDb` and invisible to generic call sites, whose
/// connection is the opaque `<Ctx::Db as XmtpDb>::DbQuery` associated type.
#[cfg(all(feature = "sqlx", not(feature = "sqlite")))]
pub trait PgConnectionProvider: MaybeSend + MaybeSync {
    fn pg_conn(
        &self,
    ) -> impl std::future::Future<Output = Result<crate::pg::PgConn<'_>, crate::ConnectionError>>
    + MaybeSend;
}

// Mirror the `Query*` impls, which cover both `PgDb` and `&PgDb`: a reference to
// a provider is itself a provider. Keeps `&PgDb: DbQuery` well-formed.
#[cfg(all(feature = "sqlx", not(feature = "sqlite")))]
impl<T: PgConnectionProvider> PgConnectionProvider for &T {
    fn pg_conn(
        &self,
    ) -> impl std::future::Future<Output = Result<crate::pg::PgConn<'_>, crate::ConnectionError>>
    + MaybeSend {
        (**self).pg_conn()
    }
}

#[cfg(all(feature = "sqlx", not(feature = "sqlite")))]
pub trait BackendSpecific: PgConnectionProvider {}
#[cfg(all(feature = "sqlx", not(feature = "sqlite")))]
impl<T: ?Sized> BackendSpecific for T where T: PgConnectionProvider {}

// Degenerate build (neither track's feature): keep `DbQuery` well-formed.
#[cfg(all(not(feature = "sqlite"), not(feature = "sqlx")))]
pub trait BackendSpecific {}
#[cfg(all(not(feature = "sqlite"), not(feature = "sqlx")))]
impl<T: ?Sized> BackendSpecific for T {}

pub trait DbQuery:
    MaybeSend
    + MaybeSync
    + BackendSpecific
    + QueryConsentRecord
    + QueryConversationList
    + QueryDms
    + QueryGroup
    + QueryGroupVersion
    + QueryGroupIntent
    + QueryGroupMessage
    + QueryIdentity
    + QueryIdentityCache
    + QueryKeyPackageHistory
    + QueryKeyStoreEntry
    + QueryDeviceSyncMessages
    + QueryRefreshState
    + QueryIdentityUpdates
    + QueryLocalCommitLog
    + QueryRemoteCommitLog
    + QueryAssociationStateCache
    + QueryReaddStatus
    + QueryTasks
    + QueryPendingRemove
    + QueryIcebox
    + QueryMessageDeletion
    + QueryMigrationCutover
    + QueryUserPreferences
{
}

// Compile-time parity guard — the sync-side twin of `pg.rs`'s
// `assert_db_query::<PgDb>()`. Both storage backends must satisfy the SAME
// `DbQuery` supertrait bundle above, so adding a `Query*` supertrait forces an
// implementation on BOTH: you cannot implement it for SQLite (`DbConnection`)
// and forget Postgres (`PgDb`), or the reverse — either omission fails to
// compile here (sync) or in `pg.rs` (async), not in some distant consumer.
// The bodies are type-checked (enforcing `DbConnection<C>: DbQuery`) even though
// nothing calls them — that IS the guard; silence the resulting dead-code lint.
#[cfg(feature = "sqlite")]
#[allow(dead_code)]
const _: fn() = || {
    fn assert_db_query<T: DbQuery>() {}
    fn assert_sqlite_backend<C: crate::ConnectionExt>() {
        assert_db_query::<crate::DbConnection<C>>();
    }
};

impl<T: ?Sized> DbQuery for T where
    T: MaybeSend
        + MaybeSync
        + BackendSpecific
        + QueryConsentRecord
        + QueryConversationList
        + QueryDms
        + QueryGroup
        + QueryGroupVersion
        + QueryGroupIntent
        + QueryGroupMessage
        + QueryIdentity
        + QueryIdentityCache
        + QueryKeyPackageHistory
        + QueryKeyStoreEntry
        + QueryDeviceSyncMessages
        + QueryRefreshState
        + QueryIdentityUpdates
        + QueryLocalCommitLog
        + QueryRemoteCommitLog
        + QueryAssociationStateCache
        + QueryReaddStatus
        + QueryTasks
        + QueryPendingRemove
        + QueryIcebox
        + QueryMessageDeletion
        + QueryMigrationCutover
        + QueryUserPreferences
{
}

pub use crate::xmtp_openmls_provider::XmtpMlsStorageProvider;
