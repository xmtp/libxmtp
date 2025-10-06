use std::collections::HashMap;

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use xmtp_configuration::Originators;
use xmtp_proto::types::Cursor;

use super::{ConnectionExt, Sqlite, db_connection::DbConnection, schema::refresh_state};
use crate::{StorageError, StoreOrIgnore, impl_store_or_ignore};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, Hash, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum EntityKind {
    Welcome = 1,
    ApplicationMessage = 2,       // Application messages (originator 10)
    CommitLogUpload = 3, // Rowid of the last local entry we uploaded to the remote commit log
    CommitLogDownload = 4, // Server log sequence id of last remote entry we downloaded from the remote commit log
    CommitLogForkCheckLocal = 5, // Last rowid verified in local commit log
    CommitLogForkCheckRemote = 6, // Last rowid verified in remote commit log
    CommitMessage = 7,     // MLS commit messages (originator 0)
}

pub trait HasEntityKind {
    fn entity_kind(&self) -> EntityKind;
}

impl HasEntityKind for xmtp_proto::types::GroupMessage {
    fn entity_kind(&self) -> EntityKind {
        if self.is_commit() {
            EntityKind::CommitMessage
        } else {
            EntityKind::ApplicationMessage
        }
    }
}

impl HasEntityKind for xmtp_proto::types::WelcomeMessage {
    fn entity_kind(&self) -> EntityKind {
        EntityKind::Welcome
    }
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EntityKind::*;
        match self {
            Welcome => write!(f, "welcome"),
            ApplicationMessage => write!(f, "group"),
            CommitLogUpload => write!(f, "commit_log_upload"),
            CommitLogDownload => write!(f, "commit_log_download"),
            CommitLogForkCheckLocal => write!(f, "commit_log_fork_check_local"),
            CommitLogForkCheckRemote => write!(f, "commit_log_fork_check_remote"),
            CommitMessage => write!(f, "commit_message"),
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
            2 => Ok(EntityKind::ApplicationMessage),
            3 => Ok(EntityKind::CommitLogUpload),
            4 => Ok(EntityKind::CommitLogDownload),
            5 => Ok(EntityKind::CommitLogForkCheckLocal),
            6 => Ok(EntityKind::CommitLogForkCheckRemote),
            7 => Ok(EntityKind::CommitMessage),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[derive(Insertable, Identifiable, Queryable, Debug, Clone)]
#[diesel(table_name = refresh_state)]
#[diesel(primary_key(entity_id, entity_kind, originator_id))]
pub struct RefreshState {
    pub entity_id: Vec<u8>,
    pub entity_kind: EntityKind,
    pub sequence_id: i64,
    pub originator_id: i32,
}

impl_store_or_ignore!(RefreshState, refresh_state);

pub trait QueryRefreshState {
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
        originator_id: u32,
    ) -> Result<Option<RefreshState>, StorageError>;

    fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_ids: &[u32],
    ) -> Result<Vec<Cursor>, StorageError>;

    fn get_last_cursor_for_originator<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_id: u32,
    ) -> Result<Cursor, StorageError> {
        // get_last_cursor guaranteed to return entry for id
        self.get_last_cursor_for_originators(id, entity_kind, &[originator_id])
            .map(|c| c[0])
    }

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError>;

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError>;
}

impl<T: QueryRefreshState> QueryRefreshState for &'_ T {
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
        originator: u32,
    ) -> Result<Option<RefreshState>, StorageError> {
        (**self).get_refresh_state(entity_id, entity_kind, originator)
    }

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError> {
        (**self).update_cursor(entity_id, entity_kind, cursor)
    }

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError> {
        (**self).get_remote_log_cursors(conversation_ids)
    }

    fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_ids: &[u32],
    ) -> Result<Vec<Cursor>, StorageError> {
        (**self).get_last_cursor_for_originators(id, entity_kind, originator_ids)
    }
}

