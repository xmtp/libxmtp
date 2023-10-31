use super::{schema::groups, DbConnection};
use crate::{Fetch, Store, storage::StorageError, impl_fetch_and_store};
use diesel::prelude::*;
use diesel::{sqlite::Sqlite, serialize::{Output, ToSql, IsNull}, sql_types::Integer, expression::AsExpression};

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
pub struct StoredGroup {
    pub id: Vec<u8>,
    pub created_at_ns: i64,
    pub membership_state: i32,
}

impl_fetch_and_store!(StoredGroup, groups, Vec<u8>);

#[repr(i32)]
#[derive(Debug, Clone, Copy, AsExpression)]
#[diesel(sql_type = Integer)]
pub enum GroupMembershipState {
    Allowed = 1,
    Rejected = 2,
    Pending = 3,
}


impl ToSql<Integer, Sqlite> for GroupMembershipState 
where
    i32: ToSql<Integer, Sqlite> 
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> diesel::serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}
