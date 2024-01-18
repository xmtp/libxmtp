use diesel::prelude::*;

use super::{db_connection::DbConnection, schema::refresh_state};
use crate::{impl_fetch, impl_store, storage::StorageError, Fetch, Store};

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = refresh_state)]
#[diesel(primary_key(id))]
pub struct RefreshState {
    pub id: Vec<u8>,
    pub cursor: i64,
}

impl_fetch!(RefreshState, refresh_state, Vec<u8>);
impl_store!(RefreshState, refresh_state);

impl DbConnection<'_> {
    pub fn get_last_cursor_for_id<IdType: AsRef<Vec<u8>>>(
        &self,
        id: IdType,
    ) -> Result<i64, StorageError> {
        let state: Option<RefreshState> = self.fetch(id.as_ref())?;
        match state {
            Some(state) => Ok(state.cursor),
            None => {
                let new_state = RefreshState {
                    id: id.as_ref().clone(),
                    cursor: 0,
                };
                new_state.store(self)?;
                Ok(0)
            }
        }
    }

    pub fn update_cursor<IdType: AsRef<Vec<u8>>>(
        &self,
        id: IdType,
        cursor: i64,
    ) -> Result<bool, StorageError> {
        let state: Option<RefreshState> = self.fetch(id.as_ref())?;
        match state {
            Some(state) => {
                use super::schema::refresh_state::dsl;
                let num_updated = self.raw_query(|conn| {
                    diesel::update(&state)
                        .filter(dsl::cursor.lt(cursor))
                        .set(dsl::cursor.eq(cursor))
                        .execute(conn)
                })?;
                Ok(num_updated == 1)
            }
            None => Err(StorageError::NotFound),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{storage::encrypted_store::tests::with_connection, Fetch, Store};

    #[test]
    fn get_cursor_with_no_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entry: Option<RefreshState> = conn.fetch(&id).unwrap();
            assert!(entry.is_none());
            assert_eq!(conn.get_last_cursor_for_id(&id).unwrap(), 0);
            let entry: Option<RefreshState> = conn.fetch(&id).unwrap();
            assert!(entry.is_some());
        })
    }

    #[test]
    fn get_timestamp_with_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entry = RefreshState {
                id: id.clone(),
                cursor: 123,
            };
            entry.store(conn).unwrap();
            assert_eq!(conn.get_last_cursor_for_id(&id).unwrap(), 123);
        })
    }

    #[test]
    fn update_timestamp_when_bigger() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entry = RefreshState {
                id: id.clone(),
                cursor: 123,
            };
            entry.store(conn).unwrap();
            assert!(conn.update_cursor(&id, 124).unwrap());
            let entry: Option<RefreshState> = conn.fetch(&id).unwrap();
            assert_eq!(entry.unwrap().cursor, 124);
        })
    }

    #[test]
    fn dont_update_timestamp_when_smaller() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entry = RefreshState {
                id: id.clone(),
                cursor: 123,
            };
            entry.store(conn).unwrap();
            assert!(!conn.update_cursor(&id, 122).unwrap());
            let entry: Option<RefreshState> = conn.fetch(&id).unwrap();
            assert_eq!(entry.unwrap().cursor, 123);
        })
    }
}
