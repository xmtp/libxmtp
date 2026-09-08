use std::collections::HashMap;

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use xmtp_proto::types::Cursor;

use super::{ConnectionExt, Sqlite, db_connection::DbConnection, schema::refresh_state};
use crate::{StorageError, StoreOrIgnore, impl_store_or_ignore};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, Hash, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum EntityKind {
    Welcome = 1,
    ApplicationMessage = 2,       // Application messages
    CommitLogUpload = 3, // Rowid of the last local entry we uploaded to the remote commit log
    CommitLogDownload = 4, // Server log sequence id of last remote entry we downloaded from the remote commit log
    CommitLogForkCheckLocal = 5, // Last rowid verified in local commit log
    CommitLogForkCheckRemote = 6, // Last rowid verified in remote commit log
    CommitMessage = 7,     // MLS commit messages
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
#[diesel(primary_key(entity_id, entity_kind))]
pub struct RefreshState {
    pub entity_id: Vec<u8>,
    pub entity_kind: EntityKind,
    pub sequence_id: i64,
}

impl_store_or_ignore!(RefreshState, refresh_state);

pub trait QueryRefreshState {
    fn get_refresh_state<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError>;

    /// Read one ledger position. Create a zero position when it is absent.
    fn get_last_cursor<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
    ) -> Result<Cursor, StorageError>;

    /// Return the minimum position across the requested kinds for each stored id.
    /// An absent kind has position zero. Ids with no rows are absent from the map.
    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, Cursor>, StorageError>;

    /// Advance a ledger position only when the new position is greater.
    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError>;

    fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
    ) -> Result<Cursor, StorageError> {
        Ok(self
            .get_last_cursor_for_ids(&[entity_id.as_ref()], entities)?
            .remove(entity_id.as_ref())
            .unwrap_or_default())
    }

    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, StorageError> {
        conversation_ids
            .iter()
            .map(|id| {
                self.get_last_cursor(id, EntityKind::CommitLogDownload)
                    .map(|cursor| (id.to_vec(), cursor))
            })
            .collect()
    }
}

impl<T: QueryRefreshState> QueryRefreshState for &T {
    fn get_refresh_state<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError> {
        (**self).get_refresh_state(entity_id, entity_kind)
    }

    fn get_last_cursor<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
    ) -> Result<Cursor, StorageError> {
        (**self).get_last_cursor(id, entity_kind)
    }

    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, Cursor>, StorageError> {
        (**self).get_last_cursor_for_ids(ids, entities)
    }

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError> {
        (**self).update_cursor(entity_id, entity_kind, cursor)
    }
}

