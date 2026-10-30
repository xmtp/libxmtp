//! The Group database table. Stored information surrounding group membership and ID's.
use super::group_message::{ContentType, GroupMessageKind};
use super::schema::consent_records;
use super::schema::group_messages;
use super::{
    ConnectionExt, Sqlite,
    consent_record::ConsentState,
    db_connection::DbConnection,
    schema::groups::{self, dsl},
};
use crate::NotFound;
use crate::{DuplicateItem, StorageError, impl_fetch, impl_store, impl_store_or_ignore};
use derive_builder::Builder;
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
use xmtp_proto::types::{Cursor, GroupId};

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
#[builder(setter(into), build_fn(error = "StorageError"))]
#[derive(AsChangeset)]
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
    /// Whether the user should publish the commit log for this group
    #[builder(default = false)]
    pub should_publish_commit_log: bool,
    /// The group's commit-log id: the public key of the commit-log signing
    /// key derived from its group root, stored at creation and join. It
    /// names the commit-log topic. `None` for a group without a root, such
    /// as a restored placeholder.
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
    /// Optional notification rule for this conversation.
    #[builder(default = None)]
    #[serde(default)]
    pub push_override: Option<i32>,
    //todo: store member role?
}

impl StoredGroup {
    pub fn cursor(&self) -> Option<Cursor> {
        self.sequence_id.map(|sequence| Cursor(sequence as u64))
    }
}

/// Read the row values needed by a conversation state snapshot.
pub trait QueryConversationState {
    fn conversation_state_row(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<(StoredGroup, Option<ConsentState>)>, StorageError>;

    fn last_activity_ns(
        &self,
        group_id: &GroupId,
        content_types: &[ContentType],
    ) -> Result<Option<i64>, StorageError>;
}

impl<T: QueryConversationState + ?Sized> QueryConversationState for &T {
    fn conversation_state_row(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<(StoredGroup, Option<ConsentState>)>, StorageError> {
        (**self).conversation_state_row(group_id)
    }

    fn last_activity_ns(
        &self,
        group_id: &GroupId,
        content_types: &[ContentType],
    ) -> Result<Option<i64>, StorageError> {
        (**self).last_activity_ns(group_id, content_types)
    }
}

/// The join from the `groups` row named `g` to its consent record. A DM's
/// record is the other member's inbox record, in exactly the cases where
/// `DmIdExt::other_inbox_id` returns one: a two-member `dm:` id with
/// distinct, non-empty members, one of which is the own inbox (the one
/// `identity` row). Otherwise the key is NULL, which matches no record, so the
/// DM reads unknown. Both sides are equalities on the primary key, so SQLite
/// looks the record up by index.
// implements: CONS-052, CONS-060
pub fn consent_join_sql(g: &str) -> String {
    let r = format!("substr({g}.dm_id, 4)");
    let a = format!("substr({r}, 1, instr({r}, ':') - 1)");
    let b = format!("substr({r}, instr({r}, ':') + 1)");
    let me = "(SELECT inbox_id FROM identity)";
    format!(
        "consent_records.entity_type = CASE WHEN {g}.dm_id IS NULL THEN 1 ELSE 2 END \
         AND consent_records.entity = CASE \
           WHEN {g}.dm_id IS NULL THEN lower(hex({g}.id)) \
           WHEN substr({g}.dm_id, 1, 3) <> 'dm:' OR instr({r}, ':') = 0 \
             OR {a} = '' OR {b} = '' OR instr({b}, ':') > 0 OR {a} = {b} THEN NULL \
           WHEN {a} = {me} THEN lower({b}) \
           WHEN {b} = {me} THEN lower({a}) \
           END"
    )
}

fn conversation_consent_join() -> diesel::expression::SqlLiteral<diesel::sql_types::Bool> {
    sql::<diesel::sql_types::Bool>(&consent_join_sql("groups"))
}

/// A sync group's local join time: after the clock and after every sync
/// group held, so the group joined last is the one publishing selects.
// implements: SYNC-031
pub fn next_sync_join_time(db: &impl QueryGroup) -> Result<i64, crate::ConnectionError> {
    let newest = db
        .all_sync_groups()?
        .first()
        .map_or(i64::MIN, |g| g.created_at_ns.saturating_add(1));
    Ok(xmtp_common::time::now_ns().max(newest))
}

/// The conversations whose commit log the client reads: allowed DMs and
/// groups other than one-shot, sync and restored groups. The download and
/// the fork check both select from it, so their scopes cannot drift. A
/// correlated `EXISTS` keeps it a plain groups query.
// implements: FORK-030
fn allowed_commit_log_groups() -> groups::BoxedQuery<'static, Sqlite> {
    dsl::groups
        .filter(dsl::membership_state.ne(GroupMembershipState::Restored))
        .filter(dsl::conversation_type.ne_all(ConversationType::virtual_types()))
        .filter(diesel::dsl::exists(
            consent_records::table
                .filter(conversation_consent_join())
                .filter(consent_records::state.eq(ConsentState::Allowed)),
        ))
        .into_boxed()
}

impl<C: ConnectionExt> QueryConversationState for DbConnection<C> {
    fn conversation_state_row(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<(StoredGroup, Option<ConsentState>)>, StorageError> {
        Ok(self.raw_query(|conn| {
            groups::table
                .left_join(consent_records::table.on(conversation_consent_join()))
                .filter(groups::id.eq(group_id))
                .select((StoredGroup::as_select(), consent_records::state.nullable()))
                .first(conn)
                .optional()
        })?)
    }

    /// With the list content types, this is the activity time that an
    /// activity-ordered conversation list sorts by (`activity_time` in
    /// conversation_list.rs).
    // implements: CONS-048
    fn last_activity_ns(
        &self,
        group_id: &GroupId,
        content_types: &[ContentType],
    ) -> Result<Option<i64>, StorageError> {
        let Some(created_at_ns) = self.raw_query(|conn| {
            groups::table
                .filter(groups::id.eq(group_id))
                .select(groups::created_at_ns)
                .first::<i64>(conn)
                .optional()
        })?
        else {
            return Ok(None);
        };
        if content_types.is_empty() {
            return Ok(Some(created_at_ns));
        }
        let latest = self.raw_query(|conn| {
            group_messages::table
                .filter(group_messages::group_id.eq(group_id))
                .filter(group_messages::kind.eq(GroupMessageKind::Application))
                .filter(group_messages::content_type.eq_any(content_types))
                .filter(
                    group_messages::expire_at_ns
                        .is_null()
                        .or(group_messages::expire_at_ns.gt(xmtp_common::time::now_ns())),
                )
                .select(diesel::dsl::max(group_messages::sent_at_ns))
                .first::<Option<i64>>(conn)
        })?;
        Ok(Some(latest.unwrap_or(created_at_ns)))
    }
}

impl StoredGroupBuilder {
    pub fn cursor(&mut self, cursor: Cursor) -> &mut Self {
        self.sequence_id = Some(Some(cursor.0 as i64));
        self
    }
}

/// A subset of the group table for fetching the commit log public key
#[derive(Queryable)]
#[diesel(table_name = groups)]
pub struct StoredGroupCommitLogPublicKey {
    pub id: GroupId,
    pub commit_log_public_key: Option<Vec<u8>>,
}

/// A struct for fetching groups that need readd requests with their latest epoch
#[derive(Debug, Clone, Queryable, QueryableByName)]
pub struct StoredGroupForReaddRequest {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub group_id: GroupId,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub latest_commit_sequence_id: Option<i64>,
}

/// A struct for fetching groups that need to respond to readd requests
#[derive(Debug, Clone, Queryable, QueryableByName)]
pub struct StoredGroupForRespondingReadds {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub group_id: GroupId,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub dm_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub conversation_type: ConversationType,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub created_at_ns: i64,
}

/// The columns an internal operation needs to enumerate a stored group. It has
/// no message or other peer-sized field, so an enumeration costs the same for
/// every group.
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = groups)]
#[diesel(check_for_backend(Sqlite))]
pub struct StoredGroupRef {
    pub id: GroupId,
    pub dm_id: Option<String>,
    pub conversation_type: ConversationType,
    pub created_at_ns: i64,
    pub membership_state: GroupMembershipState,
}

