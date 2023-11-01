use super::schema::groups;
use crate::impl_fetch_and_store;
use diesel::prelude::*;
use diesel::{backend::Backend, sqlite::Sqlite, serialize::{self, Output, ToSql, IsNull}, deserialize::{self, FromSql}, sql_types::Integer, expression::AsExpression};

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
    pub membership_state: i32,
}

impl_fetch_and_store!(StoredGroup, groups, Vec<u8>);

impl StoredGroup {
    pub fn new(id: ID, created_at_ns: i64, membership_state: GroupMembershipState) -> Self {
        Self {
            id, created_at_ns, membership_state: membership_state as i32
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, AsExpression)]
#[diesel(sql_type = Integer)]
/// Status of membership in a group, once a user sends a request to join
pub enum GroupMembershipState {
    Allowed = 1,
    Rejected = 2,
    Pending = 3,
}


impl ToSql<Integer, Sqlite> for GroupMembershipState 
where
    i32: ToSql<Integer, Sqlite> 
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
    use crate::storage::encrypted_store::tests::with_store;    
    use crate::storage::encrypted_store::schema::groups::dsl::groups;
    use crate::{Fetch, Store};

    #[test] 
    fn it_stores_group() {
        with_store(|store| {
            let mut conn = store.conn().unwrap();
            StoredGroup::new(vec![0x0], 100, GroupMembershipState::Allowed).store(&mut conn).unwrap();
            assert_eq!(
                groups.first::<StoredGroup>(&mut conn).unwrap(), 
                StoredGroup { id: vec![0x0], created_at_ns: 100, membership_state: GroupMembershipState::Allowed as i32 }
            );
        })
    }

    #[test]
    fn it_fetches_group() {
        with_store(|store| {
            let mut conn = store.conn().unwrap();
            let test_group = StoredGroup::new(vec![0x0], 100, GroupMembershipState::Allowed);
            diesel::insert_into(groups)
                .values(test_group.clone())
                .execute(&mut conn)
                .unwrap();
            let fetched_group = conn.fetch(vec![0x0]).ok().flatten().unwrap();
            assert_eq!(test_group, fetched_group);
        })
    }
    
    #[test]
    fn it_updates_group_membership_state() {
        with_store(|store| {
            let id = vec![0x0];
            let mut conn = store.conn().unwrap();
            StoredGroup::new(id.clone(), 100, GroupMembershipState::Pending).store(&mut conn).unwrap();
            let updated_group = store.update_group_membership(&mut conn, id, GroupMembershipState::Rejected).unwrap();
            assert_eq!(updated_group, StoredGroup { id: vec![0x0], created_at_ns: 100, membership_state: GroupMembershipState::Rejected as i32});
        })
    }

    #[test]
    fn it_should_
}
