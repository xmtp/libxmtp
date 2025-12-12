//! The Group database table. Stored information surrounding group membership and ID's.
use super::{
    ConnectionExt, Sqlite,
    consent_record::ConsentState,
    db_connection::DbConnection,
    schema::groups::{self, dsl},
};
use crate::NotFound;
use crate::{DuplicateItem, StorageError, impl_fetch, impl_store, impl_store_or_ignore};
use derive_builder::{Builder, UninitializedFieldError};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    dsl::sql,
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
mod convert;
mod dms;
mod version;

pub use dms::QueryDms;
pub use version::QueryGroupVersion;
use xmtp_proto::types::Cursor;

pub type ID = Vec<u8>;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Insertable,
    Identifiable,
    Queryable,
    Builder,
    Selectable,
    QueryableByName,
)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
#[diesel(check_for_backend(Sqlite))]
#[builder(
    setter(into),
    build_fn(error = "StorageError", validate = "Self::validate")
)]
#[derive(AsChangeset)]
/// A Unique group chat
pub struct StoredGroup {
    /// Randomly generated ID by group creator
    pub id: Vec<u8>,
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
#[derive(Queryable)]
#[diesel(table_name = groups)]
pub struct StoredGroupCommitLogPublicKey {
    pub id: Vec<u8>,
    pub commit_log_public_key: Option<Vec<u8>>,
}

/// A struct for fetching groups that need readd requests with their latest epoch
#[derive(Debug, Clone, Queryable, QueryableByName)]
pub struct StoredGroupForReaddRequest {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub group_id: Vec<u8>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub latest_commit_sequence_id: Option<i64>,
}

/// A struct for fetching groups that need to respond to readd requests
#[derive(Debug, Clone, Queryable, QueryableByName)]
pub struct StoredGroupForRespondingReadds {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub group_id: Vec<u8>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub dm_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub conversation_type: ConversationType,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub created_at_ns: i64,
}

// TODO: Create two more structs that delegate to StoredGroup
impl_fetch!(StoredGroup, groups, Vec<u8>);
impl_store!(StoredGroup, groups);
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

impl AsRef<GroupQueryArgs> for GroupQueryArgs {
    fn as_ref(&self) -> &GroupQueryArgs {
        self
    }
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
    fn find_groups<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError>;

    fn find_groups_by_id_paged<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
        offset: i64,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError>;

    /// Updates group membership state
    fn update_group_membership<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError>;

    fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError>;

    fn find_sync_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    fn primary_sync_group(&self) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    /// Return a single group that matches the given ID
    fn find_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    /// Return a single group that matches the given welcome ID
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    fn get_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<i64, StorageError>;

    /// Updates the 'last time checked' we checked for new installations.
    fn update_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<(), StorageError>;

    fn get_installations_time_checked(&self, group_id: Vec<u8>) -> Result<i64, StorageError>;

    /// Updates the 'last time checked' we checked for new installations.
    fn update_installations_time_checked(&self, group_id: Vec<u8>) -> Result<(), StorageError>;

