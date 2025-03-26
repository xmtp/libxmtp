//! The Group database table. Stored information surrounding group membership and ID's.
use super::{
    consent_record::{ConsentState, StoredConsentRecord},
    db_connection::DbConnection,
    schema::groups::{self, dsl},
    Sqlite,
};

use crate::{
    groups::group_metadata::DmMembers, impl_fetch, impl_store, DuplicateItem, StorageError,
};

use crate::storage::NotFound;

use crate::groups::group_mutable_metadata::MessageDisappearingSettings;
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

pub type ID = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Insertable, Identifiable, Queryable)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
/// A Unique group chat
pub struct StoredGroup {
    /// Randomly generated ID by group creator
    pub id: Vec<u8>,
    /// Based on timestamp of this welcome message
    pub created_at_ns: i64,
    /// Enum, [`GroupMembershipState`] representing access to the group
    pub membership_state: GroupMembershipState,
    /// Track when the latest, most recent installations were checked
    pub installations_last_checked: i64,
    /// The inbox_id of who added the user to a group.
    pub added_by_inbox_id: String,
    /// The sequence id of the welcome message
    pub welcome_id: Option<i64>,
    /// The last time the leaf node encryption key was rotated
    pub rotated_at_ns: i64,
    /// Enum, [`ConversationType`] signifies the group conversation type which extends to who can access it.
    pub conversation_type: ConversationType,
    /// The inbox_id of the DM target
    pub dm_id: Option<String>,
    /// Timestamp of when the last message was sent for this group (updated automatically in a trigger)
    pub last_message_ns: Option<i64>,
    /// The Time in NS when the messages should be deleted
    pub message_disappear_from_ns: Option<i64>,
    /// How long a message in the group can live in NS
    pub message_disappear_in_ns: Option<i64>,
    /// The version of the protocol that the group is paused for, None is not paused
    pub paused_for_version: Option<String>,
}

impl_fetch!(StoredGroup, groups, Vec<u8>);
impl_store!(StoredGroup, groups);

impl StoredGroup {
    /// Create a new group from a welcome message
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_welcome(
        id: ID,
        created_at_ns: i64,
        membership_state: GroupMembershipState,
        added_by_inbox_id: String,
        welcome_id: i64,
        conversation_type: ConversationType,
        dm_members: Option<DmMembers<String>>,
        message_disappearing_settings: Option<MessageDisappearingSettings>,
        paused_for_version: Option<String>,
        last_message_ns: Option<i64>,
    ) -> Self {
        Self {
            id,
            created_at_ns,
            membership_state,
            installations_last_checked: 0,
            conversation_type,
            added_by_inbox_id,
            welcome_id: Some(welcome_id),
            rotated_at_ns: 0,
            dm_id: dm_members.map(String::from),
            last_message_ns,
            message_disappear_from_ns: message_disappearing_settings.as_ref().map(|s| s.from_ns),
            message_disappear_in_ns: message_disappearing_settings.map(|s| s.in_ns),
            paused_for_version,
        }
    }

    /// Create a new [`Purpose::Conversation`] group. This is the default type of group.
    pub fn new(
        id: ID,
        created_at_ns: i64,
        membership_state: GroupMembershipState,
        added_by_inbox_id: String,
        dm_members: Option<DmMembers<String>>,
        message_disappearing_settings: Option<MessageDisappearingSettings>,
        paused_for_version: Option<String>,
    ) -> Self {
        Self {
            id,
            created_at_ns,
            membership_state,
            installations_last_checked: 0,
            conversation_type: match dm_members {
                Some(_) => ConversationType::Dm,
                None => ConversationType::Group,
            },
            added_by_inbox_id,
            welcome_id: None,
            rotated_at_ns: 0,
            dm_id: dm_members.map(String::from),
            last_message_ns: None,
            message_disappear_from_ns: message_disappearing_settings.as_ref().map(|s| s.from_ns),
            message_disappear_in_ns: message_disappearing_settings.map(|s| s.in_ns),
            paused_for_version,
        }
    }

