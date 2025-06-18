use super::{ConnectionExt, Sqlite, db_connection::DbConnection, schema::refresh_state};
use crate::{StorageError, StoreOrIgnore, impl_store_or_ignore};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, Hash, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum EntityKind {
    Welcome = 1,
    Group = 2,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EntityKind::*;
        match self {
            Welcome => write!(f, "welcome"),
            Group => write!(f, "group"),
        }
    }
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

impl_store_or_ignore!(RefreshState, refresh_state);

impl<C: ConnectionExt> DbConnection<C> {
    pub fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError> {
        use super::schema::refresh_state::dsl;
        let res = self.raw_query_read(|conn| {
            dsl::refresh_state
                .find((entity_id.as_ref(), entity_kind))
                .first(conn)
                .optional()
        })?;

        Ok(res)
    }

    pub fn get_last_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
    ) -> Result<i64, StorageError> {
        let state: Option<RefreshState> = self.get_refresh_state(&id, entity_kind)?;
        match state {
            Some(state) => Ok(state.cursor),
            None => {
                let new_state = RefreshState {
                    entity_id: id.as_ref().to_vec(),
                    entity_kind,
                    cursor: 0,
                };
                new_state.store_or_ignore(self)?;
                Ok(0)
            }
        }
    }

    pub fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: i64,
    ) -> Result<bool, StorageError> {
        use super::schema::refresh_state::dsl;

        let entity_id_bytes = entity_id.as_ref().to_vec();

        let result = self.raw_query_write(|conn| {
            // First, try to update existing record
            let updated = diesel::update(dsl::refresh_state)
                .filter(dsl::entity_id.eq(&entity_id_bytes))
                .filter(dsl::entity_kind.eq(entity_kind))
                .filter(dsl::cursor.lt(cursor))
                .set(dsl::cursor.eq(cursor))
                .execute(conn)?;

            if updated > 0 {
                return Ok(true);
            }

            // If no update, try to insert
            match diesel::insert_into(dsl::refresh_state)
                .values((
                    dsl::entity_id.eq(&entity_id_bytes),
                    dsl::entity_kind.eq(entity_kind),
                    dsl::cursor.eq(cursor),
                ))
                .execute(conn)
            {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        });

        Ok(result?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::test_utils::with_connection;

    #[xmtp_common::test]
    async fn get_cursor_with_no_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let kind = EntityKind::Group;
            let entry: Option<RefreshState> = conn.get_refresh_state(&id, kind).unwrap();
            assert!(entry.is_none());
            assert_eq!(conn.get_last_cursor_for_id(&id, kind).unwrap(), 0);
            let entry: Option<RefreshState> = conn.get_refresh_state(&id, kind).unwrap();
            assert!(entry.is_some());
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_timestamp_with_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::Welcome;
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                cursor: 123,
            };
            entry.store_or_ignore(conn).unwrap();
            assert_eq!(conn.get_last_cursor_for_id(&id, entity_kind).unwrap(), 123);
        })
        .await
    }

    #[xmtp_common::test]
    async fn update_timestamp_when_bigger() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::Group;
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                cursor: 123,
            };
            entry.store_or_ignore(conn).unwrap();
            assert!(conn.update_cursor(&id, entity_kind, 124).unwrap());
            let entry: Option<RefreshState> = conn.get_refresh_state(&id, entity_kind).unwrap();
            assert_eq!(entry.unwrap().cursor, 124);
        })
        .await
    }

    #[xmtp_common::test]
    async fn dont_update_timestamp_when_smaller() {
        with_connection(|conn| {
            let entity_id = vec![1, 2, 3];
            let entity_kind = EntityKind::Welcome;

            let entry = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind,
                cursor: 123,
            };
            entry.store_or_ignore(conn).unwrap();
            assert!(!conn.update_cursor(&entity_id, entity_kind, 122).unwrap());
            let entry: Option<RefreshState> =
                conn.get_refresh_state(&entity_id, entity_kind).unwrap();
            assert_eq!(entry.unwrap().cursor, 123);
        })
        .await
    }

    #[xmtp_common::test]
    async fn allow_installation_and_welcome_same_id() {
        with_connection(|conn| {
            let entity_id = vec![1, 2, 3];
            let welcome_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::Welcome,
                cursor: 123,
            };
            welcome_state.store_or_ignore(conn).unwrap();

            let group_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::Group,
                cursor: 456,
            };
            group_state.store_or_ignore(conn).unwrap();

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
        .await
    }
}