    fn update_message_disappearing_from_ns(
        &self,
        group_id: Vec<u8>,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError>;

    fn update_message_disappearing_in_ns(
        &self,
        group_id: Vec<u8>,
        in_ns: Option<i64>,
    ) -> Result<(), StorageError>;

    fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError>;

    /// Get all the welcome ids turned into groups
    fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError>;

    fn mark_group_as_maybe_forked(
        &self,
        group_id: &[u8],
        fork_details: String,
    ) -> Result<(), StorageError>;

    fn clear_fork_flag_for_group(&self, group_id: &[u8]) -> Result<(), crate::ConnectionError>;

    fn has_duplicate_dm(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError>;

    /// Get conversations for all conversations that require a remote commit log publish (DMs and groups where user is super admin, excluding sync groups)
    fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

    /// Get conversations for all conversations that require a remote commit log download (DMs and groups that are not sync groups)
    fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

    /// Get conversation IDs for fork checking (excludes already forked conversations and sync groups)
    fn get_conversation_ids_for_fork_check(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError>;

    /// Get conversation IDs for conversations that are forked and need readd requests
    fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError>;

    /// Get conversation IDs for conversations that need to respond to readd requests
    fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError>;

    fn get_conversation_type(
        &self,
        group_id: &[u8],
    ) -> Result<ConversationType, crate::ConnectionError>;

    /// Updates the commit log public key for a group
    fn set_group_commit_log_public_key(
        &self,
        group_id: &[u8],
        public_key: &[u8],
    ) -> Result<(), StorageError>;

    /// Updates the is_commit_log_forked status for a group
    fn set_group_commit_log_forked_status(
        &self,
        group_id: &[u8],
        is_forked: Option<bool>,
    ) -> Result<(), StorageError>;

    /// Gets the is_commit_log_forked status for a group
    fn get_group_commit_log_forked_status(
        &self,
        group_id: &[u8],
    ) -> Result<Option<bool>, StorageError>;

    /// Updates the has_pending_leave_request status for a group
    fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &[u8],
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError>;

    fn get_groups_have_pending_leave_request(&self)
    -> Result<Vec<Vec<u8>>, crate::ConnectionError>;
}

impl<T> QueryGroup for &T
where
    T: QueryGroup,
{
    /// Return regular `Purpose::Conversation` groups with additional optional filters
    fn find_groups<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).find_groups(args)
    }

    fn find_groups_by_id_paged<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
        offset: i64,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).find_groups_by_id_paged(args, offset)
    }

    /// Updates group membership state
    fn update_group_membership<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_group_membership(group_id, state)
    }

    fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).all_sync_groups()
    }

    fn find_sync_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_sync_group(id)
    }

    fn primary_sync_group(&self) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).primary_sync_group()
    }

    /// Return a single group that matches the given ID
    fn find_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_group(id)
    }

    /// Return a single group that matches the given welcome ID
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_group_by_sequence_id(cursor)
    }

    fn get_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        (**self).get_rotated_at_ns(group_id)
    }

    /// Updates the 'last time checked' we checked for new installations.
    fn update_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<(), StorageError> {
        (**self).update_rotated_at_ns(group_id)
    }

    fn get_installations_time_checked(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        (**self).get_installations_time_checked(group_id)
    }

    /// Updates the 'last time checked' we checked for new installations.
    fn update_installations_time_checked(&self, group_id: Vec<u8>) -> Result<(), StorageError> {
        (**self).update_installations_time_checked(group_id)
    }

    fn update_message_disappearing_from_ns(
        &self,
        group_id: Vec<u8>,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        (**self).update_message_disappearing_from_ns(group_id, from_ns)
    }

    fn update_message_disappearing_in_ns(
        &self,
        group_id: Vec<u8>,
        in_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        (**self).update_message_disappearing_in_ns(group_id, in_ns)
    }

    fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError> {
        (**self).insert_or_replace_group(group)
    }

    /// Get all the welcome ids turned into groups
    fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError> {
        (**self).group_cursors()
    }

    fn mark_group_as_maybe_forked(
        &self,
        group_id: &[u8],
        fork_details: String,
    ) -> Result<(), StorageError> {
        (**self).mark_group_as_maybe_forked(group_id, fork_details)
    }

    fn clear_fork_flag_for_group(&self, group_id: &[u8]) -> Result<(), crate::ConnectionError> {
        (**self).clear_fork_flag_for_group(group_id)
    }

    fn has_duplicate_dm(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        (**self).has_duplicate_dm(group_id)
    }

    /// Get conversation IDs for all conversations that require a remote commit log publish (DMs and groups where user is super admin, excluding sync groups)
    fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_remote_log_publish()
    }

    fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_remote_log_download()
    }

    fn get_conversation_ids_for_fork_check(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_fork_check()
    }

    fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_requesting_readds()
    }

    fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError> {
        (**self).get_conversation_ids_for_responding_readds()
    }

    fn get_conversation_type(
        &self,
        group_id: &[u8],
    ) -> Result<ConversationType, crate::ConnectionError> {
        (**self).get_conversation_type(group_id)
    }

    fn set_group_commit_log_public_key(
        &self,
        group_id: &[u8],
        public_key: &[u8],
    ) -> Result<(), StorageError> {
        (**self).set_group_commit_log_public_key(group_id, public_key)
    }

    fn set_group_commit_log_forked_status(
        &self,
        group_id: &[u8],
        is_forked: Option<bool>,
    ) -> Result<(), StorageError> {
        (**self).set_group_commit_log_forked_status(group_id, is_forked)
    }

    fn get_group_commit_log_forked_status(
        &self,
        group_id: &[u8],
    ) -> Result<Option<bool>, StorageError> {
        (**self).get_group_commit_log_forked_status(group_id)
    }

    fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &[u8],
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError> {
        (**self).set_group_has_pending_leave_request_status(group_id, has_pending_leave_request)
    }

    fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        (**self).get_groups_have_pending_leave_request()
    }
}

