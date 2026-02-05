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

pub trait IntoConnection {
    type Connection: ConnectionExt;
    fn into_connection(self) -> Self::Connection;
}

pub trait DbQuery:
    MaybeSend
    + MaybeSync
    + ReadOnly
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
    + Pragmas
    + crate::ConnectionExt
{
}

impl<T: ?Sized> DbQuery for T where
    T: MaybeSend
        + MaybeSync
        + ReadOnly
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
        + Pragmas
        + crate::ConnectionExt
{
}

pub use crate::xmtp_openmls_provider::XmtpMlsStorageProvider;
