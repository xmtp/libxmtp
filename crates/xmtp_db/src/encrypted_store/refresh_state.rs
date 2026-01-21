use std::collections::HashMap;

use diesel::{
    backend::Backend,
    connection::DefaultLoadingMode,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::{BigInt, Binary, Integer},
};
use itertools::Itertools;
use xmtp_configuration::Originators;
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, Topic, TopicKind};

use super::{ConnectionExt, Sqlite, db_connection::DbConnection, schema::refresh_state};
use crate::{StorageError, StoreOrIgnore, impl_store_or_ignore};

allow_columns_to_appear_in_same_group_by_clause!(
    super::schema::identity_updates::originator_id,
    super::schema::identity_updates::sequence_id,
    super::schema::refresh_state::originator_id,
    super::schema::refresh_state::sequence_id
);

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

#[derive(QueryableByName, Selectable)]
#[diesel(check_for_backend(Sqlite), table_name = super::schema::refresh_state)]
struct SingleCursor {
    #[diesel(sql_type = Integer)]
    originator_id: i32,
    #[diesel(sql_type = BigInt)]
    sequence_id: i64,
}

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

    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, GlobalCursor>, StorageError>;

    fn update_cursor<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entity_kind: EntityKind,
        cursor: Cursor,
    ) -> Result<bool, StorageError>;

    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, StorageError>;

    fn lowest_common_cursor_combined(
        &self,
        topics: &[&Topic],
    ) -> Result<GlobalCursor, StorageError>;

    fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError>;

    fn latest_cursor_combined<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError>;

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

    fn get_last_cursor_for_ids<Id: AsRef<[u8]>>(
        &self,
        ids: &[Id],
        entities: &[EntityKind],
    ) -> Result<HashMap<Vec<u8>, GlobalCursor>, StorageError> {
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

    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, StorageError> {
        (**self).lowest_common_cursor(topics)
    }

    fn lowest_common_cursor_combined(
        &self,
        topics: &[&Topic],
    ) -> Result<GlobalCursor, StorageError> {
        (**self).lowest_common_cursor_combined(topics)
    }

    fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError> {
        (**self).latest_cursor_for_id(entity_id, entities, originators)
    }

    fn latest_cursor_combined<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError> {
        (**self).latest_cursor_combined(entity_id, entities, originators)
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
        use super::schema::refresh_state::dsl;

        let id_ref = id.as_ref();

        let originator_ids_i32: Vec<i32> = originator_ids.iter().map(|o| *o as i32).collect();
        let found_states: Vec<RefreshState> = self.raw_query_read(|conn| {
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

        let map = self.raw_query_read(|conn| {
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
                    Originators::REMOTE_COMMIT_LOG,
                )
                .unwrap_or_default();
            cursor_map.insert(conversation_id.to_vec(), cursor);
        }
        Ok(cursor_map)
    }

    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, StorageError> {
        use super::schema::identity_updates::dsl as idsl;
        use super::schema::refresh_state::dsl as rdsl;

        // diesel does not support eq_any (IN) on tuple types.
        // so, something like `.filter((dsl::entity_id, dsl::entity_kind).eq_any(entities))` will not compile. its possible to implement
        // with a custom QueryFragment, but maybe that's a future
        // exercise. ref: https://github.com/diesel-rs/diesel/issues/3222#issuecomment-2079474318
        // it also does not support group_by on boxed queries
        let entities = topics
            .iter()
            .flat_map(|t| match t.kind() {
                TopicKind::GroupMessagesV1 => {
                    vec![
                        (t.identifier().to_vec(), EntityKind::ApplicationMessage),
                        (t.identifier().to_vec(), EntityKind::CommitMessage),
                    ]
                }
                TopicKind::WelcomeMessagesV1 => {
                    vec![(t.identifier().to_vec(), EntityKind::Welcome)]
                }
                TopicKind::IdentityUpdatesV1 | TopicKind::KeyPackagesV1 | _ => vec![],
            })
            .collect::<Vec<_>>();

        let identity_inbox_ids: Vec<String> = topics
            .iter()
            .filter_map(|t| Topic::identity_updates(t))
            .map(|t| hex::encode(t.identifier()))
            .collect();

        let mut refresh = rdsl::refresh_state
            .select((rdsl::originator_id, rdsl::sequence_id))
            .filter(rdsl::entity_kind.eq(-1)) // Start with a query that will never return any results
            .into_boxed();
        for (entity_id, entity_kind) in &entities {
            refresh = refresh.or_filter(
                rdsl::entity_id
                    .eq(entity_id)
                    .and(rdsl::entity_kind.eq(entity_kind)),
            );
        }

        let identity = idsl::identity_updates
            .select((idsl::originator_id, idsl::sequence_id))
            .filter(idsl::inbox_id.eq_any(identity_inbox_ids))
            .into_boxed();
        let cursor = self.raw_query_read(|conn| {
            refresh
                .select((rdsl::originator_id, rdsl::sequence_id))
                .union_all(identity)
                .load_iter::<(i32, i64), DefaultLoadingMode>(conn)?
                .map_ok(|(o, s)| (o as u32, s as u64))
                .process_results(|iter| iter.into_grouping_map().min())
        })?;

        Ok(GlobalCursor::with_hashmap(cursor))
    }

    // _NOTE:_ TEMP until reliable streams
    // and cursor can be updated from streams
    fn lowest_common_cursor_combined(
        &self,
        topics: &[&Topic],
    ) -> Result<GlobalCursor, StorageError> {
        // Build entities list from topics, including both refresh_state entries and group_messages
        let entities = topics
            .iter()
            .flat_map(|t| match t.kind() {
                TopicKind::GroupMessagesV1 => {
                    vec![
                        (t.identifier().to_vec(), EntityKind::ApplicationMessage),
                        (t.identifier().to_vec(), EntityKind::CommitMessage),
                    ]
                }
                TopicKind::WelcomeMessagesV1 => {
                    vec![(t.identifier().to_vec(), EntityKind::Welcome)]
                }
                TopicKind::IdentityUpdatesV1 | TopicKind::KeyPackagesV1 | _ => vec![],
            })
            .collect::<Vec<_>>();

        // Collect identity update inbox IDs
        let identity_inbox_ids: Vec<String> = topics
            .iter()
            .filter_map(|t| match t.kind() {
                TopicKind::IdentityUpdatesV1 => Some(hex::encode(t.identifier())),
                _ => None,
            })
            .collect();

        let has_identity_updates = !identity_inbox_ids.is_empty();
        let has_entities = !entities.is_empty();

        if !has_entities && !has_identity_updates {
            return Ok(GlobalCursor::default());
        }

        let mut query_parts = Vec::new();

        // Add refresh_state and group_messages parts if we have entities
        if has_entities {
            let placeholders = entities
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");

            query_parts.push(format!(
                "SELECT originator_id, sequence_id
                FROM refresh_state
                WHERE (entity_id, entity_kind) IN ({})",
                placeholders
            ));

            query_parts.push(format!(
                "SELECT originator_id, sequence_id
                FROM conversation_list
                WHERE (id, CASE message_kind
                    WHEN 1 THEN 2  -- GroupMessageKind::Application -> EntityKind::ApplicationMessage
                    WHEN 2 THEN 7  -- GroupMessageKind::MembershipChange -> EntityKind::CommitMessage
                END) IN ({})",
                placeholders
            ));
        }

        // Add identity_updates part if we have inbox IDs
        if has_identity_updates {
            let inbox_placeholders = identity_inbox_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            query_parts.push(format!(
                "SELECT originator_id, sequence_id
                FROM identity_updates
                WHERE inbox_id IN ({})",
                inbox_placeholders
            ));
        }

        // Build a query that unions all sources, then finds MIN per originator
        let query = format!(
            "SELECT originator_id, MIN(sequence_id) AS sequence_id
            FROM ({})
            GROUP BY originator_id",
            query_parts.join(" UNION ALL ")
        );

        let cursor = self.raw_query_read(|conn| {
            let mut q = diesel::sql_query(query).into_boxed();

            if has_entities {
                // Bind entity_id and entity_kind pairs for refresh_state
                for (id, kind) in &entities {
                    q = q.bind::<Binary, _>(id);
                    q = q.bind::<Integer, _>(*kind);
                }

                // Bind entity_id and entity_kind pairs for group_messages
                for (id, kind) in &entities {
                    q = q.bind::<Binary, _>(id);
                    q = q.bind::<Integer, _>(*kind);
                }
            }

            // Bind identity_updates parameters
            if has_identity_updates {
                for inbox_id in identity_inbox_ids {
                    q = q.bind::<diesel::sql_types::Text, _>(inbox_id);
                }
            }

            q.load_iter::<SingleCursor, DefaultLoadingMode>(conn)?
                .map_ok(|c| (c.originator_id as u32, c.sequence_id as u64))
                .collect::<QueryResult<GlobalCursor>>()
        })?;

        Ok(cursor)
    }

    fn latest_cursor_for_id<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError> {
        use super::schema::refresh_state::dsl;
        use diesel::dsl::max;

        let entity_ref = entity_id.as_ref();

        let cursor_map = self.raw_query_read(|conn| {
            // Build base query with entity_id and entity_kind filters
            let base_query = dsl::refresh_state
                .filter(dsl::entity_id.eq(entity_ref))
                .filter(dsl::entity_kind.eq_any(entities));

            // Add originator filter if provided, then group and select
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

    // _NOTE:_ TEMP until reliable streams
    // and cursor can be updated from streams
    fn latest_cursor_combined<Id: AsRef<[u8]>>(
        &self,
        entity_id: Id,
        entities: &[EntityKind],
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, StorageError> {
        let entity_ref = entity_id.as_ref();

        // Build entity_kind placeholders for refresh_state
        let entity_kind_placeholders = entities.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

        // Build a query that unions refresh_state and group_messages
        let mut query = format!(
            "SELECT originator_id, MAX(sequence_id) AS sequence_id
            FROM (
                SELECT originator_id, sequence_id
                FROM refresh_state
                WHERE entity_id = ? AND entity_kind IN ({})
                UNION ALL
                SELECT originator_id, sequence_id
                FROM group_messages
                WHERE group_id = ? AND kind IN (",
            entity_kind_placeholders
        );

        // Map EntityKind to GroupMessageKind
        let group_message_kinds: Vec<i32> = entities
            .iter()
            .filter_map(|e| match e {
                EntityKind::ApplicationMessage => Some(1), // GroupMessageKind::Application
                EntityKind::CommitMessage => Some(2),      // GroupMessageKind::MembershipChange
                _ => None,
            })
            .collect();

        // Add placeholders for group_message kinds
        let kind_placeholders = group_message_kinds
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        query.push_str(&kind_placeholders);
        query.push(')');

        // Add originator filter if provided
        if let Some(oids) = originators {
            let originator_placeholders = oids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            query.push_str(&format!(
                "
            ) WHERE originator_id IN ({})
            GROUP BY originator_id",
                originator_placeholders
            ));
        } else {
            query.push_str(
                "
            ) GROUP BY originator_id",
            );
        }

        let cursor_map = self.raw_query_read(|conn| {
            let mut q = diesel::sql_query(query).into_boxed();

            // Bind entity_id for refresh_state
            q = q.bind::<Binary, _>(entity_ref);

            // Bind entity_kinds for refresh_state
            for kind in entities {
                q = q.bind::<Integer, _>(*kind);
            }

            // Bind group_id for group_messages
            q = q.bind::<Binary, _>(entity_ref);

            // Bind group_message_kinds for group_messages
            for kind in &group_message_kinds {
                q = q.bind::<Integer, _>(*kind);
            }

            // Bind originators if provided
            if let Some(oids) = originators {
                for oid in oids {
                    q = q.bind::<Integer, _>(**oid as i32);
                }
            }

            q.load_iter::<SingleCursor, DefaultLoadingMode>(conn)?
                .map_ok(|c| (c.originator_id as u32, c.sequence_id as u64))
                .collect::<QueryResult<GlobalCursor>>()
        })?;

        Ok(cursor_map)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::identity_update::StoredIdentityUpdateBuilder;
    use crate::test_utils::with_connection;
    use crate::{Store, StoreOrIgnore};
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

    // Helper function to create and store a RefreshState
    fn create_identity_update<C: ConnectionExt>(
        conn: &DbConnection<C>,
        originator_id: i32,
        sequence_id: i64,
    ) {
        StoredIdentityUpdateBuilder::default()
            .inbox_id(xmtp_common::rand_string::<32>())
            .sequence_id(sequence_id)
            .originator_id(originator_id)
            .payload(xmtp_common::rand_vec::<32>())
            .server_timestamp_ns(xmtp_common::rand_i64())
            .build()
            .unwrap()
            .store(conn)
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
    #[case::finds_maximum_per_originator(
        vec![
            (EntityKind::ApplicationMessage, 5, 100),  // Originator 5, ApplicationMessage
            (EntityKind::CommitMessage, 5, 150),       // Originator 5, CommitMessage (higher)
            (EntityKind::ApplicationMessage, 10, 500), // Originator 10
            (EntityKind::CommitMessage, 0, 250),       // Originator 0
        ],
        vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage],
        vec![0, 5, 10],
        vec![(0, 250), (5, 150), (10, 500)] // Expected: max per originator across entity kinds
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

    #[rstest]
    #[case::single_topic_minimium(
        vec![
            (vec![1, 1, 1], EntityKind::ApplicationMessage, 200, 127),
            (vec![1, 1, 1], EntityKind::CommitMessage, 0, 115),
            (vec![1, 1, 1], EntityKind::CommitLogDownload, 100, 0),
            (vec![1, 1, 1], EntityKind::CommitLogUpload, 100, 2),
            (vec![1, 1, 1], EntityKind::CommitLogForkCheckLocal, 100, 0),
            (vec![1, 1, 1], EntityKind::CommitLogForkCheckRemote, 100, 0)
        ],
        vec![
            TopicKind::GroupMessagesV1.create(vec![1, 1, 1]),
        ],
        vec![(200, 127), (0, 115)]  // MIN across both topics: min(min(100, 150), min(50, 75)) = 50
    )]
    #[case::multiple_topics_finds_minimum(
        vec![
            (vec![1, 1, 1], EntityKind::ApplicationMessage, 0, 100),
            (vec![1, 1, 1], EntityKind::CommitMessage, 0, 150),
            (vec![2, 2, 2], EntityKind::ApplicationMessage, 0, 50),  // Lower value in topic 2
            (vec![2, 2, 2], EntityKind::CommitMessage, 0, 75),
        ],
        vec![
            TopicKind::GroupMessagesV1.create(vec![1, 1, 1]),
            TopicKind::GroupMessagesV1.create(vec![2, 2, 2]),
        ],
        vec![(0, 50)]  // MIN across both topics: min(min(100, 150), min(50, 75)) = 50
    )]
    #[case::multiple_topics_different_originators(
        vec![
            (vec![3, 3, 3], EntityKind::ApplicationMessage, 5, 500),
            (vec![3, 3, 3], EntityKind::CommitMessage, 5, 600),
            (vec![4, 4, 4], EntityKind::ApplicationMessage, 10, 1000),
            (vec![4, 4, 4], EntityKind::CommitMessage, 10, 1100),
            (vec![4, 4, 4], EntityKind::ApplicationMessage, 5, 300),  // Lower value for originator 5
        ],
        vec![
            TopicKind::GroupMessagesV1.create(vec![3, 3, 3]),
            TopicKind::GroupMessagesV1.create(vec![4, 4, 4]),
        ],
        vec![(5, 300), (10, 1000)]  // MIN for each originator across topics
    )]
    #[case::mixed_group_and_welcome_topics(
        vec![
            (vec![6, 6, 6], EntityKind::ApplicationMessage, 0, 100),
            (vec![6, 6, 6], EntityKind::CommitMessage, 0, 150),
            (vec![7, 7, 7], EntityKind::Welcome, 0, 50),  // Lower value for originator 0
            (vec![7, 7, 7], EntityKind::Welcome, 10, 200),
        ],
        vec![
            TopicKind::GroupMessagesV1.create(vec![6, 6, 6]),
            TopicKind::WelcomeMessagesV1.create(vec![7, 7, 7]),
        ],
        vec![(0, 50), (10, 200)]  // MIN across different entity kinds
    )]
    #[case::originator_in_some_topics_only(
        vec![
            (vec![8, 8, 8], EntityKind::ApplicationMessage, 5, 100),
            (vec![8, 8, 8], EntityKind::CommitMessage, 5, 200),
            (vec![9, 9, 9], EntityKind::ApplicationMessage, 10, 300),
            (vec![9, 9, 9], EntityKind::CommitMessage, 10, 400),
        ],
        vec![
            TopicKind::GroupMessagesV1.create(vec![8, 8, 8]),
            TopicKind::GroupMessagesV1.create(vec![9, 9, 9]),
        ],
        vec![(5, 100), (10, 300)]  // Each originator appears in only one topic
    )]
    #[xmtp_common::test]
    async fn lowest_common_cursor_scenarios(
        #[case] pre_populate: Vec<(Vec<u8>, EntityKind, i32, i64)>,
        #[case] query_topics: Vec<xmtp_proto::types::Topic>,
        #[case] expected: Vec<(u32, u64)>,
    ) {
        with_connection(|conn| {
            // Pre-populate states
            for (entity_id, kind, orig, seq) in pre_populate {
                create_state(conn, &entity_id, kind, orig, seq);
            }

            // Execute query
            let topic_refs: Vec<&xmtp_proto::types::Topic> = query_topics.iter().collect();
            let cursor = conn.lowest_common_cursor(&topic_refs).unwrap();

            // Verify results
            assert_eq!(
                cursor.len(),
                expected.len(),
                "Expected {} originators, got {}",
                expected.len(),
                cursor.len()
            );
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
    fn lowest_common_cursor_empty_topics() {
        with_connection(|conn| {
            create_state(conn, &[1, 2, 3], EntityKind::ApplicationMessage, 0, 100);
            create_identity_update(conn, 1, 100);
            let result = conn.lowest_common_cursor(&[]);
            match result {
                Ok(cursor) => {
                    tracing::info!("{:?}", cursor);
                    assert_eq!(cursor.len(), 0, "Empty topics should return empty cursor");
                }
                Err(_e) => {
                    // Also acceptable to return an error for empty topics
                }
            }
        })
    }

    #[xmtp_common::test]
    fn lowest_common_cursor_no_matching_states() {
        with_connection(|conn| {
            let topics = [
                TopicKind::GroupMessagesV1.create(vec![99, 99, 99]),
                TopicKind::WelcomeMessagesV1.create(vec![88, 88, 88]),
                TopicKind::IdentityUpdatesV1.create(b"test inbox"),
                TopicKind::IdentityUpdatesV1.create(b"inbox test 2"),
            ];
            let topic_refs: Vec<&xmtp_proto::types::Topic> = topics.iter().collect();
            create_identity_update(conn, 1, 100);
            let cursor = conn.lowest_common_cursor(&topic_refs).unwrap();
            assert_eq!(cursor.len(), 0);
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