impl<C: ConnectionExt> QueryGroup for DbConnection<C> {
    /// Return regular `Purpose::Conversation` groups with additional optional filters
    fn find_groups<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        use crate::schema::consent_records::dsl as consent_dsl;

        args.as_ref().validate()?;

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
        } = args.as_ref();

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
            self.raw_query_read(|conn| query.load::<StoredGroup>(conn))?
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

            self.raw_query_read(|conn| left_joined_query.load::<StoredGroup>(conn))?
        } else {
            // INNER JOIN: strict match only to specific states (no Unknown or NULL)
            let inner_joined_query = query
                .inner_join(consent_dsl::consent_records.on(
                    sql::<diesel::sql_types::Text>("lower(hex(groups.id))").eq(consent_dsl::entity),
                ))
                .filter(consent_dsl::state.eq_any(filtered_states.clone()))
                .select(dsl::groups::all_columns());

            self.raw_query_read(|conn| inner_joined_query.load::<StoredGroup>(conn))?
        };

        // Were sync groups explicitly asked for? Was the include_sync_groups flag set to true?
        // Then query for those separately
        if matches!(conversation_type, Some(ConversationType::Sync)) || *include_sync_groups {
            let query = dsl::groups.filter(dsl::conversation_type.eq(ConversationType::Sync));
            let mut sync_groups = self.raw_query_read(|conn| query.load(conn))?;
            groups.append(&mut sync_groups);
        }

        Ok(groups)
    }

    fn find_groups_by_id_paged<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
        offset: i64,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        let GroupQueryArgs {
            created_after_ns,
            created_before_ns,
            limit,
            ..
        } = args.as_ref();

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

        self.raw_query_read(|conn| query.load::<StoredGroup>(conn))
    }

    /// Updates group membership state
    fn update_group_membership<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(group_id.as_ref()))
                .set(dsl::membership_state.eq(state))
                .execute(conn)
        })?;

        Ok(())
    }

    fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync));

        self.raw_query_read(|conn| query.load(conn))
    }

    fn find_sync_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .filter(dsl::id.eq(id));

        self.raw_query_read(|conn| query.first(conn).optional())
    }

    fn primary_sync_group(&self) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync));

        self.raw_query_read(|conn| query.first(conn).optional())
    }

    /// Return a single group that matches the given ID
    fn find_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .limit(1)
            .filter(dsl::id.eq(id));
        let groups = self.raw_query_read(|conn| query.load(conn))?;

        Ok(groups.into_iter().next())
    }

    /// Return a single group that matches the given welcome ID
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .filter(dsl::sequence_id.eq(cursor.sequence_id as i64))
            .filter(dsl::originator_id.eq(cursor.originator_id as i64));

        let groups = self.raw_query_read(|conn| query.load(conn))?;

        if groups.len() > 1 {
            tracing::warn!(
                cursor.sequence_id,
                "More than one group found for welcome_id {}",
                cursor.sequence_id
            );
        }
        Ok(groups.into_iter().next())
    }

    fn get_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        let last_ts: Option<i64> = self.raw_query_read(|conn| {
            dsl::groups
                .find(&group_id)
                .select(dsl::rotated_at_ns)
                .first(conn)
                .optional()
        })?;

        last_ts.ok_or(StorageError::NotFound(NotFound::InstallationTimeForGroup(
            group_id,
        )))
    }

    /// Updates the 'last time checked' we checked for new installations.
    fn update_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::rotated_at_ns.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    fn get_installations_time_checked(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        let last_ts = self.raw_query_read(|conn| {
            dsl::groups
                .find(&group_id)
                .select(dsl::installations_last_checked)
                .first(conn)
                .optional()
        })?;

        last_ts.ok_or(NotFound::InstallationTimeForGroup(group_id).into())
    }

    /// Updates the 'last time checked' we checked for new installations.
    fn update_installations_time_checked(&self, group_id: Vec<u8>) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::installations_last_checked.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    fn update_message_disappearing_from_ns(
        &self,
        group_id: Vec<u8>,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::message_disappear_from_ns.eq(from_ns))
                .execute(conn)
        })?;

        Ok(())
    }

    fn update_message_disappearing_in_ns(
        &self,
        group_id: Vec<u8>,
        in_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::message_disappear_in_ns.eq(in_ns))
                .execute(conn)
        })?;

        Ok(())
    }

    fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError> {
        let maybe_inserted_group: Option<StoredGroup> = self.raw_query_write(|conn| {
            diesel::insert_into(dsl::groups)
                .values(&group)
                .on_conflict_do_nothing()
                .get_result(conn)
                .optional()
        })?;

        if maybe_inserted_group.is_none() {
            let mut existing_group: StoredGroup =
                self.raw_query_read(|conn| dsl::groups.find(&group.id).first(conn))?;
            // A restored group should be overwritten
            if matches!(
                existing_group.membership_state,
                GroupMembershipState::Restored
            ) {
                self.raw_query_write(|c| {
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
                    self.raw_query_write(|c| {
                        diesel::update(dsl::groups.find(&group.id))
                            .set(dsl::sequence_id.eq(group.sequence_id))
                            .execute(c)
                    })?;
                    existing_group.sequence_id = group.sequence_id;
                }
                Ok(existing_group)
            }
        } else {
            Ok(self.raw_query_read(|c| dsl::groups.find(group.id).first(c))?)
        }
    }

    /// Get all the welcome ids turned into groups
    fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            Ok(dsl::groups
                .filter(dsl::sequence_id.is_not_null())
                .select((dsl::sequence_id, dsl::originator_id))
                .load::<(Option<i64>, Option<i64>)>(conn)?
                .into_iter()
                .map(|(seq, orig)| {
                    Cursor::new(
                        seq.expect("Filtered for not null") as u64,
                        orig.expect("if seq is not null, originator must not be null") as u32,
                    )
                })
                .collect())
        })
    }

    fn mark_group_as_maybe_forked(
        &self,
        group_id: &[u8],
        fork_details: String,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(&group_id))
                .set((
                    dsl::maybe_forked.eq(true),
                    dsl::fork_details.eq(fork_details),
                ))
                .execute(conn)
        })?;

        Ok(())
    }

    fn clear_fork_flag_for_group(&self, group_id: &[u8]) -> Result<(), crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(&group_id))
                .set((dsl::maybe_forked.eq(false), dsl::fork_details.eq("")))
                .execute(conn)
        })?;
        Ok(())
    }

    fn has_duplicate_dm(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        self.raw_query_read(|conn| {
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
    fn get_conversation_ids_for_remote_log_publish(
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

        self.raw_query_read(|conn| query.load::<StoredGroupCommitLogPublicKey>(conn))
    }

    // All dms and groups that are not sync groups and have consent state Allowed
    fn get_conversation_ids_for_remote_log_download(
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

        self.raw_query_read(|conn| query.load::<StoredGroupCommitLogPublicKey>(conn))
    }

    // Get conversation IDs for fork checking (excludes already forked conversations and sync groups)
    fn get_conversation_ids_for_fork_check(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
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

        self.raw_query_read(|conn| query.load::<Vec<u8>>(conn))
    }

    fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError> {
        use super::schema::{groups::dsl as groups_dsl, remote_commit_log::dsl as rcl_dsl};
        use diesel::dsl::max;

        self.raw_query_read(|conn| {
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

    fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError> {
        use super::schema::{groups::dsl as groups_dsl, readd_status::dsl as readd_dsl};
        use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl};

        self.raw_query_read(|conn| {
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

    fn get_conversation_type(
        &self,
        group_id: &[u8],
    ) -> Result<ConversationType, crate::ConnectionError> {
        let query = dsl::groups
            .filter(dsl::id.eq(group_id))
            .select(dsl::conversation_type);
        let conversation_type = self.raw_query_read(|conn| query.first(conn))?;
        Ok(conversation_type)
    }

    fn set_group_commit_log_public_key(
        &self,
        group_id: &[u8],
        public_key: &[u8],
    ) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;
        let num_updated = self.raw_query_write(|conn| {
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
                group_id.to_vec(),
            )));
        }
        Ok(())
    }

    fn set_group_commit_log_forked_status(
        &self,
        group_id: &[u8],
        is_forked: Option<bool>,
    ) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::is_commit_log_forked.eq(is_forked))
                .execute(conn)
        })?;
        Ok(())
    }

    fn get_group_commit_log_forked_status(
        &self,
        group_id: &[u8],
    ) -> Result<Option<bool>, StorageError> {
        use crate::schema::groups::dsl;
        self.raw_query_read(|conn| {
            dsl::groups
                .find(group_id)
                .select(dsl::is_commit_log_forked)
                .first::<Option<bool>>(conn)
        })
        .map_err(StorageError::from)
    }

    fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &[u8],
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError> {
        use crate::schema::groups::dsl;
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::has_pending_leave_request.eq(has_pending_leave_request))
                .execute(conn)
        })?;
        Ok(())
    }

    fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        let query = dsl::groups
            .filter(
                dsl::conversation_type
                    .ne(ConversationType::Sync)
                    .and(dsl::has_pending_leave_request.eq(Some(true))),
            )
            .select(dsl::id);

        self.raw_query_read(|conn| query.load::<Vec<u8>>(conn))
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
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

