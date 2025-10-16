use std::collections::HashMap;

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};

use super::{ConnectionExt, Sqlite, db_connection::DbConnection, schema::refresh_state};
use crate::{StorageError, StoreOrIgnore, impl_store_or_ignore};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, Hash, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum EntityKind {
    Welcome = 1,
    Group = 2,
    CommitLogUpload = 3, // Rowid of the last local entry we uploaded to the remote commit log
    CommitLogDownload = 4, // Server log sequence id of last remote entry we downloaded from the remote commit log
    CommitLogForkCheckLocal = 5, // Last rowid verified in local commit log
    CommitLogForkCheckRemote = 6, // Last rowid verified in remote commit log
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EntityKind::*;
        match self {
            Welcome => write!(f, "welcome"),
            Group => write!(f, "group"),
            CommitLogUpload => write!(f, "commit_log_upload"),
            CommitLogDownload => write!(f, "commit_log_download"),
            CommitLogForkCheckLocal => write!(f, "commit_log_fork_check_local"),
            CommitLogForkCheckRemote => write!(f, "commit_log_fork_check_remote"),
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
            3 => Ok(EntityKind::CommitLogUpload),
            4 => Ok(EntityKind::CommitLogDownload),
            5 => Ok(EntityKind::CommitLogForkCheckLocal),
            6 => Ok(EntityKind::CommitLogForkCheckRemote),
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

pub trait QueryRefreshState {
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError>;

    fn get_last_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
    ) -> Result<i64, StorageError>;

    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entity_kind: EntityKind,
    ) -> Result<HashMap<Vec<u8>, i64>, StorageError>;

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: i64,
    ) -> Result<bool, StorageError>;

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, i64>, crate::ConnectionError>;
}

impl<T: QueryRefreshState> QueryRefreshState for &'_ T {
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError> {
        (**self).get_refresh_state(entity_id, entity_kind)
    }

    fn get_last_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
    ) -> Result<i64, StorageError> {
        (**self).get_last_cursor_for_id(id, entity_kind)
    }

    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entity_kind: EntityKind,
    ) -> Result<HashMap<Vec<u8>, i64>, StorageError> {
        (**self).get_last_cursor_for_ids(ids, entity_kind)
    }

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: i64,
    ) -> Result<bool, StorageError> {
        (**self).update_cursor(entity_id, entity_kind, cursor)
    }

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, i64>, crate::ConnectionError> {
        (**self).get_remote_log_cursors(conversation_ids)
    }
}

impl<C: ConnectionExt> QueryRefreshState for DbConnection<C> {
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
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

