//! The Group database table. Stored information surrounding group membership and ID's.
use super::{
    Sqlite,
    consent_record::ConsentState,
    db_connection::DbConnection,
    schema::groups::{self, dsl},
};
use crate::NotFound;
use crate::{DuplicateItem, StorageError, impl_fetch, impl_store};

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

pub type ID = Vec<u8>;

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Insertable, Identifiable, Queryable, Builder,
)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
#[builder(setter(into), build_fn(error = "StorageError"))]
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
    pub welcome_id: Option<i64>,
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
}

// TODO: Create two more structs that delegate to StoredGroup
impl_fetch!(StoredGroup, groups, Vec<u8>);
impl_store!(StoredGroup, groups);

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

    pub fn new_sync_group(
        id: ID,
        created_at_ns: i64,
        membership_state: GroupMembershipState,
    ) -> Self {
        StoredGroup::builder()
            .id(id)
            .conversation_type(ConversationType::Sync)
            .created_at_ns(created_at_ns)
            .membership_state(membership_state)
            .added_by_inbox_id("")
            .build()
            .expect("No fields should be uninitialized")
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
    /// Return regular [`Purpose::Conversation`] groups with additional optional filters
    pub fn find_groups<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<StoredGroup>, StorageError> {
        use crate::schema::consent_records::dsl as consent_dsl;
        use crate::schema::groups::dsl as groups_dsl;
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

    pub fn find_groups_by_id_paged<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
        offset: i64,
    ) -> Result<Vec<StoredGroup>, StorageError> {
        let GroupQueryArgs {
            created_after_ns,
            created_before_ns,
            limit,
            ..
        } = args.as_ref();

        let mut query = groups::table
            .filter(groups::conversation_type.ne(ConversationType::Sync))
            .order_by(groups::id)
            .into_boxed();

        if let Some(start_ns) = created_after_ns {
            query = query.filter(groups::created_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = created_before_ns {
            query = query.filter(groups::created_at_ns.le(end_ns));
        }

        query = query.limit(limit.unwrap_or(100)).offset(offset);

        Ok(self.raw_query_read(|conn| query.load::<StoredGroup>(conn))?)
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
    pub fn group_welcome_ids(&self) -> Result<Vec<i64>, StorageError> {
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

    pub use super::dms::tests::*;
    use super::*;

    use crate::{
        Fetch, Store,
        consent_record::{ConsentType, StoredConsentRecord},
        schema::groups::dsl::groups,
        test_utils::{with_connection, with_connection_async},
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
            .welcome_id(welcome_id.unwrap_or(xmtp_common::rand_i64()))
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
        }
    }

    #[xmtp_common::test]
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
    }

    #[xmtp_common::test]
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
    }

    #[xmtp_common::test]
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
                .find_dm_group(format!("dm:placeholder_inbox_id_1:{}", &other_inbox_id))
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
    }

    #[xmtp_common::test]
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
    }

    #[xmtp_common::test]
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
    }

    #[xmtp_common::test]
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
    }

    #[xmtp_common::test]
    fn new_sync_group_does_not_panic() {
        let _ = StoredGroup::new_sync_group(
            xmtp_common::rand_vec::<24>(),
            100,
            GroupMembershipState::Allowed,
        );
    }
}
