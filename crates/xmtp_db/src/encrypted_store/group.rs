//! The Group database table. Stored information surrounding group membership and ID's.
use super::consent_record::ConsentState;
#[cfg(feature = "sqlite")]
use super::{
    ConnectionExt, Sqlite,
    db_connection::DbConnection,
    schema::groups::{self, dsl},
};
use crate::NotFound;
use crate::{DuplicateItem, StorageError};
#[cfg(feature = "sqlite")]
use crate::{impl_fetch, impl_store, impl_store_or_ignore};
use derive_builder::{Builder, UninitializedFieldError};
#[cfg(feature = "sqlite")]
use diesel::{
    deserialize::FromSqlRow, dsl::sql, expression::AsExpression, prelude::*, sql_types::Integer,
};
use serde::{Deserialize, Serialize};
mod convert;
mod dms;
mod version;

pub use dms::QueryDms;
pub use version::QueryGroupVersion;
use xmtp_proto::types::{Cursor, GroupId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Builder)]
#[cfg_attr(
    feature = "sqlite",
    derive(Insertable, Identifiable, Queryable, Selectable, QueryableByName)
)]
#[cfg_attr(feature = "sqlite", diesel(table_name = groups))]
#[cfg_attr(feature = "sqlite", diesel(primary_key(id)))]
#[cfg_attr(feature = "sqlite", diesel(check_for_backend(Sqlite)))]
#[builder(
    setter(into),
    build_fn(error = "StorageError", validate = "Self::validate")
)]
#[cfg_attr(feature = "sqlite", derive(AsChangeset))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "groups")]
/// A Unique group chat
pub struct StoredGroup {
    /// Randomly generated ID by group creator
    pub id: GroupId,
    /// Based on timestamp of this welcome message
    pub created_at_ns: i64,
    /// Enum, [`GroupMembershipState`] representing access to the group
    pub membership_state: GroupMembershipState,
    /// Track when the latest, most recent installations were checked
    #[builder(default = "0")]
    pub installations_last_checked: i64,
    /// The inbox_id of who added the user to a group.
    pub added_by_inbox_id: String,
    /// The sequence id of the welcome message
    #[builder(default = None)]
    pub sequence_id: Option<i64>,
    /// The last time the leaf node encryption key was rotated
    #[builder(default = "0")]
    pub rotated_at_ns: i64,
    /// Enum, [`ConversationType`] signifies the group conversation type which extends to who can access it.
    #[builder(default = "self.default_conversation_type()")]
    pub conversation_type: ConversationType,
    /// The inbox_id of the DM target
    #[builder(default = None)]
    pub dm_id: Option<String>,
    /// Timestamp of when the last message was sent for this group (updated automatically in a trigger)
    #[builder(default = None)]
    pub last_message_ns: Option<i64>,
    /// The Time in NS when the messages should be deleted
    #[builder(default = None)]
    pub message_disappear_from_ns: Option<i64>,
    /// How long a message in the group can live in NS
    #[builder(default = None)]
    pub message_disappear_in_ns: Option<i64>,
    /// The version of the protocol that the group is paused for, None is not paused
    #[builder(default = None)]
    pub paused_for_version: Option<String>,
    #[builder(default = false)]
    pub maybe_forked: bool,
    #[builder(default = "String::new()")]
    pub fork_details: String,
    /// The Originator Node ID of the WelcomeMessage
    #[builder(default = None)]
    pub originator_id: Option<i64>,
    /// Whether the user should publish the commit log for this group
    #[builder(default = false)]
    pub should_publish_commit_log: bool,
    /// The consensus public key of the commit log for this group
    /// Derived from the first entry of the commit log
    #[builder(default = None)]
    pub commit_log_public_key: Option<Vec<u8>>,
    /// Whether the local commit log has diverged from the remote commit log
    /// NULL if the remote commit log is not up to date yet
    #[builder(default = None)]
    pub is_commit_log_forked: Option<bool>,
    /// Whether the pending-remove list is empty
    /// NULL if the pending-remove didn't receive an update yet
    #[builder(default = None)]
    pub has_pending_leave_request: Option<bool>,
    //todo: store member role?
}

impl StoredGroupBuilder {
    fn validate(&self) -> Result<(), StorageError> {
        if self.sequence_id.is_some() && self.originator_id.is_none() {
            return Err(UninitializedFieldError::new("originator_id").into());
        }
        if self.originator_id.is_some() && self.sequence_id.is_none() {
            return Err(UninitializedFieldError::new("sequence_id").into());
        }
        Ok(())
    }
}
impl StoredGroup {
    pub fn cursor(&self) -> Option<Cursor> {
        // if a group specifies a sequence_id/originator_id, then it must
        // specify both sequence id and originator
        // else DB and Builder error
        if let Some(sequence_id) = self.sequence_id
            && let Some(originator) = self.originator_id
        {
            return Some(Cursor::new(sequence_id as u64, originator as u32));
        }
        None
    }
}

impl StoredGroupBuilder {
    pub fn cursor(&mut self, cursor: Cursor) -> &mut Self {
        self.originator_id = Some(Some(cursor.originator_id as i64));
        self.sequence_id = Some(Some(cursor.sequence_id as i64));
        self
    }
}

/// A subset of the group table for fetching the commit log public key
#[cfg_attr(feature = "sqlite", derive(Queryable))]
#[cfg_attr(feature = "sqlite", diesel(table_name = groups))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "groups")]
pub struct StoredGroupCommitLogPublicKey {
    pub id: GroupId,
    pub commit_log_public_key: Option<Vec<u8>>,
}

/// A struct for fetching groups that need readd requests with their latest epoch
///
/// Deliberately not a `PgModel`: `latest_commit_sequence_id` is a `MAX()` over
/// `remote_commit_log`, not a column of any one table, so there is no column
/// list for a derive to emit or for `schema_check` to verify.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlite", derive(Queryable, QueryableByName))]
pub struct StoredGroupForReaddRequest {
    #[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Binary))]
    pub group_id: GroupId,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>))]
    pub latest_commit_sequence_id: Option<i64>,
}

/// A struct for fetching groups that need to respond to readd requests
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlite", derive(Queryable, QueryableByName))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "groups")]
pub struct StoredGroupForRespondingReadds {
    #[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Binary))]
    #[xmtp(rename = "id")]
    pub group_id: GroupId,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>))]
    pub dm_id: Option<String>,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::Integer))]
    pub conversation_type: ConversationType,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = diesel::sql_types::BigInt))]
    pub created_at_ns: i64,
}

// TODO: Create two more structs that delegate to StoredGroup
#[cfg(feature = "sqlite")]
impl_fetch!(StoredGroup, groups, GroupId);
#[cfg(feature = "sqlite")]
impl_store!(StoredGroup, groups);
#[cfg(feature = "sqlite")]
impl_store_or_ignore!(StoredGroup, groups);

impl StoredGroupBuilder {
    fn default_conversation_type(&self) -> ConversationType {
        if self.dm_id.is_some() {
            ConversationType::Dm
        } else {
            ConversationType::Group
        }
    }
}

impl StoredGroup {
    pub fn builder() -> StoredGroupBuilder {
        StoredGroupBuilder::default()
    }
}

#[derive(Debug, Clone, Default)]
pub enum GroupQueryOrderBy {
    #[default]
    CreatedAt,
    LastActivity,
}

#[derive(Debug, Default, Clone)]
pub struct GroupQueryArgs {
    pub allowed_states: Option<Vec<GroupMembershipState>>,
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub last_activity_after_ns: Option<i64>,
    pub last_activity_before_ns: Option<i64>,
    pub limit: Option<i64>,
    pub conversation_type: Option<ConversationType>,
    pub consent_states: Option<Vec<ConsentState>>,
    pub include_sync_groups: bool,
    pub include_duplicate_dms: bool,
    pub should_publish_commit_log: Option<bool>,
    pub order_by: Option<GroupQueryOrderBy>,
}

impl GroupQueryArgs {
    pub fn validate(&self) -> Result<(), crate::ConnectionError> {
        if self.last_activity_after_ns.is_some() && self.created_after_ns.is_some() {
            return Err(crate::ConnectionError::InvalidQuery(
                "last_activity_after_ns and created_after_ns cannot be used together".to_string(),
            ));
        }

        if self.last_activity_before_ns.is_some() && self.created_before_ns.is_some() {
            return Err(crate::ConnectionError::InvalidQuery(
                "last_activity_before_ns and created_before_ns cannot be used together".to_string(),
            ));
        }

        Ok(())
    }
}

