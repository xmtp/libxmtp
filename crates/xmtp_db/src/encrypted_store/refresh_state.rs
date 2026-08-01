use std::collections::HashMap;

#[cfg(feature = "sync")]
use diesel::{deserialize::FromSqlRow, expression::AsExpression, prelude::*, sql_types::Integer};
use xmtp_configuration::Originators;
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId};

#[cfg(feature = "sync")]
use super::{ConnectionExt, db_connection::DbConnection, schema::refresh_state};
use crate::StorageError;
#[cfg(feature = "sync")]
use crate::{StoreOrIgnore, impl_store_or_ignore};

#[cfg(feature = "sync")]
allow_columns_to_appear_in_same_group_by_clause!(
    super::schema::identity_updates::originator_id,
    super::schema::identity_updates::sequence_id,
    super::schema::refresh_state::originator_id,
    super::schema::refresh_state::sequence_id
);

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
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

crate::impl_sql_int_enum!(EntityKind {
    Welcome = 1,
    ApplicationMessage = 2,
    CommitLogUpload = 3,
    CommitLogDownload = 4,
    CommitLogForkCheckLocal = 5,
    CommitLogForkCheckRemote = 6,
    CommitMessage = 7,
});

#[derive(Debug, Clone)]
#[cfg_attr(feature = "sync", derive(Insertable, Identifiable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = refresh_state))]
#[cfg_attr(
    feature = "sync",
    diesel(primary_key(entity_id, entity_kind, originator_id))
)]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "refresh_state")]
pub struct RefreshState {
    pub entity_id: Vec<u8>,
    pub entity_kind: EntityKind,
    pub sequence_id: i64,
    pub originator_id: i32,
}

#[cfg(feature = "sync")]
impl_store_or_ignore!(RefreshState, refresh_state);

/// Helper function to convert rows of (entity_id, originator_id, sequence_id) into a HashMap
/// where each entity_id maps to a GlobalCursor containing all its originator->sequence_id pairs.
/// Null sequence_id values are coalesced to 0.
fn rows_to_global_cursor_map(
    rows: Vec<(Vec<u8>, i32, Option<i64>)>,
) -> HashMap<Vec<u8>, GlobalCursor> {
    let mut map: HashMap<Vec<u8>, GlobalCursor> = HashMap::new();

    for (entity_id, originator_id, sequence_id) in rows {
        let cursors = map.entry(entity_id).or_default();
        let originator_id_u32 = originator_id as u32;
        let sequence_id_u64 = sequence_id.unwrap_or(0) as u64;

        cursors.insert(originator_id_u32, sequence_id_u64);
    }

    map
}