    /// Create a new [`Purpose::Sync`] group.  This is less common and is used to sync message history.
    /// TODO: Set added_by_inbox to your own inbox_id
    pub fn new_sync_group(
        id: ID,
        created_at_ns: i64,
        membership_state: GroupMembershipState,
    ) -> Self {
        Self {
            id,
            created_at_ns,
            membership_state,
            installations_last_checked: 0,
            conversation_type: ConversationType::Sync,
            added_by_inbox_id: "".into(),
            welcome_id: None,
            rotated_at_ns: 0,
            dm_id: None,
            last_message_ns: None,
            message_disappear_from_ns: None,
            message_disappear_in_ns: None,
            paused_for_version: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct GroupQueryArgs {
    pub allowed_states: Option<Vec<GroupMembershipState>>,
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub limit: Option<i64>,
    pub conversation_type: Option<ConversationType>,
    pub consent_states: Option<Vec<ConsentState>>,
    pub include_sync_groups: bool,
    pub include_duplicate_dms: bool,
}

impl AsRef<GroupQueryArgs> for GroupQueryArgs {
    fn as_ref(&self) -> &GroupQueryArgs {
        self
    }
}

impl GroupQueryArgs {
    pub fn allowed_states(self, allowed_states: Vec<GroupMembershipState>) -> Self {
        self.maybe_allowed_states(Some(allowed_states))
    }

    pub fn maybe_allowed_states(
        mut self,
        allowed_states: Option<Vec<GroupMembershipState>>,
    ) -> Self {
        self.allowed_states = allowed_states;
        self
    }

    pub fn created_after_ns(self, created_after_ns: i64) -> Self {
        self.maybe_created_after_ns(Some(created_after_ns))
    }

    pub fn maybe_created_after_ns(mut self, created_after_ns: Option<i64>) -> Self {
        self.created_after_ns = created_after_ns;
        self
    }

    pub fn created_before_ns(self, created_before_ns: i64) -> Self {
        self.maybe_created_before_ns(Some(created_before_ns))
    }

    pub fn maybe_created_before_ns(mut self, created_before_ns: Option<i64>) -> Self {
        self.created_before_ns = created_before_ns;
        self
    }

    pub fn limit(self, limit: i64) -> Self {
        self.maybe_limit(Some(limit))
    }

    pub fn maybe_limit(mut self, limit: Option<i64>) -> Self {
        self.limit = limit;
        self
    }

    pub fn conversation_type(self, conversation_type: ConversationType) -> Self {
        self.maybe_conversation_type(Some(conversation_type))
    }

    pub fn maybe_conversation_type(mut self, conversation_type: Option<ConversationType>) -> Self {
        self.conversation_type = conversation_type;
        self
    }

    pub fn consent_states(self, consent_states: Vec<ConsentState>) -> Self {
        self.maybe_consent_states(Some(consent_states))
    }
    pub fn maybe_consent_states(mut self, consent_states: Option<Vec<ConsentState>>) -> Self {
        self.consent_states = consent_states;
        self
    }

    pub fn include_sync_groups(mut self) -> Self {
        self.include_sync_groups = true;
        self
    }
}

impl DbConnection {
    /// Same behavior as fetched, but will stitch DM groups
    pub fn fetch_stitched(&self, key: &[u8]) -> Result<Option<StoredGroup>, StorageError> {
        let group = self.raw_query_read(|conn| {
            Ok::<_, StorageError>(
                groups::table
                    .filter(groups::id.eq(key))
                    .first::<StoredGroup>(conn)
                    .optional()?,
            )
        })?;

        // Is this group a DM?
        let Some(StoredGroup {
            dm_id: Some(dm_id), ..
        }) = group
        else {
            // If not, return the group
            return Ok(group);
        };

        // Otherwise, return the stitched DM
        self.raw_query_read(|conn| {
            Ok(groups::table
                .filter(groups::dm_id.eq(dm_id))
                .order_by(groups::last_message_ns.desc())
                .first::<StoredGroup>(conn)
                .optional()?)
        })
    }

    /// Return regular [`Purpose::Conversation`] groups with additional optional filters
    pub fn find_groups<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<StoredGroup>, StorageError> {
        use crate::storage::schema::consent_records::dsl as consent_dsl;
        use crate::storage::schema::groups::dsl as groups_dsl;
        let GroupQueryArgs {
            allowed_states,
            created_after_ns,
            created_before_ns,
            limit,
            conversation_type,
            consent_states,
            include_sync_groups,
            include_duplicate_dms,
        } = args.as_ref();

        let mut query = groups_dsl::groups
            .filter(groups_dsl::conversation_type.ne(ConversationType::Sync))
            .order(groups_dsl::created_at_ns.asc())
            .into_boxed();

        if !include_duplicate_dms {
            // Group by dm_id and grab the latest group (conversation stitching)
            query = query.filter(sql::<diesel::sql_types::Bool>(
                "id IN (
                    SELECT id FROM (
                        SELECT id,
                            ROW_NUMBER() OVER (PARTITION BY COALESCE(dm_id, id) ORDER BY last_message_ns DESC) AS row_num
                        FROM groups
                    ) AS ranked_groups
                    WHERE row_num = 1
                )",
            ));
        }

        if let Some(limit) = limit {
            query = query.limit(*limit);
        }

        if let Some(allowed_states) = allowed_states {
            query = query.filter(groups_dsl::membership_state.eq_any(allowed_states));
        }

        if let Some(created_after_ns) = created_after_ns {
            query = query.filter(groups_dsl::created_at_ns.gt(created_after_ns));
        }

        if let Some(created_before_ns) = created_before_ns {
            query = query.filter(groups_dsl::created_at_ns.lt(created_before_ns));
        }

        if let Some(conversation_type) = conversation_type {
            query = query.filter(groups_dsl::conversation_type.eq(conversation_type));
        }

        let mut groups = if let Some(consent_states) = consent_states {
            if consent_states
                .iter()
                .any(|state| *state == ConsentState::Unknown)
            {
                // Include both `Unknown`, `null`, and other specified states
                let query = query
                    .left_join(
                        consent_dsl::consent_records
                            .on(sql::<diesel::sql_types::Text>("lower(hex(groups.id))")
                                .eq(consent_dsl::entity)),
                    )
                    .filter(
                        consent_dsl::state
                            .is_null()
                            .or(consent_dsl::state.eq(ConsentState::Unknown))
                            .or(consent_dsl::state.eq_any(
                                consent_states
                                    .iter()
                                    .filter(|state| **state != ConsentState::Unknown)
                                    .cloned()
                                    .collect::<Vec<_>>(),
                            )),
                    )
                    .select(groups_dsl::groups::all_columns())
                    .order(groups_dsl::created_at_ns.asc());

                self.raw_query_read(|conn| query.load::<StoredGroup>(conn))?
            } else {
                // Only include the specified states
                let query = query
                    .inner_join(
                        consent_dsl::consent_records
                            .on(sql::<diesel::sql_types::Text>("lower(hex(groups.id))")
                                .eq(consent_dsl::entity)),
                    )
                    .filter(consent_dsl::state.eq_any(consent_states.clone()))
                    .select(groups_dsl::groups::all_columns())
                    .order(groups_dsl::created_at_ns.asc());

                self.raw_query_read(|conn| query.load::<StoredGroup>(conn))?
            }
        } else {
            // Handle the case where `consent_states` is `None`
            self.raw_query_read(|conn| query.load::<StoredGroup>(conn))?
        };

        // Were sync groups explicitly asked for? Was the include_sync_groups flag set to true?
        // Then query for those separately
        if matches!(conversation_type, Some(ConversationType::Sync)) || *include_sync_groups {
            let query =
                groups_dsl::groups.filter(groups_dsl::conversation_type.eq(ConversationType::Sync));
            let mut sync_groups = self.raw_query_read(|conn| query.load(conn))?;
            groups.append(&mut sync_groups);
        }

        Ok(groups)
    }

    pub fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, StorageError> {
        Ok(self.raw_query_read(|conn| super::schema::consent_records::table.load(conn))?)
    }

    pub fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync));

