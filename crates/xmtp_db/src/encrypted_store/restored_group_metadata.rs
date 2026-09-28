//! Archived group metadata is history, never MLS authority.

use crate::schema::{groups, restored_group_metadata};
use crate::{ConnectionExt, DbConnection, StorageError, group::GroupMembershipState};
use diesel::prelude::*;
use prost::Message;
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::device_sync::group_backup::GroupSave;

#[derive(Clone, Debug, PartialEq, Eq, Insertable, Queryable)]
#[diesel(table_name = restored_group_metadata)]
pub struct StoredRestoredGroupMetadata {
    pub group_id: GroupId,
    /// The first accepted GroupSave, including optional source metadata.
    pub group_save: Vec<u8>,
}

crate::impl_store!(StoredRestoredGroupMetadata, restored_group_metadata);

pub trait QueryRestoredGroupMetadata {
    fn restored_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<StoredRestoredGroupMetadata>, StorageError>;

    /// The archived record of a group that is still `Restored`.
    ///
    /// Membership and the record come from one statement, so a read cannot
    /// pair archived metadata with a row that a Welcome has activated.
    fn restored_group_history(&self, group_id: &GroupId)
    -> Result<Option<GroupSave>, StorageError>;

    /// Remove the archived record. Returns whether a record existed.
    fn delete_restored_group_metadata(&self, group_id: &GroupId) -> Result<bool, StorageError>;
}

impl<T: QueryRestoredGroupMetadata> QueryRestoredGroupMetadata for &T {
    fn restored_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<StoredRestoredGroupMetadata>, StorageError> {
        (**self).restored_group_metadata(group_id)
    }

    fn restored_group_history(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupSave>, StorageError> {
        (**self).restored_group_history(group_id)
    }

    fn delete_restored_group_metadata(&self, group_id: &GroupId) -> Result<bool, StorageError> {
        (**self).delete_restored_group_metadata(group_id)
    }
}

impl<C: ConnectionExt> QueryRestoredGroupMetadata for DbConnection<C> {
    fn restored_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<StoredRestoredGroupMetadata>, StorageError> {
        use restored_group_metadata::dsl;
        Ok(self.raw_query(|conn| {
            dsl::restored_group_metadata
                .find(group_id)
                .first(conn)
                .optional()
        })?)
    }

    // implements: ARCH-020
    fn restored_group_history(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<GroupSave>, StorageError> {
        let encoded: Option<Vec<u8>> = self.raw_query(|conn| {
            restored_group_metadata::table
                .inner_join(groups::table)
                .filter(restored_group_metadata::group_id.eq(group_id))
                .filter(groups::membership_state.eq(GroupMembershipState::Restored))
                .select(restored_group_metadata::group_save)
                .first(conn)
                .optional()
        })?;
        encoded
            .map(|bytes| {
                GroupSave::decode(bytes.as_slice()).map_err(|_| StorageError::DbDeserialize)
            })
            .transpose()
    }

    fn delete_restored_group_metadata(&self, group_id: &GroupId) -> Result<bool, StorageError> {
        use restored_group_metadata::dsl;
        let deleted = self.raw_query(|conn| {
            diesel::delete(dsl::restored_group_metadata.find(group_id)).execute(conn)
        })?;
        Ok(deleted > 0)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