impl<C: ConnectionExt> QueryRefreshState for DbConnection<C> {
    #[xmtp_common::db_span]
    fn get_refresh_state<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
    ) -> Result<Option<RefreshState>, StorageError> {
        Ok(self.raw_query(|conn| {
            refresh_state::table
                .find((entity_id.as_ref(), entity_kind))
                .first(conn)
                .optional()
        })?)
    }

    #[xmtp_common::db_span]
    fn get_last_cursor<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
    ) -> Result<Cursor, StorageError> {
        RefreshState {
            entity_id: id.as_ref().to_vec(),
            entity_kind,
            sequence_id: 0,
        }
        .store_or_ignore(self)?;
        Ok(Cursor(
            self.get_refresh_state(id, entity_kind)?
                .ok_or(StorageError::DbDeserialize)?
                .sequence_id as u64,
        ))
    }

    #[xmtp_common::db_span]
    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, Cursor>, StorageError> {
        use super::schema::refresh_state::dsl;
        use diesel::dsl::{count, min};
        use std::collections::HashSet;

        // Leave room for the kind filters under SQLite's bind parameter limit.
        const IDS_PER_QUERY: usize = 900;
        let entities: HashSet<_> = entities.iter().copied().collect();
        if entities.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self.raw_query(|conn| {
            let mut result = HashMap::new();
            for chunk in ids.chunks(IDS_PER_QUERY) {
                let ids: Vec<_> = chunk.iter().map(AsRef::as_ref).collect();
                let rows = dsl::refresh_state
                    .filter(dsl::entity_kind.eq_any(&entities))
                    .filter(dsl::entity_id.eq_any(ids))
                    .group_by(dsl::entity_id)
                    .select((
                        dsl::entity_id,
                        min(dsl::sequence_id),
                        count(dsl::entity_kind),
                    ))
                    .load::<(Vec<u8>, Option<i64>, i64)>(conn)?;
                for (id, sequence, kinds) in rows {
                    let sequence = if kinds as usize == entities.len() {
                        sequence.unwrap_or_default() as u64
                    } else {
                        0
                    };
                    result.insert(id, Cursor(sequence));
                }
            }
            Ok(result)
        })?)
    }

    #[xmtp_common::db_span]
    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError> {
        use super::schema::refresh_state::dsl;
        use diesel::{query_dsl::methods::FilterDsl, upsert::excluded};
        let state = RefreshState {
            entity_id: entity_id.as_ref().to_vec(),
            entity_kind,
            sequence_id: i64::try_from(cursor.0).map_err(|_| StorageError::DbSerialize)?,
        };
        Ok(self.raw_query(|conn| {
            diesel::insert_into(dsl::refresh_state)
                .values(&state)
                .on_conflict((dsl::entity_id, dsl::entity_kind))
                .do_update()
                .set(dsl::sequence_id.eq(excluded(dsl::sequence_id)))
                .filter(dsl::sequence_id.lt(excluded(dsl::sequence_id)))
                .execute(conn)
        })? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::with_connection;

    #[xmtp_common::test(unwrap_try = true)]
    fn cursor_defaults_and_advances_only_in_order() {
        with_connection(|conn| {
            let id = [1, 2, 3];
            let kind = EntityKind::ApplicationMessage;
            assert!(conn.get_refresh_state(id, kind).unwrap().is_none());
            assert_eq!(conn.get_last_cursor(id, kind).unwrap(), Cursor(0));
            assert!(conn.get_refresh_state(id, kind).unwrap().is_some());
            assert!(conn.update_cursor(id, kind, Cursor(123)).unwrap());
            assert!(!conn.update_cursor(id, kind, Cursor(122)).unwrap());
            assert!(!conn.update_cursor(id, kind, Cursor(123)).unwrap());
            assert!(conn.update_cursor(id, kind, Cursor(124)).unwrap());
            assert_eq!(conn.get_last_cursor(id, kind).unwrap(), Cursor(124));
        });
    }

    #[rstest::rstest]
    #[case(Some(500), Some(250), 250)]
    #[case(Some(100), Some(200), 100)]
    #[case(Some(500), None, 0)]
    #[case(None, Some(250), 0)]
    #[case(None, None, 0)]
    #[xmtp_common::test(unwrap_try = true)]
    fn cursor_meets_requested_kinds(
        #[case] application: Option<u64>,
        #[case] commit: Option<u64>,
        #[case] expected: u64,
    ) {
        with_connection(|conn| {
            let id = [1, 2, 3];
            for (kind, value) in [
                (EntityKind::ApplicationMessage, application),
                (EntityKind::CommitMessage, commit),
            ] {
                if let Some(value) = value {
                    conn.update_cursor(id, kind, Cursor(value)).unwrap();
                }
            }
            conn.update_cursor(id, EntityKind::Welcome, Cursor(999))
                .unwrap();
            let kinds = [EntityKind::ApplicationMessage, EntityKind::CommitMessage];
            assert_eq!(
                conn.latest_cursor_for_id(id, &kinds).unwrap(),
                Cursor(expected)
            );
            assert_eq!(
                conn.latest_cursor_for_id(id, &[EntityKind::Welcome])
                    .unwrap(),
                Cursor(999)
            );
        });
    }

    #[rstest::rstest]
    #[case(0)]
    #[case(1)]
    #[case(900)]
    #[case(1000)]
    #[case(2000)]
    #[xmtp_common::test(unwrap_try = true)]
    fn cursor_queries_batch_ids(#[case] count: u64) {
        with_connection(|conn| {
            let ids: Vec<_> = (0..count).map(u64::to_be_bytes).collect();
            for (index, id) in ids.iter().enumerate() {
                conn.update_cursor(id, EntityKind::ApplicationMessage, Cursor(index as u64))
                    .unwrap();
            }
            let found = conn
                .get_last_cursor_for_ids(&ids, &[EntityKind::ApplicationMessage])
                .unwrap();
            assert_eq!(found.len(), ids.len());
            for (index, id) in ids.iter().enumerate() {
                assert_eq!(found.get(id.as_slice()), Some(&Cursor(index as u64)));
            }
            let missing = conn
                .get_last_cursor_for_ids(&[[255; 8]], &[EntityKind::ApplicationMessage])
                .unwrap();
            assert!(missing.is_empty());
        });
    }
}
