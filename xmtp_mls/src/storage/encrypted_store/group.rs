use super::{schema::groups, DbConnection};
use crate::{Fetch, Store, storage::StorageError, impl_fetch_and_store};
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
pub struct StoredGroup {
    pub id: Vec<u8>,
    pub created_at_ns: i64,
    pub membership_state: i32,
}


impl_fetch_and_store!(StoredGroup, groups, Vec<u8>);

