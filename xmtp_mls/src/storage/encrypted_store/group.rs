//! The Group database table. Stored information surrounding group membership and ID's.
use super::{
    consent_record::{ConsentState, StoredConsentRecord},
    db_connection::DbConnection,
    schema::groups::{self, dsl},
    Sqlite,
};
use crate::{impl_fetch, impl_store, storage::NotFound, DuplicateItem, StorageError};
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
    /// The inbox_id of the DM target
    pub dm_inbox_id: Option<String>,
    /// The last time the leaf node encryption key was rotated
    pub rotated_at_ns: i64,
    /// Enum, [`ConversationType`] signifies the group conversation type which extends to who can access it.
    pub conversation_type: ConversationType,
}

impl_fetch!(StoredGroup, groups, Vec<u8>);
impl_store!(StoredGroup, groups);

impl StoredGroup {
    /// Create a new group from a welcome message
    pub fn new_from_welcome(
        id: ID,
        created_at_ns: i64,
        membership_state: GroupMembershipState,
        added_by_inbox_id: String,
        welcome_id: i64,
        conversation_type: ConversationType,
        dm_inbox_id: Option<String>,
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
            dm_inbox_id,
        }
    }

    /// Create a new [`Purpose::Conversation`] group. This is the default type of group.
    pub fn new(
        id: ID,
        created_at_ns: i64,
        membership_state: GroupMembershipState,
        added_by_inbox_id: String,
        dm_inbox_id: Option<String>,
    ) -> Self {
        Self {
            id,
            created_at_ns,
            membership_state,
            installations_last_checked: 0,
            conversation_type: match dm_inbox_id {
                Some(_) => ConversationType::Dm,
                None => ConversationType::Group,
            },
            added_by_inbox_id,
            welcome_id: None,
            rotated_at_ns: 0,
            dm_inbox_id,
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
            dm_inbox_id: None,
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
    pub consent_state: Option<ConsentState>,
    pub include_sync_groups: bool,
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

    pub fn consent_state(self, consent_state: ConsentState) -> Self {
        self.maybe_consent_state(Some(consent_state))
    }
    pub fn maybe_consent_state(mut self, consent_state: Option<ConsentState>) -> Self {
        self.consent_state = consent_state;
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
        use crate::storage::schema::consent_records::dsl as consent_dsl;
        use crate::storage::schema::groups::dsl as groups_dsl;
        let GroupQueryArgs {
            allowed_states,
            created_after_ns,
            created_before_ns,
            limit,
            conversation_type,
            consent_state,
            include_sync_groups,
        } = args.as_ref();

        let mut query = groups_dsl::groups
            // Filter out sync groups from the main query
            .filter(groups_dsl::conversation_type.ne(ConversationType::Sync))
            .order(groups_dsl::created_at_ns.asc())
            .into_boxed();

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

        let mut groups = if let Some(consent_state) = consent_state {
            if *consent_state == ConsentState::Unknown {
                let query = query
                    .left_join(
                        consent_dsl::consent_records
                            .on(sql::<diesel::sql_types::Text>("lower(hex(groups.id))")
                                .eq(consent_dsl::entity)),
                    )
                    .filter(
                        consent_dsl::state
                            .is_null()
                            .or(consent_dsl::state.eq(ConsentState::Unknown)),
                    )
                    .select(groups_dsl::groups::all_columns())
                    .order(groups_dsl::created_at_ns.asc());

                self.raw_query(|conn| query.load::<StoredGroup>(conn))?
            } else {
                let query = query
                    .inner_join(
                        consent_dsl::consent_records
                            .on(sql::<diesel::sql_types::Text>("lower(hex(groups.id))")
                                .eq(consent_dsl::entity)),
                    )
                    .filter(consent_dsl::state.eq(*consent_state))
                    .select(groups_dsl::groups::all_columns())
                    .order(groups_dsl::created_at_ns.asc());

                self.raw_query(|conn| query.load::<StoredGroup>(conn))?
            }
        } else {
            self.raw_query(|conn| query.load::<StoredGroup>(conn))?
        };

        // Were sync groups explicitly asked for? Was the include_sync_groups flag set to true?
        // Then query for those separately
        if matches!(conversation_type, Some(ConversationType::Sync)) || *include_sync_groups {
            let query =
                groups_dsl::groups.filter(groups_dsl::conversation_type.eq(ConversationType::Sync));
            let mut sync_groups = self.raw_query(|conn| query.load(conn))?;
            groups.append(&mut sync_groups);
        }

        Ok(groups)
    }

    pub fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, StorageError> {
        Ok(self.raw_query(|conn| super::schema::consent_records::table.load(conn))?)
    }

    pub fn all_sync_groups(&self) -> Result<Vec<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync));

        Ok(self.raw_query(|conn| query.load(conn))?)
    }

    pub fn latest_sync_group(&self) -> Result<Option<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.desc())
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .limit(1);

        Ok(self.raw_query(|conn| query.load(conn))?.pop())
    }

    /// Return a single group that matches the given ID
    pub fn find_group(&self, id: Vec<u8>) -> Result<Option<StoredGroup>, StorageError> {
        let mut query = dsl::groups.order(dsl::created_at_ns.asc()).into_boxed();

        query = query.limit(1).filter(dsl::id.eq(id));
        let groups: Vec<StoredGroup> = self.raw_query(|conn| query.load(conn))?;

        // Manually extract the first element
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

        let groups: Vec<StoredGroup> = self.raw_query(|conn| query.load(conn))?;
        if groups.len() > 1 {
            tracing::error!("More than one group found for welcome_id {}", welcome_id);
        }

        Ok(groups.into_iter().next())
    }

    pub fn find_dm_group(
        &self,
        target_inbox_id: &str,
    ) -> Result<Option<StoredGroup>, StorageError> {
        let query = dsl::groups
            .order(dsl::created_at_ns.asc())
            .filter(dsl::dm_inbox_id.eq(Some(target_inbox_id)));

        let groups: Vec<StoredGroup> = self.raw_query(|conn| query.load(conn))?;
        if groups.len() > 1 {
            tracing::info!(
                "More than one group found for dm_inbox_id {}",
                target_inbox_id
            );
        }

        Ok(groups.into_iter().next())
    }

    /// Updates group membership state
    pub fn update_group_membership<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        state: GroupMembershipState,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::groups.find(group_id.as_ref()))
                .set(dsl::membership_state.eq(state))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_rotated_at_ns(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        let last_ts: Option<i64> = self.raw_query(|conn| {
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
        self.raw_query(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::rotated_at_ns.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_installations_time_checked(&self, group_id: Vec<u8>) -> Result<i64, StorageError> {
        let last_ts = self.raw_query(|conn| {
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
        self.raw_query(|conn| {
            let now = xmtp_common::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::installations_last_checked.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn insert_or_replace_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError> {
        tracing::info!("Trying to insert group");
        let stored_group = self.raw_query(|conn| {
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
        self.raw_query(|conn| {
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
        .map_err(Into::into)
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

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
        let id = rand_vec::<24>();
        let created_at_ns = now_ns();
        let membership_state = state.unwrap_or(GroupMembershipState::Allowed);
        StoredGroup::new(
            id,
            created_at_ns,
            membership_state,
            "placeholder_address".to_string(),
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
        )
    }

    /// Generate a test consent
    fn generate_consent_record(
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

    /// Generate a test dm group
    pub fn generate_dm(state: Option<GroupMembershipState>) -> StoredGroup {
        let id = rand_vec::<24>();
        let created_at_ns = now_ns();
        let membership_state = state.unwrap_or(GroupMembershipState::Allowed);
        let dm_inbox_id = Some("placeholder_inbox_id".to_string());
        StoredGroup::new(
            id,
            created_at_ns,
            membership_state,
            "placeholder_address".to_string(),
            dm_inbox_id,
        )
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_it_stores_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            test_group.store(conn).unwrap();
            assert_eq!(
                conn.raw_query(|raw_conn| groups.first::<StoredGroup>(raw_conn))
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

            conn.raw_query(|raw_conn| {
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
    async fn test_find_groups() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Pending));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).unwrap();

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
            let dm_result = conn.find_dm_group("placeholder_inbox_id").unwrap();
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
                    consent_state: Some(ConsentState::Allowed),
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
                .find_groups(GroupQueryArgs::default().consent_state(ConsentState::Allowed))
                .unwrap();
            assert_eq!(allowed_results.len(), 2);

            let denied_results = conn
                .find_groups(GroupQueryArgs::default().consent_state(ConsentState::Denied))
                .unwrap();
            assert_eq!(denied_results.len(), 1);
            assert_eq!(denied_results[0].id, test_group_2.id);

            let unknown_results = conn
                .find_groups(GroupQueryArgs::default().consent_state(ConsentState::Unknown))
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