/// Which stored groups `stored_group_refs` returns. There is no limit field: an
/// internal operation reads every group its selection names. Oneshot groups
/// are never selected, and every group of a DM is selected.
#[derive(Debug, Clone)]
pub struct GroupScope {
    /// The consent states of the non-sync groups to select. A group with no
    /// consent record is Unknown. `None` selects every state; `Some` with no
    /// states selects no non-sync group.
    pub consent: Option<Vec<ConsentState>>,
    /// Also select every sync group, whatever its consent.
    pub include_sync: bool,
    /// Also select Restored archive placeholders.
    pub include_restored: bool,
}

impl GroupScope {
    /// The groups a conversation listing with `consent` names (none means
    /// Allowed and Unknown), with every group of a DM, sync groups and
    /// Restored placeholders.
    pub fn listing(consent: Option<Vec<ConsentState>>) -> Self {
        Self {
            consent: Some(
                consent.unwrap_or_else(|| vec![ConsentState::Allowed, ConsentState::Unknown]),
            ),
            include_sync: true,
            include_restored: true,
        }
    }

    /// Every stored group whatever its consent, sync groups and Restored
    /// placeholders included.
    pub fn every_group() -> Self {
        Self {
            consent: None,
            include_sync: true,
            include_restored: true,
        }
    }

    /// The same selection without Restored archive placeholders.
    pub fn without_restored(self) -> Self {
        Self {
            include_restored: false,
            ..self
        }
    }
}

// TODO: Create two more structs that delegate to StoredGroup
impl_fetch!(StoredGroup, groups, GroupId);
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
    /// No limit returns every matching group. Callers that need every group
    /// (stream catch-up, barriers, device sync, commit-log scans) rely on it.
    pub limit: Option<i64>,
    pub conversation_type: Option<ConversationType>,
    pub consent_states: Option<Vec<ConsentState>>,
    pub include_sync_groups: bool,
    pub include_duplicate_dms: bool,
    pub should_publish_commit_log: Option<bool>,
    pub order_by: Option<GroupQueryOrderBy>,
    /// Breaks ties at the list order's before time
    /// ([`GroupQueryArgs::order_before_ns`]): a group at exactly that time is
    /// returned only when its id is lower in byte order. It needs that before
    /// time. Only `fetch_conversation_list` honors it; `find_groups` ignores it.
    pub before_id: Option<GroupId>,
}

impl AsRef<GroupQueryArgs> for GroupQueryArgs {
    fn as_ref(&self) -> &GroupQueryArgs {
        self
    }
}

impl GroupQueryArgs {
    /// Whether the app asked for sync groups. Requested sync groups bypass the
    /// regular filters, limit, and consent filter.
    pub(crate) fn requests_sync_groups(&self) -> bool {
        matches!(self.conversation_type, Some(ConversationType::Sync)) || self.include_sync_groups
    }

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

        if self.before_id.is_some() && self.order_before_ns().is_none() {
            return Err(crate::ConnectionError::InvalidQuery(
                "before_id needs the before time of the list order".to_string(),
            ));
        }

        Ok(())
    }

    /// The before time of the list order, which `before_id` breaks ties at:
    /// `created_before_ns` in creation order, `last_activity_before_ns` in
    /// activity order.
    pub fn order_before_ns(&self) -> Option<i64> {
        match self.order_by.clone().unwrap_or_default() {
            GroupQueryOrderBy::CreatedAt => self.created_before_ns,
            GroupQueryOrderBy::LastActivity => self.last_activity_before_ns,
        }
    }
}

pub trait QueryGroup {
    /// Delete a settled one-shot's child records and row in the caller's MLS-state transaction.
    fn delete_oneshot_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError>;

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
    fn update_group_membership<Id: AsRef<[u8]>>(
        &self,
        group_id: Id,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError>;

    fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError>;

    fn find_sync_group(&self, id: &GroupId) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    /// Return a single group that matches the given ID
    fn find_group(&self, id: &GroupId) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    /// Return a single group that matches the given welcome ID
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError>;

    fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError>;

    /// Updates the 'last time checked' we checked for new installations.
    fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError>;

    fn get_installations_time_checked(&self, group_id: &GroupId) -> Result<i64, StorageError>;

    /// Updates the 'last time checked' we checked for new installations.
    fn update_installations_time_checked(&self, group_id: &GroupId) -> Result<(), StorageError>;

    fn update_message_disappearing_from_ns(
        &self,
        group_id: &GroupId,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError>;

    fn update_message_disappearing_in_ns(
        &self,
        group_id: &GroupId,
        in_ns: Option<i64>,
    ) -> Result<(), StorageError>;

    /// Store the group a Welcome installs: insert it, replace a Restored
    /// placeholder, or keep an existing row and advance its Welcome cursor.
    /// Every successful call changes the group-set generation.
    fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError>;

    /// Get all the welcome ids turned into groups
    fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError>;

    fn mark_group_as_maybe_forked(
        &self,
        group_id: &GroupId,
        fork_details: String,
    ) -> Result<(), StorageError>;

    fn clear_fork_flag_for_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError>;

    fn has_duplicate_dm(&self, group_id: &GroupId) -> Result<bool, crate::ConnectionError>;

    /// Get conversations for all conversations that require a remote commit log publish (DMs and groups where user is super admin, excluding sync groups)
    fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

    /// The conversations whose commit log the client reads: allowed DMs and
    /// groups other than one-shot, sync and restored groups.
    fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError>;

    /// The conversations whose commit log the client reads, other than those
    /// already forked. Any other conversation keeps its last fork state.
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
        group_id: &GroupId,
    ) -> Result<ConversationType, crate::ConnectionError>;

    /// Update the cached publisher role in the caller's state transaction.
    /// The caller owns publication eligibility and any role-transition cutoff.
    fn set_should_publish_commit_log(
        &self,
        group_id: &GroupId,
        should_publish: bool,
    ) -> Result<(), crate::ConnectionError>;

    /// Updates the is_commit_log_forked status for a group
    fn set_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
        is_forked: Option<bool>,
    ) -> Result<(), StorageError>;

    /// Gets the is_commit_log_forked status for a group
    fn get_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<bool>, StorageError>;

    /// Updates the has_pending_leave_request status for a group
    fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &GroupId,
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError>;

    fn get_groups_have_pending_leave_request(&self)
    -> Result<Vec<Vec<u8>>, crate::ConnectionError>;

    /// Every stored group that `scope` selects, newest first: regular groups,
    /// then sync groups. Reads no message.
    fn stored_group_refs(
        &self,
        scope: &GroupScope,
    ) -> Result<Vec<StoredGroupRef>, crate::ConnectionError>;

    /// A counter that changes whenever a group is added or removed, a group's
    /// id, membership state, Welcome cursor, DM id or type changes, a
    /// conversation's consent changes, or a Welcome discovery is recorded.
    /// Message writes and inbox consent do not change it.
    fn group_set_generation(&self) -> Result<i64, crate::ConnectionError>;
}

