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

use super::{schema::groups, DbConnection, EncryptedMessageStore};
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
    /// based on timestamp of this welcome message
    pub created_at_ns: i64,
    /// enum, [`GroupMembershipState`] representing access to the group.
    pub membership_state: GroupMembershipState,
}

impl_fetch!(StoredGroup, groups, Vec<u8>);
impl_store!(StoredGroup, groups);

impl StoredGroup {
    pub fn new(id: ID, created_at_ns: i64, membership_state: GroupMembershipState) -> Self {
        Self {
            id,
            created_at_ns,
            membership_state,
        }
    }
}

impl EncryptedMessageStore {
    /// Updates group membership state
    pub fn update_group_membership<GroupId: AsRef<ID>>(
        &self,
        conn: &mut DbConnection,
        id: GroupId,
        state: GroupMembershipState,
    ) -> Result<StoredGroup, StorageError> {
        use super::schema::groups::dsl;

        diesel::update(dsl::groups.find(id.as_ref()))
            .set(dsl::membership_state.eq(state))
            .get_result::<StoredGroup>(conn)
            .map_err(Into::into)
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
        storage::encrypted_store::{
            schema::groups::dsl::groups,
            tests::{rand_time, rand_vec, with_store},
        },
        Fetch, Store,
    };

    /// Generate a test group
    pub fn generate_group(state: Option<GroupMembershipState>) -> StoredGroup {
        StoredGroup {
            id: rand_vec(),
            created_at_ns: rand_time(),
            membership_state: state.unwrap_or(GroupMembershipState::Allowed),
        }
    }

    #[test]
    fn it_stores_group() {
        with_store(|_, mut conn| {
            let test_group = generate_group(None);

            test_group.store(&mut conn).unwrap();
            assert_eq!(groups.first::<StoredGroup>(&mut conn).unwrap(), test_group);
        })
    }

    #[test]
    fn it_fetches_group() {
        with_store(|_, mut conn| {
            let test_group = generate_group(None);

            diesel::insert_into(groups)
                .values(test_group.clone())
                .execute(&mut conn)
                .unwrap();

            let fetched_group = Fetch::<StoredGroup>::fetch(&mut conn, &test_group.id);
            assert_ok!(fetched_group, Some(test_group));
        })
    }

    #[test]
    fn it_updates_group_membership_state() {
        with_store(|store, mut conn| {
            let test_group = generate_group(Some(GroupMembershipState::Pending));

            test_group.store(&mut conn).unwrap();
            let updated_group = store
                .update_group_membership(&mut conn, &test_group.id, GroupMembershipState::Rejected)
                .unwrap();
            assert_eq!(
                updated_group,
                StoredGroup {
                    membership_state: GroupMembershipState::Rejected,
                    ..test_group
                }
            );
        })
    }
}