pub trait QueryGroup {
    /// Return regular `Purpose::Conversation` groups with additional optional filters
    fn find_groups(
        &self,
        args: &GroupQueryArgs,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn find_groups_by_id_paged(
        &self,
        args: &GroupQueryArgs,
        offset: i64,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Updates group membership state
    fn update_group_membership(
        &self,
        group_id: &GroupId,
        state: GroupMembershipState,
    ) -> impl std::future::Future<Output = Result<(), crate::ConnectionError>> + xmtp_common::MaybeSend;

    fn all_sync_groups(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn find_sync_group(
        &self,
        id: &GroupId,
    ) -> impl std::future::Future<Output = Result<Option<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn primary_sync_group(
        &self,
    ) -> impl std::future::Future<Output = Result<Option<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Return a single group that matches the given ID
    fn find_group(
        &self,
        id: &GroupId,
    ) -> impl std::future::Future<Output = Result<Option<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Return a single group that matches the given welcome ID
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> impl std::future::Future<Output = Result<Option<StoredGroup>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn get_rotated_at_ns(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + xmtp_common::MaybeSend;

    /// Updates the 'last time checked' we checked for new installations.
    fn update_rotated_at_ns(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn get_installations_time_checked(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<i64, StorageError>> + xmtp_common::MaybeSend;

    /// Updates the 'last time checked' we checked for new installations.
    fn update_installations_time_checked(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn update_message_disappearing_from_ns(
        &self,
        group_id: &GroupId,
        from_ns: Option<i64>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn update_message_disappearing_in_ns(
        &self,
        group_id: &GroupId,
        in_ns: Option<i64>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn insert_or_replace_group(
        &self,
        group: StoredGroup,
    ) -> impl std::future::Future<Output = Result<StoredGroup, StorageError>> + xmtp_common::MaybeSend;

    /// Get all the welcome ids turned into groups
    fn group_cursors(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Cursor>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn mark_group_as_maybe_forked(
        &self,
        group_id: &GroupId,
        fork_details: String,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn clear_fork_flag_for_group(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<(), crate::ConnectionError>> + xmtp_common::MaybeSend;

    fn has_duplicate_dm(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<bool, crate::ConnectionError>> + xmtp_common::MaybeSend;

    /// Get conversations for all conversations that require a remote commit log publish (DMs and groups where user is super admin, excluding sync groups)
    fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> impl std::future::Future<
        Output = Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    /// Get conversations for all conversations that require a remote commit log download (DMs and groups that are not sync groups)
    fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> impl std::future::Future<
        Output = Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    /// Get conversation IDs for fork checking (excludes already forked conversations and sync groups)
    fn get_conversation_ids_for_fork_check(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Vec<u8>>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Get conversation IDs for conversations that are forked and need readd requests
    fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> impl std::future::Future<
        Output = Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    /// Get conversation IDs for conversations that need to respond to readd requests
    fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> impl std::future::Future<
        Output = Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn get_conversation_type(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<ConversationType, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Updates the commit log public key for a group
    fn set_group_commit_log_public_key(
        &self,
        group_id: &GroupId,
        public_key: &[u8],
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    /// Updates the is_commit_log_forked status for a group
    fn set_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
        is_forked: Option<bool>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    /// Gets the is_commit_log_forked status for a group
    fn get_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<Option<bool>, StorageError>> + xmtp_common::MaybeSend;

    /// Updates the has_pending_leave_request status for a group
    fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &GroupId,
        has_pending_leave_request: Option<bool>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn get_groups_have_pending_leave_request(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Vec<u8>>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;
}

impl<T> QueryGroup for &T
where
    T: QueryGroup + xmtp_common::MaybeSync,
{
    /// Return regular `Purpose::Conversation` groups with additional optional filters
    async fn find_groups(
        &self,
        args: &GroupQueryArgs,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).find_groups(args).await
    }

    async fn find_groups_by_id_paged(
        &self,
        args: &GroupQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).find_groups_by_id_paged(args, offset).await
    }

    /// Updates group membership state
    async fn update_group_membership(
        &self,
        group_id: &GroupId,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_group_membership(group_id, state).await
    }

    async fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).all_sync_groups().await
    }

    async fn find_sync_group(
        &self,
        id: &GroupId,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_sync_group(id).await
    }

    async fn primary_sync_group(&self) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).primary_sync_group().await
    }

    /// Return a single group that matches the given ID
    async fn find_group(
        &self,
        id: &GroupId,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_group(id).await
    }

    /// Return a single group that matches the given welcome ID
    async fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_group_by_sequence_id(cursor).await
    }

    async fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError> {
        (**self).get_rotated_at_ns(group_id).await
    }

    /// Updates the 'last time checked' we checked for new installations.
    async fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError> {
        (**self).update_rotated_at_ns(group_id).await
    }

    async fn get_installations_time_checked(
        &self,
        group_id: &GroupId,
    ) -> Result<i64, StorageError> {
        (**self).get_installations_time_checked(group_id).await
    }

    /// Updates the 'last time checked' we checked for new installations.
    async fn update_installations_time_checked(
        &self,
        group_id: &GroupId,
    ) -> Result<(), StorageError> {
        (**self).update_installations_time_checked(group_id).await
    }

    async fn update_message_disappearing_from_ns(
        &self,
        group_id: &GroupId,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        (**self)
            .update_message_disappearing_from_ns(group_id, from_ns)
            .await
    }

    async fn update_message_disappearing_in_ns(
        &self,
        group_id: &GroupId,
        in_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        (**self)
            .update_message_disappearing_in_ns(group_id, in_ns)
            .await
    }

    async fn insert_or_replace_group(
        &self,
        group: StoredGroup,
    ) -> Result<StoredGroup, StorageError> {
        (**self).insert_or_replace_group(group).await
    }

    /// Get all the welcome ids turned into groups
    async fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError> {
        (**self).group_cursors().await
    }

    async fn mark_group_as_maybe_forked(
        &self,
        group_id: &GroupId,
        fork_details: String,
    ) -> Result<(), StorageError> {
        (**self)
            .mark_group_as_maybe_forked(group_id, fork_details)
            .await
    }

    async fn clear_fork_flag_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<(), crate::ConnectionError> {
        (**self).clear_fork_flag_for_group(group_id).await
    }

    async fn has_duplicate_dm(&self, group_id: &GroupId) -> Result<bool, crate::ConnectionError> {
        (**self).has_duplicate_dm(group_id).await
    }

    /// Get conversation IDs for all conversations that require a remote commit log publish (DMs and groups where user is super admin, excluding sync groups)
    async fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_remote_log_publish().await
    }

    async fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        (**self)
            .get_conversation_ids_for_remote_log_download()
            .await
    }

    async fn get_conversation_ids_for_fork_check(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_fork_check().await
    }

    async fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_requesting_readds().await
    }

    async fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_responding_readds().await
    }

    async fn get_conversation_type(
        &self,
        group_id: &GroupId,
    ) -> Result<ConversationType, crate::ConnectionError> {
        (**self).get_conversation_type(group_id).await
    }

    async fn set_group_commit_log_public_key(
        &self,
        group_id: &GroupId,
        public_key: &[u8],
    ) -> Result<(), StorageError> {
        (**self)
            .set_group_commit_log_public_key(group_id, public_key)
            .await
    }

    async fn set_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
        is_forked: Option<bool>,
    ) -> Result<(), StorageError> {
        (**self)
            .set_group_commit_log_forked_status(group_id, is_forked)
            .await
    }

    async fn get_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<bool>, StorageError> {
        (**self).get_group_commit_log_forked_status(group_id).await
    }

    async fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &GroupId,
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError> {
        (**self)
            .set_group_has_pending_leave_request_status(group_id, has_pending_leave_request)
            .await
    }

    async fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        (**self).get_groups_have_pending_leave_request().await
    }
}

#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> QueryGroup for DbConnection<C> {
    /// Return regular `Purpose::Conversation` groups with additional optional filters
    #[xmtp_common::db_span]
    async fn find_groups(
        &self,
        args: &GroupQueryArgs,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        use crate::schema::consent_records::dsl as consent_dsl;

        args.validate()?;

        let GroupQueryArgs {
            allowed_states,
            created_after_ns,
            created_before_ns,
            limit,
            conversation_type,
            consent_states,
            include_sync_groups,
            include_duplicate_dms,
            last_activity_after_ns,
            last_activity_before_ns,
            should_publish_commit_log,
            order_by,
        } = args;

        let order_expression = match order_by.clone().unwrap_or_default() {
            GroupQueryOrderBy::CreatedAt => {
                diesel::dsl::sql::<diesel::sql_types::BigInt>("created_at_ns ASC")
            }
            GroupQueryOrderBy::LastActivity => diesel::dsl::sql::<diesel::sql_types::BigInt>(
                "COALESCE(last_message_ns, created_at_ns) DESC",
            ),
        };

        let mut query = dsl::groups
            .filter(dsl::conversation_type.ne_all(ConversationType::virtual_types()))
            .order(order_expression)
            .into_boxed();

        if !include_duplicate_dms {
            // Fast DM deduplication using EXISTS - avoids expensive window functions
            // Keep only the latest group for each dm_id (or regular group if not a DM)
            query = query.filter(sql::<diesel::sql_types::Bool>(
                "NOT EXISTS (
                    SELECT 1 FROM groups g2
                    WHERE COALESCE(g2.dm_id, g2.id) = COALESCE(groups.dm_id, groups.id)
                    AND (COALESCE(g2.last_message_ns, 0), g2.id) > (COALESCE(groups.last_message_ns, 0), groups.id)
                )",
            ));
        }

        if let Some(limit) = limit {
            query = query.limit(*limit);
        }

        if let Some(allowed_states) = allowed_states {
            query = query.filter(dsl::membership_state.eq_any(allowed_states));
        }

        // last_activity_after_ns takes precedence over created_after_ns
        if let Some(last_activity_after_ns) = last_activity_after_ns {
            // "Activity after" means groups that were either created,
            // or have sent a message after the specified time.
            query = query.filter(
                diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "COALESCE(last_message_ns, created_at_ns)",
                )
                .gt(last_activity_after_ns),
            );
        }

        if let Some(created_after_ns) = created_after_ns {
            query = query.filter(dsl::created_at_ns.gt(created_after_ns));
        }

        if let Some(last_activity_before_ns) = last_activity_before_ns {
            query = query.filter(
                diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "COALESCE(last_message_ns, created_at_ns)",
                )
                .lt(last_activity_before_ns),
            );
        }

        if let Some(created_before_ns) = created_before_ns {
            query = query.filter(dsl::created_at_ns.lt(created_before_ns));
        }

        if let Some(conversation_type) = conversation_type {
            query = query.filter(dsl::conversation_type.eq(conversation_type));
        }

        let effective_consent_states = match &consent_states {
            Some(states) if !states.is_empty() => states.clone(),
            _ => vec![ConsentState::Allowed, ConsentState::Unknown],
        };

        let includes_unknown = effective_consent_states.contains(&ConsentState::Unknown);
        let includes_all = effective_consent_states.len() == 3;

        if let Some(should_publish_commit_log) = should_publish_commit_log {
            query = query.filter(dsl::should_publish_commit_log.eq(should_publish_commit_log));
        }

        let filtered_states: Vec<_> = effective_consent_states
            .iter()
            .filter(|state| **state != ConsentState::Unknown)
            .cloned()
            .collect();

        let mut groups = if includes_all {
            // No filtering at all
            self.raw_query(|conn| query.load::<StoredGroup>(conn))?
        } else if includes_unknown {
            // LEFT JOIN: include Unknown + NULL + filtered states
            let left_joined_query = query
                .left_join(consent_dsl::consent_records.on(
                    sql::<diesel::sql_types::Text>("lower(hex(groups.id))").eq(consent_dsl::entity),
                ))
                .filter(
                    consent_dsl::state
                        .is_null()
                        .or(consent_dsl::state.eq(ConsentState::Unknown))
                        .or(consent_dsl::state.eq_any(filtered_states.clone())),
                )
                .select(dsl::groups::all_columns());

            self.raw_query(|conn| left_joined_query.load::<StoredGroup>(conn))?
        } else {
            // INNER JOIN: strict match only to specific states (no Unknown or NULL)
            let inner_joined_query = query
                .inner_join(consent_dsl::consent_records.on(
                    sql::<diesel::sql_types::Text>("lower(hex(groups.id))").eq(consent_dsl::entity),
                ))
                .filter(consent_dsl::state.eq_any(filtered_states.clone()))
                .select(dsl::groups::all_columns());

            self.raw_query(|conn| inner_joined_query.load::<StoredGroup>(conn))?
        };

        // Were sync groups explicitly asked for? Was the include_sync_groups flag set to true?
        // Then query for those separately
        if matches!(conversation_type, Some(ConversationType::Sync)) || *include_sync_groups {
            let query = dsl::groups.filter(dsl::conversation_type.eq(ConversationType::Sync));
            let mut sync_groups = self.raw_query(|conn| query.load(conn))?;
            groups.append(&mut sync_groups);
        }

        Ok(groups)
    }

    #[xmtp_common::db_span]
    async fn find_groups_by_id_paged(
        &self,
        args: &GroupQueryArgs,
        offset: i64,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        let GroupQueryArgs {
            created_after_ns,
            created_before_ns,
            limit,
            ..
        } = args;

        let mut query = groups::table
            .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
            .order(groups::id)
            .into_boxed();

        if let Some(start_ns) = created_after_ns {
            query = query.filter(groups::created_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = created_before_ns {
            query = query.filter(groups::created_at_ns.le(end_ns));
        }

        query = query.limit(limit.unwrap_or(100)).offset(offset);

        self.raw_query(|conn| query.load::<StoredGroup>(conn))
    }

    /// Updates group membership state
    #[xmtp_common::db_span]
    async fn update_group_membership(
        &self,
        group_id: &GroupId,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::membership_state.eq(state))
                .execute(conn)
        })?;

