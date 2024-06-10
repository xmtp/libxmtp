use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

use super::{db_connection::DbConnection, schema::refresh_state};
use crate::{impl_store, storage::StorageError, Store};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, Hash, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum EntityKind {
    Welcome = 1,
    Group = 2,
}

impl ToSql<Integer, Sqlite> for EntityKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for EntityKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(EntityKind::Welcome),
            2 => Ok(EntityKind::Group),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = refresh_state)]
#[diesel(primary_key(entity_id, entity_kind))]
pub struct RefreshState {
    pub entity_id: Vec<u8>,
    pub entity_kind: EntityKind,
    pub cursor: i64,
}

impl_store!(RefreshState, refresh_state);

impl DbConnection {
    pub fn get_refresh_state<EntityId: AsRef<Vec<u8>>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError> {
        use super::schema::refresh_state::dsl;
        let res = self.raw_query(|conn| {
            dsl::refresh_state
                .find((entity_id.as_ref(), entity_kind))
                .first(conn)
                .optional()
        })?;

        Ok(res)
    }
    pub fn get_last_cursor_for_id<IdType: AsRef<Vec<u8>>>(
        &self,
        id: IdType,
        entity_kind: EntityKind,
    ) -> Result<i64, StorageError> {
        let state: Option<RefreshState> = self.get_refresh_state(&id, entity_kind)?;
        match state {
            Some(state) => Ok(state.cursor),
            None => {
                let new_state = RefreshState {
                    entity_id: id.as_ref().clone(),
                    entity_kind,
                    cursor: 0,
                };
                new_state.store(self)?;
                Ok(0)
            }
        }
    }

    pub fn update_cursor(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: i64,
    ) -> Result<bool, StorageError> {
        let state: Option<RefreshState> = self.get_refresh_state(entity_id, entity_kind)?;
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
            None => Err(StorageError::NotFound(format!(
                "state for entity ID {} with kind {:?}",
                hex::encode(entity_id),
                entity_kind
            ))),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::encrypted_store::tests::with_connection;

    #[test]
    fn get_cursor_with_no_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let kind = EntityKind::Group;
            let entry: Option<RefreshState> = conn.get_refresh_state(&id, kind).unwrap();
            assert!(entry.is_none());
            assert_eq!(conn.get_last_cursor_for_id(&id, kind).unwrap(), 0);
            let entry: Option<RefreshState> = conn.get_refresh_state(&id, kind).unwrap();
            assert!(entry.is_some());
        })
    }

    #[test]
    fn get_timestamp_with_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::Welcome;
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                cursor: 123,
            };
            entry.store(conn).unwrap();
            assert_eq!(conn.get_last_cursor_for_id(&id, entity_kind).unwrap(), 123);
        })
    }

    #[test]
    fn update_timestamp_when_bigger() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::Group;
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                cursor: 123,
            };
            entry.store(conn).unwrap();
            assert!(conn.update_cursor(&id, entity_kind, 124).unwrap());
            let entry: Option<RefreshState> = conn.get_refresh_state(&id, entity_kind).unwrap();
            assert_eq!(entry.unwrap().cursor, 124);
        })
    }

    #[test]
    fn dont_update_timestamp_when_smaller() {
        with_connection(|conn| {
            let entity_id = vec![1, 2, 3];
            let entity_kind = EntityKind::Welcome;

            let entry = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind,
                cursor: 123,
            };
            entry.store(conn).unwrap();
            assert!(!conn.update_cursor(&entity_id, entity_kind, 122).unwrap());
            let entry: Option<RefreshState> =
                conn.get_refresh_state(&entity_id, entity_kind).unwrap();
            assert_eq!(entry.unwrap().cursor, 123);
        })
    }

    #[test]
    fn allow_installation_and_welcome_same_id() {
        with_connection(|conn| {
            let entity_id = vec![1, 2, 3];
            let welcome_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::Welcome,
                cursor: 123,
            };
            welcome_state.store(conn).unwrap();

            let group_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::Group,
                cursor: 456,
            };
            group_state.store(conn).unwrap();

            let welcome_state_retrieved = conn
                .get_refresh_state(&entity_id, EntityKind::Welcome)
                .unwrap()
                .unwrap();
            assert_eq!(welcome_state_retrieved.cursor, 123);

            let group_state_retrieved = conn
                .get_refresh_state(&entity_id, EntityKind::Group)
                .unwrap()
                .unwrap();
            assert_eq!(group_state_retrieved.cursor, 456);
        })
    }
}
