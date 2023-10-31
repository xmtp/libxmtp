use super::{schema::groups, DbConnection};
use crate::{Fetch, Store, storage::StorageError};
use diesel::prelude::*;

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = groups)]
#[diesel(primary_key(id))]
pub struct StoredGroup {
    pub id: Vec<u8>,
    pub created_at_ns: i64,
    pub membership_state: i32,
}


impl Store<DbConnection> for StoredGroup {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(groups::table)
            .values(self)
            .execute(into)?;
        Ok(())
    }
}

impl Fetch<StoredGroup> for DbConnection {
    type Key = Vec<u8>;
    fn fetch(&mut self, key: Self::Key) -> Result<Option<StoredGroup>, StorageError> {
        use super::schema::groups::dsl::*;
        Ok(groups.find(key).first(self).optional()?)
    }
}