        Ok(())
    }

    #[xmtp_common::db_span]
    async fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync));

        self.raw_query(|conn| query.load(conn))
    }

    #[xmtp_common::db_span]
    async fn find_sync_group(
        &self,
        id: &GroupId,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .filter(dsl::id.eq(id));

        self.raw_query(|conn| query.first(conn).optional())
    }

    #[xmtp_common::db_span]
    async fn primary_sync_group(&self) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync));

        self.raw_query(|conn| query.first(conn).optional())
    }

    /// Return a single group that matches the given ID
    #[xmtp_common::db_span]
    async fn find_group(
        &self,
        id: &GroupId,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .limit(1)
            .filter(dsl::id.eq(id));
        let groups = self.raw_query(|conn| query.load(conn))?;

        Ok(groups.into_iter().next())
    }

    /// Return a single group that matches the given welcome ID
    #[xmtp_common::db_span]
    async fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .filter(dsl::sequence_id.eq(cursor.sequence_id as i64))
            .filter(dsl::originator_id.eq(cursor.originator_id as i64));

        let groups = self.raw_query(|conn| query.load(conn))?;

        if groups.len() > 1 {
            tracing::warn!(
                cursor.sequence_id,
                "More than one group found for welcome_id {}",
                cursor.sequence_id
            );
        }
        Ok(groups.into_iter().next())
    }

    async fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError> {
        let last_ts: Option<i64> = self.raw_query(|conn| {
            dsl::groups
                .find(&group_id)
                .select(dsl::rotated_at_ns)
                .first(conn)
                .optional()
        })?;

        last_ts.ok_or(StorageError::NotFound(NotFound::InstallationTimeForGroup(
            *group_id,
        )))
    }

    /// Updates the 'last time checked' we checked for new installations.
    async fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::rotated_at_ns.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    async fn get_installations_time_checked(
        &self,
        group_id: &GroupId,
    ) -> Result<i64, StorageError> {
        let last_ts = self.raw_query(|conn| {
            dsl::groups
                .find(&group_id)
                .select(dsl::installations_last_checked)
                .first(conn)
                .optional()
        })?;

        last_ts.ok_or(NotFound::InstallationTimeForGroup(*group_id).into())
    }

    /// Updates the 'last time checked' we checked for new installations.
    async fn update_installations_time_checked(
        &self,
        group_id: &GroupId,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::installations_last_checked.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    async fn update_message_disappearing_from_ns(
        &self,
        group_id: &GroupId,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::message_disappear_from_ns.eq(from_ns))
                .execute(conn)
        })?;

        Ok(())
    }

    async fn update_message_disappearing_in_ns(
        &self,
        group_id: &GroupId,
        in_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::message_disappear_in_ns.eq(in_ns))
                .execute(conn)
        })?;

        Ok(())
    }

    async fn insert_or_replace_group(
        &self,
        group: StoredGroup,
    ) -> Result<StoredGroup, StorageError> {
        let maybe_inserted_group: Option<StoredGroup> = self.raw_query(|conn| {
            diesel::insert_into(dsl::groups)
                .values(&group)
                .on_conflict_do_nothing()
                .get_result(conn)
                .optional()
        })?;

        if maybe_inserted_group.is_none() {
            let mut existing_group: StoredGroup =
                self.raw_query(|conn| dsl::groups.find(&group.id).first(conn))?;
            // A restored group should be overwritten
            if matches!(
                existing_group.membership_state,
                GroupMembershipState::Restored
            ) {
                self.raw_query(|c| {
                    diesel::update(dsl::groups.find(&group.id))
                        .set(&group)
                        .execute(c)
                })?;
            }

            if existing_group.sequence_id == group.sequence_id {
                tracing::info!("Group welcome id already exists");
                // Error so OpenMLS db transaction are rolled back on duplicate welcomes
                Err(StorageError::Duplicate(DuplicateItem::WelcomeId(
                    existing_group.cursor(),
                )))
            } else {
                tracing::info!("Group already exists");
                // If the welcome id is greater than the existing group welcome, update the welcome id
                // on the existing group
                if group.sequence_id.is_some()
                    && (existing_group.sequence_id.is_none()
                        || group.sequence_id > existing_group.sequence_id)
                {
                    // Co-set `originator_id` alongside `sequence_id`. The incoming
                    // (welcome-built) group already carries the correct originator
                    // (e.g. `Originators::WELCOME_MESSAGES` via `Cursor::v3_welcomes`),
                    // and the builder invariant guarantees that whenever
                    // `sequence_id.is_some()` the `originator_id` is also set. Writing
                    // only `sequence_id` here (the previous behavior) could leave a row
                    // in the invalid `(sequence_id = NOT NULL, originator_id = NULL)`
                    // state, which later aborts `group_cursors()`.
                    self.raw_query(|c| {
                        diesel::update(dsl::groups.find(&group.id))
                            .set((
                                dsl::sequence_id.eq(group.sequence_id),
                                dsl::originator_id.eq(group.originator_id),
                            ))
                            .execute(c)
                    })?;
                    existing_group.sequence_id = group.sequence_id;
                    existing_group.originator_id = group.originator_id;
                }
                Ok(existing_group)
            }
        } else {
            Ok(self.raw_query(|c| dsl::groups.find(group.id).first(c))?)
        }
    }

    /// Get all the welcome ids turned into groups
    async fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError> {
        self.raw_query(|conn| {
            Ok(dsl::groups
                .filter(dsl::sequence_id.is_not_null())
                .select((dsl::sequence_id, dsl::originator_id))
                .load::<(Option<i64>, Option<i64>)>(conn)?
                .into_iter()
                .filter_map(|(seq, orig)| match (seq, orig) {
                    (Some(seq), Some(orig)) => Some(Cursor::new(seq as u64, orig as u32)),
                    // Defense in depth: a row with a `sequence_id` but a NULL
                    // `originator_id` violates the builder invariant and previously
                    // aborted the whole conversation stream on startup (bricking the
                    // app until local data was wiped). Skip such a row and warn rather
                    // than crash, so a single half-populated cursor can't take the
                    // client down. The write-path fix above prevents new rows from
                    // reaching this state; a heal migration backfills existing ones.
                    (Some(seq), None) => {
                        tracing::warn!(
                            sequence_id = seq,
                            "group row has sequence_id but NULL originator_id; skipping cursor"
                        );
                        None
                    }
                    // `sequence_id.is_not_null()` filters these out at the SQL layer;
                    // handled here only for exhaustiveness.
                    (None, _) => None,
                })
                .collect())
        })
    }

    async fn mark_group_as_maybe_forked(
        &self,
        group_id: &GroupId,
        fork_details: String,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set((
                    dsl::maybe_forked.eq(true),
                    dsl::fork_details.eq(fork_details),
                ))
                .execute(conn)
        })?;

        Ok(())
    }

    async fn clear_fork_flag_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set((dsl::maybe_forked.eq(false), dsl::fork_details.eq("")))
                .execute(conn)
        })?;
        Ok(())
    }

    async fn has_duplicate_dm(&self, group_id: &GroupId) -> Result<bool, crate::ConnectionError> {
        self.raw_query(|conn| {
            let dm_id: Option<String> = dsl::groups
                .filter(dsl::id.eq(group_id))
                .select(dsl::dm_id)
                .first::<Option<String>>(conn)
                .optional()?
                .flatten();

            if let Some(dm_id) = dm_id {
                let count: i64 = dsl::groups
                    .filter(dsl::conversation_type.eq(ConversationType::Dm))
                    .filter(dsl::dm_id.eq(dm_id))
                    .count()
                    .get_result(conn)?;

                Ok(count > 1)
            } else {
                Ok(false)
            }
        })
    }

    /// Get conversation IDs for all conversations that require a remote commit log publish
    /// (DMs and groups where user is super admin, excluding sync groups and rejected groups)
    #[xmtp_common::db_span]
    async fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        use crate::schema::consent_records::dsl as consent_dsl;

        let query = dsl::groups
            .filter(
                dsl::conversation_type
                    .eq(ConversationType::Dm)
                    .or(dsl::conversation_type
                        .eq(ConversationType::Group)
                        .and(dsl::should_publish_commit_log.eq(true))),
            )
            .inner_join(consent_dsl::consent_records.on(
                sql::<diesel::sql_types::Text>("lower(hex(groups.id))").eq(consent_dsl::entity),
            ))
            .filter(consent_dsl::state.eq(ConsentState::Allowed))
            .select((dsl::id, dsl::commit_log_public_key))
            .order(dsl::created_at_ns.asc());

        self.raw_query(|conn| query.load::<StoredGroupCommitLogPublicKey>(conn))
    }

    // All dms and groups that are not sync groups and have consent state Allowed
    #[xmtp_common::db_span]
    async fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        use crate::schema::consent_records::dsl as consent_dsl;

        let query = dsl::groups
            .filter(dsl::conversation_type.ne_all(ConversationType::virtual_types()))
            .inner_join(consent_dsl::consent_records.on(
                sql::<diesel::sql_types::Text>("lower(hex(groups.id))").eq(consent_dsl::entity),
            ))
            .filter(consent_dsl::state.eq(ConsentState::Allowed))
            .select((dsl::id, dsl::commit_log_public_key));

        self.raw_query(|conn| query.load::<StoredGroupCommitLogPublicKey>(conn))
    }

    // Get conversation IDs for fork checking (excludes already forked conversations and sync groups)
    #[xmtp_common::db_span]
    async fn get_conversation_ids_for_fork_check(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        let query = dsl::groups
            .filter(
                dsl::conversation_type
                    .ne_all(ConversationType::virtual_types())
                    .and(
                        dsl::is_commit_log_forked
                            .is_null()
                            .or(dsl::is_commit_log_forked.ne(Some(true))),
                    ),
            )
            .select(dsl::id);

        self.raw_query(|conn| query.load::<Vec<u8>>(conn))
    }

    #[xmtp_common::db_span]
    async fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError> {
        use super::schema::{groups::dsl as groups_dsl, remote_commit_log::dsl as rcl_dsl};
        use diesel::dsl::max;

        self.raw_query(|conn| {
            groups_dsl::groups
                .left_join(rcl_dsl::remote_commit_log.on(groups_dsl::id.eq(rcl_dsl::group_id)))
                .filter(
                    groups_dsl::conversation_type
                        .ne_all(ConversationType::virtual_types())
                        .and(groups_dsl::is_commit_log_forked.eq(true)),
                )
                .group_by(groups_dsl::id)
                .select((groups_dsl::id, max(rcl_dsl::commit_sequence_id).nullable()))
                .load::<StoredGroupForReaddRequest>(conn)
        })
    }

    #[xmtp_common::db_span]
    async fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError> {
        use super::schema::{groups::dsl as groups_dsl, readd_status::dsl as readd_dsl};
        use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl};

        self.raw_query(|conn| {
            readd_dsl::readd_status
                .inner_join(groups_dsl::groups.on(readd_dsl::group_id.eq(groups_dsl::id)))
                .filter(readd_dsl::requested_at_sequence_id.is_not_null())
                .filter(
                    readd_dsl::requested_at_sequence_id
                        .ge(readd_dsl::responded_at_sequence_id)
                        .or(readd_dsl::responded_at_sequence_id.is_null()),
                )
                .select((
                    groups_dsl::id,
                    groups_dsl::dm_id,
                    groups_dsl::conversation_type,
                    groups_dsl::created_at_ns,
                ))
                .distinct()
                .load::<StoredGroupForRespondingReadds>(conn)
        })
    }

    #[xmtp_common::db_span]
    async fn get_conversation_type(
        &self,
        group_id: &GroupId,
    ) -> Result<ConversationType, crate::ConnectionError> {
        let query = dsl::groups
            .filter(dsl::id.eq(group_id))
            .select(dsl::conversation_type);
        let conversation_type = self.raw_query(|conn| query.first(conn))?;
        Ok(conversation_type)
    }

    async fn set_group_commit_log_public_key(
        &self,
        group_id: &GroupId,
        public_key: &[u8],
    ) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;
        let num_updated = self.raw_query(|conn| {
            diesel::update(dsl::groups)
                .filter(
                    dsl::id
                        .eq(group_id)
                        .and(dsl::commit_log_public_key.is_null()),
                )
                .set(dsl::commit_log_public_key.eq(public_key))
                .execute(conn)
        })?;
        if num_updated == 0 {
            return Err(StorageError::Duplicate(DuplicateItem::CommitLogPublicKey(
                group_id.as_ref().to_vec(),
            )));
        }
        Ok(())
    }

    async fn set_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
        is_forked: Option<bool>,
    ) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::is_commit_log_forked.eq(is_forked))
                .execute(conn)
        })?;
        Ok(())
    }

    async fn get_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<bool>, StorageError> {
        use crate::schema::groups::dsl;
        self.raw_query(|conn| {
            dsl::groups
                .find(group_id)
                .select(dsl::is_commit_log_forked)
                .first::<Option<bool>>(conn)
        })
        .map_err(StorageError::from)
    }

    async fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &GroupId,
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::has_pending_leave_request.eq(has_pending_leave_request))
                .execute(conn)
        })?;
        Ok(())
    }

    #[xmtp_common::db_span]
    async fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        let query = dsl::groups
            .filter(
                dsl::conversation_type
                    .ne(ConversationType::Sync)
                    .and(dsl::has_pending_leave_request.eq(Some(true))),
            )
            .select(dsl::id);

        self.raw_query(|conn| query.load::<Vec<u8>>(conn))
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sqlite", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Integer))]
/// Status of membership in a group, once a user sends a request to join
pub enum GroupMembershipState {
    /// User is allowed to interact with this Group
    Allowed = 1,
    /// User has been Rejected from this Group
    Rejected = 2,
    /// User is Pending acceptance to the Group
    Pending = 3,
    /// Group has been restored from an archive, but is not active yet.
    Restored = 4,
    /// User is Pending to get removed of the Group
    PendingRemove = 5,
}