        Ok(self.raw_query_read(|conn| query.load(conn))?)
    }

    pub fn latest_sync_group(&self) -> Result<Option<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .limit(1);

        Ok(self.raw_query_read(|conn| query.load(conn))?.pop())
    }

    /// Return a single group that matches the given ID
    pub fn find_group(&self, id: &[u8]) -> Result<Option<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .limit(1)
            .filter(dsl::id.eq(id));
        let groups = self.raw_query_read(|conn| query.load(conn))?;

        Ok(groups.into_iter().next())
    }

    /// Return a single group that matches the given welcome ID
    pub fn find_group_by_welcome_id(
        &self,
        welcome_id: i64,
    ) -> Result<Option<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .filter(dsl::welcome_id.eq(welcome_id));

        let groups = self.raw_query_read(|conn| query.load(conn))?;

        if groups.len() > 1 {
            tracing::warn!(
                welcome_id,
                "More than one group found for welcome_id {welcome_id}"
            );
        }
        Ok(groups.into_iter().next())
    }

    pub fn find_dm_group(
        &self,
        members: &DmMembers<&str>,
    ) -> Result<Option<StoredGroup>, StorageError> {
        let query = dsl::groups
            .filter(dsl::dm_id.eq(Some(format!("{members}"))))
            .order(dsl::last_message_ns.desc());

        self.raw_query_read(|conn| Ok(query.first(conn).optional()?))
    }

    /// Load the other DMs that are stitched into this group
    pub fn other_dms(&self, group_id: &[u8]) -> Result<Vec<StoredGroup>, StorageError> {
        let query = dsl::groups.filter(dsl::id.eq(group_id));
        let groups: Vec<StoredGroup> = self.raw_query_read(|conn| query.load(conn))?;

        // Grab the dm_id of the group
        let Some(StoredGroup {
            id,
            dm_id: Some(dm_id),
            ..
        }) = groups.into_iter().next()
        else {
            return Ok(vec![]);
        };

        let query = dsl::groups
            .filter(dsl::dm_id.eq(dm_id))
            .filter(dsl::id.ne(id));

        let other_dms: Vec<StoredGroup> = self.raw_query_read(|conn| query.load(conn))?;
        Ok(other_dms)
    }

    /// Updates group membership state
    pub fn update_group_membership<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        state: GroupMembershipState,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.find(group_id.as_ref()))
                .set(dsl::membership_state.eq(state))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        let last_ts: Option<i64> = self.raw_query_read(|conn| {
            let ts = dsl::groups
                .find(&group_id)
                .select(dsl::rotated_at_ns)
                .first(conn)
                .optional()?;
            Ok::<Option<i64>, StorageError>(ts)
        })?;

        last_ts.ok_or(StorageError::NotFound(NotFound::InstallationTimeForGroup(
            group_id,
        )))
    }

    /// Updates the 'last time checked' we checked for new installations.
    pub fn update_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::rotated_at_ns.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_installations_time_checked(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        let last_ts = self.raw_query_read(|conn| {
            let ts = dsl::groups
                .find(&group_id)
                .select(dsl::installations_last_checked)
                .first(conn)
                .optional()?;
            Ok::<_, StorageError>(ts)
        })?;

        last_ts.ok_or(NotFound::InstallationTimeForGroup(group_id).into())
    }

    /// Updates the 'last time checked' we checked for new installations.
    pub fn update_installations_time_checked(&self, group_id: Vec<u8>) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::installations_last_checked.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn update_message_disappearing_from_ns(
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

    pub fn update_message_disappearing_in_ns(
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

    pub fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError> {
        tracing::info!("Trying to insert group");
        let stored_group = self.raw_query_write(|conn| {
            let maybe_inserted_group: Option<StoredGroup> = diesel::insert_into(dsl::groups)
                .values(&group)
                .on_conflict_do_nothing()
                .get_result(conn)
                .optional()?;

            if maybe_inserted_group.is_none() {
                let existing_group: StoredGroup = dsl::groups.find(group.id).first(conn)?;
                if existing_group.welcome_id == group.welcome_id {
                    tracing::info!("Group welcome id already exists");
                    // Error so OpenMLS db transaction are rolled back on duplicate welcomes
                    return Err(StorageError::Duplicate(DuplicateItem::WelcomeId(
                        existing_group.welcome_id,
                    )));
                } else {
                    tracing::info!("Group already exists");
                    return Ok(existing_group);
                }
            } else {
                tracing::info!("Group is inserted");
            }

            match maybe_inserted_group {
                Some(group) => Ok(group),
                None => Ok(dsl::groups.find(group.id).first(conn)?),
            }
        })?;

        Ok(stored_group)
    }

    /// Get all the welcome ids turned into groups
    pub(crate) fn group_welcome_ids(&self) -> Result<Vec<i64>, StorageError> {
        self.raw_query_read(|conn| {
            Ok::<_, StorageError>(
                dsl::groups
                    .filter(dsl::welcome_id.is_not_null())
                    .select(dsl::welcome_id)
                    .load::<Option<i64>>(conn)?
                    .into_iter()
                    .map(|id| id.expect("SQL explicity filters for none"))
                    .collect(),
            )
        })
    }

    pub fn set_group_paused(&self, group_id: &[u8], min_version: &str) -> Result<(), StorageError> {
        use crate::storage::schema::groups::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq(Some(min_version.to_string())))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn unpause_group(&self, group_id: &[u8]) -> Result<(), StorageError> {
        use crate::storage::schema::groups::dsl;

        self.raw_query_write(|conn| {
            diesel::update(dsl::groups.filter(dsl::id.eq(group_id)))
                .set(dsl::paused_for_version.eq::<Option<String>>(None))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_group_paused_version(
        &self,
        group_id: &[u8],
    ) -> Result<Option<String>, StorageError> {
        use crate::storage::schema::groups::dsl;

        let paused_version = self.raw_query_read(|conn| {
            dsl::groups
                .select(dsl::paused_for_version)
                .filter(dsl::id.eq(group_id))
                .first::<Option<String>>(conn)
        })?;

        Ok(paused_version)
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

    use std::sync::atomic::{AtomicU16, Ordering};

    use super::*;
    use crate::{
        storage::{
            consent_record::{ConsentType, StoredConsentRecord},
            encrypted_store::{schema::groups::dsl::groups, tests::with_connection},
        },
        Fetch, Store,
    };
    use xmtp_common::{assert_ok, rand_vec, time::now_ns};

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
        StoredGroup::new(
            id,
            created_at_ns,
            membership_state,
            "placeholder_address".to_string(),
            None,
            None,
            None,
        )
    }

    /// Generate a test group with welcome
    pub fn generate_group_with_welcome(
        state: Option<GroupMembershipState>,
        welcome_id: Option<i64>,
    ) -> StoredGroup {
        let id = rand_vec::<24>();
        let created_at_ns = now_ns();
        let membership_state = state.unwrap_or(GroupMembershipState::Allowed);
        StoredGroup::new_from_welcome(
            id,
            created_at_ns,
            membership_state,
            "placeholder_address".to_string(),
            welcome_id.unwrap_or(xmtp_common::rand_i64()),
            ConversationType::Group,
            None,
            None,
            None,
            None,
        )
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
        }
    }

    static TARGET_INBOX_ID: AtomicU16 = AtomicU16::new(2);

    /// Generate a test dm group
    pub fn generate_dm(state: Option<GroupMembershipState>) -> StoredGroup {
        let members = DmMembers {
            member_one_inbox_id: "placeholder_inbox_id_1".to_string(),
            member_two_inbox_id: format!(
                "placeholder_inbox_id_{}",
                TARGET_INBOX_ID.fetch_add(1, Ordering::SeqCst)
            ),
        };
        StoredGroup::new(
            rand_vec::<24>(),
            now_ns(),
            state.unwrap_or(GroupMembershipState::Allowed),
            "placeholder_address".to_string(),
            Some(members),
            None,
            None,
        )
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_it_stores_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            test_group.store(conn).unwrap();
            assert_eq!(
                conn.raw_query_read(|raw_conn| groups.first::<StoredGroup>(raw_conn))
                    .unwrap(),
                test_group
            );
        })
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_it_fetches_group() {
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
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_it_updates_group_membership_state() {
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
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_dm_stitching() {
        with_connection(|conn| {
            let dm1 = StoredGroup::new(
                rand_vec::<24>(),
                now_ns(),
                GroupMembershipState::Allowed,
                "placeholder_address".to_string(),
                Some(DmMembers {
                    member_one_inbox_id: "thats_me".to_string(),
                    member_two_inbox_id: "some_wise_guy".to_string(),
                }),
                None,
                None,
            );
            dm1.store(conn).unwrap();

            let dm2 = StoredGroup::new(
                rand_vec::<24>(),
                now_ns(),
                GroupMembershipState::Allowed,
                "placeholder_address".to_string(),
                Some(DmMembers {
                    member_one_inbox_id: "some_wise_guy".to_string(),
                    member_two_inbox_id: "thats_me".to_string(),
                }),
                None,
                None,
            );
            dm2.store(conn).unwrap();

            let all_groups = conn.find_groups(GroupQueryArgs::default()).unwrap();

            assert_eq!(all_groups.len(), 1);
        })
        .await;
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_find_groups() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Pending));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).unwrap();

            let other_inbox_id = test_group_3
                .dm_id
                .unwrap()
                .other_inbox_id("placeholder_inbox_id_1");

            let all_results = conn
                .find_groups(GroupQueryArgs::default().conversation_type(ConversationType::Group))
                .unwrap();
            assert_eq!(all_results.len(), 2);

            let pending_results = conn
                .find_groups(
                    GroupQueryArgs::default()
                        .allowed_states(vec![GroupMembershipState::Pending])
                        .conversation_type(ConversationType::Group),
                )
                .unwrap();
            assert_eq!(pending_results[0].id, test_group_1.id);
            assert_eq!(pending_results.len(), 1);

            // Offset and limit
            let results_with_limit = conn
                .find_groups(
                    GroupQueryArgs::default()
                        .limit(1)
                        .conversation_type(ConversationType::Group),
                )
                .unwrap();
            assert_eq!(results_with_limit.len(), 1);
            assert_eq!(results_with_limit[0].id, test_group_1.id);

            let results_with_created_at_ns_after = conn
                .find_groups(
                    GroupQueryArgs::default()
                        .created_after_ns(test_group_1.created_at_ns)
                        .conversation_type(ConversationType::Group)
                        .limit(1),
                )
                .unwrap();
            assert_eq!(results_with_created_at_ns_after.len(), 1);
            assert_eq!(results_with_created_at_ns_after[0].id, test_group_2.id);

            // Sync groups SHOULD NOT be returned
            let synced_groups = conn.latest_sync_group().unwrap();
            assert!(synced_groups.is_none());

            // test that dm groups are included
            let dm_results = conn.find_groups(GroupQueryArgs::default()).unwrap();
            assert_eq!(dm_results.len(), 3);
            assert_eq!(dm_results[2].id, test_group_3.id);

            // test find_dm_group
            let dm_result = conn
                .find_dm_group(&DmMembers {
                    member_one_inbox_id: "placeholder_inbox_id_1",
                    member_two_inbox_id: &other_inbox_id,
                })
                .unwrap();
            assert!(dm_result.is_some());

            // test only dms are returned
            let dm_results = conn
                .find_groups(GroupQueryArgs::default().conversation_type(ConversationType::Dm))
                .unwrap();
            assert_eq!(dm_results.len(), 1);
            assert_eq!(dm_results[0].id, test_group_3.id);
        })
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_installations_last_checked_is_updated() {
        with_connection(|conn| {
            let test_group = generate_group(None);
            test_group.store(conn).unwrap();

            // Check that the installations update has not been performed, yet
            assert_eq!(test_group.installations_last_checked, 0);

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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_new_group_has_correct_purpose() {
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
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_new_sync_group() {
        with_connection(|conn| {
            let id = rand_vec::<24>();
            let created_at_ns = now_ns();
            let membership_state = GroupMembershipState::Allowed;

            let sync_group = StoredGroup::new_sync_group(id, created_at_ns, membership_state);
            let conversation_type = sync_group.conversation_type;
            assert_eq!(conversation_type, ConversationType::Sync);

            sync_group.store(conn).unwrap();

            let found = conn.latest_sync_group().unwrap();
            assert!(found.is_some());
            assert_eq!(found.unwrap().conversation_type, ConversationType::Sync);

            // Load the sync group with a consent filter
            let allowed_groups = conn
                .find_groups(&GroupQueryArgs {
                    consent_states: Some([ConsentState::Allowed].to_vec()),
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();

            assert_eq!(allowed_groups.len(), 1);
            assert_eq!(allowed_groups[0].id, sync_group.id);
        })
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_find_groups_by_consent_state() {
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

            let all_results = conn.find_groups(GroupQueryArgs::default()).unwrap();
            assert_eq!(all_results.len(), 4);

            let allowed_results = conn
                .find_groups(
                    GroupQueryArgs::default().consent_states([ConsentState::Allowed].to_vec()),
                )
                .unwrap();
            assert_eq!(allowed_results.len(), 2);

            let allowed_unknown_results = conn
                .find_groups(
                    GroupQueryArgs::default()
                        .consent_states([ConsentState::Allowed, ConsentState::Unknown].to_vec()),
                )
                .unwrap();
            assert_eq!(allowed_unknown_results.len(), 3);

            let denied_results = conn
                .find_groups(
                    GroupQueryArgs::default().consent_states([ConsentState::Denied].to_vec()),
                )
                .unwrap();
            assert_eq!(denied_results.len(), 1);
            assert_eq!(denied_results[0].id, test_group_2.id);

            let unknown_results = conn
                .find_groups(
                    GroupQueryArgs::default().consent_states([ConsentState::Unknown].to_vec()),
                )
                .unwrap();
            assert_eq!(unknown_results.len(), 1);
            assert_eq!(unknown_results[0].id, test_group_4.id);
        })
        .await
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_get_group_welcome_ids() {
        with_connection(|conn| {
            let mls_groups = vec![
                generate_group_with_welcome(None, Some(30)),
                generate_group(None),
                generate_group(None),
                generate_group_with_welcome(None, Some(10)),
            ];
            for g in mls_groups.iter() {
                g.store(conn).unwrap();
            }
            assert_eq!(vec![30, 10], conn.group_welcome_ids().unwrap());
        })
        .await
    }
}