impl ToSql<Integer, Sqlite> for GroupMembershipState
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for GroupMembershipState
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(GroupMembershipState::Allowed),
            2 => Ok(GroupMembershipState::Rejected),
            3 => Ok(GroupMembershipState::Pending),
            4 => Ok(GroupMembershipState::Restored),
            5 => Ok(GroupMembershipState::PendingRemove),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
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

impl ToSql<Integer, Sqlite> for ConversationType
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for ConversationType
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(ConversationType::Group),
            2 => Ok(ConversationType::Dm),
            3 => Ok(ConversationType::Sync),
            4 => Ok(ConversationType::Oneshot),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

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
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    pub use super::dms::tests::*;
    use super::*;

    use crate::{
        Fetch, Store,
        consent_record::{ConsentType, StoredConsentRecord},
        readd_status::ReaddStatus,
        schema::groups::dsl::groups,
        test_utils::{with_connection, with_connection_async},
    };
    use xmtp_common::{assert_ok, rand_vec, time::now_ns};
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
        let id = rand_vec::<24>();
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
        let id = rand_vec::<24>();
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
    fn test_it_stores_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            test_group.store(conn).unwrap();
            assert_eq!(
                conn.raw_query_read(|raw_conn| groups.first::<StoredGroup>(raw_conn))
                    .unwrap(),
                test_group
            );
        })
    }

    #[xmtp_common::test]
    fn test_it_fetches_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            conn.raw_query_write(|raw_conn| {
                diesel::insert_into(groups)
                    .values(test_group.clone())
                    .execute(raw_conn)
            })
            .unwrap();

            let fetched_group: Option<StoredGroup> = conn.fetch(&test_group.id).unwrap();
            assert_eq!(fetched_group, Some(test_group));
        })
    }

    #[xmtp_common::test]
    fn test_it_updates_group_membership_state() {
        with_connection(|conn| {
            let test_group = generate_group(Some(GroupMembershipState::Pending));

            test_group.store(conn).unwrap();
            conn.update_group_membership(&test_group.id, GroupMembershipState::Rejected)
                .unwrap();

            let updated_group: StoredGroup = conn.fetch(&test_group.id).ok().flatten().unwrap();
            assert_eq!(
                updated_group,
                StoredGroup {
                    membership_state: GroupMembershipState::Rejected,
                    ..test_group
                }
            );
        })
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
            test_group_1.store(&conn).unwrap();
            wait_in_wasm().await;
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(&conn).unwrap();
            wait_in_wasm().await;
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(&conn).unwrap();

            let other_inbox_id = test_group_3
                .dm_id
                .unwrap()
                .other_inbox_id("placeholder_inbox_id_1");

            let all_results = conn
                .find_groups(GroupQueryArgs {
                    conversation_type: Some(ConversationType::Group),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(all_results.len(), 2);

            let pending_results = conn
                .find_groups(GroupQueryArgs {
                    allowed_states: Some(vec![GroupMembershipState::Pending]),
                    conversation_type: Some(ConversationType::Group),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(pending_results[0].id, test_group_1.id);
            assert_eq!(pending_results.len(), 1);

            // Offset and limit
            let results_with_limit = conn
                .find_groups(GroupQueryArgs {
                    conversation_type: Some(ConversationType::Group),
                    limit: Some(1),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results_with_limit.len(), 1);
            assert_eq!(results_with_limit[0].id, test_group_1.id);

            let results_with_created_at_ns_after = conn
                .find_groups(GroupQueryArgs {
                    conversation_type: Some(ConversationType::Group),
                    limit: Some(1),
                    created_after_ns: Some(test_group_1.created_at_ns),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results_with_created_at_ns_after.len(), 1);
            assert_eq!(results_with_created_at_ns_after[0].id, test_group_2.id);

            // Sync groups SHOULD NOT be returned
            let synced_groups = conn.primary_sync_group().unwrap();
            assert!(synced_groups.is_none());

            // test that dm groups are included
            let dm_results = conn.find_groups(GroupQueryArgs::default()).unwrap();
            assert_eq!(dm_results.len(), 3);
            assert_eq!(dm_results[2].id, test_group_3.id);

            // test find_dm_group
            let dm_result = conn
                .find_active_dm_group(format!("dm:placeholder_inbox_id_1:{}", &other_inbox_id))
                .unwrap();
            assert!(dm_result.is_some());

            // test only dms are returned
            let dm_results = conn
                .find_groups(GroupQueryArgs {
                    conversation_type: Some(ConversationType::Dm),
                    ..Default::default()
                })
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
            test_group.store(&conn).unwrap();

            // Check that the installations update has not been performed, yet
            assert_eq!(test_group.installations_last_checked, 0);

            if cfg!(target_arch = "wasm32") {
                // web has current time resolution only to millisecond,
                // which is too slow for this test to pass and the timestamps to be different
                xmtp_common::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            // Check that some event occurred which triggers an installation list update.
            // Here we invoke that event directly
            let result = conn.update_installations_time_checked(test_group.id.clone());
            assert_ok!(result);

            // Check that the latest installation list timestamp has been updated
            let fetched_group: StoredGroup = conn.fetch(&test_group.id).ok().flatten().unwrap();
            assert_ne!(fetched_group.installations_last_checked, 0);
            assert!(fetched_group.created_at_ns < fetched_group.installations_last_checked);
        })
        .await
    }

    #[xmtp_common::test]
    fn test_new_group_has_correct_purpose() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            conn.raw_query_write(|raw_conn| {
                diesel::insert_into(groups)
                    .values(test_group.clone())
                    .execute(raw_conn)
            })
            .unwrap();

            let fetched_group: Option<StoredGroup> = conn.fetch(&test_group.id).unwrap();
            assert_eq!(fetched_group, Some(test_group));
            let conversation_type = fetched_group.unwrap().conversation_type;
            assert_eq!(conversation_type, ConversationType::Group);
        })
    }

    #[xmtp_common::test]
    fn test_find_groups_by_consent_state() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).unwrap();
            let test_group_4 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_4.store(conn).unwrap();

            let test_group_1_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_1.id.clone()),
            );
            test_group_1_consent.store(conn).unwrap();
            let test_group_2_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(test_group_2.id.clone()),
            );
            test_group_2_consent.store(conn).unwrap();
            let test_group_3_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_3.id.clone()),
            );
            test_group_3_consent.store(conn).unwrap();

            let all_results = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![
                        ConsentState::Allowed,
                        ConsentState::Unknown,
                        ConsentState::Denied,
                    ]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(all_results.len(), 4);

            let default_results = conn.find_groups(GroupQueryArgs::default()).unwrap();
            assert_eq!(default_results.len(), 3);

            let allowed_results = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(allowed_results.len(), 2);

            let allowed_unknown_results = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(allowed_unknown_results.len(), 3);

            let denied_results = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Denied]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(denied_results.len(), 1);
            assert_eq!(denied_results[0].id, test_group_2.id);

            let unknown_results = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Unknown]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(unknown_results.len(), 1);
            assert_eq!(unknown_results[0].id, test_group_4.id);

            let empty_array_results = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(empty_array_results.len(), 3);
        })
    }

    #[xmtp_common::test]
    fn test_get_sequence_ids() {
        with_connection(|conn| {
            let mls_groups = [
                generate_group_with_welcome(None, Some(30)),
                generate_group(None),
                generate_group(None),
                generate_group_with_welcome(None, Some(10)),
            ];
            for g in mls_groups.iter() {
                g.store(conn).unwrap();
            }
            assert_eq!(
                vec![30, 10],
                conn.group_cursors()
                    .unwrap()
                    .into_iter()
                    .map(|c| c.sequence_id)
                    .collect::<Vec<u64>>()
            );
        })
    }

    #[xmtp_common::test]
    fn test_find_group_default_excludes_denied() {
        with_connection(|conn| {
            // Create three groups: one allowed, one denied, one unknown (no consent)
            let allowed_group = generate_group(Some(GroupMembershipState::Allowed));
            allowed_group.store(conn).unwrap();

            let denied_group = generate_group(Some(GroupMembershipState::Allowed));
            denied_group.store(conn).unwrap();

            let unknown_group = generate_group(Some(GroupMembershipState::Allowed));
            unknown_group.store(conn).unwrap();

            // Create consent records for allowed and denied; leave unknown_group without one
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id.clone()),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id.clone()),
            );
            denied_consent.store(conn).unwrap();

            // Query using default args (no consent_states specified)
            let default_results = conn.find_groups(GroupQueryArgs::default()).unwrap();

            // Expect to include only: allowed_group and unknown_group (2 total)
            assert_eq!(default_results.len(), 2);
            let returned_ids: Vec<_> = default_results.iter().map(|g| &g.id).collect();
            assert!(returned_ids.contains(&&allowed_group.id));
            assert!(returned_ids.contains(&&unknown_group.id));
            assert!(!returned_ids.contains(&&denied_group.id));
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_conversation_ids_for_remote_log_publish() {
        with_connection(|conn| {
            let mut group1 = generate_group(None);
            let mut group2 = generate_group(None);
            let mut group3 = generate_group(None);
            let mut group4 = generate_group(None);
            group1.should_publish_commit_log = true;
            group1.commit_log_public_key = None;
            generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(group1.id.clone()),
            )
            .store(conn)?;
            group2.should_publish_commit_log = true;
            group2.commit_log_public_key = Some(rand_vec::<32>());

            group3.should_publish_commit_log = true;
            group3.commit_log_public_key = Some(rand_vec::<32>());
            generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(group3.id.clone()),
            )
            .store(conn)?;
            group4.should_publish_commit_log = false;
            group1.store(conn)?;
            group2.store(conn)?;
            group3.store(conn)?;
            group4.store(conn)?;

            let commit_log_keys = conn.get_conversation_ids_for_remote_log_publish().unwrap();
            assert_eq!(commit_log_keys.len(), 2);
            assert_eq!(commit_log_keys[0].id, group1.id);
            assert_eq!(commit_log_keys[1].id, group3.id);
            assert_eq!(commit_log_keys[0].commit_log_public_key, None);
            assert_eq!(
                commit_log_keys[1].commit_log_public_key,
                group3.commit_log_public_key
            );
        })
    }

    #[xmtp_common::test]
    fn test_get_conversation_ids_for_remote_log_publish_with_consent() {
        with_connection(|conn| {
            // Create groups: one with Allowed consent, one with Denied consent, one with no consent
            let mut allowed_group = generate_group(None);
            allowed_group.should_publish_commit_log = true;
            allowed_group.store(conn).unwrap();

            let mut denied_group = generate_group(None);
            denied_group.should_publish_commit_log = true;
            denied_group.store(conn).unwrap();

            let mut no_consent_group = generate_group(None);
            no_consent_group.should_publish_commit_log = true;
            no_consent_group.store(conn).unwrap();

            // Create consent records
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id.clone()),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id.clone()),
            );
            denied_consent.store(conn).unwrap();

            // Function should only return groups with Allowed consent state
            let commit_log_keys = conn.get_conversation_ids_for_remote_log_publish().unwrap();
            assert_eq!(commit_log_keys.len(), 1);
            assert_eq!(commit_log_keys[0].id, allowed_group.id);
        })
    }

    #[xmtp_common::test]
    fn test_get_conversation_ids_for_remote_log_download_with_consent() {
        with_connection(|conn| {
            // Create groups: one with Allowed consent, one with Denied consent, one with no consent
            let allowed_group = generate_group(None);
            allowed_group.store(conn).unwrap();

            let denied_group = generate_group(None);
            denied_group.store(conn).unwrap();

            let no_consent_group = generate_group(None);
            no_consent_group.store(conn).unwrap();

            // Create a sync group (should be excluded regardless of consent)
            let mut sync_group = generate_group(None);
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn).unwrap();
            let sync_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(sync_group.id.clone()),
            );
            sync_consent.store(conn).unwrap();

            // Create consent records
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id.clone()),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id.clone()),
            );
            denied_consent.store(conn).unwrap();

            // Function should only return groups with Allowed consent state, excluding sync groups
            let conversation_ids = conn.get_conversation_ids_for_remote_log_download().unwrap();
            assert_eq!(conversation_ids.len(), 1);
            assert_eq!(conversation_ids[0].id, allowed_group.id);
        })
    }

    #[xmtp_common::test]
    fn test_get_conversation_ids_for_responding_readds() {
        with_connection(|conn| {
            // Create test groups
            let group_id_1 = vec![1, 2, 3];
            let group_id_2 = vec![4, 5, 6];
            let group_id_3 = vec![7, 8, 9];

            let group1 = StoredGroup::builder()
                .id(group_id_1.clone())
                .created_at_ns(1000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group1.store(conn).unwrap();

            let group2 = StoredGroup::builder()
                .id(group_id_2.clone())
                .created_at_ns(2000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group2.store(conn).unwrap();

            let group3 = StoredGroup::builder()
                .id(group_id_3.clone())
                .created_at_ns(3000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group3.store(conn).unwrap();

            // Create readd status entries with various test cases
            let test_cases = vec![
                // Case 1: Pending readd (requested_at > responded_at)
                ReaddStatus {
                    group_id: group_id_1.clone(),
                    installation_id: vec![1],
                    requested_at_sequence_id: Some(10),
                    responded_at_sequence_id: Some(5),
                },
                // Case 2: Pending readd (responded_at is None)
                ReaddStatus {
                    group_id: group_id_1.clone(),
                    installation_id: vec![2],
                    requested_at_sequence_id: Some(8),
                    responded_at_sequence_id: None,
                },
                // Case 4: Not pending (requested_at < responded_at)
                ReaddStatus {
                    group_id: group_id_2.clone(),
                    installation_id: vec![4],
                    requested_at_sequence_id: Some(12),
                    responded_at_sequence_id: Some(15),
                },
                // Case 5: Not pending (requested_at is None)
                ReaddStatus {
                    group_id: group_id_2.clone(),
                    installation_id: vec![5],
                    requested_at_sequence_id: None,
                    responded_at_sequence_id: Some(20),
                },
                // Case 6: Pending readd (requested_at == responded_at, should be pending)
                ReaddStatus {
                    group_id: group_id_3.clone(),
                    installation_id: vec![6],
                    requested_at_sequence_id: Some(25),
                    responded_at_sequence_id: Some(25),
                },
            ];

            // Store all test cases
            for status in test_cases {
                status.store(conn).unwrap();
            }

            // Call the method under test
            let result = conn.get_conversation_ids_for_responding_readds().unwrap();

            // Should return groups 1 and 3 (both have pending readd requests)
            // Group 2 has no pending readds
            assert_eq!(result.len(), 2);

            // Results should be sorted by group_id (since we used distinct())
            let mut result_group_ids: Vec<Vec<u8>> =
                result.iter().map(|r| r.group_id.clone()).collect();
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
    }
}