crate::impl_sql_int_enum!(GroupMembershipState {
    Allowed = 1,
    Rejected = 2,
    Pending = 3,
    Restored = 4,
    PendingRemove = 5,
});

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sqlite", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Integer))]
pub enum ConversationType {
    Group = 1,
    Dm = 2,
    Sync = 3,
    Oneshot = 4,
}

impl ConversationType {
    pub fn virtual_types() -> Vec<ConversationType> {
        vec![ConversationType::Sync, ConversationType::Oneshot]
    }

    pub fn is_virtual(&self) -> bool {
        // Use match to force exhaustive pattern matching
        match self {
            ConversationType::Group => false,
            ConversationType::Dm => false,
            ConversationType::Sync => true,
            ConversationType::Oneshot => true,
        }
    }
}

crate::impl_sql_int_enum!(ConversationType {
    Group = 1,
    Dm = 2,
    Sync = 3,
    Oneshot = 4,
});

impl std::fmt::Display for ConversationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ConversationType::*;
        match self {
            Group => write!(f, "group"),
            Dm => write!(f, "dm"),
            Sync => write!(f, "sync"),
            Oneshot => write!(f, "oneshot"),
        }
    }
}

pub trait DmIdExt {
    fn other_inbox_id(&self, id: &str) -> String;
}