impl<C: ConnectionExt> QueryRefreshState for DbConnection<C> {
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
        originator_id: u32,
    ) -> Result<Option<RefreshState>, StorageError> {
        use super::schema::refresh_state::dsl;

        let res = self.raw_query_read(|conn| {
            dsl::refresh_state
                .find((entity_id.as_ref(), entity_kind, originator_id as i32))
                .first(conn)
                .optional()
        })?;
        Ok(res)
    }

    fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_ids: &[u32],
    ) -> Result<Vec<Cursor>, StorageError> {
        let mut last_seen = Vec::with_capacity(originator_ids.len());
        for originator in originator_ids {
            let state: Option<RefreshState> =
                self.get_refresh_state(&id, entity_kind, *originator)?;
            let state = match state {
                Some(state) => Cursor {
                    sequence_id: state.sequence_id as u64,
                    originator_id: state.originator_id as u32,
                },
                None => {
                    let new_state = RefreshState {
                        entity_id: id.as_ref().to_vec(),
                        entity_kind,
                        sequence_id: 0,
                        originator_id: *originator as i32,
                    };
                    new_state.store_or_ignore(self)?;
                    Cursor::new(0, *originator)
                }
            };
            last_seen.push(state);
        }
        Ok(last_seen)
    }

    #[tracing::instrument(level = "info", skip(self), fields(entity_id = %hex::encode(&entity_id)))]
    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError> {
        use super::schema::refresh_state::dsl;
        use crate::diesel::upsert::excluded;
        use diesel::query_dsl::methods::FilterDsl;

        let state = RefreshState {
            entity_id: entity_id.as_ref().to_vec(),
            entity_kind,
            sequence_id: cursor.sequence_id as i64,
            originator_id: cursor.originator_id as i32,
        };
        let num_updated = self.raw_query_write(|conn| {
            diesel::insert_into(dsl::refresh_state)
                .values(&state)
                .on_conflict((dsl::entity_id, dsl::entity_kind, dsl::originator_id))
                .do_update()
                .set(dsl::sequence_id.eq(excluded(dsl::sequence_id)))
                .filter(dsl::sequence_id.lt(excluded(dsl::sequence_id)))
                .execute(conn)
        })?;
        Ok(num_updated >= 1)
    }

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError> {
        let mut cursor_map: HashMap<Vec<u8>, Cursor> = HashMap::new();
        for conversation_id in conversation_ids {
            let cursor = self
                .get_last_cursor_for_originator(
                    conversation_id,
                    EntityKind::CommitLogDownload,
                    Originators::REMOTE_COMMIT_LOG.into(),
                )
                .unwrap_or_default();
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
            let kind = EntityKind::ApplicationMessage;
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS.into())
                .unwrap();
            assert!(entry.is_none());
            assert_eq!(
                conn.get_last_cursor_for_originator(&id, kind, Originators::MLS_COMMITS.into())
                    .unwrap(),
                Cursor {
                    sequence_id: 0,
                    originator_id: Originators::MLS_COMMITS.into()
                }
            );
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS.into())
                .unwrap();
            assert!(entry.is_some());
        })
        .await
    }

    #[xmtp_common::test]
    async fn get_cursor_with_no_existing_state_originator() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let kind = EntityKind::ApplicationMessage;
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS.into())
                .unwrap();
            assert!(entry.is_none());
            assert_eq!(
                conn.get_last_cursor_for_originators(&id, kind, &[0])
                    .unwrap()[0],
                Cursor {
                    sequence_id: 0,
                    originator_id: Originators::MLS_COMMITS.into()
                }
            );
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS.into())
                .unwrap();
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
                sequence_id: 123,
                originator_id: Originators::MLS_COMMITS.into(),
            };
            entry.store_or_ignore(conn).unwrap();
            assert_eq!(
                conn.get_last_cursor_for_originator(
                    &id,
                    entity_kind,
                    Originators::MLS_COMMITS.into()
                )
                .unwrap(),
                Cursor {
                    sequence_id: 123,
                    originator_id: Originators::MLS_COMMITS.into()
                }
            );
        })
        .await
    }

    #[xmtp_common::test]
    async fn update_timestamp_when_bigger() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::ApplicationMessage;
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                sequence_id: 123,
                originator_id: 10,
            };
            entry.store_or_ignore(conn).unwrap();
            assert!(
                conn.update_cursor(
                    &id,
                    entity_kind,
                    Cursor::new(124, Originators::APPLICATION_MESSAGES)
                )
                .unwrap()
            );
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, entity_kind, Originators::APPLICATION_MESSAGES.into())
                .unwrap();
            assert_eq!(entry.unwrap().sequence_id, 124);
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
                sequence_id: 123,
                originator_id: 10,
            };
            entry.store_or_ignore(conn).unwrap();
            assert!(
                !conn
                    .update_cursor(
                        &entity_id,
                        entity_kind,
                        Cursor::new(122, Originators::APPLICATION_MESSAGES)
                    )
                    .unwrap()
            );
            let entry: Option<RefreshState> = conn
                .get_refresh_state(
                    &entity_id,
                    entity_kind,
                    Originators::APPLICATION_MESSAGES.into(),
                )
                .unwrap();
            assert_eq!(entry.unwrap().sequence_id, 123);
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
                sequence_id: 123,
                originator_id: Originators::MLS_COMMITS.into(),
            };
            welcome_state.store_or_ignore(conn).unwrap();

            let group_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::ApplicationMessage,
                sequence_id: 456,
                originator_id: Originators::MLS_COMMITS.into(),
            };
            group_state.store_or_ignore(conn).unwrap();

            let welcome_state_retrieved = conn
                .get_refresh_state(
                    &entity_id,
                    EntityKind::Welcome,
                    Originators::MLS_COMMITS.into(),
                )
                .unwrap()
                .unwrap();
            assert_eq!(welcome_state_retrieved.sequence_id, 123);

            let group_state_retrieved = conn
                .get_refresh_state(
                    &entity_id,
                    EntityKind::ApplicationMessage,
                    Originators::MLS_COMMITS.into(),
                )
                .unwrap()
                .unwrap();
            assert_eq!(group_state_retrieved.sequence_id, 456);
        })
        .await
    }
}