#[maybe_async::maybe_async(AFIT)]
pub trait QueryRefreshState {
    async fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
        originator_id: u32,
    ) -> Result<Option<RefreshState>, StorageError>;

    async fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_ids: &[u32],
    ) -> Result<Vec<Cursor>, StorageError>;

    async fn get_last_cursor_for_originator<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_id: u32,
    ) -> Result<Cursor, StorageError> {
        // get_last_cursor guaranteed to return entry for id
        self.get_last_cursor_for_originators(id, entity_kind, &[originator_id])
            .await
            .map(|c| c[0])
    }

    async fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, GlobalCursor>, StorageError>;

    async fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError>;

    async fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError>;

    async fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T: QueryRefreshState> QueryRefreshState for &'_ T {
    async fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
        originator: u32,
    ) -> Result<Option<RefreshState>, StorageError> {
        (**self)
            .get_refresh_state(entity_id, entity_kind, originator)
            .await
    }

    async fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, GlobalCursor>, StorageError> {
        (**self).get_last_cursor_for_ids(ids, entities).await
    }

    async fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError> {
        (**self).update_cursor(entity_id, entity_kind, cursor).await
    }

    async fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError> {
        (**self).get_remote_log_cursors(conversation_ids).await
    }

    async fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_ids: &[u32],
    ) -> Result<Vec<Cursor>, StorageError> {
        (**self)
            .get_last_cursor_for_originators(id, entity_kind, originator_ids)
            .await
    }

    async fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError> {
        (**self)
            .latest_cursor_for_id(entity_id, entities, originators)
            .await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryRefreshState for DbConnection<C> {
    #[tracing::instrument(level = "debug", skip_all)]
    fn get_refresh_state<EntityId: AsRef<[u8]>>(
        &self,
        entity_id: EntityId,
        entity_kind: EntityKind,
        originator_id: u32,
    ) -> Result<Option<RefreshState>, StorageError> {
        use super::schema::refresh_state::dsl;

        let res = self.raw_query(|conn| {
            dsl::refresh_state
                .find((entity_id.as_ref(), entity_kind, originator_id as i32))
                .first(conn)
                .optional()
        })?;
        Ok(res)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
        &self,
        id: Id,
        entity_kind: EntityKind,
        originator_ids: &[u32],
    ) -> Result<Vec<Cursor>, StorageError> {
        use super::schema::refresh_state::dsl;

        let id_ref = id.as_ref();

        let originator_ids_i32: Vec<i32> = originator_ids.iter().map(|o| *o as i32).collect();
        let found_states: Vec<RefreshState> = self.raw_query(|conn| {
            dsl::refresh_state
                .filter(dsl::entity_id.eq(id_ref))
                .filter(dsl::entity_kind.eq(entity_kind))
                .filter(dsl::originator_id.eq_any(originator_ids_i32))
                .load(conn)
        })?;
        let state_map: HashMap<u32, &RefreshState> = found_states
            .iter()
            .map(|s| (s.originator_id as u32, s))
            .collect();
        // Identify missing originators and create default states
        let mut missing_states = Vec::new();
        for originator in originator_ids {
            if !state_map.contains_key(originator) {
                missing_states.push(RefreshState {
                    entity_id: id_ref.to_vec(),
                    entity_kind,
                    sequence_id: 0,
                    originator_id: *originator as i32,
                });
            }
        }

        // Insert missing states
        for missing_state in &missing_states {
            missing_state.store_or_ignore(self)?;
        }

        // Build result vector maintaining input order
        let result: Vec<Cursor> = originator_ids
            .iter()
            .map(|originator| match state_map.get(originator) {
                Some(state) => Cursor::new(state.sequence_id as u64, state.originator_id as u32),
                None => Cursor::new(0, *originator),
            })
            .collect();

        Ok(result)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, GlobalCursor>, StorageError> {
        use super::schema::refresh_state::dsl;
        use std::collections::HashMap;

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Run multiple small IN-queries and merge results.
        // Keep chunks comfortably under SQLite's default 999-bind limit.
        const CHUNK: usize = 900;

        let map = self.raw_query(|conn| {
            ids.chunks(CHUNK)
                .map(|chunk| {
                    let id_refs: Vec<&[u8]> = chunk.iter().map(|id| id.as_ref()).collect();
                    let rows = dsl::refresh_state
                        .filter(dsl::entity_kind.eq_any(entities))
                        .filter(dsl::entity_id.eq_any(&id_refs))
                        .group_by((dsl::entity_id, dsl::originator_id))
                        .select((
                            dsl::entity_id,
                            dsl::originator_id,
                            diesel::dsl::max(dsl::sequence_id),
                        ))
                        .load::<(Vec<u8>, i32, Option<i64>)>(conn)?;

                    // Convert this chunk's rows to a partial map immediately
                    Ok(rows_to_global_cursor_map(rows))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|partial_maps| {
                    // Flatten all partial maps into a single map
                    // No merging needed since entity_ids don't repeat across chunks
                    partial_maps
                        .into_iter()
                        .flat_map(|partial_map| partial_map.into_iter())
                        .collect()
                })
        })?;

        Ok(map)
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
        let num_updated = self.raw_query(|conn| {
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

    #[tracing::instrument(level = "debug", skip_all)]
    fn get_remote_log_cursors(
        &self,
        conversation_ids: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError> {
        let mut cursor_map: HashMap<Vec<u8>, Cursor> = HashMap::new();
        for conversation_id in conversation_ids {
            let cursor = self
                .get_last_cursor_for_originator(
                    conversation_id,
                    EntityKind::CommitLogDownload,
                    Originators::REMOTE_COMMIT_LOG,
                )
                .unwrap_or_default();
            cursor_map.insert(conversation_id.to_vec(), cursor);
        }
        Ok(cursor_map)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError> {
        use super::schema::refresh_state::dsl;
        use diesel::dsl::max;

        let entity_ref = entity_id.as_ref();

        let cursor_map = self.raw_query(|conn| {
            let base_query = dsl::refresh_state
                .filter(dsl::entity_id.eq(entity_ref))
                .filter(dsl::entity_kind.eq_any(entities));

            // Each entity kind uses a dedicated originator (e.g. ApplicationMessage -> originator 10,
            // CommitMessage -> originator 0), so MIN vs MAX is equivalent here — each originator
            // only ever has one entity kind. We use MAX for clarity.
            let results = if let Some(oids) = originators {
                let originator_ids_i32: Vec<i32> = oids.iter().map(|o| **o as i32).collect();
                base_query
                    .filter(dsl::originator_id.eq_any(originator_ids_i32))
                    .group_by(dsl::originator_id)
                    .select((dsl::originator_id, max(dsl::sequence_id)))
                    .load::<(i32, Option<i64>)>(conn)?
            } else {
                base_query
                    .group_by(dsl::originator_id)
                    .select((dsl::originator_id, max(dsl::sequence_id)))
                    .load::<(i32, Option<i64>)>(conn)?
            };

            Ok(results
                .into_iter()
                .filter_map(|(orig_id, seq_id)| seq_id.map(|seq| (orig_id as u32, seq as u64)))
                .collect::<GlobalCursor>())
        })?;

        Ok(cursor_map)
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};
    use sqlx::Row;

    /// Decode via the `FromRow` that `#[derive(PgModel)]` emits: by column
    /// name, from the same fields the column list comes from.
    fn state(row: &sqlx::postgres::PgRow) -> Result<RefreshState, crate::ConnectionError> {
        use sqlx::FromRow;
        Ok(RefreshState::from_row(row)?)
    }

    fn kinds_as_i32(entities: &[EntityKind]) -> Vec<i32> {
        entities.iter().map(|k| *k as i32).collect()
    }

    impl QueryRefreshState for PgDb {
        async fn get_refresh_state<EntityId: AsRef<[u8]>>(
            &self,
            entity_id: EntityId,
            entity_kind: EntityKind,
            originator_id: u32,
        ) -> Result<Option<RefreshState>, StorageError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(&format!(
                "SELECT {} FROM refresh_state \
                 WHERE entity_id = $1 AND entity_kind = $2 AND originator_id = $3",
                RefreshState::select_columns()
            ))
            .bind(entity_id.as_ref())
            .bind(entity_kind)
            .bind(originator_id as i32)
            .fetch_optional(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(row.as_ref().map(state).transpose()?)
        }

        /// Reads the cursor for each originator, seeding a zero row for any that
        /// has none, and returns one cursor per requested originator **in the
        /// order asked for**. Callers index the result positionally.
        async fn get_last_cursor_for_originators<Id: AsRef<[u8]>>(
            &self,
            id: Id,
            entity_kind: EntityKind,
            originator_ids: &[u32],
        ) -> Result<Vec<Cursor>, StorageError> {
            let id_ref = id.as_ref();
            let wanted: Vec<i32> = originator_ids.iter().map(|o| *o as i32).collect();

            // Read-then-seed across two statements, so they share a transaction.
            self.atomic(async |db| {
                let found: Vec<RefreshState> = {
                    let mut c = db.conn().await?;
                    let rows = sqlx::query(&format!("SELECT {} FROM refresh_state \
                         WHERE entity_id = $1 AND entity_kind = $2 AND originator_id = ANY($3)", RefreshState::select_columns()))
                    .bind(id_ref)
                    .bind(entity_kind)
                    .bind(&wanted)
                    .fetch_all(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;
                    rows.iter()
                        .map(state)
                        .collect::<Result<_, crate::ConnectionError>>()?
                };

                let state_map: HashMap<u32, &RefreshState> = found
                    .iter()
                    .map(|s| (s.originator_id as u32, s))
                    .collect();

                // Seed the originators with no row yet, in one statement rather
                // than one per missing originator.
                let missing: Vec<i32> = originator_ids
                    .iter()
                    .filter(|o| !state_map.contains_key(o))
                    .map(|o| *o as i32)
                    .collect();
                if !missing.is_empty() {
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id) \
                         SELECT $1, $2, 0, o FROM UNNEST($3::int4[]) AS o \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(id_ref)
                    .bind(entity_kind)
                    .bind(&missing)
                    .execute(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;
                }

                Ok(originator_ids
                    .iter()
                    .map(|o| match state_map.get(o) {
                        Some(s) => Cursor::new(s.sequence_id as u64, s.originator_id as u32),
                        None => Cursor::new(0, *o),
                    })
                    .collect())
            })
            .await
        }

        /// Unlike the sync track this issues a single query: SQLite's 999-bind
        /// ceiling forces that impl to chunk the id list, but Postgres takes the
        /// whole set as one array parameter.
        async fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
            &self,
            ids: &[Id],
            entities: &[EntityKind],
        ) -> Result<HashMap<Vec<u8>, GlobalCursor>, StorageError> {
            if ids.is_empty() {
                return Ok(HashMap::new());
            }
            let id_refs: Vec<&[u8]> = ids.iter().map(|id| id.as_ref()).collect();

            let mut c = self.conn().await?;
            let rows = sqlx::query(
                "SELECT entity_id, originator_id, MAX(sequence_id) FROM refresh_state \
                 WHERE entity_kind = ANY($1) AND entity_id = ANY($2) \
                 GROUP BY entity_id, originator_id",
            )
            .bind(kinds_as_i32(entities))
            .bind(&id_refs)
            .fetch_all(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;

            let rows: Vec<(Vec<u8>, i32, Option<i64>)> = rows
                .iter()
                .map(|r| {
                    Ok((
                        r.try_get(0)?,
                        r.try_get(1)?,
                        r.try_get::<Option<i64>, _>(2)?,
                    ))
                })
                .collect::<Result<_, crate::ConnectionError>>()?;
            Ok(rows_to_global_cursor_map(rows))
        }

        /// Advances the cursor, never rewinds it. The `WHERE` on the `DO UPDATE`
        /// is what makes that atomic: a stale writer's row is rejected by the
        /// database rather than by a read-then-compare that could race.
        /// `false` means the stored cursor was already at or past this one.
        async fn update_cursor<Id: AsRef<[u8]>>(
            &self,
            entity_id: Id,
            entity_kind: EntityKind,
            cursor: Cursor,
        ) -> Result<bool, StorageError> {
            let mut c = self.conn().await?;
            let updated = sqlx::query(
                "INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (entity_id, entity_kind, originator_id) DO UPDATE \
                 SET sequence_id = excluded.sequence_id \
                 WHERE refresh_state.sequence_id < excluded.sequence_id",
            )
            .bind(entity_id.as_ref())
            .bind(entity_kind)
            .bind(cursor.sequence_id as i64)
            .bind(cursor.originator_id as i32)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?
            .rows_affected();
            Ok(updated >= 1)
        }

        /// One round trip for the whole batch. The sync track loops
        /// `get_last_cursor_for_originator` per conversation, which would be 2N
        /// network calls here; conversations with no row still come back as a
        /// zero cursor, and are seeded in the same pass.
        ///
        /// Errors propagate rather than collapsing to a default cursor as the
        /// sync path's `unwrap_or_default()` does — a failed read must not be
        /// indistinguishable from "no progress yet".
        async fn get_remote_log_cursors(
            &self,
            conversation_ids: &[&[u8]],
        ) -> Result<HashMap<Vec<u8>, Cursor>, crate::ConnectionError> {
            if conversation_ids.is_empty() {
                return Ok(HashMap::new());
            }
            let originator = Originators::REMOTE_COMMIT_LOG;

            self.atomic(async |db| {
                let found: HashMap<Vec<u8>, i64> = {
                    let mut c = db.conn().await?;
                    let rows = sqlx::query(
                        "SELECT entity_id, sequence_id FROM refresh_state \
                         WHERE entity_kind = $1 AND originator_id = $2 AND entity_id = ANY($3)",
                    )
                    .bind(EntityKind::CommitLogDownload)
                    .bind(originator as i32)
                    .bind(conversation_ids)
                    .fetch_all(&mut *c)
                    .await?;
                    rows.iter()
                        .map(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
                        .collect::<Result<_, crate::ConnectionError>>()?
                };

                let missing: Vec<&[u8]> = conversation_ids
                    .iter()
                    .filter(|id| !found.contains_key(**id))
                    .copied()
                    .collect();
                if !missing.is_empty() {
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "INSERT INTO refresh_state (entity_id, entity_kind, sequence_id, originator_id) \
                         SELECT e, $1, 0, $2 FROM UNNEST($3::bytea[]) AS e \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(EntityKind::CommitLogDownload)
                    .bind(originator as i32)
                    .bind(&missing)
                    .execute(&mut *c)
                    .await?;
                }

                Ok(conversation_ids
                    .iter()
                    .map(|id| {
                        let seq = found.get(*id).copied().unwrap_or(0);
                        (id.to_vec(), Cursor::new(seq as u64, originator))
                    })
                    .collect())
            })
            .await
        }

        async fn latest_cursor_for_id<Id: AsRef<[u8]>>(
            &self,
            entity_id: Id,
            entities: &[EntityKind],
            originators: Option<&[&OriginatorId]>,
        ) -> Result<GlobalCursor, StorageError> {
            // Each entity kind uses a dedicated originator, so grouping by
            // originator and taking MAX is unambiguous.
            let mut sql = String::from(
                "SELECT originator_id, MAX(sequence_id) FROM refresh_state \
                 WHERE entity_id = $1 AND entity_kind = ANY($2)",
            );
            if originators.is_some() {
                sql.push_str(" AND originator_id = ANY($3)");
            }
            sql.push_str(" GROUP BY originator_id");

            let mut query = sqlx::query(&sql)
                .bind(entity_id.as_ref())
                .bind(kinds_as_i32(entities));
            if let Some(oids) = originators {
                let ids: Vec<i32> = oids.iter().map(|o| **o as i32).collect();
                query = query.bind(ids);
            }

            let mut c = self.conn().await?;
            let rows = query
                .fetch_all(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;

            let pairs: Vec<(u32, Option<i64>)> = rows
                .iter()
                .map(|r| {
                    Ok((
                        r.try_get::<i32, _>(0)? as u32,
                        r.try_get::<Option<i64>, _>(1)?,
                    ))
                })
                .collect::<Result<_, crate::ConnectionError>>()?;

            // A NULL MAX means the group had no non-null sequence_id; drop it
            // rather than reporting a cursor of 0.
            Ok(pairs
                .into_iter()
                .filter_map(|(orig, seq)| seq.map(|s| (orig, s as u64)))
                .collect())
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::StoreOrIgnore;
    use crate::test_utils::with_connection;
    use rstest::rstest;

    #[xmtp_common::test]
    fn get_cursor_with_no_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let kind = EntityKind::ApplicationMessage;
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS)
                .unwrap();
            assert!(entry.is_none());
            assert_eq!(
                conn.get_last_cursor_for_originator(&id, kind, Originators::MLS_COMMITS)
                    .unwrap(),
                Cursor::mls_commits(0)
            );
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS)
                .unwrap();
            assert!(entry.is_some());
        })
    }

    #[xmtp_common::test]
    fn get_cursor_with_no_existing_state_originator() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let kind = EntityKind::ApplicationMessage;
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS)
                .unwrap();
            assert!(entry.is_none());
            assert_eq!(
                conn.get_last_cursor_for_originators(&id, kind, &[0])
                    .unwrap()[0],
                Cursor::mls_commits(0)
            );
            let entry: Option<RefreshState> = conn
                .get_refresh_state(&id, kind, Originators::MLS_COMMITS)
                .unwrap();
            assert!(entry.is_some());
        })
    }

    #[xmtp_common::test]
    fn get_timestamp_with_existing_state() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::Welcome;
            let entry = RefreshState {
                entity_id: id.clone(),
                entity_kind,
                sequence_id: 123,
                originator_id: Originators::MLS_COMMITS as i32,
            };
            entry.store_or_ignore(conn).unwrap();
            assert_eq!(
                conn.get_last_cursor_for_originator(&id, entity_kind, Originators::MLS_COMMITS)
                    .unwrap(),
                Cursor::mls_commits(123)
            );
        })
    }

    #[xmtp_common::test]
    fn update_timestamp_when_bigger() {
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
                .get_refresh_state(&id, entity_kind, Originators::APPLICATION_MESSAGES)
                .unwrap();
            assert_eq!(entry.unwrap().sequence_id, 124);
        })
    }

    #[xmtp_common::test]
    fn dont_update_timestamp_when_smaller() {
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
                .get_refresh_state(&entity_id, entity_kind, Originators::APPLICATION_MESSAGES)
                .unwrap();
            assert_eq!(entry.unwrap().sequence_id, 123);
        })
    }

    #[xmtp_common::test]
    fn allow_installation_and_welcome_same_id() {
        with_connection(|conn| {
            let entity_id = vec![1, 2, 3];
            let welcome_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::Welcome,
                sequence_id: 123,
                originator_id: Originators::MLS_COMMITS as i32,
            };
            welcome_state.store_or_ignore(conn).unwrap();

            let group_state = RefreshState {
                entity_id: entity_id.clone(),
                entity_kind: EntityKind::ApplicationMessage,
                sequence_id: 456,
                originator_id: Originators::MLS_COMMITS as i32,
            };
            group_state.store_or_ignore(conn).unwrap();

            let welcome_state_retrieved = conn
                .get_refresh_state(&entity_id, EntityKind::Welcome, Originators::MLS_COMMITS)
                .unwrap()
                .unwrap();
            assert_eq!(welcome_state_retrieved.sequence_id, 123);

            let group_state_retrieved = conn
                .get_refresh_state(
                    &entity_id,
                    EntityKind::ApplicationMessage,
                    Originators::MLS_COMMITS,
                )
                .unwrap()
                .unwrap();
            assert_eq!(group_state_retrieved.sequence_id, 456);
        })
    }

    // Helper function to create and store a RefreshState
    fn create_state<C: ConnectionExt>(
        conn: &DbConnection<C>,
        entity_id: &[u8],
        entity_kind: EntityKind,
        originator_id: i32,
        sequence_id: i64,
    ) {
        RefreshState {
            entity_id: entity_id.to_vec(),
            entity_kind,
            sequence_id,
            originator_id,
        }
        .store_or_ignore(conn)
        .unwrap();
    }

    #[rstest]
    #[case::mixed_existing_missing(
        vec![(0, 100), (10, 200)], // Pre-populate originators 0 and 10
        vec![0, 10, 20],            // Request 0, 10, and missing 20
        vec![(0, 100), (10, 200), (20, 0)] // Expected results
    )]
    #[case::preserves_order(
        vec![(5, 555), (10, 1010), (15, 1515)],
        vec![15, 5, 10], // Non-sequential order
        vec![(15, 1515), (5, 555), (10, 1010)]
    )]
    #[case::all_missing(
        vec![], // No pre-populated states
        vec![1, 2, 3],
        vec![(1, 0), (2, 0), (3, 0)]
    )]
    #[case::empty_request(
        vec![(5, 500)],
        vec![], // Empty request
        vec![]  // Empty result
    )]
    #[xmtp_common::test]
    async fn batch_query_scenarios(
        #[case] pre_populate: Vec<(i32, i64)>,
        #[case] request_originators: Vec<u32>,
        #[case] expected: Vec<(u32, u64)>,
    ) {
        with_connection(|conn| {
            let entity_id = vec![1, 1, 1];
            let entity_kind = EntityKind::CommitMessage;
            // Pre-populate states
            for (orig, seq) in pre_populate {
                create_state(conn, &entity_id, entity_kind, orig, seq);
            }

            // Execute query
            let cursors = conn
                .get_last_cursor_for_originators(&entity_id, entity_kind, &request_originators)
                .unwrap();

            // Verify results
            assert_eq!(cursors.len(), expected.len());
            for (i, (expected_orig, expected_seq)) in expected.iter().enumerate() {
                assert_eq!(cursors[i].originator_id, *expected_orig);
                assert_eq!(cursors[i].sequence_id, *expected_seq);
            }

            // Verify missing originators were persisted
            for orig in &request_originators {
                let state = conn
                    .get_refresh_state(&entity_id, entity_kind, *orig)
                    .unwrap();
                assert!(state.is_some(), "Originator {} should be persisted", orig);
            }
        })
    }

    #[rstest]
    #[case::finds_latest_per_originator(
        vec![
            // Each entity kind has a dedicated originator:
            // ApplicationMessage -> originator 10, CommitMessage -> originator 0
            (EntityKind::ApplicationMessage, 10, 500),
            (EntityKind::CommitMessage, 0, 250),
        ],
        vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage],
        vec![0, 10],
        vec![(0, 250), (10, 500)]
    )]
    #[case::single_entry(
        vec![(EntityKind::Welcome, 11, 999)],
        vec![EntityKind::Welcome],
        vec![11],
        vec![(11, 999)]
    )]
    #[case::filters_by_entity_kind(
        vec![
            (EntityKind::ApplicationMessage, 5, 1000),
            (EntityKind::CommitMessage, 5, 2000),  // Higher but filtered out
            (EntityKind::Welcome, 5, 3000),        // Highest but filtered out
        ],
        vec![EntityKind::ApplicationMessage],  // Only query ApplicationMessage
        vec![5],
        vec![(5, 1000)]  // Should get ApplicationMessage's value, not others
    )]
    #[case::filters_by_originator(
        vec![
            (EntityKind::ApplicationMessage, 5, 500),
            (EntityKind::ApplicationMessage, 10, 1000),
            (EntityKind::ApplicationMessage, 15, 1500), // Filtered out
        ],
        vec![EntityKind::ApplicationMessage],
        vec![5, 10],  // Don't include 15
        vec![(5, 500), (10, 1000)]  // Should get originator 5 and 10, not 15
    )]
    #[xmtp_common::test]
    async fn latest_cursor_for_id(
        #[case] pre_populate: Vec<(EntityKind, i32, i64)>,
        #[case] query_entities: Vec<EntityKind>,
        #[case] query_originators: Vec<u32>,
        #[case] expected: Vec<(u32, u64)>,
    ) {
        with_connection(|conn| {
            let entity_id = vec![99, 88, 77];

            // Pre-populate states
            for (kind, orig, seq) in pre_populate {
                create_state(conn, &entity_id, kind, orig, seq);
            }

            // Convert to OriginatorId references
            let originator_refs: Vec<&OriginatorId> = query_originators
                .iter()
                .map(|o| o as &OriginatorId)
                .collect();

            // Execute query
            let cursor = conn
                .latest_cursor_for_id(&entity_id, &query_entities, Some(&originator_refs))
                .unwrap();

            // Verify results
            assert_eq!(cursor.len(), expected.len());
            for (expected_orig, expected_seq) in expected {
                assert_eq!(
                    cursor.get(&expected_orig),
                    expected_seq,
                    "Mismatch for originator {}: expected {}, got {}",
                    expected_orig,
                    expected_seq,
                    cursor.get(&expected_orig)
                );
            }
        })
    }

    #[xmtp_common::test]
    fn get_last_cursor_for_ids_empty() {
        with_connection(|conn| {
            let ids: Vec<Vec<u8>> = vec![];
            let entities = vec![EntityKind::ApplicationMessage];
            let result = conn.get_last_cursor_for_ids(&ids, &entities).unwrap();
            assert!(result.is_empty());
        })
    }

    #[xmtp_common::test]
    async fn get_last_cursor_for_ids_single() {
        with_connection(|conn| {
            let id = vec![1, 2, 3];
            let entity_kind = EntityKind::ApplicationMessage;

            // Store a state with originator 10 and sequence_id 456
            create_state(conn, &id, entity_kind, 10, 456);

            // Query for it
            let ids = vec![id.clone()];
            let entities = vec![entity_kind];
            let result = conn.get_last_cursor_for_ids(&ids, &entities).unwrap();

            assert_eq!(result.len(), 1);
            let cursor = result.get(&id).expect("Should have cursor for id");
            assert_eq!(cursor.get(&10), 456);
        })
    }

    #[xmtp_common::test]
    fn get_last_cursor_for_ids_multiple_mixed() {
        with_connection(|conn| {
            let entity_kind = EntityKind::ApplicationMessage;

            // Create some ids with existing state
            let id1 = vec![1, 0, 0];
            let id2 = vec![2, 0, 0];
            let id3 = vec![3, 0, 0];
            let id4 = vec![4, 0, 0]; // This one won't have state

            create_state(conn, &id1, entity_kind, 10, 100);
            create_state(conn, &id2, entity_kind, 10, 200);
            create_state(conn, &id3, entity_kind, 10, 300);

            // Query for all ids including one without state
            let ids = vec![id1.clone(), id2.clone(), id3.clone(), id4.clone()];
            let entities = vec![entity_kind];
            let result = conn.get_last_cursor_for_ids(&ids, &entities).unwrap();

            // Should only return the ones with existing state
            assert_eq!(result.len(), 3);
            assert_eq!(result.get(&id1).unwrap().get(&10), 100);
            assert_eq!(result.get(&id2).unwrap().get(&10), 200);
            assert_eq!(result.get(&id3).unwrap().get(&10), 300);
            assert!(!result.contains_key(&id4));
        })
    }

    #[xmtp_common::test]
    fn get_last_cursor_for_ids_exactly_900() {
        with_connection(|conn| {
            let entity_kind = EntityKind::ApplicationMessage;

            // Create exactly 900 ids
            let mut ids = Vec::new();
            for i in 0..900 {
                let id = vec![(i / 256) as u8, (i % 256) as u8];
                create_state(conn, &id, entity_kind, 10, i as i64);
                ids.push(id);
            }

            // Query for all 900 ids
            let entities = vec![entity_kind];
            let result = conn.get_last_cursor_for_ids(&ids, &entities).unwrap();

            assert_eq!(result.len(), 900);
            for (idx, id) in ids.iter().enumerate() {
                assert_eq!(result.get(id).unwrap().get(&10), idx as u64);
            }
        })
    }

    #[xmtp_common::test]
    fn get_last_cursor_for_ids_over_900() {
        with_connection(|conn| {
            let entity_kind = EntityKind::ApplicationMessage;

            // Create 1000 ids to test chunking
            let mut ids = Vec::new();
            for i in 0..1000 {
                let id = vec![(i / 256) as u8, (i % 256) as u8, 0];
                create_state(conn, &id, entity_kind, 10, i as i64);
                ids.push(id);
            }

            // Query for all 1000 ids (should use 2 chunks)
            let entities = vec![entity_kind];
            let result = conn.get_last_cursor_for_ids(&ids, &entities).unwrap();

            assert_eq!(result.len(), 1000);
            for (idx, id) in ids.iter().enumerate() {
                assert_eq!(
                    result.get(id).unwrap().get(&10),
                    idx as u64,
                    "Mismatch for id at index {}",
                    idx
                );
            }
        })
    }

    #[xmtp_common::test]
    fn get_last_cursor_for_ids_over_1800() {
        with_connection(|conn| {
            let entity_kind = EntityKind::ApplicationMessage;

            // Create 2000 ids to test multiple chunks
            let mut ids = Vec::new();
            for i in 0..2000 {
                let id = vec![(i / 256) as u8, (i % 256) as u8, 1];
                create_state(conn, &id, entity_kind, 10, i as i64);
                ids.push(id);
            }

            // Query for all 2000 ids (should use 3 chunks: 900, 900, 200)
            let entities = vec![entity_kind];
            let result = conn.get_last_cursor_for_ids(&ids, &entities).unwrap();

            assert_eq!(result.len(), 2000);
            for (idx, id) in ids.iter().enumerate() {
                assert_eq!(
                    result.get(id).unwrap().get(&10),
                    idx as u64,
                    "Mismatch for id at index {}",
                    idx
                );
            }
        })
    }

    #[xmtp_common::test]
    fn get_last_cursor_for_ids_different_entity_kinds() {
        with_connection(|conn| {
            let id1 = vec![1, 2, 3];
            let id2 = vec![4, 5, 6];

            // Store same ids with different entity kinds
            create_state(conn, &id1, EntityKind::ApplicationMessage, 10, 100);
            create_state(conn, &id1, EntityKind::Welcome, 10, 200);
            create_state(conn, &id2, EntityKind::ApplicationMessage, 10, 300);

            // Query for ApplicationMessage entity kind only
            let ids = vec![id1.clone(), id2.clone()];
            let result = conn
                .get_last_cursor_for_ids(&ids, &[EntityKind::ApplicationMessage])
                .unwrap();

            assert_eq!(result.len(), 2);
            assert_eq!(result.get(&id1).unwrap().get(&10), 100);
            assert_eq!(result.get(&id2).unwrap().get(&10), 300);

            // Query for Welcome entity kind only
            let result = conn
                .get_last_cursor_for_ids(&ids, &[EntityKind::Welcome])
                .unwrap();

            assert_eq!(result.len(), 1);
            assert_eq!(result.get(&id1).unwrap().get(&10), 200);
            assert!(!result.contains_key(&id2));
        })
    }
}
