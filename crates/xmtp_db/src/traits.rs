#[cfg(feature = "sync")]
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
#[cfg(feature = "sync")]
#[cfg_attr(any(feature = "test-utils", test), mockall::automock(type Store = crate::sql_key_store::mock::MockSqlKeyStore;))]
pub trait TransactionalKeyStore {
    type Store<'a>: XmtpMlsStorageProvider
    where
        Self: 'a;

    fn key_store<'a>(&'a mut self) -> Self::Store<'a>;
}

/// Inserts a model to the underlying data store, erroring if it already exists
pub trait Store<StorageConnection> {
    type Output;
    fn store(&self, into: &StorageConnection) -> Result<Self::Output, StorageError>;
}

/// Inserts a model to the underlying data store, silent no-op on unique constraint violations
pub trait StoreOrIgnore<StorageConnection> {
    type Output;
    fn store_or_ignore(&self, into: &StorageConnection) -> Result<Self::Output, StorageError>;
}

/// Fetches a model from the underlying data store, returning None if it does not exist
pub trait Fetch<Model> {
    type Key;
    fn fetch(&self, key: &Self::Key) -> Result<Option<Model>, StorageError>;
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

#[cfg(feature = "sync")]
pub trait IntoConnection {
    type Connection: ConnectionExt;
    fn into_connection(self) -> Self::Connection;
}

/// The parts of `DbQuery` that only one storage track can provide.
///
/// `ReadOnly` (`PRAGMA query_only`), `Pragmas` (`busy_timeout`, `cipher_log_level`)
/// and `ConnectionExt` (`raw_query` over a `&mut SqliteConnection`) are all
/// SQLite-shaped, so they are supertraits of `DbQuery` on the sync track only.
/// On the async track this collapses to an empty blanket-implemented trait,
/// which keeps the `DbQuery` list below single-copy instead of cfg-forking it.
#[cfg(feature = "sync")]
pub trait BackendSpecific: ReadOnly + Pragmas + crate::ConnectionExt {}
#[cfg(feature = "sync")]
impl<T: ?Sized> BackendSpecific for T where T: ReadOnly + Pragmas + crate::ConnectionExt {}

#[cfg(not(feature = "sync"))]
pub trait BackendSpecific {}
#[cfg(not(feature = "sync"))]
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

#[cfg(feature = "sync")]
pub use crate::xmtp_openmls_provider::XmtpMlsStorageProvider;
