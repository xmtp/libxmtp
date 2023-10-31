use super::schema::groups;
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


impl<StorageConnection> Store<StorageConnection> for StoredGroup {
    fn store(&self, _into: &mut StorageConnection) -> Result<(), StorageError> {
        todo!();
    }
}

impl<Model> Fetch<Model> for StoredGroup {
    type Key = ();
    fn fetch(&mut self, _key: Self::Key) -> Result<Option<Model>, StorageError> {
        todo!();
    }
}