impl<T> QueryGroup for &T
where
    T: QueryGroup,
{
    fn delete_oneshot_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError> {
        (**self).delete_oneshot_group(group_id)
    }

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
    fn update_group_membership<Id: AsRef<[u8]>>(
        &self,
        group_id: Id,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_group_membership(group_id, state)
    }

    fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        (**self).all_sync_groups()
    }

    fn find_sync_group(&self, id: &GroupId) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_sync_group(id)
    }

    /// Return a single group that matches the given ID
    fn find_group(&self, id: &GroupId) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_group(id)
    }

    /// Return a single group that matches the given welcome ID
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        (**self).find_group_by_sequence_id(cursor)
    }

    fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError> {
        (**self).get_rotated_at_ns(group_id)
    }

    /// Updates the 'last time checked' we checked for new installations.
    fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError> {
        (**self).update_rotated_at_ns(group_id)
    }

    fn get_installations_time_checked(&self, group_id: &GroupId) -> Result<i64, StorageError> {
        (**self).get_installations_time_checked(group_id)
    }

    /// Updates the 'last time checked' we checked for new installations.
    fn update_installations_time_checked(&self, group_id: &GroupId) -> Result<(), StorageError> {
        (**self).update_installations_time_checked(group_id)
    }

    fn update_message_disappearing_from_ns(
        &self,
        group_id: &GroupId,
        from_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        (**self).update_message_disappearing_from_ns(group_id, from_ns)
    }

    fn update_message_disappearing_in_ns(
        &self,
        group_id: &GroupId,
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
        group_id: &GroupId,
        fork_details: String,
    ) -> Result<(), StorageError> {
        (**self).mark_group_as_maybe_forked(group_id, fork_details)
    }

    fn clear_fork_flag_for_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError> {
        (**self).clear_fork_flag_for_group(group_id)
    }

    fn has_duplicate_dm(&self, group_id: &GroupId) -> Result<bool, crate::ConnectionError> {
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
        group_id: &GroupId,
    ) -> Result<ConversationType, crate::ConnectionError> {
        (**self).get_conversation_type(group_id)
    }

    fn set_should_publish_commit_log(
        &self,
        group_id: &GroupId,
        should_publish: bool,
    ) -> Result<(), crate::ConnectionError> {
        (**self).set_should_publish_commit_log(group_id, should_publish)
    }

    fn set_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
        is_forked: Option<bool>,
    ) -> Result<(), StorageError> {
        (**self).set_group_commit_log_forked_status(group_id, is_forked)
    }

    fn get_group_commit_log_forked_status(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<bool>, StorageError> {
        (**self).get_group_commit_log_forked_status(group_id)
    }

    fn set_group_has_pending_leave_request_status(
        &self,
        group_id: &GroupId,
        has_pending_leave_request: Option<bool>,
    ) -> Result<(), StorageError> {
        (**self).set_group_has_pending_leave_request_status(group_id, has_pending_leave_request)
    }

    fn get_groups_have_pending_leave_request(
        &self,
    ) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        (**self).get_groups_have_pending_leave_request()
    }

    fn stored_group_refs(
        &self,
        scope: &GroupScope,
    ) -> Result<Vec<StoredGroupRef>, crate::ConnectionError> {
        (**self).stored_group_refs(scope)
    }

    fn group_set_generation(&self) -> Result<i64, crate::ConnectionError> {
        (**self).group_set_generation()
    }
}

impl<C: ConnectionExt> DbConnection<C> {
    /// Change the group-set generation for a write its triggers do not see.
    fn bump_group_set_generation(&self) -> Result<(), crate::ConnectionError> {
        use crate::schema::group_set_generation::dsl as generation_dsl;
        self.raw_query(|conn| {
            diesel::update(generation_dsl::group_set_generation)
                .set(generation_dsl::generation.eq(generation_dsl::generation + 1))
                .execute(conn)
        })?;
        Ok(())
    }

    /// The stored rows of the sync groups the app asked for, or none. The
    /// conversation list's `requested_sync_groups` applies the same rule.
    // implements: SYNC-005
    fn requested_sync_group_rows(
        &self,
        args: &GroupQueryArgs,
    ) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        if !args.requests_sync_groups() {
            return Ok(Vec::new());
        }
        let query = dsl::groups.filter(dsl::conversation_type.eq(ConversationType::Sync));
        self.raw_query(|conn| query.load(conn))
    }
}

impl<C: ConnectionExt> QueryGroup for DbConnection<C> {
    fn delete_oneshot_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError> {
        // Publisher boundary cursors have composite keys, not the bare group id.
        crate::remote_commit_log::QueryRemoteCommitLog::set_commit_log_publisher_boundaries(
            self,
            group_id,
            &Default::default(),
        )?;
        self.raw_query(|conn| {
            macro_rules! delete_rows {
                ($($table:ident.$key:ident),* $(,)?) => { $(
                    diesel::delete(super::schema::$table::table.filter(super::schema::$table::$key.eq(group_id))).execute(conn)?;
                )* };
            }
            delete_rows!(group_intents.group_id, group_messages.group_id, local_commit_log.group_id,
                remote_commit_log.group_id, refresh_state.entity_id, incoming_envelopes.entity_id,
                pending_remove.group_id, readd_status.group_id);
            diesel::delete(dsl::groups.filter(dsl::id.eq(group_id)).filter(dsl::conversation_type.eq(ConversationType::Oneshot))).execute(conn)?;
            Ok(())
        })
    }