    fn get_last_cursor_for_id<Id: AsRef<[u8]>>(
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

    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entity_kind: EntityKind,
    ) -> Result<HashMap<Vec<u8>, i64>, StorageError> {
        use super::schema::refresh_state::dsl;
        use std::collections::HashMap;

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Run multiple small IN-queries and merge results.
        // Keep chunks comfortably under SQLite's default 999-bind limit.
        const CHUNK: usize = 900;

        let map = self.raw_query_read(|conn| {
            ids.chunks(CHUNK)
                .map(|chunk| {
                    let id_refs: Vec<&[u8]> = chunk.iter().map(|id| id.as_ref()).collect();
                    dsl::refresh_state
                        .filter(dsl::entity_kind.eq(entity_kind))
                        .filter(dsl::entity_id.eq_any(&id_refs))
                        .select((dsl::entity_id, dsl::cursor))
                        .load::<(Vec<u8>, i64)>(conn)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|vecs| vecs.into_iter().flatten().collect::<HashMap<_, _>>())
        })?;

        Ok(map)
    }

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: i64,
    ) -> Result<bool, StorageError> {
        use super::schema::refresh_state::dsl;
        use crate::diesel::upsert::excluded;
        use diesel::query_dsl::methods::FilterDsl;

        let num_updated = self.raw_query_write(|conn| {
            diesel::insert_into(dsl::refresh_state)
                .values(RefreshState {
                    entity_id: entity_id.as_ref().to_vec(),
                    entity_kind,
                    cursor,
                })
                .on_conflict((dsl::entity_id, dsl::entity_kind))
                .do_update()
                // Only update if the existing cursor is lower than the incoming one:
                .set(dsl::cursor.eq(excluded(dsl::cursor)))
                .filter(dsl::cursor.lt(excluded(dsl::cursor)))
                .execute(conn)
        })?;

        Ok(num_updated >= 1)
    }

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, i64>, crate::ConnectionError> {
        let mut cursor_map: HashMap<Vec<u8>, i64> = HashMap::new();
        for conversation_id in conversation_ids {
            let cursor = self
                .get_last_cursor_for_id(conversation_id, EntityKind::CommitLogDownload)
                .unwrap_or(0);
            cursor_map.insert(conversation_id.to_vec(), cursor);
        }
        Ok(cursor_map)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::StoreOrIgnore;
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

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_empty() {
        with_connection(|conn| {
            let ids: Vec<Vec<u8>> = vec![];
            let entity_kind = EntityKind::Group;
            let result = conn.get_last_cursor_for_ids(&ids, entity_kind).unwrap();
            assert!(result.is_empty());
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_single() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::Group;

            // Store a state
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                cursor: 456,
            };
            entry.store_or_ignore(conn).unwrap();

            // Query for it
            let ids = vec![id.clone()];
            let result = conn.get_last_cursor_for_ids(&ids, entity_kind).unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result.get(&id), Some(&456));
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_multiple_mixed() {
        with_connection(|conn| {
            let entity_kind = EntityKind::Group;

            // Create some ids with existing state
            let id1 = vec![1, 0, 0];
            let id2 = vec![2, 0, 0];
            let id3 = vec![3, 0, 0];
            let id4 = vec![4, 0, 0]; // This one won't have state

            RefreshState {
                entity_id: id1.clone(),
                entity_kind,
                cursor: 100,
            }
            .store_or_ignore(conn)
            .unwrap();

            RefreshState {
                entity_id: id2.clone(),
                entity_kind,
                cursor: 200,
            }
            .store_or_ignore(conn)
            .unwrap();

            RefreshState {
                entity_id: id3.clone(),
                entity_kind,
                cursor: 300,
            }
            .store_or_ignore(conn)
            .unwrap();

            // Query for all ids including one without state
            let ids = vec![id1.clone(), id2.clone(), id3.clone(), id4.clone()];
            let result = conn.get_last_cursor_for_ids(&ids, entity_kind).unwrap();

            // Should only return the ones with existing state
            assert_eq!(result.len(), 3);
            assert_eq!(result.get(&id1), Some(&100));
            assert_eq!(result.get(&id2), Some(&200));
            assert_eq!(result.get(&id3), Some(&300));
            assert_eq!(result.get(&id4), None);
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_exactly_900() {
        with_connection(|conn| {
            let entity_kind = EntityKind::Group;

            // Create exactly 900 ids
            let mut ids = Vec::new();
            for i in 0..900 {
                let id = vec![(i / 256) as u8, (i % 256) as u8];
                RefreshState {
                    entity_id: id.clone(),
                    entity_kind,
                    cursor: i as i64,
                }
                .store_or_ignore(conn)
                .unwrap();
                ids.push(id);
            }

            // Query for all 900 ids
            let result = conn.get_last_cursor_for_ids(&ids, entity_kind).unwrap();

            assert_eq!(result.len(), 900);
            for (idx, id) in ids.iter().enumerate() {
                assert_eq!(result.get(id), Some(&(idx as i64)));
            }
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_over_900() {
        with_connection(|conn| {
            let entity_kind = EntityKind::Group;

            // Create 1000 ids to test chunking
            let mut ids = Vec::new();
            for i in 0..1000 {
                let id = vec![(i / 256) as u8, (i % 256) as u8, 0];
                RefreshState {
                    entity_id: id.clone(),
                    entity_kind,
                    cursor: i as i64,
                }
                .store_or_ignore(conn)
                .unwrap();
                ids.push(id);
            }

            // Query for all 1000 ids (should use 2 chunks)
            let result = conn.get_last_cursor_for_ids(&ids, entity_kind).unwrap();

            assert_eq!(result.len(), 1000);
            for (idx, id) in ids.iter().enumerate() {
                assert_eq!(
                    result.get(id),
                    Some(&(idx as i64)),
                    "Mismatch for id at index {}",
                    idx
                );
            }
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_over_1800() {
        with_connection(|conn| {
            let entity_kind = EntityKind::Group;

            // Create 2000 ids to test multiple chunks
            let mut ids = Vec::new();
            for i in 0..2000 {
                let id = vec![(i / 256) as u8, (i % 256) as u8, 1];
                RefreshState {
                    entity_id: id.clone(),
                    entity_kind,
                    cursor: i as i64,
                }
                .store_or_ignore(conn)
                .unwrap();
                ids.push(id);
            }

            // Query for all 2000 ids (should use 3 chunks: 900, 900, 200)
            let result = conn.get_last_cursor_for_ids(&ids, entity_kind).unwrap();

            assert_eq!(result.len(), 2000);
            for (idx, id) in ids.iter().enumerate() {
                assert_eq!(
                    result.get(id),
                    Some(&(idx as i64)),
                    "Mismatch for id at index {}",
                    idx
                );
            }
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_different_entity_kinds() {
        with_connection(|conn| {
            let id1 = vec![1, 2, 3];
            let id2 = vec![4, 5, 6];

            // Store same ids with different entity kinds
            RefreshState {
                entity_id: id1.clone(),
                entity_kind: EntityKind::Group,
                cursor: 100,
            }
            .store_or_ignore(conn)
            .unwrap();

            RefreshState {
                entity_id: id1.clone(),
                entity_kind: EntityKind::Welcome,
                cursor: 200,
            }
            .store_or_ignore(conn)
            .unwrap();

            RefreshState {
                entity_id: id2.clone(),
                entity_kind: EntityKind::Group,
                cursor: 300,
            }
            .store_or_ignore(conn)
            .unwrap();

            // Query for Group entity kind only
            let ids = vec![id1.clone(), id2.clone()];
            let result = conn
                .get_last_cursor_for_ids(&ids, EntityKind::Group)
                .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result.get(&id1), Some(&100));
            assert_eq!(result.get(&id2), Some(&300));

            // Query for Welcome entity kind only
            let result = conn
                .get_last_cursor_for_ids(&ids, EntityKind::Welcome)
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result.get(&id1), Some(&200));
            assert_eq!(result.get(&id2), None);
        })
        .await
    }
}
