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

use super::schema::groups;
use crate::{impl_fetch, impl_store};

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
mod tests {
    use super::*;
    use crate::{
        storage::encrypted_store::{schema::groups::dsl::groups, tests::with_store},
        Fetch, Store,
    };

    #[test]
    fn it_stores_group() {
        with_store(|_, conn| {
            let test_group = StoredGroup::new(vec![0x0], 100, GroupMembershipState::Allowed);

            test_group.store(conn).unwrap();
            assert_eq!(groups.first::<StoredGroup>(conn).unwrap(), test_group);
        })
    }

    #[test]
    fn it_fetches_group() {
        with_store(|_, conn| {
            let test_group = StoredGroup::new(vec![0x0], 100, GroupMembershipState::Allowed);
            diesel::insert_into(groups)
                .values(test_group.clone())
                .execute(conn)
                .unwrap();
            let fetched_group = conn.fetch(vec![0x0]).ok().flatten().unwrap();
            assert_eq!(test_group, fetched_group);
        })
    }

    #[test]
    fn it_updates_group_membership_state() {
        with_store(|store, conn| {
            let id = vec![0x0];
            let test_group = StoredGroup::new(id.clone(), 100, GroupMembershipState::Pending);

            test_group.store(conn).unwrap();
            let updated_group = store
                .update_group_membership(conn, id, GroupMembershipState::Rejected)
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