    /// Return regular `Purpose::Conversation` groups with additional optional filters
    #[xmtp_common::db_span]
    // implements: CONS-060, CONS-031
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
            include_sync_groups: _,
            include_duplicate_dms,
            last_activity_after_ns,
            last_activity_before_ns,
            should_publish_commit_log,
            order_by,
            before_id: _,
        } = args.as_ref();

        // An empty filter names no consent state, so it matches no conversation.
        // Requested sync groups ignore the consent filter.
        if matches!(consent_states, Some(states) if states.is_empty()) {
            return self.requested_sync_group_rows(args.as_ref());
        }

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
            // Fast DM deduplication using EXISTS - avoids expensive window functions.
            // Keep one group for each dm_id (or the regular group if not a DM):
            // a joined row before a `Restored` archive placeholder, then the
            // latest message, then the highest id. `fetch_stitched` and
            // `conversation_list` rank the same way.
            query = query.filter(sql::<diesel::sql_types::Bool>(&format!(
                "NOT EXISTS (
                    SELECT 1 FROM groups g2
                    WHERE COALESCE(g2.dm_id, g2.id) = COALESCE(groups.dm_id, groups.id)
                    AND (g2.membership_state != {restored}, COALESCE(g2.last_message_ns, 0), g2.id)
                      > (groups.membership_state != {restored}, COALESCE(groups.last_message_ns, 0), groups.id)
                )",
                restored = GroupMembershipState::Restored as i32,
            )));
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

        let effective_consent_states = match consent_states {
            Some(states) => states.clone(),
            None => vec![ConsentState::Allowed, ConsentState::Unknown],
        };

        let includes_unknown = effective_consent_states.contains(&ConsentState::Unknown);
        // "All states requested" means the request covers every distinct
        // ConsentState, not merely that it has three elements — a caller passing
        // duplicates (e.g. `[Allowed, Allowed, Allowed]`) must not be treated as
        // an unfiltered query and leak Denied/Unknown conversations.
        let includes_all = [
            ConsentState::Allowed,
            ConsentState::Denied,
            ConsentState::Unknown,
        ]
        .iter()
        .all(|state| effective_consent_states.contains(state));

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
                .left_join(consent_dsl::consent_records.on(conversation_consent_join()))
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
                .inner_join(consent_dsl::consent_records.on(conversation_consent_join()))
                .filter(consent_dsl::state.eq_any(filtered_states.clone()))
                .select(dsl::groups::all_columns());

            self.raw_query(|conn| inner_joined_query.load::<StoredGroup>(conn))?
        };

        groups.append(&mut self.requested_sync_group_rows(args.as_ref())?);
        Ok(groups)
    }

    #[xmtp_common::db_span]
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

        self.raw_query(|conn| query.load::<StoredGroup>(conn))
    }

    /// Updates group membership state
    #[xmtp_common::db_span]
    fn update_group_membership<Id: AsRef<[u8]>>(
        &self,
        group_id: Id,
        state: GroupMembershipState,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id.as_ref()))
                .set(dsl::membership_state.eq(state))
                .execute(conn)
        })?;

        Ok(())
    }

    /// Newest local join time first; on an equal time, the greatest id first.
    /// A Restored placeholder is never selected: its archived time is
    /// import-controlled and the group cannot send or process traffic.
    // implements: SYNC-002, ARCH-015
    #[xmtp_common::db_span]
    fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .then_order_by(dsl::id.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .filter(dsl::membership_state.ne(GroupMembershipState::Restored));

        self.raw_query(|conn| query.load(conn))
    }

    /// The sync group with this id, unless it is a Restored placeholder.
    #[xmtp_common::db_span]
    fn find_sync_group(&self, id: &GroupId) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .filter(dsl::membership_state.ne(GroupMembershipState::Restored))
            .filter(dsl::id.eq(id));

        self.raw_query(|conn| query.first(conn).optional())
    }

    /// Return a single group that matches the given ID
    #[xmtp_common::db_span]
    fn find_group(&self, id: &GroupId) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .limit(1)
            .filter(dsl::id.eq(id));
        let groups = self.raw_query(|conn| query.load(conn))?;

        Ok(groups.into_iter().next())
    }

    /// Return a single group that matches the given welcome ID
    #[xmtp_common::db_span]
    fn find_group_by_sequence_id(
        &self,
        cursor: Cursor,
    ) -> Result<Option<StoredGroup>, crate::ConnectionError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .filter(dsl::sequence_id.eq(cursor.0 as i64));

        let groups = self.raw_query(|conn| query.load(conn))?;

        if groups.len() > 1 {
            tracing::warn!(
                welcome_id = cursor.0,
                "More than one group found for welcome_id {}",
                cursor.0
            );
        }
        Ok(groups.into_iter().next())
    }

    fn get_rotated_at_ns(&self, group_id: &GroupId) -> Result<i64, StorageError> {
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
    fn update_rotated_at_ns(&self, group_id: &GroupId) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::rotated_at_ns.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    fn get_installations_time_checked(&self, group_id: &GroupId) -> Result<i64, StorageError> {
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
    fn update_installations_time_checked(&self, group_id: &GroupId) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::installations_last_checked.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    fn update_message_disappearing_from_ns(
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

    fn update_message_disappearing_in_ns(
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

    // implements: JOIN-080
    fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError> {
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
                return Ok(self.raw_query(|c| dsl::groups.find(&group.id).first(c))?);
            }

            if existing_group.sequence_id == group.sequence_id {
                tracing::info!("Group welcome id already exists");
                // Error so OpenMLS db transaction are rolled back on duplicate welcomes
                Err(StorageError::Duplicate(DuplicateItem::WelcomeId(
                    existing_group.cursor(),
                )))
            } else {
                tracing::info!("Group already exists");
                // The row keeps its conversation type and dm id. Its only
                // production caller, XmtpWelcome::commit, rejects a Welcome
                // that would change either before it calls this.
                // If the welcome id is greater than the existing group welcome, update the welcome id
                // on the existing group
                if group.sequence_id.is_some()
                    && (existing_group.sequence_id.is_none()
                        || group.sequence_id > existing_group.sequence_id)
                {
                    self.raw_query(|c| {
                        diesel::update(dsl::groups.find(&group.id))
                            .set((dsl::sequence_id.eq(group.sequence_id),))
                            .execute(c)
                    })?;
                    existing_group.sequence_id = group.sequence_id;
                } else {
                    // Welcome publication order is not epoch order: an older
                    // Welcome can still rejoin an inactive group. It writes no
                    // triggered column, so record the change here.
                    self.bump_group_set_generation()?;
                }
                Ok(existing_group)
            }
        } else {
            Ok(self.raw_query(|c| dsl::groups.find(group.id).first(c))?)
        }
    }

    /// Get all the welcome ids turned into groups
    fn group_cursors(&self) -> Result<Vec<Cursor>, crate::ConnectionError> {
        self.raw_query(|conn| {
            Ok(dsl::groups
                .filter(dsl::sequence_id.is_not_null())
                .select(dsl::sequence_id)
                .load::<Option<i64>>(conn)?
                .into_iter()
                .flatten()
                .map(|sequence| Cursor(sequence as u64))
                .collect())
        })
    }

    fn mark_group_as_maybe_forked(
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

    fn clear_fork_flag_for_group(&self, group_id: &GroupId) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set((dsl::maybe_forked.eq(false), dsl::fork_details.eq("")))
                .execute(conn)
        })?;
        Ok(())
    }

    fn has_duplicate_dm(&self, group_id: &GroupId) -> Result<bool, crate::ConnectionError> {
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

    /// Get allowed publisher candidates, including groups whose legacy role
    /// has not yet been recovered. Sync and restored groups are excluded.
    // implements: FORK-086
    #[xmtp_common::db_span]
    fn get_conversation_ids_for_remote_log_publish(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        use crate::schema::consent_records::dsl as consent_dsl;

        let unrecovered = diesel::dsl::not(diesel::dsl::exists(
            crate::schema::refresh_state::table
                .filter(crate::schema::refresh_state::entity_id.eq(dsl::id))
                .filter(
                    crate::schema::refresh_state::entity_kind
                        .eq(super::refresh_state::EntityKind::CommitLogPublisherRecovery),
                ),
        ));
        let query = dsl::groups
            .filter(dsl::membership_state.ne(GroupMembershipState::Restored))
            .filter(
                dsl::conversation_type
                    .eq(ConversationType::Dm)
                    .or(dsl::conversation_type
                        .eq(ConversationType::Group)
                        .and(dsl::should_publish_commit_log.eq(true).or(unrecovered))),
            )
            .inner_join(consent_dsl::consent_records.on(conversation_consent_join()))
            .filter(consent_dsl::state.eq(ConsentState::Allowed))
            .select((dsl::id, dsl::commit_log_public_key))
            .order(dsl::created_at_ns.asc());

        self.raw_query(|conn| query.load::<StoredGroupCommitLogPublicKey>(conn))
    }

    #[xmtp_common::db_span]
    fn get_conversation_ids_for_remote_log_download(
        &self,
    ) -> Result<Vec<StoredGroupCommitLogPublicKey>, crate::ConnectionError> {
        let query = allowed_commit_log_groups().select((dsl::id, dsl::commit_log_public_key));
        self.raw_query(|conn| query.load::<StoredGroupCommitLogPublicKey>(conn))
    }

    // implements: FORK-082
    #[xmtp_common::db_span]
    fn get_conversation_ids_for_fork_check(&self) -> Result<Vec<Vec<u8>>, crate::ConnectionError> {
        let query = allowed_commit_log_groups()
            .filter(
                dsl::is_commit_log_forked
                    .is_null()
                    .or(dsl::is_commit_log_forked.ne(Some(true))),
            )
            .select(dsl::id);
        self.raw_query(|conn| query.load::<Vec<u8>>(conn))
    }

    #[xmtp_common::db_span]
    fn get_conversation_ids_for_requesting_readds(
        &self,
    ) -> Result<Vec<StoredGroupForReaddRequest>, crate::ConnectionError> {
        use super::schema::{groups::dsl as groups_dsl, remote_commit_log::dsl as rcl_dsl};
        use diesel::dsl::max;

        self.raw_query(|conn| {
            groups_dsl::groups
                .left_join(rcl_dsl::remote_commit_log.on(groups_dsl::id.eq(rcl_dsl::group_id)))
                .filter(groups_dsl::membership_state.ne(GroupMembershipState::Restored))
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
    fn get_conversation_ids_for_responding_readds(
        &self,
    ) -> Result<Vec<StoredGroupForRespondingReadds>, crate::ConnectionError> {
        use super::schema::{groups::dsl as groups_dsl, readd_status::dsl as readd_dsl};
        use diesel::{ExpressionMethods, JoinOnDsl, QueryDsl};

        self.raw_query(|conn| {
            readd_dsl::readd_status
                .inner_join(groups_dsl::groups.on(readd_dsl::group_id.eq(groups_dsl::id)))
                .filter(groups_dsl::membership_state.ne(GroupMembershipState::Restored))
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
    fn get_conversation_type(
        &self,
        group_id: &GroupId,
    ) -> Result<ConversationType, crate::ConnectionError> {
        let query = dsl::groups
            .filter(dsl::id.eq(group_id))
            .select(dsl::conversation_type);
        let conversation_type = self.raw_query(|conn| query.first(conn))?;
        Ok(conversation_type)
    }

    // implements: FORK-085
    fn set_should_publish_commit_log(
        &self,
        group_id: &GroupId,
        should_publish: bool,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id))
                .set(dsl::should_publish_commit_log.eq(should_publish))
                .execute(conn)?;
            Ok(())
        })
    }

    fn set_group_commit_log_forked_status(
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

    fn get_group_commit_log_forked_status(
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

    fn set_group_has_pending_leave_request_status(
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

        self.raw_query(|conn| query.load::<Vec<u8>>(conn))
    }

    // implements: PROC-015
    #[xmtp_common::db_span]
    fn stored_group_refs(
        &self,
        scope: &GroupScope,
    ) -> Result<Vec<StoredGroupRef>, crate::ConnectionError> {
        let GroupScope {
            consent,
            include_sync,
            include_restored,
        } = scope;
        let filter_restored = |query: groups::BoxedQuery<'static, Sqlite>| {
            if *include_restored {
                query
            } else {
                query.filter(dsl::membership_state.ne(GroupMembershipState::Restored))
            }
        };
        let newest_first = (dsl::created_at_ns.desc(), dsl::id.desc());

        let mut refs = match consent {
            Some(states) if states.is_empty() => Vec::new(),
            consent => {
                let mut query = filter_restored(
                    dsl::groups
                        .filter(dsl::conversation_type.ne_all(ConversationType::virtual_types()))
                        .into_boxed(),
                );
                let all_states = [
                    ConsentState::Unknown,
                    ConsentState::Allowed,
                    ConsentState::Denied,
                ];
                // A group with no consent record is Unknown. A filter that
                // names every state selects every group without a lookup.
                if let Some(states) = consent
                    && !all_states.iter().all(|state| states.contains(state))
                {
                    let has_state = diesel::dsl::exists(
                        consent_records::table
                            .filter(conversation_consent_join())
                            .filter(consent_records::state.eq_any(states.clone())),
                    );
                    query = if states.contains(&ConsentState::Unknown) {
                        let has_record = diesel::dsl::exists(
                            consent_records::table.filter(conversation_consent_join()),
                        );
                        query.filter(has_state.or(diesel::dsl::not(has_record)))
                    } else {
                        query.filter(has_state)
                    };
                }
                let query = query
                    .order(newest_first)
                    .select(StoredGroupRef::as_select());
                self.raw_query(|conn| query.load(conn))?
            }
        };
        if *include_sync {
            let query = filter_restored(
                dsl::groups
                    .filter(dsl::conversation_type.eq(ConversationType::Sync))
                    .into_boxed(),
            )
            .order(newest_first)
            .select(StoredGroupRef::as_select());
            refs.extend(self.raw_query(|conn| query.load(conn))?);
        }
        Ok(refs)
    }

    #[xmtp_common::db_span]
    fn group_set_generation(&self) -> Result<i64, crate::ConnectionError> {
        use crate::schema::group_set_generation::dsl as generation_dsl;
        self.raw_query(|conn| {
            generation_dsl::group_set_generation
                .select(generation_dsl::generation)
                .first(conn)
        })
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

pub use xmtp_proto::types::ConversationType;

pub trait DmIdExt {
    /// Return the peer only when this inbox is in a two-member DM ID.
    fn other_inbox_id(&self, id: &str) -> Option<String>;
}

impl DmIdExt for String {
    fn other_inbox_id(&self, id: &str) -> Option<String> {
        // Archive and MLS admission validate canonical inbox IDs. This lookup
        // checks the structure and membership before it returns a peer.
        let (first, second) = self.strip_prefix("dm:")?.split_once(':')?;
        if first.is_empty() || second.is_empty() || second.contains(':') || first == second {
            return None;
        }
        if id == first {
            Some(second.to_string())
        } else if id == second {
            Some(first.to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub use super::dms::tests::*;
    use super::*;

    use crate::{
        Fetch, Store,
        consent_record::{ConsentType, StoredConsentRecord},
        readd_status::ReaddStatus,
        schema::groups::dsl::groups,
        test_utils::{with_connection, with_connection_async},
    };
    use xmtp_common::{Generate, assert_ok, rand_vec, time::now_ns};

    // verifies: DMS-017
    #[xmtp_common::test(unwrap_try = true)]
    fn dm_peer_requires_member_and_pair_structure() {
        let pair = "dm:alix:bo".to_string();
        assert_eq!(pair.other_inbox_id("alix").as_deref(), Some("bo"));
        assert_eq!(pair.other_inbox_id("bo").as_deref(), Some("alix"));
        assert_eq!(pair.other_inbox_id("charlie"), None);
        for malformed in [
            "dm:short",
            "dm:alix:",
            "dm::bo",
            "dm:alix:bo:extra",
            "dm:alix:alix",
        ] {
            assert_eq!(malformed.to_string().other_inbox_id("alix"), None);
        }
    }

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
            .conversation_type(ConversationType::Group)
            .build()
            .unwrap()
    }

    /// The own identity `generate_dm` names, and the consent record of `dm`'s
    /// peer in `state`.
    pub fn dm_peer_consent(
        conn: &impl crate::DbQuery,
        dm: &StoredGroup,
        state: ConsentState,
    ) -> StoredConsentRecord {
        let own = "placeholder_inbox_id_1";
        if conn
            .raw_query(|c| crate::schema::identity::table.count().get_result::<i64>(c))
            .unwrap()
            == 0
        {
            crate::identity::StoredIdentity::new(own.into(), vec![0], vec![0])
                .store(conn)
                .unwrap();
        }
        let peer = dm.dm_id.clone().unwrap().other_inbox_id(own).unwrap();
        generate_consent_record(ConsentType::InboxId, state, peer)
    }

    /// Generate a test consent
    pub fn generate_consent_record(
        entity_type: ConsentType,
        state: ConsentState,
        entity: String,
    ) -> StoredConsentRecord {
        StoredConsentRecord::new(entity_type, state, entity)
    }

    #[xmtp_common::test]
    fn test_it_stores_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            test_group.store(conn).unwrap();
            assert_eq!(
                conn.raw_query(|raw_conn| groups.first::<StoredGroup>(raw_conn))
                    .unwrap(),
                test_group
            );
        })
    }

    #[xmtp_common::test]
    fn test_it_fetches_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            conn.raw_query(|raw_conn| {
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
            conn.update_group_membership(test_group.id, GroupMembershipState::Rejected)
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
                .other_inbox_id("placeholder_inbox_id_1")
                .expect("DM peer");

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
            let synced_groups = conn.all_sync_groups().unwrap();
            assert!(synced_groups.is_empty());

            // test that dm groups are included
            let dm_results = conn.find_groups(GroupQueryArgs::default()).unwrap();
            assert_eq!(dm_results.len(), 3);
            assert_eq!(dm_results[2].id, test_group_3.id);

            // test find_dm_group
            let dm_result = conn
                .find_active_dm_group(format!("dm:placeholder_inbox_id_1:{}", other_inbox_id))
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
            let result = conn.update_installations_time_checked(&test_group.id);
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

            conn.raw_query(|raw_conn| {
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

    /// A keyed group's 64-hex id has the shape of an inbox id. An inbox
    /// consent record whose entity equals that id is not the group's consent.
    /// Both groups would publish their commit logs if their consent were
    /// Allowed.
    #[xmtp_common::test]
    fn inbox_consent_is_not_group_consent() {
        with_connection(|conn| {
            let keyed = || {
                let mut group = generate_group(Some(GroupMembershipState::Allowed));
                group.should_publish_commit_log = true;
                group.commit_log_public_key = Some(rand_vec::<32>());
                group.store(conn).unwrap();
                group
            };
            let denied = keyed();
            let allowed = keyed();
            for (group, state) in [
                (&denied, ConsentState::Denied),
                (&allowed, ConsentState::Allowed),
            ] {
                generate_consent_record(ConsentType::InboxId, state, hex::encode(group.id))
                    .store(conn)
                    .unwrap();
            }
            let found = |states: Vec<ConsentState>| {
                let mut ids: Vec<_> = conn
                    .find_groups(GroupQueryArgs {
                        consent_states: Some(states),
                        ..Default::default()
                    })
                    .unwrap()
                    .into_iter()
                    .map(|group| group.id)
                    .collect();
                ids.sort();
                ids
            };
            let mut both = vec![denied.id, allowed.id];
            both.sort();
            assert_eq!(found(vec![ConsentState::Unknown]), both);
            assert!(found(vec![ConsentState::Denied]).is_empty());
            assert!(found(vec![ConsentState::Allowed]).is_empty());
            assert!(
                conn.get_conversation_ids_for_remote_log_download()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                conn.get_conversation_ids_for_remote_log_publish()
                    .unwrap()
                    .is_empty()
            );
            for group in [&denied, &allowed] {
                let (_, consent) = conn.conversation_state_row(&group.id).unwrap().unwrap();
                assert_eq!(consent, None);
            }
        })
    }

    // verifies: CONS-060, SYNC-005
    #[xmtp_common::test(unwrap_try = true)]
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
                hex::encode(test_group_1.id),
            );
            test_group_1_consent.store(conn).unwrap();
            let test_group_2_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(test_group_2.id),
            );
            test_group_2_consent.store(conn).unwrap();
            // A DM's consent is its peer's inbox record.
            let test_group_3_consent = dm_peer_consent(conn, &test_group_3, ConsentState::Allowed);
            test_group_3_consent.store(conn).unwrap();

            use ConsentState::{Allowed, Denied, Unknown};
            let (g1, g2, g3, g4) = (
                &test_group_1.id,
                &test_group_2.id,
                &test_group_3.id,
                &test_group_4.id,
            );
            // Repeats do not count toward naming every state.
            let cases = [
                (Some(vec![Allowed, Unknown, Denied]), vec![g1, g2, g3, g4]),
                (None, vec![g1, g3, g4]),
                (Some(vec![Allowed]), vec![g1, g3]),
                (Some(vec![Allowed, Unknown]), vec![g1, g3, g4]),
                (Some(vec![Denied]), vec![g2]),
                (Some(vec![Unknown]), vec![g4]),
                (Some(vec![]), vec![]),
                (Some(vec![Allowed, Allowed, Allowed]), vec![g1, g3]),
                (Some(vec![Allowed, Denied, Denied]), vec![g1, g2, g3]),
                (Some(vec![Unknown, Unknown, Allowed]), vec![g1, g3, g4]),
            ];
            for (consent_states, mut expected) in cases {
                let mut ids: Vec<_> = conn
                    .find_groups(GroupQueryArgs {
                        consent_states: consent_states.clone(),
                        ..Default::default()
                    })?
                    .into_iter()
                    .map(|group| group.id)
                    .collect();
                ids.sort();
                expected.sort();
                assert_eq!(
                    ids.iter().collect::<Vec<_>>(),
                    expected,
                    "{consent_states:?}"
                );
            }

            let mut sync_group = generate_group(Some(GroupMembershipState::Allowed));
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn).unwrap();
            let with_sync = conn
                .find_groups(GroupQueryArgs {
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            assert!(with_sync.iter().any(|group| group.id == sync_group.id));
            let empty_with_sync = conn
                .find_groups(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            // The empty filter still matches no normal group, but a requested
            // sync group ignores the consent filter, as in fetch_conversation_list.
            let ids = |rows: Vec<StoredGroup>| rows.into_iter().map(|g| g.id).collect::<Vec<_>>();
            assert_eq!(ids(empty_with_sync), vec![sync_group.id]);
            use crate::conversation_list::QueryConversationList;
            let listed = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                listed.into_iter().map(|g| g.id).collect::<Vec<_>>(),
                vec![sync_group.id]
            );
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
                    .map(|c| c.0)
                    .collect::<Vec<u64>>()
            );
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_insert_or_replace_group_update_preserves_cursor() {
        with_connection(|conn| {
            let group = generate_group(None);
            assert!(group.sequence_id.is_none());
            group.store(conn).unwrap();
            conn.insert_or_replace_group(StoredGroup {
                sequence_id: Some(5),
                ..group.clone()
            })
            .unwrap();
            let stored: StoredGroup = conn.fetch(&group.id).unwrap().unwrap();
            assert_eq!(stored.sequence_id, Some(5));
            assert_eq!(conn.group_cursors().unwrap(), vec![Cursor(5)]);
        });
    }

    // verifies: JOIN-080
    #[xmtp_common::test(unwrap_try = true)]
    fn restored_replacement_returns_activated_group() {
        with_connection(|conn| {
            let restored = generate_group(Some(GroupMembershipState::Restored));
            restored.store(conn).unwrap();
            let activated = StoredGroup {
                membership_state: GroupMembershipState::Allowed,
                sequence_id: Some(5),
                ..restored.clone()
            };
            let returned = conn.insert_or_replace_group(activated.clone()).unwrap();
            assert_eq!(returned.membership_state, GroupMembershipState::Allowed);
            assert_eq!(returned.sequence_id, Some(5));
            let stored: StoredGroup = conn.fetch(&restored.id).unwrap().unwrap();
            assert_eq!(stored.membership_state, GroupMembershipState::Allowed);
            assert_eq!(stored.sequence_id, Some(5));
        });
    }

    // verifies: CONS-031
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
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
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
                hex::encode(group1.id),
            )
            .store(conn)?;
            group2.should_publish_commit_log = true;
            group2.commit_log_public_key = Some(rand_vec::<32>());

            group3.should_publish_commit_log = true;
            group3.commit_log_public_key = Some(rand_vec::<32>());
            generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(group3.id),
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
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
            );
            denied_consent.store(conn).unwrap();

            // Function should only return groups with Allowed consent state
            let commit_log_keys = conn.get_conversation_ids_for_remote_log_publish().unwrap();
            assert_eq!(commit_log_keys.len(), 1);
            assert_eq!(commit_log_keys[0].id, allowed_group.id);
        })
    }

    // verifies: SYNC-002
    #[xmtp_common::test]
    fn sync_groups_with_equal_time_order_by_greatest_id() {
        with_connection(|conn| {
            let sync_group = |id: u8, created_at_ns| {
                let mut group = generate_group_with_created_at(None, created_at_ns);
                group.id = GroupId::from([id; 16]);
                group.conversation_type = ConversationType::Sync;
                group.store(conn).unwrap();
                group.id
            };
            // One pair is stored in id order and one in reverse, so an
            // order that ignores the id fails for one of them.
            let older = [sync_group(1, 1_000), sync_group(2, 1_000)];
            let newer = [sync_group(4, 2_000), sync_group(3, 2_000)];

            let ids: Vec<_> = conn
                .all_sync_groups()
                .unwrap()
                .into_iter()
                .map(|group| group.id)
                .collect();
            assert_eq!(ids, [newer[0], newer[1], older[1], older[0]]);
        })
    }

    /// A sync group is stored after every sync group held, even when the
    /// clock was set back after the older group was stored.
    // verifies: SYNC-031
    #[xmtp_common::test]
    fn sync_join_time_grows_after_clock_set_back() {
        with_connection(|conn| {
            let ahead = now_ns() + 2 * xmtp_common::NS_IN_HOUR;
            let mut older = generate_group_with_created_at(None, ahead);
            older.conversation_type = ConversationType::Sync;
            older.store(conn).unwrap();
            let joined = next_sync_join_time(conn).unwrap();
            assert_eq!(joined, ahead + 1);
            let mut newer = generate_group_with_created_at(None, joined);
            newer.conversation_type = ConversationType::Sync;
            newer.store(conn).unwrap();
            assert_eq!(conn.all_sync_groups().unwrap()[0].id, newer.id);
        })
    }

    /// The SQL join and `ConsentKey::of_group` give a group the same consent
    /// record, or both none, for every dm id shape.
    // verifies: CONS-052, CONS-060
    #[xmtp_common::test(unwrap_try = true)]
    fn consent_join_sql_matches_of_group() {
        use crate::consent_record::ConsentKey;
        use diesel::sql_types::{Binary, Integer, Nullable, Text};
        #[derive(QueryableByName)]
        struct Joined {
            #[diesel(sql_type = Nullable<Integer>)]
            entity_type: Option<i32>,
            #[diesel(sql_type = Nullable<Text>)]
            entity: Option<String>,
        }
        with_connection(|conn| {
            crate::identity::StoredIdentity::new("me".into(), vec![0], vec![0]).store(conn)?;
            let entities = ["p", "q", "me", "a", "b", "a:b", "p:q", "x"];
            for entity in entities {
                generate_consent_record(ConsentType::InboxId, ConsentState::Denied, entity.into())
                    .store(conn)?;
            }
            let dm_ids = [
                None,
                Some("dm:me:p"),
                Some("dm:p:me"),
                Some("dm:me:P"),
                Some("dm:a:b"),
                Some("dm:me:me"),
                Some("dm:me:p:q"),
                Some("dm::me"),
                Some("dm:me:"),
                Some("dm:me"),
                Some("x:me:p"),
            ];
            for dm_id in dm_ids {
                let mut group = generate_group(None);
                group.dm_id = dm_id.map(String::from);
                group.store(conn)?;
                generate_consent_record(
                    ConsentType::ConversationId,
                    ConsentState::Denied,
                    hex::encode(group.id),
                )
                .store(conn)?;
                let joined = conn.raw_query(|c| {
                    diesel::sql_query(format!(
                        "SELECT consent_records.entity_type, consent_records.entity FROM groups \
                         LEFT JOIN consent_records ON {} WHERE groups.id = ?",
                        consent_join_sql("groups")
                    ))
                    .bind::<Binary, _>(group.id)
                    .get_result::<Joined>(c)
                })?;
                let rust = ConsentKey::of_group(&group, "me")
                    .map(|key| (key.entity_type as i32, key.entity));
                let sql = joined.entity_type.zip(joined.entity);
                assert_eq!(sql, rust, "{dm_id:?}");
            }
            Ok::<_, StorageError>(())
        })?;
    }

    /// An imported Restored sync group carries an archive-controlled join
    /// time. It is never selected, however new that time is: only active sync
    /// groups can publish or process preference updates.
    // verifies: SYNC-002, ARCH-015
    #[xmtp_common::test]
    fn restored_sync_group_is_never_selected() {
        with_connection(|conn| {
            let sync_group = |id: u8, created_at_ns, membership_state| {
                let mut group = generate_group_with_created_at(None, created_at_ns);
                group.id = GroupId::from([id; 16]);
                group.conversation_type = ConversationType::Sync;
                group.membership_state = membership_state;
                group.store(conn).unwrap();
                group.id
            };
            let active = sync_group(1, 1_000, GroupMembershipState::Allowed);
            // Newer than the active group, and greater id: it would win on both keys.
            let restored = sync_group(9, 2_000, GroupMembershipState::Restored);

            let ids: Vec<_> = conn
                .all_sync_groups()
                .unwrap()
                .into_iter()
                .map(|group| group.id)
                .collect();
            assert_eq!(ids, [active]);
            assert!(conn.find_sync_group(&restored).unwrap().is_none());
            assert_eq!(
                conn.find_sync_group(&active).unwrap().map(|group| group.id),
                Some(active)
            );
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
                hex::encode(sync_group.id),
            );
            sync_consent.store(conn).unwrap();

            // Create consent records
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
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
            group1.store(conn).unwrap();

            let group2 = StoredGroup::builder()
                .id(group_id_2)
                .created_at_ns(2000)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("placeholder_address")
                .build()
                .unwrap();
            group2.store(conn).unwrap();

            let group3 = StoredGroup::builder()
                .id(group_id_3)
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
                status.store(conn).unwrap();
            }

            // Call the method under test
            let result = conn.get_conversation_ids_for_responding_readds().unwrap();

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
    #[test]
    fn test_find_group_span_emits_operation_and_skips_group_id() {
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

        with_connection(|conn| {
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
                // `find_group` is synchronous; no runtime needed for the call.
                let _ = conn.find_group(&test_group.id);
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
            // Stronger: only `operation` plus the static `sentry.*` and `otel.*`
            // vendor hints (emitted by the span macros for Sentry op/name mapping
            // and for the exported OpenTelemetry span name) may appear — every one
            // is a compile-time literal, so the cardinality contract holds.
            let fields = logged
                .split_once('{')
                .and_then(|(_, rest)| rest.split_once('}'))
                .map(|(inner, _)| inner.trim())
                .unwrap_or("");
            assert_eq!(
                fields,
                "operation=\"db.find_group\" sentry.op=\"db\" sentry.name=\"db.find_group\" \
                 otel.name=\"db.find_group\"",
                "find_group span must carry only operation + static sentry.*/otel.* fields, \
                 got: {fields:?}"
            );
        })
    }

    /// Welcome sync and group discovery enumerate stored groups through this
    /// projection, which has no message field and no row limit. Each scope the
    /// callers use selects, in the same order, what that caller's former query
    /// selected, in every membership state.
    // verifies: PROC-015
    #[xmtp_common::test(unwrap_try = true)]
    fn stored_group_refs_select_like_listing_without_previews() {
        use crate::conversation_list::QueryConversationList;
        use ConsentState::{Allowed, Denied, Unknown};

        // More than a default page, so a row limit would leave some out.
        const UNKNOWN_GROUPS: usize = 150;

        // The pattern names every field, so a field added to the projection
        // fails to compile here.
        fn ids(refs: Vec<StoredGroupRef>) -> Vec<GroupId> {
            refs.into_iter()
                .map(
                    |StoredGroupRef {
                         id,
                         dm_id: _,
                         conversation_type: _,
                         created_at_ns: _,
                         membership_state: _,
                     }| id,
                )
                .collect()
        }
        fn sorted(ids: impl IntoIterator<Item = GroupId>) -> Vec<GroupId> {
            let mut ids: Vec<_> = ids.into_iter().collect();
            ids.sort();
            ids
        }

        with_connection(|conn| -> Result<(), StorageError> {
            let mut created_at_ns = 0;
            let mut store = |state: GroupMembershipState,
                             conversation_type: ConversationType,
                             dm_id: Option<&str>,
                             consent: Option<ConsentState>|
             -> Result<GroupId, StorageError> {
                created_at_ns += 1;
                let mut group = generate_group_with_created_at(Some(state), created_at_ns);
                group.conversation_type = conversation_type;
                group.dm_id = dm_id.map(str::to_string);
                group.store(conn)?;
                if let Some(consent) = consent {
                    generate_consent_record(
                        ConsentType::ConversationId,
                        consent,
                        hex::encode(group.id),
                    )
                    .store(conn)?;
                }
                Ok(group.id)
            };
            use ConversationType::{Dm, Group, Oneshot, Sync};
            use GroupMembershipState::{
                Allowed as Joined, Pending, PendingRemove, Rejected, Restored,
            };
            let mut unknown = Vec::new();
            for _ in 0..UNKNOWN_GROUPS {
                unknown.push(store(Joined, Group, None, None)?);
            }
            let allowed = store(Joined, Group, None, Some(Allowed))?;
            let denied = store(Joined, Group, None, Some(Denied))?;
            let recorded_unknown = store(Joined, Group, None, Some(Unknown))?;
            let pending = store(Pending, Group, None, None)?;
            let leaving = store(PendingRemove, Group, None, None)?;
            let rejected = store(Rejected, Group, None, None)?;
            let restored = store(Restored, Group, None, None)?;
            let oneshot = store(Joined, Oneshot, None, Some(Allowed))?;
            let sync = store(Joined, Sync, None, Some(Denied))?;
            let restored_sync = store(Restored, Sync, None, None)?;
            let dm = store(Joined, Dm, Some("dm:alix:bo"), None)?;
            let duplicate_dm = store(Joined, Dm, Some("dm:alix:bo"), None)?;

            let listed = |consent_states: Option<Vec<ConsentState>>| {
                conn.fetch_conversation_list(GroupQueryArgs {
                    consent_states,
                    include_sync_groups: true,
                    include_duplicate_dms: true,
                    ..Default::default()
                })
            };
            let found = |consent_states: Option<Vec<ConsentState>>| {
                let found = conn.find_groups(GroupQueryArgs {
                    consent_states,
                    include_sync_groups: true,
                    include_duplicate_dms: true,
                    ..Default::default()
                })?;
                Ok::<_, StorageError>(found)
            };
            let refs = |scope: GroupScope| conn.stored_group_refs(&scope).map(ids);

            let filters = [
                None,
                Some(vec![Allowed]),
                Some(vec![Denied]),
                Some(vec![Unknown, Denied]),
                Some(vec![]),
            ];
            for states in filters {
                // sync_welcomes and barrier Welcome discovery: the listing,
                // Restored rows kept.
                let listing = refs(GroupScope::listing(states.clone()))?;
                let rows = listed(states.clone())?;
                assert_eq!(listing, rows.iter().map(|g| g.id).collect::<Vec<_>>());
                assert_eq!(
                    sorted(listing),
                    sorted(found(states.clone())?.into_iter().map(|g| g.id))
                );
                // sync_all_welcomes_and_groups: the listing without Restored rows.
                let unrestored = rows
                    .iter()
                    .filter(|g| g.membership_state != Restored)
                    .map(|g| g.id);
                assert_eq!(
                    refs(GroupScope::listing(states).without_restored())?,
                    unrestored.collect::<Vec<_>>()
                );
            }
            let expected = unknown.iter().copied().chain([
                allowed,
                recorded_unknown,
                pending,
                leaving,
                rejected,
                restored,
                sync,
                restored_sync,
                dm,
                duplicate_dm,
            ]);
            assert_eq!(sorted(refs(GroupScope::listing(None))?), sorted(expected));
            // Newest first.
            assert_eq!(
                refs(GroupScope::listing(Some(vec![])))?,
                vec![restored_sync, sync]
            );

            // The incoming controller: every group, without Restored rows.
            let all_groups = refs(GroupScope::every_group().without_restored())?;
            let selected = found(Some(vec![Allowed, Unknown, Denied]))?
                .into_iter()
                .filter(|g| g.membership_state != Restored)
                .map(|g| g.id);
            assert_eq!(sorted(all_groups.clone()), sorted(selected));
            let expected = unknown.iter().copied().chain([
                allowed,
                denied,
                recorded_unknown,
                pending,
                leaving,
                rejected,
                sync,
                dm,
                duplicate_dm,
            ]);
            assert_eq!(sorted(all_groups), sorted(expected));
            assert!(
                !refs(GroupScope::every_group())?.contains(&oneshot),
                "a Oneshot group was selected"
            );

            // The listing query can be row-limited; the projection has no limit.
            let limited = conn.fetch_conversation_list(GroupQueryArgs {
                limit: Some(4),
                include_duplicate_dms: true,
                ..Default::default()
            })?;
            assert_eq!(limited.len(), 4);
            Ok(())
        })?;
    }

    /// The generation changes with each write that can change a gated group
    /// selection, and with no other write.
    #[xmtp_common::test(unwrap_try = true)]
    fn group_set_generation_tracks_selection_changes() {
        use crate::group_message::tests::generate_message;
        use crate::prelude::*;
        use crate::schema::groups::dsl;

        with_connection(|conn| -> Result<(), StorageError> {
            assert_eq!(conn.group_set_generation()?, 0);
            let mut last = 0;
            let mut changed_by = || -> Result<i64, StorageError> {
                let current = conn.group_set_generation()?;
                let change = current - last;
                last = current;
                Ok(change)
            };

            let group = generate_group(None);
            group.store(conn)?;
            assert_eq!(changed_by()?, 1, "group insert");
            let removed = generate_group(None);
            removed.store(conn)?;
            conn.raw_query(|c| diesel::delete(dsl::groups.find(&removed.id)).execute(c))?;
            assert_eq!(changed_by()?, 2, "group insert and delete");
            conn.update_group_membership(group.id, GroupMembershipState::PendingRemove)?;
            assert_eq!(changed_by()?, 1, "membership state");
            conn.update_group_membership(group.id, GroupMembershipState::PendingRemove)?;
            assert_eq!(changed_by()?, 0, "same membership state");
            conn.raw_query(|c| {
                diesel::update(dsl::groups.find(&group.id))
                    .set(dsl::sequence_id.eq(Some(5)))
                    .execute(c)
            })?;
            assert_eq!(changed_by()?, 1, "Welcome cursor");
            let welcome = |sequence_id| StoredGroup {
                sequence_id: Some(sequence_id),
                ..group.clone()
            };
            conn.insert_or_replace_group(welcome(7))?;
            assert_eq!(changed_by()?, 1, "newer Welcome for a stored group");
            conn.insert_or_replace_group(welcome(3))?;
            assert_eq!(changed_by()?, 1, "older Welcome for a stored group");
            conn.raw_query(|c| {
                diesel::update(dsl::groups.find(&group.id))
                    .set(dsl::conversation_type.eq(ConversationType::Dm))
                    .execute(c)
            })?;
            assert_eq!(changed_by()?, 1, "conversation type");
            conn.raw_query(|c| {
                diesel::update(dsl::groups.find(&group.id))
                    .set(dsl::dm_id.eq(Some("dm:alix:bo")))
                    .execute(c)
            })?;
            assert_eq!(changed_by()?, 1, "DM id");
            conn.update_installations_time_checked(&group.id)?;
            assert_eq!(changed_by()?, 0, "unselected column");
            generate_message(None, Some(&group.id), None, None, None, None).store(conn)?;
            assert_eq!(changed_by()?, 0, "message insert");

            let consent = |entity_type, state, entity: String| {
                use crate::consent_record::{ConsentKey, ConsentWrite, Decision, Owed};
                let key = ConsentKey {
                    entity_type,
                    entity,
                };
                let write = ConsentWrite::Decide(Decision::Set(state));
                crate::consent_record::write_consent(conn, &key, write, Owed::Mark)
            };
            let entity = hex::encode(group.id);
            consent(
                ConsentType::ConversationId,
                ConsentState::Denied,
                entity.clone(),
            )?;
            assert_eq!(changed_by()?, 1, "conversation consent insert");
            consent(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                entity.clone(),
            )?;
            assert_eq!(changed_by()?, 1, "conversation consent change");
            consent(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                entity.clone(),
            )?;
            assert_eq!(changed_by()?, 0, "same conversation consent");
            conn.raw_query(|c| {
                diesel::delete(
                    consent_records::table
                        .filter(consent_records::entity_type.eq(ConsentType::ConversationId))
                        .filter(consent_records::entity.eq(&entity)),
                )
                .execute(c)
            })?;
            assert_eq!(changed_by()?, 1, "conversation consent delete");
            // Inbox consent decides DMs, so it changes the group set.
            consent(ConsentType::InboxId, ConsentState::Denied, "bo".into())?;
            assert_eq!(changed_by()?, 1, "inbox consent insert");
            consent(ConsentType::InboxId, ConsentState::Allowed, "bo".into())?;
            assert_eq!(changed_by()?, 1, "inbox consent change");
            crate::sync_outbox::mark_all_owed(conn)?;
            assert_eq!(changed_by()?, 0, "owed marks only");

            conn.record_welcome_discovery(group.id, Cursor(5))?;
            assert_eq!(changed_by()?, 1, "Welcome discovery");
            conn.record_welcome_discovery(group.id, Cursor(6))?;
            assert_eq!(changed_by()?, 0, "repeated Welcome discovery");

            Ok(())
        })?;
    }
}
