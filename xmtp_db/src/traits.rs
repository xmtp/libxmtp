use crate::ConnectionExt;
use crate::StorageError;
use crate::prelude::*;

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

pub trait DbQuery<C: crate::ConnectionExt>:
    ReadOnly<C>
    + QueryConsentRecord<C>
    + QueryConversationList<C>
    + QueryDms<C>
    + QueryGroup<C>
    + QueryGroupVersion<C>
    + QueryGroupIntent<C>
    + QueryGroupMessage<C>
    + QueryIdentity<C>
    + QueryIdentityCache<C>
    + QueryKeyPackageHistory<C>
    + QueryKeyStoreEntry<C>
    + QueryDeviceSyncMessages<C>
    + QueryRefreshState<C>
    + QueryIdentityUpdates<C>
    + crate::ConnectionExt
    + IntoConnection<Connection = C>
{
}

impl<C: crate::ConnectionExt, T: ?Sized> DbQuery<C> for T where
    T: ReadOnly<C>
        + QueryConsentRecord<C>
        + QueryConversationList<C>
        + QueryDms<C>
        + QueryGroup<C>
        + QueryGroupVersion<C>
        + QueryGroupIntent<C>
        + QueryGroupMessage<C>
        + QueryIdentity<C>
        + QueryIdentityCache<C>
        + QueryKeyPackageHistory<C>
        + QueryKeyStoreEntry<C>
        + QueryDeviceSyncMessages<C>
        + QueryRefreshState<C>
        + QueryIdentityUpdates<C>
        + crate::ConnectionExt
        + IntoConnection<Connection = C>
{
}