impl DmIdExt for String {
    fn other_inbox_id(&self, id: &str) -> String {
        // drop the "dm:"
        let dm_id = &self[3..];

        // If my id is the first half, return the second half, otherwise return first half
        let target_inbox = if dm_id[..id.len()] == *id {
            // + 1 because there is a colon (:)
            &dm_id[(id.len() + 1)..]
        } else {
            &dm_id[..id.len()]
        };

        target_inbox.to_string()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub use super::dms::tests::*;
    use super::*;

    use crate::{
        Store,
        consent_record::{ConsentType, StoredConsentRecord},
        readd_status::ReaddStatus,
        schema::groups::dsl::groups,
        test_utils::{with_connection, with_connection_async},
    };
    use xmtp_common::{Generate, assert_ok, rand_vec, time::now_ns};
    use xmtp_configuration::Originators;

    /// Generate a test group
    pub fn generate_group(state: Option<GroupMembershipState>) -> StoredGroup {
        // Default behavior: Use `now_ns()` as the creation time
        generate_group_with_created_at(state, now_ns())
    }

    pub fn generate_group_with_created_at(
        state: Option<GroupMembershipState>,
        created_at_ns: i64,
    ) -> StoredGroup {
        let id = GroupId::generate();
        let membership_state = state.unwrap_or(GroupMembershipState::Allowed);
        StoredGroup::builder()
            .id(id)
            .created_at_ns(created_at_ns)
            .membership_state(membership_state)
            .added_by_inbox_id("placeholder_address")
            .build()
            .unwrap()
    }

    /// Generate a test group with welcome
    pub fn generate_group_with_welcome(
        state: Option<GroupMembershipState>,
        welcome_id: Option<i64>,
    ) -> StoredGroup {
        let id = GroupId::generate();
        let created_at_ns = now_ns();
        let membership_state = state.unwrap_or(GroupMembershipState::Allowed);
        StoredGroup::builder()
            .id(id)
            .created_at_ns(created_at_ns)
            .membership_state(membership_state)
            .added_by_inbox_id("placeholder_address")
            .sequence_id(welcome_id.unwrap_or(xmtp_common::rand_i64()))
            .originator_id(Originators::WELCOME_MESSAGES as i64)
            .conversation_type(ConversationType::Group)
            .build()
            .unwrap()
    }

    /// Generate a test consent
    pub fn generate_consent_record(
        entity_type: ConsentType,
        state: ConsentState,
        entity: String,
    ) -> StoredConsentRecord {
        StoredConsentRecord {
            entity_type,
            state,
            entity,
            consented_at_ns: now_ns(),
        }
    }

    #[xmtp_common::test]
    async fn test_it_stores_group() {
        with_connection(async |conn| {
            let test_group = generate_group(None);

            test_group.store(conn).await.unwrap();
            assert_eq!(
                conn.raw_query(|raw_conn| groups.first::<StoredGroup>(raw_conn))
                    .unwrap(),
                test_group
            );
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_it_fetches_group() {
        with_connection(async |conn| {
            let test_group = generate_group(None);

            conn.raw_query(|raw_conn| {
                diesel::insert_into(groups)
                    .values(test_group.clone())
                    .execute(raw_conn)
            })
            .unwrap();

            let fetched_group: Option<StoredGroup> =
                crate::Fetch::<StoredGroup>::fetch(conn, &test_group.id)
                    .await
                    .unwrap();
            assert_eq!(fetched_group, Some(test_group));
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_it_updates_group_membership_state() {
        with_connection(async |conn| {
            let test_group = generate_group(Some(GroupMembershipState::Pending));

            test_group.store(conn).await.unwrap();
            conn.update_group_membership(&test_group.id, GroupMembershipState::Rejected)
                .await
                .unwrap();

            let updated_group: StoredGroup =
                crate::Fetch::<StoredGroup>::fetch(conn, &test_group.id)
                    .await
                    .ok()
                    .flatten()
                    .unwrap();
            assert_eq!(
                updated_group,
                StoredGroup {
                    membership_state: GroupMembershipState::Rejected,
                    ..test_group
                }
            );
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_find_groups() {
        let wait_in_wasm = async || {
            // web has current time resolution only to millisecond,
            // which is too slow for this test to pass and the timestamps to be different
            // force generated groups to be created at different times

            if cfg!(target_arch = "wasm32") {
                xmtp_common::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        };
        with_connection_async(|conn| async move {
            let test_group_1 = generate_group(Some(GroupMembershipState::Pending));
            test_group_1.store(&conn).await.unwrap();
            wait_in_wasm().await;
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(&conn).await.unwrap();
            wait_in_wasm().await;
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(&conn).await.unwrap();

            let other_inbox_id = test_group_3
                .dm_id
                .unwrap()
                .other_inbox_id("placeholder_inbox_id_1");

            let all_results = conn
                .find_groups(&GroupQueryArgs {
                    conversation_type: Some(ConversationType::Group),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(all_results.len(), 2);

            let pending_results = conn
                .find_groups(&GroupQueryArgs {
                    allowed_states: Some(vec![GroupMembershipState::Pending]),
                    conversation_type: Some(ConversationType::Group),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(pending_results[0].id, test_group_1.id);
            assert_eq!(pending_results.len(), 1);

            // Offset and limit
            let results_with_limit = conn
                .find_groups(&GroupQueryArgs {
                    conversation_type: Some(ConversationType::Group),
                    limit: Some(1),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(results_with_limit.len(), 1);
            assert_eq!(results_with_limit[0].id, test_group_1.id);

            let results_with_created_at_ns_after = conn
                .find_groups(&GroupQueryArgs {
                    conversation_type: Some(ConversationType::Group),
                    limit: Some(1),
                    created_after_ns: Some(test_group_1.created_at_ns),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(results_with_created_at_ns_after.len(), 1);
            assert_eq!(results_with_created_at_ns_after[0].id, test_group_2.id);

            // Sync groups SHOULD NOT be returned
            let synced_groups = conn.primary_sync_group().await.unwrap();
            assert!(synced_groups.is_none());

            // test that dm groups are included
            let dm_results = conn.find_groups(&GroupQueryArgs::default()).await.unwrap();
            assert_eq!(dm_results.len(), 3);
            assert_eq!(dm_results[2].id, test_group_3.id);

            // test find_dm_group
            let dm_result = conn
                .find_active_dm_group(&format!("dm:placeholder_inbox_id_1:{}", other_inbox_id))
                .await
                .unwrap();
            assert!(dm_result.is_some());

            // test only dms are returned
            let dm_results = conn
                .find_groups(&GroupQueryArgs {
                    conversation_type: Some(ConversationType::Dm),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(dm_results.len(), 1);
            assert_eq!(dm_results[0].id, test_group_3.id);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_installations_last_checked_is_updated() {
        with_connection_async(|conn| async move {
            let test_group = generate_group(None);
            test_group.store(&conn).await.unwrap();

            // Check that the installations update has not been performed, yet
            assert_eq!(test_group.installations_last_checked, 0);

            if cfg!(target_arch = "wasm32") {
                // web has current time resolution only to millisecond,
                // which is too slow for this test to pass and the timestamps to be different
                xmtp_common::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            // Check that some event occurred which triggers an installation list update.
            // Here we invoke that event directly
            let result = conn.update_installations_time_checked(&test_group.id);
            assert_ok!(result.await);

            // Check that the latest installation list timestamp has been updated
            let fetched_group: StoredGroup =
                crate::Fetch::<StoredGroup>::fetch(&conn, &test_group.id)
                    .await
                    .ok()
                    .flatten()
                    .unwrap();
            assert_ne!(fetched_group.installations_last_checked, 0);
            assert!(fetched_group.created_at_ns < fetched_group.installations_last_checked);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_new_group_has_correct_purpose() {
        with_connection(async |conn| {
            let test_group = generate_group(None);

            conn.raw_query(|raw_conn| {
                diesel::insert_into(groups)
                    .values(test_group.clone())
                    .execute(raw_conn)
            })
            .unwrap();

            let fetched_group: Option<StoredGroup> =
                crate::Fetch::<StoredGroup>::fetch(conn, &test_group.id)
                    .await
                    .unwrap();
            assert_eq!(fetched_group, Some(test_group));
            let conversation_type = fetched_group.unwrap().conversation_type;
            assert_eq!(conversation_type, ConversationType::Group);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_find_groups_by_consent_state() {
        with_connection(async |conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_1.store(conn).await.unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).await.unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).await.unwrap();
            let test_group_4 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_4.store(conn).await.unwrap();

            let test_group_1_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_1.id),
            );
            test_group_1_consent.store(conn).await.unwrap();
            let test_group_2_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(test_group_2.id),
            );
            test_group_2_consent.store(conn).await.unwrap();
            let test_group_3_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_3.id),
            );
            test_group_3_consent.store(conn).await.unwrap();

            let all_results = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some(vec![
                        ConsentState::Allowed,
                        ConsentState::Unknown,
                        ConsentState::Denied,
                    ]),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(all_results.len(), 4);

            let default_results = conn.find_groups(&GroupQueryArgs::default()).await.unwrap();
            assert_eq!(default_results.len(), 3);

            let allowed_results = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed]),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(allowed_results.len(), 2);

            let allowed_unknown_results = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(allowed_unknown_results.len(), 3);

            let denied_results = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Denied]),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(denied_results.len(), 1);
            assert_eq!(denied_results[0].id, test_group_2.id);

            let unknown_results = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Unknown]),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(unknown_results.len(), 1);
            assert_eq!(unknown_results[0].id, test_group_4.id);

            let empty_array_results = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some(vec![]),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(empty_array_results.len(), 3);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_get_sequence_ids() {
        with_connection(async |conn| {
            let mls_groups = [
                generate_group_with_welcome(None, Some(30)),
                generate_group(None),
                generate_group(None),
                generate_group_with_welcome(None, Some(10)),
            ];
            for g in mls_groups.iter() {
                g.store(conn).await.unwrap();
            }
            assert_eq!(
                vec![30, 10],
                conn.group_cursors()
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|c| c.sequence_id)
                    .collect::<Vec<u64>>()
            );
        })
        .await
    }

    /// Regression test for the `group_cursors` abort
    /// (`if seq is not null, originator must not be null`).
    ///
    /// A group can legitimately exist locally with no cursor (the creator /
    /// local-create path stores `sequence_id = NULL, originator_id = NULL`). When a
    /// welcome for that same, already-existing group is later processed,
    /// `insert_or_replace_group` takes its "group already exists" update branch. That
    /// update must co-set `originator_id` with `sequence_id`; otherwise the row lands
    /// in the invalid `(sequence_id = NOT NULL, originator_id = NULL)` state that
    /// aborts `group_cursors()` on the next conversation-stream startup.
    #[xmtp_common::test]
    async fn test_insert_or_replace_group_update_preserves_originator() {
        with_connection(async |conn| {
            // 1. Cursorless group created locally (both fields NULL — valid).
            let group = generate_group(None);
            assert!(group.sequence_id.is_none());
            assert!(group.originator_id.is_none());
            group.store(conn).await.unwrap();

            // 2. A welcome for the same group arrives carrying a real v3 welcome
            //    cursor (seq set, originator = WELCOME_MESSAGES = 11).
            let incoming = StoredGroup {
                sequence_id: Some(5),
                originator_id: Some(Originators::WELCOME_MESSAGES as i64),
                ..group.clone()
            };
            conn.insert_or_replace_group(incoming).await.unwrap();

            // 3. The stored row must keep the invariant: seq set => originator set.
            let stored: StoredGroup = crate::Fetch::<StoredGroup>::fetch(conn, &group.id)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(stored.sequence_id, Some(5));
            assert_eq!(
                stored.originator_id,
                Some(Originators::WELCOME_MESSAGES as i64),
                "update path must co-set originator_id with sequence_id"
            );

            // 4. group_cursors() (run on conversation-stream startup) must not abort.
            let cursors = conn.group_cursors().await.unwrap();
            assert_eq!(cursors.len(), 1);
            assert_eq!(cursors[0].sequence_id, 5);
            assert_eq!(cursors[0].originator_id, Originators::WELCOME_MESSAGES);
        })
        .await
    }

    /// Defense in depth: even if a legacy / half-populated row already exists with
    /// `sequence_id` set but `originator_id` NULL (users may be in this state in the
    /// wild since the originator-id migration), `group_cursors()` must not abort the
    /// stream. It should skip the bad row instead of `.expect()`-panicking.
    #[xmtp_common::test]
    async fn test_group_cursors_skips_row_with_null_originator() {
        with_connection(async |conn| {
            // A healthy cursor'd group.
            let good = generate_group_with_welcome(None, Some(30));
            good.store(conn).await.unwrap();

            // A cursorless group, then force the invalid state directly via a raw
            // UPDATE that sets sequence_id only (mimicking the pre-fix write path and
            // legacy data). Deliberately bypasses the builder invariant.
            let bad = generate_group(None);
            bad.store(conn).await.unwrap();
            conn.raw_query(|c| {
                diesel::update(dsl::groups.find(&bad.id))
                    .set(dsl::sequence_id.eq(Some(7i64)))
                    .execute(c)
            })
            .unwrap();

            // Must not panic; the bad row is skipped, the good one is returned.
            let cursors = conn.group_cursors().await.unwrap();
            assert_eq!(cursors.len(), 1);
            assert_eq!(cursors[0].sequence_id, 30);
            assert_eq!(cursors[0].originator_id, Originators::WELCOME_MESSAGES);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_find_group_default_excludes_denied() {
        with_connection(async |conn| {
            // Create three groups: one allowed, one denied, one unknown (no consent)
            let allowed_group = generate_group(Some(GroupMembershipState::Allowed));
            allowed_group.store(conn).await.unwrap();

            let denied_group = generate_group(Some(GroupMembershipState::Allowed));
            denied_group.store(conn).await.unwrap();

            let unknown_group = generate_group(Some(GroupMembershipState::Allowed));
            unknown_group.store(conn).await.unwrap();

            // Create consent records for allowed and denied; leave unknown_group without one
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).await.unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
            );
            denied_consent.store(conn).await.unwrap();

            // Query using default args (no consent_states specified)
            let default_results = conn.find_groups(&GroupQueryArgs::default()).await.unwrap();

            // Expect to include only: allowed_group and unknown_group (2 total)
            assert_eq!(default_results.len(), 2);
            let returned_ids: Vec<_> = default_results.iter().map(|g| &g.id).collect();
            assert!(returned_ids.contains(&&allowed_group.id));
            assert!(returned_ids.contains(&&unknown_group.id));
            assert!(!returned_ids.contains(&&denied_group.id));
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_conversation_ids_for_remote_log_publish() {
        with_connection(async |conn| {
            let mut group1 = generate_group(None);
            let mut group2 = generate_group(None);
            let mut group3 = generate_group(None);
            let mut group4 = generate_group(None);
            group1.should_publish_commit_log = true;
            group1.commit_log_public_key = None;
            generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(group1.id),
            )
            .store(conn)
            .await?;
            group2.should_publish_commit_log = true;
            group2.commit_log_public_key = Some(rand_vec::<32>());

            group3.should_publish_commit_log = true;
            group3.commit_log_public_key = Some(rand_vec::<32>());
            generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(group3.id),
            )
            .store(conn)
            .await?;
            group4.should_publish_commit_log = false;
            group1.store(conn).await?;
            group2.store(conn).await?;
            group3.store(conn).await?;
            group4.store(conn).await?;

            let commit_log_keys = conn
                .get_conversation_ids_for_remote_log_publish()
                .await
                .unwrap();
            assert_eq!(commit_log_keys.len(), 2);
            assert_eq!(commit_log_keys[0].id, group1.id);
            assert_eq!(commit_log_keys[1].id, group3.id);
            assert_eq!(commit_log_keys[0].commit_log_public_key, None);
            assert_eq!(
                commit_log_keys[1].commit_log_public_key,
                group3.commit_log_public_key
            );
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_get_conversation_ids_for_remote_log_publish_with_consent() {
        with_connection(async |conn| {
            // Create groups: one with Allowed consent, one with Denied consent, one with no consent
            let mut allowed_group = generate_group(None);
            allowed_group.should_publish_commit_log = true;
            allowed_group.store(conn).await.unwrap();

            let mut denied_group = generate_group(None);
            denied_group.should_publish_commit_log = true;
            denied_group.store(conn).await.unwrap();

            let mut no_consent_group = generate_group(None);
            no_consent_group.should_publish_commit_log = true;
            no_consent_group.store(conn).await.unwrap();

            // Create consent records
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).await.unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
            );
            denied_consent.store(conn).await.unwrap();

            // Function should only return groups with Allowed consent state
            let commit_log_keys = conn
                .get_conversation_ids_for_remote_log_publish()
                .await
                .unwrap();
            assert_eq!(commit_log_keys.len(), 1);
            assert_eq!(commit_log_keys[0].id, allowed_group.id);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_get_conversation_ids_for_remote_log_download_with_consent() {
        with_connection(async |conn| {
            // Create groups: one with Allowed consent, one with Denied consent, one with no consent
            let allowed_group = generate_group(None);
            allowed_group.store(conn).await.unwrap();

            let denied_group = generate_group(None);
            denied_group.store(conn).await.unwrap();

            let no_consent_group = generate_group(None);
            no_consent_group.store(conn).await.unwrap();

            // Create a sync group (should be excluded regardless of consent)
            let mut sync_group = generate_group(None);
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn).await.unwrap();
            let sync_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(sync_group.id),
            );
            sync_consent.store(conn).await.unwrap();

            // Create consent records
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).await.unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
            );
            denied_consent.store(conn).await.unwrap();

            // Function should only return groups with Allowed consent state, excluding sync groups
            let conversation_ids = conn
                .get_conversation_ids_for_remote_log_download()
                .await
                .unwrap();
            assert_eq!(conversation_ids.len(), 1);
            assert_eq!(conversation_ids[0].id, allowed_group.id);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_get_conversation_ids_for_responding_readds() {
        with_connection(async |conn| {
            // Create test groups
            let group_id_1 = GroupId::ONE;
            let group_id_2 = GroupId::TWO;
            let group_id_3 = GroupId::THREE;

            let group1 = StoredGroup::builder()
                .id(group_id_1)
                .created_at_ns(1000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group1.store(conn).await.unwrap();

            let group2 = StoredGroup::builder()
                .id(group_id_2)
                .created_at_ns(2000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group2.store(conn).await.unwrap();

            let group3 = StoredGroup::builder()
                .id(group_id_3)
                .created_at_ns(3000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group3.store(conn).await.unwrap();

            // Create readd status entries with various test cases
            let test_cases = vec![
                // Case 1: Pending readd (requested_at > responded_at)
                ReaddStatus {
                    group_id: group_id_1,
                    installation_id: vec![1],
                    requested_at_sequence_id: Some(10),
                    responded_at_sequence_id: Some(5),
                },
                // Case 2: Pending readd (responded_at is None)
                ReaddStatus {
                    group_id: group_id_1,
                    installation_id: vec![2],
                    requested_at_sequence_id: Some(8),
                    responded_at_sequence_id: None,
                },
                // Case 4: Not pending (requested_at < responded_at)
                ReaddStatus {
                    group_id: group_id_2,
                    installation_id: vec![4],
                    requested_at_sequence_id: Some(12),
                    responded_at_sequence_id: Some(15),
                },
                // Case 5: Not pending (requested_at is None)
                ReaddStatus {
                    group_id: group_id_2,
                    installation_id: vec![5],
                    requested_at_sequence_id: None,
                    responded_at_sequence_id: Some(20),
                },
                // Case 6: Pending readd (requested_at == responded_at, should be pending)
                ReaddStatus {
                    group_id: group_id_3,
                    installation_id: vec![6],
                    requested_at_sequence_id: Some(25),
                    responded_at_sequence_id: Some(25),
                },
            ];

            // Store all test cases
            for status in test_cases {
                status.store(conn).await.unwrap();
            }

            // Call the method under test
            let result = conn
                .get_conversation_ids_for_responding_readds()
                .await
                .unwrap();

            // Should return groups 1 and 3 (both have pending readd requests)
            // Group 2 has no pending readds
            assert_eq!(result.len(), 2);

            // Results should be sorted by group_id (since we used distinct())
            let mut result_group_ids: Vec<GroupId> = result.iter().map(|r| r.group_id).collect();
            result_group_ids.sort();

            assert_eq!(result_group_ids[0], group_id_1);
            assert_eq!(result_group_ids[1], group_id_3);

            // Check that the correct metadata is returned
            let group1_result = result.iter().find(|r| r.group_id == group_id_1).unwrap();
            assert_eq!(group1_result.dm_id, None);
            assert_eq!(group1_result.conversation_type, ConversationType::Group);
            assert_eq!(group1_result.created_at_ns, 1000);

            let group3_result = result.iter().find(|r| r.group_id == group_id_3).unwrap();
            assert_eq!(group3_result.dm_id, None);
            assert_eq!(group3_result.conversation_type, ConversationType::Group);
            assert_eq!(group3_result.created_at_ns, 3000);
        })
        .await
    }

    /// Regression guard for the `find_group` query-span instrumentation.
    ///
    /// `find_group` is annotated with `#[xmtp_common::db_span]`, which expands to
    /// `#[tracing::instrument(err, skip_all, fields(operation = "db.find_group"))]`.
    /// Two contracts must hold for the DB telemetry to be useful and safe:
    ///   1. the span carries `operation = "db.find_group"` (the metric dimension
    ///      consumed downstream), and
    ///   2. `skip_all` keeps the raw `group_id` argument OUT of the span — a leak
    ///      of the per-group id as a span field would explode metric cardinality.
    ///
    /// Capture mechanism: a tiny in-crate `tracing::Subscriber` that records the
    /// fields of every span created while it is the default dispatcher. We do NOT
    /// use `xmtp_common::traced_test!` / a `tracing_subscriber` fmt subscriber
    /// because `xmtp_db` does not depend on the `tracing-subscriber` crate (only
    /// `tracing` is a direct dependency), so naming it in source would fail to
    /// compile. A custom `Subscriber` also lets us read span *attributes* directly
    /// at `new_span` time, which is exactly where the `instrument` macro records
    /// the static `operation` field — no event needs to fire inside the span and
    /// no span-event/`FmtSpan` configuration is required, so the assertion is
    /// deterministic and not flaky. Scoping via `with_default` around only the
    /// `find_group` call keeps unrelated framework spans out of the buffer.
    #[xmtp_common::test]
    async fn test_find_group_span_emits_operation_and_skips_group_id() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        };
        use tracing::field::{Field, Visit};

        /// Records `field=Debug(value)` pairs for every span created while
        /// installed, appending them to a shared, thread-safe buffer.
        #[derive(Clone, Default)]
        struct CaptureSubscriber {
            buf: Arc<parking_lot::Mutex<String>>,
            next_id: Arc<AtomicU64>,
        }

        struct FieldVisitor<'a>(&'a mut String);
        impl Visit for FieldVisitor<'_> {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(field.name());
                self.0.push('=');
                self.0.push_str(&format!("{value:?}"));
                self.0.push(' ');
            }
        }

        impl tracing::Subscriber for CaptureSubscriber {
            fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
                true
            }

            fn new_span(&self, attrs: &tracing::span::Attributes<'_>) -> tracing::span::Id {
                let mut line = String::new();
                line.push_str("SPAN ");
                line.push_str(attrs.metadata().name());
                line.push_str(" {");
                let mut visitor = FieldVisitor(&mut line);
                attrs.record(&mut visitor);
                line.push_str("}\n");
                self.buf.lock().push_str(&line);

                // Hand out a non-zero, monotonically increasing id per span.
                let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::span::Id::from_u64(id)
            }

            fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
            fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {
            }
            fn event(&self, _event: &tracing::Event<'_>) {}
            fn enter(&self, _span: &tracing::span::Id) {}
            fn exit(&self, _span: &tracing::span::Id) {}
        }

        with_connection(async |conn| {
            // Insert a group so `find_group` exercises a real (Ok) query path.
            let test_group = generate_group(None);
            conn.raw_query(|raw_conn| {
                diesel::insert_into(groups)
                    .values(test_group.clone())
                    .execute(raw_conn)
            })
            .unwrap();

            let capture = CaptureSubscriber::default();

            // Scope the subscriber tightly around the single instrumented call so
            // only `find_group`'s span lands in the buffer.
            tracing::subscriber::with_default(capture.clone(), || {
                // The span opens when the future is *polled*, so it has to be
                // driven inside this scope -- awaiting outside would poll it
                // after the guard dropped and capture nothing. The SQLite backend's
                // futures are await-free, so a single poll resolves it here.
                use futures::FutureExt;
                let _ = conn.find_group(&test_group.id).now_or_never();
            });

            let logged = capture.buf.lock().clone();

            // Contract 1: the operation metric dimension is present.
            assert!(
                logged.contains("operation=\"db.find_group\""),
                "expected find_group span to carry operation=\"db.find_group\", got:\n{logged}"
            );

            // Contract 2: skip_all must keep the raw arg out of the span. The arg
            // is `id: &GroupId`, so a leak shows up as an `id=` field (GroupId's
            // Debug renders `GroupId(<hex>)`). Asserting on the parameter name `id=`
            // (not the substring "group_id", which `GroupId(..)` would not contain)
            // makes this a real regression guard: dropping skip_all would fail it.
            assert!(
                !logged.contains("id="),
                "skip_all contract violated: the `id` arg leaked into the find_group \
                 span as a field (cardinality risk), got:\n{logged}"
            );
            // Stronger: `operation` must be the ONLY field on the span — no other
            // `<name>=` pair may appear between the braces.
            let fields = logged
                .split_once('{')
                .and_then(|(_, rest)| rest.split_once('}'))
                .map(|(inner, _)| inner.trim())
                .unwrap_or("");
            assert_eq!(
                fields, "operation=\"db.find_group\"",
                "find_group span must carry only the operation field, got fields: {fields:?}"
            );
        })
        .await
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sqlite")`.
#[cfg(all(feature = "sqlx", not(feature = "sqlite"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};

    /// Conversation types a listing never returns, as the integers the column
    /// stores. Arrays of the `#[repr(i32)]` enums have no `PgHasArrayType`, so
    /// every array bind converts first -- the same idiom as `QueryConsentRecord`.
    fn virtual_type_ints() -> Vec<i32> {
        ConversationType::virtual_types()
            .into_iter()
            .map(|t| t as i32)
            .collect()
    }

    /// The consent lookup both `find_groups` and the commit-log listings need.
    ///
    /// `consent_records.entity` holds a group id as lowercase hex, which is what
    /// `encode(id, 'hex')` produces directly -- the sync path spells the same
    /// thing `lower(hex(groups.id))` because SQLite's `hex()` is uppercase.
    const INNER_CONSENT_JOIN: &str =
        "INNER JOIN consent_records c ON encode(groups.id, 'hex') = c.entity";
    /// As above, but keeping groups that have no consent record at all.
    const LEFT_CONSENT_JOIN: &str =
        "LEFT JOIN consent_records c ON encode(groups.id, 'hex') = c.entity";

    /// Keeps only the most recently active row per stitched DM.
    ///
    /// `encode(id, 'hex')` stands in for the sync path's bare `id`: both arms of
    /// a Postgres `COALESCE` must share a type, and a `dm:a:b` string can never
    /// collide with a hex id, so the grouping is unchanged.
    const LATEST_PER_DM: &str = "NOT EXISTS (
             SELECT 1 FROM groups g2
             WHERE COALESCE(g2.dm_id, encode(g2.id, 'hex'))
                 = COALESCE(groups.dm_id, encode(groups.id, 'hex'))
               AND (COALESCE(g2.last_message_ns, 0), g2.id)
                 > (COALESCE(groups.last_message_ns, 0), groups.id)
         )";

    /// Shared insert for `Store`/`StoreOrIgnore`. The `groups` PK `id` is the
    /// caller-supplied group id (not DB-assigned), so every column is written;
    /// the list and placeholders come from `PgModel` so they cannot drift from
    /// the struct's field order that the binds below follow.
    async fn insert_group(
        g: &StoredGroup,
        into: &impl crate::PgConnectionProvider,
        on_conflict_ignore: bool,
    ) -> Result<(), crate::StorageError> {
        let placeholders = (1..=StoredGroup::COLUMNS.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let conflict = if on_conflict_ignore {
            " ON CONFLICT DO NOTHING"
        } else {
            ""
        };
        let sql = format!(
            "INSERT INTO groups ({}) VALUES ({}){}",
            StoredGroup::select_columns(),
            placeholders,
            conflict
        );
        let mut c = into.pg_conn().await?;
        sqlx::query(&sql)
            .bind(g.id)
            .bind(g.created_at_ns)
            .bind(g.membership_state)
            .bind(g.installations_last_checked)
            .bind(&g.added_by_inbox_id)
            .bind(g.sequence_id)
            .bind(g.rotated_at_ns)
            .bind(g.conversation_type)
            .bind(&g.dm_id)
            .bind(g.last_message_ns)
            .bind(g.message_disappear_from_ns)
            .bind(g.message_disappear_in_ns)
            .bind(&g.paused_for_version)
            .bind(g.maybe_forked)
            .bind(&g.fork_details)
            .bind(g.originator_id)
            .bind(g.should_publish_commit_log)
            .bind(&g.commit_log_public_key)
            .bind(g.is_commit_log_forked)
            .bind(g.has_pending_leave_request)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
        Ok(())
    }

    impl<C: crate::PgConnectionProvider> crate::Store<C> for StoredGroup {
        type Output = ();
        async fn store(&self, into: &C) -> Result<(), crate::StorageError> {
            insert_group(self, into, false).await
        }
    }

    impl<C: crate::PgConnectionProvider> crate::StoreOrIgnore<C> for StoredGroup {
        type Output = ();
        async fn store_or_ignore(&self, into: &C) -> Result<(), crate::StorageError> {
            insert_group(self, into, true).await
        }
    }

    impl<C: crate::PgConnectionProvider> crate::Fetch<StoredGroup> for C {
        type Key = GroupId;
        async fn fetch(&self, key: &Self::Key) -> Result<Option<StoredGroup>, crate::StorageError> {
            use sqlx::FromRow;
            let mut c = self.pg_conn().await?;
            let row = sqlx::query(&format!(
                "SELECT {} FROM groups WHERE id = $1 LIMIT 1",
                StoredGroup::select_columns()
            ))
            .bind(key)
            .fetch_optional(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            row.as_ref()
                .map(|r| StoredGroup::from_row(r).map_err(crate::ConnectionError::from))
                .transpose()
                .map_err(Into::into)
        }
    }

    impl QueryGroup for PgDb {
        /// Every optional filter is expressed as `$n IS NULL OR ...` so one bind
        /// order serves all combinations; only the consent join changes the
        /// query's *shape*, and it is the last parameter so the rest keep their
        /// numbers.
        async fn find_groups(
            &self,
            args: &GroupQueryArgs,
        ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
            args.validate()?;

            let GroupQueryArgs {
                allowed_states,
                created_after_ns,
                created_before_ns,
                limit,
                conversation_type,
                consent_states,
                include_sync_groups,
                include_duplicate_dms,
                last_activity_after_ns,
                last_activity_before_ns,
                should_publish_commit_log,
                order_by,
            } = args;

            let default_states = [ConsentState::Allowed, ConsentState::Unknown];
            let effective_consent_states = match consent_states {
                Some(states) if !states.is_empty() => states.as_slice(),
                _ => &default_states[..],
            };
            let includes_all = effective_consent_states.len() == 3;
            let includes_unknown = effective_consent_states.contains(&ConsentState::Unknown);
            let consent_ints: Vec<i32> =
                effective_consent_states.iter().map(|s| *s as i32).collect();

            let (join, consent_filter) = if includes_all {
                // Every state matches, so the join would only cost rows.
                ("", "TRUE")
            } else if includes_unknown {
                // LEFT JOIN keeps groups with no consent row at all. The sync
                // impl ORs `state = Unknown` with the remaining states; the union
                // of those is just `effective_consent_states`, which is $10.
                (
                    LEFT_CONSENT_JOIN,
                    "(c.state IS NULL OR c.state = ANY($10::int4[]))",
                )
            } else {
                (INNER_CONSENT_JOIN, "c.state = ANY($10::int4[])")
            };

            let dedup = if *include_duplicate_dms {
                "TRUE"
            } else {
                LATEST_PER_DM
            };

            // Both orderings are over NOT NULL expressions, so neither needs
            // NULLS LAST.
            let order = match order_by.clone().unwrap_or_default() {
                GroupQueryOrderBy::CreatedAt => "groups.created_at_ns ASC",
                GroupQueryOrderBy::LastActivity => {
                    "COALESCE(groups.last_message_ns, groups.created_at_ns) DESC"
                }
            };

            let sql = format!(
                "SELECT {cols} FROM groups {join} \
                 WHERE groups.conversation_type <> ALL($1::int4[]) \
                   AND ($2::bigint IS NULL OR groups.created_at_ns > $2) \
                   AND ($3::bigint IS NULL OR groups.created_at_ns < $3) \
                   AND ($4::bigint IS NULL \
                        OR COALESCE(groups.last_message_ns, groups.created_at_ns) > $4) \
                   AND ($5::bigint IS NULL \
                        OR COALESCE(groups.last_message_ns, groups.created_at_ns) < $5) \
                   AND ($6::int4 IS NULL OR groups.conversation_type = $6) \
                   AND ($7::bool IS NULL OR groups.should_publish_commit_log = $7) \
                   AND ($8::int4[] IS NULL OR groups.membership_state = ANY($8)) \
                   AND {dedup} AND {consent_filter} \
                 ORDER BY {order} LIMIT $9::bigint",
                cols = StoredGroup::select_columns_for("groups"),
            );

            let mut query = sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(virtual_type_ints())
                .bind(*created_after_ns)
                .bind(*created_before_ns)
                .bind(*last_activity_after_ns)
                .bind(*last_activity_before_ns)
                .bind(conversation_type.map(|t| t as i32))
                .bind(*should_publish_commit_log)
                .bind(
                    allowed_states
                        .as_ref()
                        .map(|states| states.iter().map(|s| *s as i32).collect::<Vec<_>>()),
                )
                .bind(*limit);
            if !includes_all {
                query = query.bind(consent_ints);
            }

            // One connection for both statements. The sync path runs them on its
            // single connection too, and re-acquiring would let an unrelated
            // writer land between the two reads.
            let mut c = self.conn().await?;
            let mut groups = query.fetch_all(&mut *c).await?;

            // Sync groups are excluded by the virtual-type filter above, so they
            // are fetched separately when asked for.
            if matches!(conversation_type, Some(ConversationType::Sync)) || *include_sync_groups {
                let sql = format!(
                    "SELECT {} FROM groups WHERE conversation_type = $1",
                    StoredGroup::select_columns()
                );
                let mut sync_groups = sqlx::query_as::<_, StoredGroup>(&sql)
                    .bind(ConversationType::Sync)
                    .fetch_all(&mut *c)
                    .await?;
                groups.append(&mut sync_groups);
            }

            Ok(groups)
        }

        async fn find_groups_by_id_paged(
            &self,
            args: &GroupQueryArgs,
            offset: i64,
        ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
            let GroupQueryArgs {
                created_after_ns,
                created_before_ns,
                limit,
                ..
            } = args;

            // `created_before_ns` is inclusive here and exclusive in
            // `find_groups`; that asymmetry is the sync path's and is preserved.
            let sql = format!(
                "SELECT {} FROM groups \
                 WHERE conversation_type <> ALL($1::int4[]) \
                   AND ($2::bigint IS NULL OR created_at_ns > $2) \
                   AND ($3::bigint IS NULL OR created_at_ns <= $3) \
                 ORDER BY id LIMIT $4 OFFSET $5",
                StoredGroup::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(virtual_type_ints())
                .bind(*created_after_ns)
                .bind(*created_before_ns)
                .bind(limit.unwrap_or(100))
                .bind(offset)
                .fetch_all(&mut *c)
                .await?)
        }

        async fn update_group_membership(
            &self,
            group_id: &GroupId,
            state: GroupMembershipState,
        ) -> Result<(), crate::ConnectionError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET membership_state = $1 WHERE id = $2")
                .bind(state)
                .bind(group_id)
                .execute(&mut *c)
                .await?;
            Ok(())
        }

        async fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM groups WHERE conversation_type = $1 ORDER BY created_at_ns DESC",
                StoredGroup::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(ConversationType::Sync)
                .fetch_all(&mut *c)
                .await?)
        }

        async fn find_sync_group(
            &self,
            id: &GroupId,
        ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM groups WHERE conversation_type = $1 AND id = $2 LIMIT 1",
                StoredGroup::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(ConversationType::Sync)
                .bind(id)
                .fetch_optional(&mut *c)
                .await?)
        }

        async fn primary_sync_group(&self) -> Result<Option<StoredGroup>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM groups WHERE conversation_type = $1 \
                 ORDER BY created_at_ns DESC LIMIT 1",
                StoredGroup::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(ConversationType::Sync)
                .fetch_optional(&mut *c)
                .await?)
        }

        async fn find_group(
            &self,
            id: &GroupId,
        ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM groups WHERE id = $1 ORDER BY created_at_ns ASC LIMIT 1",
                StoredGroup::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(id)
                .fetch_optional(&mut *c)
                .await?)
        }

        /// `LIMIT 2` rather than the sync path's unbounded load: one extra row is
        /// all it takes to know whether to warn, and the returned group is the
        /// same first row either way.
        async fn find_group_by_sequence_id(
            &self,
            cursor: Cursor,
        ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM groups WHERE sequence_id = $1 AND originator_id = $2 \
                 ORDER BY created_at_ns ASC LIMIT 2",
                StoredGroup::select_columns()
            );
            let mut c = self.conn().await?;
            let groups = sqlx::query_as::<_, StoredGroup>(&sql)
                .bind(cursor.sequence_id as i64)
                .bind(cursor.originator_id as i64)
                .fetch_all(&mut *c)
                .await?;

            if groups.len() > 1 {
                tracing::warn!(
                    cursor.sequence_id,
                    "More than one group found for welcome_id {}",
                    cursor.sequence_id
                );
            }
            Ok(groups.into_iter().next())
        }

        async fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError> {
            let mut c = self.conn().await?;
            let last_ts: Option<i64> =
                sqlx::query_scalar("SELECT rotated_at_ns FROM groups WHERE id = $1")
                    .bind(group_id)
                    .fetch_optional(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;

            last_ts.ok_or(StorageError::NotFound(NotFound::InstallationTimeForGroup(
                *group_id,
            )))
        }

        async fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET rotated_at_ns = $1 WHERE id = $2")
                .bind(xmtp_common::time::now_ns())
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn get_installations_time_checked(
            &self,
            group_id: &GroupId,
        ) -> Result<i64, StorageError> {
            let mut c = self.conn().await?;
            let last_ts: Option<i64> =
                sqlx::query_scalar("SELECT installations_last_checked FROM groups WHERE id = $1")
                    .bind(group_id)
                    .fetch_optional(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;

            last_ts.ok_or(NotFound::InstallationTimeForGroup(*group_id).into())
        }

        async fn update_installations_time_checked(
            &self,
            group_id: &GroupId,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET installations_last_checked = $1 WHERE id = $2")
                .bind(xmtp_common::time::now_ns())
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn update_message_disappearing_from_ns(
            &self,
            group_id: &GroupId,
            from_ns: Option<i64>,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET message_disappear_from_ns = $1 WHERE id = $2")
                .bind(from_ns)
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn update_message_disappearing_in_ns(
            &self,
            group_id: &GroupId,
            in_ns: Option<i64>,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET message_disappear_in_ns = $1 WHERE id = $2")
                .bind(in_ns)
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        /// Insert, or reconcile with the row that is already there.
        ///
        /// `atomic()` because the insert, the read-back and the two conditional
        /// updates are a read-modify-write; without it another writer could land
        /// between the read and the update. It also makes the duplicate-welcome
        /// error roll back the restore overwrite below, which is what the sync
        /// path gets from the openmls transaction its callers open.
        async fn insert_or_replace_group(
            &self,
            group: StoredGroup,
        ) -> Result<StoredGroup, StorageError> {
            // Bind order follows `StoredGroup`'s field order, which is what
            // `COLUMNS` is generated from. A field added without a bind fails
            // loudly at the first insert ("bind message supplies N parameters").
            let placeholders = (1..=StoredGroup::COLUMNS.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let cols = StoredGroup::select_columns();

            self.atomic(async |db| {
                let inserted: Option<StoredGroup> = {
                    let sql = format!(
                        "INSERT INTO groups ({cols}) VALUES ({placeholders}) \
                         ON CONFLICT (id) DO NOTHING RETURNING {cols}"
                    );
                    let mut c = db.conn().await?;
                    sqlx::query_as::<_, StoredGroup>(&sql)
                        .bind(group.id)
                        .bind(group.created_at_ns)
                        .bind(group.membership_state)
                        .bind(group.installations_last_checked)
                        .bind(&group.added_by_inbox_id)
                        .bind(group.sequence_id)
                        .bind(group.rotated_at_ns)
                        .bind(group.conversation_type)
                        .bind(&group.dm_id)
                        .bind(group.last_message_ns)
                        .bind(group.message_disappear_from_ns)
                        .bind(group.message_disappear_in_ns)
                        .bind(&group.paused_for_version)
                        .bind(group.maybe_forked)
                        .bind(&group.fork_details)
                        .bind(group.originator_id)
                        .bind(group.should_publish_commit_log)
                        .bind(&group.commit_log_public_key)
                        .bind(group.is_commit_log_forked)
                        .bind(group.has_pending_leave_request)
                        .fetch_optional(&mut *c)
                        .await
                        .map_err(crate::ConnectionError::from)?
                };

                // `RETURNING` already gives the stored row, so unlike the sync
                // path there is no read-back after a successful insert.
                if let Some(inserted) = inserted {
                    return Ok(inserted);
                }

                let mut existing: StoredGroup = {
                    let sql = format!("SELECT {cols} FROM groups WHERE id = $1");
                    let mut c = db.conn().await?;
                    sqlx::query_as::<_, StoredGroup>(&sql)
                        .bind(group.id)
                        .fetch_one(&mut *c)
                        .await
                        .map_err(crate::ConnectionError::from)?
                };

                // A restored group should be overwritten.
                if matches!(existing.membership_state, GroupMembershipState::Restored) {
                    // This mirrors diesel's `AsChangeset`, which skips the
                    // primary key and skips `Option` fields that are `None` --
                    // hence `COALESCE(new, existing)` on exactly the nullable
                    // columns and a plain assignment on the rest.
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "UPDATE groups SET \
                           created_at_ns = $2, \
                           membership_state = $3, \
                           installations_last_checked = $4, \
                           added_by_inbox_id = $5, \
                           sequence_id = COALESCE($6, sequence_id), \
                           rotated_at_ns = $7, \
                           conversation_type = $8, \
                           dm_id = COALESCE($9, dm_id), \
                           last_message_ns = COALESCE($10, last_message_ns), \
                           message_disappear_from_ns = COALESCE($11, message_disappear_from_ns), \
                           message_disappear_in_ns = COALESCE($12, message_disappear_in_ns), \
                           paused_for_version = COALESCE($13, paused_for_version), \
                           maybe_forked = $14, \
                           fork_details = $15, \
                           originator_id = COALESCE($16, originator_id), \
                           should_publish_commit_log = $17, \
                           commit_log_public_key = COALESCE($18, commit_log_public_key), \
                           is_commit_log_forked = COALESCE($19, is_commit_log_forked), \
                           has_pending_leave_request = \
                               COALESCE($20, has_pending_leave_request) \
                         WHERE id = $1",
                    )
                    .bind(group.id)
                    .bind(group.created_at_ns)
                    .bind(group.membership_state)
                    .bind(group.installations_last_checked)
                    .bind(&group.added_by_inbox_id)
                    .bind(group.sequence_id)
                    .bind(group.rotated_at_ns)
                    .bind(group.conversation_type)
                    .bind(&group.dm_id)
                    .bind(group.last_message_ns)
                    .bind(group.message_disappear_from_ns)
                    .bind(group.message_disappear_in_ns)
                    .bind(&group.paused_for_version)
                    .bind(group.maybe_forked)
                    .bind(&group.fork_details)
                    .bind(group.originator_id)
                    .bind(group.should_publish_commit_log)
                    .bind(&group.commit_log_public_key)
                    .bind(group.is_commit_log_forked)
                    .bind(group.has_pending_leave_request)
                    .execute(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;
                }

                // Compared against the pre-update row, as the sync path does --
                // the overwrite above does not refresh `existing`.
                if existing.sequence_id == group.sequence_id {
                    tracing::info!("Group welcome id already exists");
                    // Error so OpenMLS db transactions are rolled back on
                    // duplicate welcomes.
                    return Err(StorageError::Duplicate(DuplicateItem::WelcomeId(
                        existing.cursor(),
                    )));
                }

                tracing::info!("Group already exists");
                if group.sequence_id.is_some()
                    && (existing.sequence_id.is_none() || group.sequence_id > existing.sequence_id)
                {
                    // Co-set `originator_id` alongside `sequence_id`: the builder
                    // invariant pairs them, and writing only one would leave the
                    // row in a state `group_cursors()` has to skip.
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "UPDATE groups SET sequence_id = $1, originator_id = $2 WHERE id = $3",
                    )
                    .bind(group.sequence_id)
                    .bind(group.originator_id)
                    .bind(group.id)
                    .execute(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;
                    existing.sequence_id = group.sequence_id;
                    existing.originator_id = group.originator_id;
                }
                Ok(existing)
            })
            .await
        }

        async fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let rows: Vec<(Option<i64>, Option<i64>)> = sqlx::query_as(
                "SELECT sequence_id, originator_id FROM groups WHERE sequence_id IS NOT NULL",
            )
            .fetch_all(&mut *c)
            .await?;

            Ok(rows
                .into_iter()
                .filter_map(|(seq, orig)| match (seq, orig) {
                    (Some(seq), Some(orig)) => Some(Cursor::new(seq as u64, orig as u32)),
                    // Defense in depth, matching the sync path: a row with a
                    // `sequence_id` but a NULL `originator_id` violates the
                    // builder invariant, and skipping it beats aborting the
                    // whole conversation stream.
                    (Some(seq), None) => {
                        tracing::warn!(
                            sequence_id = seq,
                            "group row has sequence_id but NULL originator_id; skipping cursor"
                        );
                        None
                    }
                    (None, _) => None,
                })
                .collect())
        }

        async fn mark_group_as_maybe_forked(
            &self,
            group_id: &GroupId,
            fork_details: String,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET maybe_forked = TRUE, fork_details = $1 WHERE id = $2")
                .bind(fork_details)
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn clear_fork_flag_for_group(
            &self,
            group_id: &GroupId,
        ) -> Result<(), crate::ConnectionError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET maybe_forked = FALSE, fork_details = '' WHERE id = $1")
                .bind(group_id)
                .execute(&mut *c)
                .await?;
            Ok(())
        }

        /// One statement rather than the sync path's two. A missing group or a
        /// NULL `dm_id` makes the subquery NULL, `dm_id = NULL` matches nothing,
        /// and the count is 0 -- the same `false` the sync path returns.
        async fn has_duplicate_dm(
            &self,
            group_id: &GroupId,
        ) -> Result<bool, crate::ConnectionError> {
            let mut c = self.conn().await?;
            Ok(sqlx::query_scalar(
                "SELECT COUNT(*) > 1 FROM groups \
                 WHERE conversation_type = $1 \
                   AND dm_id = (SELECT dm_id FROM groups WHERE id = $2)",
            )
            .bind(ConversationType::Dm)
            .bind(group_id)
            .fetch_one(&mut *c)
            .await?)
        }

        async fn get_conversation_ids_for_remote_log_publish(
            &self,
        ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {cols} FROM groups {INNER_CONSENT_JOIN} \
                 WHERE (groups.conversation_type = $1 \
                        OR (groups.conversation_type = $2 \
                            AND groups.should_publish_commit_log = TRUE)) \
                   AND c.state = $3 \
                 ORDER BY groups.created_at_ns ASC",
                cols = StoredGroupCommitLogPublicKey::select_columns_for("groups"),
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupCommitLogPublicKey>(&sql)
                .bind(ConversationType::Dm)
                .bind(ConversationType::Group)
                .bind(ConsentState::Allowed)
                .fetch_all(&mut *c)
                .await?)
        }

        async fn get_conversation_ids_for_remote_log_download(
            &self,
        ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {cols} FROM groups {INNER_CONSENT_JOIN} \
                 WHERE groups.conversation_type <> ALL($1::int4[]) AND c.state = $2",
                cols = StoredGroupCommitLogPublicKey::select_columns_for("groups"),
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupCommitLogPublicKey>(&sql)
                .bind(virtual_type_ints())
                .bind(ConsentState::Allowed)
                .fetch_all(&mut *c)
                .await?)
        }

        async fn get_conversation_ids_for_fork_check(
            &self,
        ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            Ok(sqlx::query_scalar(
                "SELECT id FROM groups \
                 WHERE conversation_type <> ALL($1::int4[]) \
                   AND (is_commit_log_forked IS NULL OR is_commit_log_forked <> TRUE)",
            )
            .bind(virtual_type_ints())
            .fetch_all(&mut *c)
            .await?)
        }

        async fn get_conversation_ids_for_requesting_readds(
            &self,
        ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            // Mapped by hand because `latest_commit_sequence_id` is an aggregate
            // rather than a column, so `PgModel` has nothing to derive from. The
            // `try_get`s are by name, matching the aliases below.
            let rows = sqlx::query(
                "SELECT groups.id AS group_id, \
                        MAX(rcl.commit_sequence_id) AS latest_commit_sequence_id \
                 FROM groups LEFT JOIN remote_commit_log rcl ON groups.id = rcl.group_id \
                 WHERE groups.conversation_type <> ALL($1::int4[]) \
                   AND groups.is_commit_log_forked = TRUE \
                 GROUP BY groups.id",
            )
            .bind(virtual_type_ints())
            .fetch_all(&mut *c)
            .await?;

            rows.iter()
                .map(|row| {
                    use sqlx::Row;
                    Ok(StoredGroupForReaddRequest {
                        group_id: row.try_get("group_id")?,
                        latest_commit_sequence_id: row.try_get("latest_commit_sequence_id")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()
                .map_err(Into::into)
        }

        async fn get_conversation_ids_for_responding_readds(
            &self,
        ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError> {
            let sql = format!(
                "SELECT DISTINCT {cols} \
                 FROM readd_status r INNER JOIN groups ON r.group_id = groups.id \
                 WHERE r.requested_at_sequence_id IS NOT NULL \
                   AND (r.requested_at_sequence_id >= r.responded_at_sequence_id \
                        OR r.responded_at_sequence_id IS NULL)",
                cols = StoredGroupForRespondingReadds::select_columns_for("groups"),
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupForRespondingReadds>(&sql)
                .fetch_all(&mut *c)
                .await?)
        }

        /// A missing group is an error, matching the sync path's `first()`.
        async fn get_conversation_type(
            &self,
            group_id: &GroupId,
        ) -> Result<ConversationType, crate::ConnectionError> {
            let mut c = self.conn().await?;
            Ok(
                sqlx::query_scalar("SELECT conversation_type FROM groups WHERE id = $1")
                    .bind(group_id)
                    .fetch_one(&mut *c)
                    .await?,
            )
        }

        /// The `IS NULL` guard makes this write-once: a second key for the same
        /// group updates no rows and is reported as a duplicate.
        async fn set_group_commit_log_public_key(
            &self,
            group_id: &GroupId,
            public_key: &[u8],
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            let updated = sqlx::query(
                "UPDATE groups SET commit_log_public_key = $1 \
                 WHERE id = $2 AND commit_log_public_key IS NULL",
            )
            .bind(public_key)
            .bind(group_id)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?
            .rows_affected();

            if updated == 0 {
                return Err(StorageError::Duplicate(DuplicateItem::CommitLogPublicKey(
                    group_id.as_ref().to_vec(),
                )));
            }
            Ok(())
        }

        async fn set_group_commit_log_forked_status(
            &self,
            group_id: &GroupId,
            is_forked: Option<bool>,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET is_commit_log_forked = $1 WHERE id = $2")
                .bind(is_forked)
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        /// A missing group is an error, matching the sync path's `first()`; the
        /// `Option` is the column's own nullability.
        async fn get_group_commit_log_forked_status(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<bool>, StorageError> {
            let mut c = self.conn().await?;
            Ok(
                sqlx::query_scalar("SELECT is_commit_log_forked FROM groups WHERE id = $1")
                    .bind(group_id)
                    .fetch_one(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?,
            )
        }

        async fn set_group_has_pending_leave_request_status(
            &self,
            group_id: &GroupId,
            has_pending_leave_request: Option<bool>,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("UPDATE groups SET has_pending_leave_request = $1 WHERE id = $2")
                .bind(has_pending_leave_request)
                .bind(group_id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn get_groups_have_pending_leave_request(
            &self,
        ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            Ok(sqlx::query_scalar(
                "SELECT id FROM groups \
                 WHERE conversation_type <> $1 AND has_pending_leave_request = TRUE",
            )
            .bind(ConversationType::Sync)
            .fetch_all(&mut *c)
            .await?)
        }
    }
}
