//! The Group database table. Stored information surrounding group membership and ID's.

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

use super::{
    db_connection::DbConnection,
    schema::{groups, groups::dsl},
};
use crate::{impl_fetch, impl_store, StorageError};

/// The Group ID type.
pub type ID = Vec<u8>;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone, PartialEq)]
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
    /// Track when the latest, most recent installation list was checked
    pub installation_list_last_checked: i64,
}

impl_fetch!(StoredGroup, groups, Vec<u8>);
impl_store!(StoredGroup, groups);

impl StoredGroup {
    pub fn new(id: ID, created_at_ns: i64, membership_state: GroupMembershipState) -> Self {
        Self {
            id,
            created_at_ns,
            membership_state,
            installation_list_last_checked: 0,
        }
    }
}

impl DbConnection<'_> {
    pub fn find_groups(
        &self,
        allowed_states: Option<Vec<GroupMembershipState>>,
        created_after_ns: Option<i64>,
        created_before_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredGroup>, StorageError> {
        let mut query = dsl::groups.order(dsl::created_at_ns.asc()).into_boxed();

        if let Some(allowed_states) = allowed_states {
            query = query.filter(dsl::membership_state.eq_any(allowed_states));
        }

        if let Some(created_after_ns) = created_after_ns {
            query = query.filter(dsl::created_at_ns.gt(created_after_ns));
        }

        if let Some(created_before_ns) = created_before_ns {
            query = query.filter(dsl::created_at_ns.lt(created_before_ns));
        }

        query = query.filter(dsl::installation_list_last_checked.gt(-1));

        if let Some(limit) = limit {
            query = query.limit(limit);
        }

        Ok(self.raw_query(|conn| query.load(conn))?)
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

    pub fn get_installation_list_time_checked(
        &self,
        group_id: Vec<u8>,
    ) -> Result<i64, StorageError> {
        let last_ts = self.raw_query(|conn| {
            let last_ts = dsl::groups
                .find(&group_id)
                .select(dsl::installation_list_last_checked)
                .first(conn).optional()?;
            match last_ts {
                Some(ts) => Ok(ts),
                None => Ok(0)
            }

        })?;
        
        Ok(last_ts)
    }
    
    /// Updates the 'last time checked' for installation lists.
    pub fn update_installation_list_time_checked(
        &self,
        group_id: Vec<u8>,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            let now = crate::utils::time::now_ns();
            diesel::update(dsl::groups.find(&group_id))
                .set(dsl::installation_list_last_checked.eq(now))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn insert_or_ignore_group(&self, group: StoredGroup) -> Result<StoredGroup, StorageError> {
        let stored_group = self.raw_query(|conn| {
            let maybe_inserted_group: Option<StoredGroup> = diesel::insert_into(dsl::groups)
                .values(&group)
                .on_conflict_do_nothing()
                .get_result(conn)
                .optional()?;

            match maybe_inserted_group {
                Some(group) => Ok(group),
                None => dsl::groups.find(group.id).first(conn),
            }
        })?;

        Ok(stored_group)
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
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

#[cfg(test)]
pub(crate) mod tests {

    use super::*;
    use crate::{
        assert_ok,
        storage::encrypted_store::{schema::groups::dsl::groups, tests::with_connection},
        utils::{test::rand_vec, time::now_ns},
        Fetch, Store,
    };

    /// Generate a test group
    pub fn generate_group(state: Option<GroupMembershipState>) -> StoredGroup {
        StoredGroup {
            id: rand_vec(),
            created_at_ns: now_ns(),
            membership_state: state.unwrap_or(GroupMembershipState::Allowed),
            installation_list_last_checked: 0,
        }
    }

    #[test]
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

    #[test]
    fn test_it_fetches_group() {
        with_connection(|conn| {
            let test_group = generate_group(None);

            conn.raw_query(|raw_conn| {
                diesel::insert_into(groups)
                    .values(test_group.clone())
                    .execute(raw_conn)
            })
            .unwrap();

            let fetched_group: Result<Option<StoredGroup>, StorageError> =
                conn.fetch(&test_group.id);
            assert_ok!(fetched_group, Some(test_group));
        })
    }

    #[test]
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

    #[test]
    fn test_find_groups() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Pending));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();

            let all_results = conn.find_groups(None, None, None, None).unwrap();
            assert_eq!(all_results.len(), 2);

            let pending_results = conn
                .find_groups(Some(vec![GroupMembershipState::Pending]), None, None, None)
                .unwrap();
            assert_eq!(pending_results[0].id, test_group_1.id);
            assert_eq!(pending_results.len(), 1);

            // Offset and limit
            let results_with_limit = conn.find_groups(None, None, None, Some(1)).unwrap();
            assert_eq!(results_with_limit.len(), 1);
            assert_eq!(results_with_limit[0].id, test_group_1.id);

            let results_with_created_at_ns_after = conn
                .find_groups(None, Some(test_group_1.created_at_ns), None, Some(1))
                .unwrap();
            assert_eq!(results_with_created_at_ns_after.len(), 1);
            assert_eq!(results_with_created_at_ns_after[0].id, test_group_2.id);
        })
    }

    #[test]
    fn test_installation_list_last_checked_is_updated() {
        with_connection(|conn| {
            let test_group = generate_group(None);
            test_group.store(conn).unwrap();

            // Check that the installation list update has not been performed, yet
            assert_eq!(test_group.installation_list_last_checked, 0);

            // Check that some event occurred which triggers an installation list update.
            // Here we invoke that event directly
            let result = conn
                .update_installation_list_time_checked(test_group.id.clone());
            assert_ok!(result);

            // Check that the latest installation list timestamp has been updated
            let fetched_group: StoredGroup = conn.fetch(&test_group.id).ok().flatten().unwrap();
            assert_ne!(fetched_group.installation_list_last_checked, 0);
            assert!(
                fetched_group.created_at_ns < fetched_group.installation_list_last_checked
            );
        })
    }
}
