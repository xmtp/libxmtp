#[cfg(feature = "sqlite")]
use super::{ConnectionExt, db_connection::DbConnection};
use crate::icebox::types::IceboxOrphans;
#[cfg(feature = "sqlite")]
use crate::icebox::types::IceboxWithDep;
#[cfg(feature = "sqlite")]
use crate::schema::icebox::dsl;
#[cfg(feature = "sqlite")]
use crate::schema::icebox_dependencies;
#[cfg(feature = "sqlite")]
use crate::{impl_store, schema::icebox};
#[cfg(feature = "sqlite")]
use diesel::prelude::*;
#[cfg(feature = "sqlite")]
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use xmtp_proto::types::{
    Cursor, GroupId, OriginatorId, OrphanedEnvelope, OrphanedEnvelopeBuilder, SequenceId,
};

mod types;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, xmtp_macro::PgModel)]
#[xmtp(table = "icebox")]
#[cfg_attr(
    feature = "sqlite",
    derive(Insertable, Identifiable, Queryable, QueryableByName)
)]
#[cfg_attr(feature = "sqlite", diesel(table_name = icebox))]
#[cfg_attr(feature = "sqlite", diesel(primary_key(originator_id, sequence_id)))]
#[cfg_attr(feature = "sqlite", diesel(belongs_to(crate::group::StoredGroup, foreign_key = group_id)))]
pub struct Icebox {
    pub originator_id: i64,
    pub sequence_id: i64,
    pub group_id: GroupId,
    pub envelope_payload: Vec<u8>,
}

#[cfg(feature = "sqlite")]
impl_store!(Icebox, icebox);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, xmtp_macro::PgModel)]
#[xmtp(table = "icebox_dependencies")]
#[cfg_attr(
    feature = "sqlite",
    derive(Insertable, Identifiable, Queryable, QueryableByName)
)]
#[cfg_attr(feature = "sqlite", diesel(table_name = icebox_dependencies))]
#[cfg_attr(
    feature = "sqlite",
    diesel(primary_key(
        envelope_originator_id,
        envelope_sequence_id,
        dependency_originator_id,
        dependency_sequence_id
    ))
)]
pub struct IceboxDependency {
    pub envelope_originator_id: i64,
    pub envelope_sequence_id: i64,
    pub dependency_originator_id: i64,
    pub dependency_sequence_id: i64,
}

#[cfg(feature = "sqlite")]
impl_store!(IceboxDependency, icebox_dependencies);

pub trait QueryIcebox {
    /// Returns the envelopes (if they exist) plus all their dependencies, and
    /// dependencies of dependencies, along with each envelope's own dependencies.
    /// This could be useful for resolving issues where a commit that could have been
    /// processed, was accidentally committed to the icebox.
    /// Generally, if an envelope has a dependency on something in the icebox already
    /// it means its dependency could not be processed, so it must also be iceboxed.
    fn past_dependents(
        &self,
        cursors: &[Cursor],
    ) -> impl std::future::Future<Output = Result<Vec<OrphanedEnvelope>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Returns envelopes that depend on any of the specified cursors,
    /// along with each envelope's own dependencies.
    /// Does not return the cursors themselves, if they exist in the chain.
    fn future_dependents(
        &self,
        cursors: &[Cursor],
    ) -> impl std::future::Future<Output = Result<Vec<OrphanedEnvelope>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// cache the orphans until its parent(s) may be found.
    fn ice(
        &self,
        orphans: Vec<OrphanedEnvelope>,
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;

    /// Removes icebox entries that have been processed according to refresh_state.
    /// Deletes entries where the refresh_state cursor for the group is at or beyond
    /// the icebox entry's sequence_id, indicating the envelope has been processed.
    fn prune_icebox(
        &self,
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;
}

impl<T> QueryIcebox for &T
where
    T: QueryIcebox + xmtp_common::MaybeSync,
{
    async fn past_dependents(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        (**self).past_dependents(cursors).await
    }

    async fn future_dependents(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        (**self).future_dependents(cursors).await
    }

    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<usize, crate::ConnectionError> {
        (**self).ice(orphans).await
    }

    async fn prune_icebox(&self) -> Result<usize, crate::ConnectionError> {
        (**self).prune_icebox().await
    }
}

#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> DbConnection<C> {
    fn do_icebox_query(
        &self,
        query_str: String,
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::sql_query(query_str)
                .load_iter::<IceboxWithDep, _>(conn)?
                .process_results(|iter| {
                    // since we're using load_iter
                    // to optimize, we load a *const [u8] into `IceboxWithDep` for group_id and
                    // envelope_payload, cloning it only once in `fold_with`.
                    // as long as we are in the scope of `load_iter` (attached to the lifetime of
                    // `conn` or `&mut SqliteConnection` within `raw_query`) the lifetime of group_id and
                    // envelope_payload is safe.
                    // the other raw pointers are safe as long as they aren't accessed once
                    // iteration ends, which is guaranteed by the end of grouping operation and
                    // conversion to `OrphanedEnvelope` type.
                    // diesel `Vec<u8>` deserialization implementation for reference:
                    // https://github.com/diesel-rs/diesel/blob/0abaf1b3f2ed24ac5643227baf841da9a63d9f1f/diesel/src/type_impls/primitives.rs#L164
                    iter.into_grouping_map_by(|row| (row.originator_id, row.sequence_id))
                        .fold_with(
                            |_key, row| {
                                let mut builder = OrphanedEnvelopeBuilder::default();
                                // safe b/c we are within the lifetime of `row_iter`
                                // so the slice in sqlites memory still exists
                                // and is immediately copied to a `Vec<u8>`.
                                let group_id = unsafe { row.group_id() };
                                let payload = unsafe { row.envelope_payload() };
                                builder
                                    .cursor(Cursor::new(
                                        row.sequence_id as SequenceId,
                                        row.originator_id as OriginatorId,
                                    ))
                                    .payload(payload);
                                if let Ok(gid) = GroupId::try_from(group_id) {
                                    builder.group_id(gid);
                                }
                                builder
                            },
                            |mut acc, _key, row| {
                                acc.depending_on(Cursor::new(
                                    row.dependency_sequence_id as SequenceId,
                                    row.dependency_originator_id as OriginatorId,
                                ));
                                acc
                            },
                        )
                        .into_values()
                        .map(|v| v.build())
                        .try_collect()
                        .map_err(|e| diesel::result::Error::DeserializationError(Box::new(e) as _))
                })?
        })
    }
}

#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> QueryIcebox for DbConnection<C> {
    async fn past_dependents(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        if cursors.is_empty() {
            return Ok(Vec::new());
        }

        let values_clause = cursors
            .iter()
            .map(|c| format!("({}, {})", c.originator_id, c.sequence_id))
            .join(", ");

        let query_str = format!(
            r#"
            WITH RECURSIVE
            start_cursors(originator_id, sequence_id) AS (
                VALUES {}
            ),
            dependency_chain AS (
                -- Base case: Start with the specified envelopes if they exist
                SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN start_cursors sc ON i.originator_id = sc.originator_id
                                      AND i.sequence_id = sc.sequence_id

                UNION

                -- OR start with their immediate dependencies if they don't
                SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.originator_id = d.dependency_originator_id
                                           AND i.sequence_id = d.dependency_sequence_id
                JOIN start_cursors sc ON d.envelope_originator_id = sc.originator_id
                                      AND d.envelope_sequence_id = sc.sequence_id

                UNION ALL

                -- Recursive case: Continue traversing the dependency chain
                SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.originator_id = d.dependency_originator_id
                                           AND i.sequence_id = d.dependency_sequence_id
                JOIN dependency_chain dc ON d.envelope_originator_id = dc.originator_id
                                         AND d.envelope_sequence_id = dc.sequence_id
            )
            SELECT
                dc.originator_id,
                dc.sequence_id,
                dc.group_id,
                dc.envelope_payload,
                d.dependency_originator_id,
                d.dependency_sequence_id
            FROM (SELECT DISTINCT * FROM dependency_chain) dc
            INNER JOIN icebox_dependencies d
                ON dc.originator_id = d.envelope_originator_id
                AND dc.sequence_id = d.envelope_sequence_id
            ORDER BY dc.originator_id DESC, dc.sequence_id DESC
            "#,
            values_clause
        );

        self.do_icebox_query(query_str)
    }

    async fn future_dependents(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        if cursors.is_empty() {
            return Ok(Vec::new());
        }

        // Build the VALUES clause with actual values (safe since they're i64)
        let values_clause = cursors
            .iter()
            .map(|c| format!("({}, {})", c.originator_id, c.sequence_id))
            .join(", ");

        let query_str = format!(
            r#"
            WITH RECURSIVE
            start_cursors(originator_id, sequence_id) AS (
                VALUES {}
            ),
            dependency_chain AS (
                -- Base case: Find all immediate dependents from any starting cursor
                SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.originator_id = d.envelope_originator_id
                                           AND i.sequence_id = d.envelope_sequence_id
                JOIN start_cursors sc ON d.dependency_originator_id = sc.originator_id
                                      AND d.dependency_sequence_id = sc.sequence_id

                UNION ALL

                -- Recursive case: Continue traversing the dependent chain
                SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.originator_id = d.envelope_originator_id
                                           AND i.sequence_id = d.envelope_sequence_id
                JOIN dependency_chain dc ON d.dependency_originator_id = dc.originator_id
                                         AND d.dependency_sequence_id = dc.sequence_id
            )
            SELECT
                dc.originator_id,
                dc.sequence_id,
                dc.group_id,
                dc.envelope_payload,
                d.dependency_originator_id,
                d.dependency_sequence_id
            FROM dependency_chain dc
            INNER JOIN icebox_dependencies d
                ON dc.originator_id = d.envelope_originator_id
                AND dc.sequence_id = d.envelope_sequence_id
            "#,
            values_clause
        );

        self.do_icebox_query(query_str)
    }

    async fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<usize, crate::ConnectionError> {
        if orphans.is_empty() {
            return Ok(0);
        }
        self.raw_query(|conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let mut total = 0;

                for orphan in &orphans {
                    let inserted = diesel::insert_into(dsl::icebox)
                        .values(Icebox::from(orphan.clone()))
                        .on_conflict_do_nothing()
                        .execute(conn)?;
                    total += inserted;
                }

                let dependencies = orphans.iter().flat_map(|o| o.deps()).collect::<Vec<_>>();
                for dep in dependencies {
                    let inserted = diesel::insert_into(icebox_dependencies::table)
                        .values(dep)
                        .on_conflict_do_nothing()
                        .execute(conn)?;
                    total += inserted;
                }

                Ok(total)
            })
        })
    }

    async fn prune_icebox(&self) -> Result<usize, crate::ConnectionError> {
        use super::refresh_state::EntityKind;
        use super::schema::{icebox, refresh_state};

        self.raw_query(|conn| {
            diesel::delete(
                icebox::table.filter(diesel::dsl::exists(
                    refresh_state::table
                        .filter(refresh_state::entity_id.eq(icebox::group_id))
                        .filter(
                            refresh_state::originator_id
                                .cast::<diesel::sql_types::BigInt>()
                                .eq(icebox::originator_id),
                        )
                        .filter(refresh_state::sequence_id.ge(icebox::sequence_id))
                        .filter(
                            refresh_state::entity_kind.eq_any(&[
                                EntityKind::ApplicationMessage,
                                EntityKind::CommitMessage,
                            ]),
                        ),
                )),
            )
            .execute(conn)
        })
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sqlite")`.
///
/// The two things that made this trait "not a mechanical port" both dissolve
/// here rather than needing a translation:
///
/// * the sync path interpolates the start cursors into a `VALUES` clause,
///   because diesel's `sql_query` cannot bind a variable-length list. Postgres
///   takes the whole set as two array parameters through `UNNEST`, so the SQL
///   is a constant and nothing is formatted into it.
/// * `IceboxWithDep` reads `group_id` and `envelope_payload` through raw
///   pointers into SQLite's memory to avoid a copy inside diesel's `load_iter`.
///   sqlx returns owned rows, so the Postgres backend decodes straight into
///   `Vec<u8>` and the `unsafe` has nothing to buy.
#[cfg(all(feature = "sqlx", not(feature = "sqlite"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::PgDb;
    use std::collections::HashMap;

    /// One flat `(envelope, dependency)` pair, before grouping.
    type DependencyRow = (i64, i64, GroupId, Vec<u8>, i64, i64);

    /// The recursive walk, shared by both directions: only the CTE differs.
    ///
    /// Both statements return the envelope columns repeated once per
    /// dependency, so the rows are grouped back into one `OrphanedEnvelope` per
    /// `(originator_id, sequence_id)`.
    fn collect(rows: Vec<DependencyRow>) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        // Insertion-ordered, so the statement's ORDER BY survives into the
        // result. The sync path groups through a `HashMap` and loses it.
        let mut order: Vec<(i64, i64)> = Vec::new();
        let mut builders: HashMap<(i64, i64), OrphanedEnvelopeBuilder> = HashMap::new();

        for (originator_id, sequence_id, group_id, payload, dep_originator, dep_sequence) in rows {
            let builder = builders
                .entry((originator_id, sequence_id))
                .or_insert_with(|| {
                    order.push((originator_id, sequence_id));
                    let mut builder = OrphanedEnvelopeBuilder::default();
                    builder
                        .cursor(Cursor::new(
                            sequence_id as SequenceId,
                            originator_id as OriginatorId,
                        ))
                        .payload(payload)
                        .group_id(group_id);
                    builder
                });
            builder.depending_on(Cursor::new(
                dep_sequence as SequenceId,
                dep_originator as OriginatorId,
            ));
        }

        order
            .into_iter()
            .filter_map(|key| builders.remove(&key))
            .map(|builder| {
                builder
                    .build()
                    .map_err(|e| crate::ConnectionError::InvalidQuery(e.to_string()))
            })
            .collect()
    }

    /// `(originator_ids, sequence_ids)` as the two arrays `UNNEST` pairs back up.
    fn split(cursors: &[Cursor]) -> (Vec<i64>, Vec<i64>) {
        cursors
            .iter()
            .map(|c| (c.originator_id as i64, c.sequence_id as i64))
            .unzip()
    }

    /// Walks *towards* the dependencies of the given cursors, so the outer
    /// `SELECT DISTINCT` is needed: the two base cases can both reach the same
    /// envelope.
    const PAST_DEPENDENTS: &str = "
        WITH RECURSIVE
        start_cursors(originator_id, sequence_id) AS (
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[])
        ),
        dependency_chain AS (
            -- Base case: the specified envelopes, if they are iceboxed
            SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
            FROM icebox i
            JOIN start_cursors sc ON i.originator_id = sc.originator_id
                                  AND i.sequence_id = sc.sequence_id

            UNION

            -- ...or their immediate dependencies, if they are not
            SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
            FROM icebox i
            JOIN icebox_dependencies d ON i.originator_id = d.dependency_originator_id
                                       AND i.sequence_id = d.dependency_sequence_id
            JOIN start_cursors sc ON d.envelope_originator_id = sc.originator_id
                                  AND d.envelope_sequence_id = sc.sequence_id

            UNION ALL

            -- Recursive case: keep walking down the dependency chain
            SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
            FROM icebox i
            JOIN icebox_dependencies d ON i.originator_id = d.dependency_originator_id
                                       AND i.sequence_id = d.dependency_sequence_id
            JOIN dependency_chain dc ON d.envelope_originator_id = dc.originator_id
                                     AND d.envelope_sequence_id = dc.sequence_id
        )
        SELECT dc.originator_id, dc.sequence_id, dc.group_id, dc.envelope_payload,
               d.dependency_originator_id, d.dependency_sequence_id
        FROM (SELECT DISTINCT * FROM dependency_chain) dc
        INNER JOIN icebox_dependencies d
            ON dc.originator_id = d.envelope_originator_id
           AND dc.sequence_id = d.envelope_sequence_id
        ORDER BY dc.originator_id DESC, dc.sequence_id DESC";

    /// Walks *away* from the given cursors, to everything blocked behind them.
    /// The cursors themselves are never returned, only their dependents.
    const FUTURE_DEPENDENTS: &str = "
        WITH RECURSIVE
        start_cursors(originator_id, sequence_id) AS (
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[])
        ),
        dependency_chain AS (
            -- Base case: everything directly depending on a starting cursor
            SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
            FROM icebox i
            JOIN icebox_dependencies d ON i.originator_id = d.envelope_originator_id
                                       AND i.sequence_id = d.envelope_sequence_id
            JOIN start_cursors sc ON d.dependency_originator_id = sc.originator_id
                                  AND d.dependency_sequence_id = sc.sequence_id

            UNION ALL

            -- Recursive case: keep walking up the dependent chain
            SELECT i.originator_id, i.sequence_id, i.group_id, i.envelope_payload
            FROM icebox i
            JOIN icebox_dependencies d ON i.originator_id = d.envelope_originator_id
                                       AND i.sequence_id = d.envelope_sequence_id
            JOIN dependency_chain dc ON d.dependency_originator_id = dc.originator_id
                                     AND d.dependency_sequence_id = dc.sequence_id
        )
        SELECT dc.originator_id, dc.sequence_id, dc.group_id, dc.envelope_payload,
               d.dependency_originator_id, d.dependency_sequence_id
        FROM dependency_chain dc
        INNER JOIN icebox_dependencies d
            ON dc.originator_id = d.envelope_originator_id
           AND dc.sequence_id = d.envelope_sequence_id";

    impl PgDb {
        async fn icebox_walk(
            &self,
            sql: &str,
            cursors: &[Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
            if cursors.is_empty() {
                return Ok(Vec::new());
            }
            let (originator_ids, sequence_ids) = split(cursors);
            let mut c = self.conn().await?;
            let rows: Vec<DependencyRow> = sqlx::query_as(sql)
                .bind(&originator_ids)
                .bind(&sequence_ids)
                .fetch_all(&mut *c)
                .await?;
            collect(rows)
        }
    }

    impl QueryIcebox for PgDb {
        async fn past_dependents(
            &self,
            cursors: &[Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
            self.icebox_walk(PAST_DEPENDENTS, cursors).await
        }

        async fn future_dependents(
            &self,
            cursors: &[Cursor],
        ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
            self.icebox_walk(FUTURE_DEPENDENTS, cursors).await
        }

        /// Two bulk inserts rather than the sync path's row-at-a-time loop, and
        /// `atomic()` for the same reason it opens a transaction: an envelope
        /// stored without its dependency rows would look ready to process.
        ///
        /// `ON CONFLICT DO NOTHING` also covers duplicates *within* a batch,
        /// which is why the row counts still match the sync path's.
        async fn ice(
            &self,
            orphans: Vec<OrphanedEnvelope>,
        ) -> Result<usize, crate::ConnectionError> {
            if orphans.is_empty() {
                return Ok(0);
            }

            let dependencies: Vec<IceboxDependency> =
                orphans.iter().flat_map(|o| o.deps()).collect();
            let entries: Vec<Icebox> = orphans.into_iter().map(Icebox::from).collect();

            let mut originator_ids = Vec::with_capacity(entries.len());
            let mut sequence_ids = Vec::with_capacity(entries.len());
            // `GroupId` has no `PgHasArrayType`, so the array element type is the
            // raw `bytea` the column already stores.
            let mut group_ids: Vec<Vec<u8>> = Vec::with_capacity(entries.len());
            let mut payloads = Vec::with_capacity(entries.len());
            for entry in entries {
                originator_ids.push(entry.originator_id);
                sequence_ids.push(entry.sequence_id);
                group_ids.push(entry.group_id.to_vec());
                payloads.push(entry.envelope_payload);
            }

            let mut dep_envelope_originators = Vec::with_capacity(dependencies.len());
            let mut dep_envelope_sequences = Vec::with_capacity(dependencies.len());
            let mut dep_originators = Vec::with_capacity(dependencies.len());
            let mut dep_sequences = Vec::with_capacity(dependencies.len());
            for dep in dependencies {
                dep_envelope_originators.push(dep.envelope_originator_id);
                dep_envelope_sequences.push(dep.envelope_sequence_id);
                dep_originators.push(dep.dependency_originator_id);
                dep_sequences.push(dep.dependency_sequence_id);
            }

            self.atomic(async |db| {
                let mut total = {
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "INSERT INTO icebox (originator_id, sequence_id, group_id, envelope_payload) \
                         SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bytea[], $4::bytea[]) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&originator_ids)
                    .bind(&sequence_ids)
                    .bind(&group_ids)
                    .bind(&payloads)
                    .execute(&mut *c)
                    .await?
                    .rows_affected()
                };

                if !dep_envelope_originators.is_empty() {
                    let mut c = db.conn().await?;
                    total += sqlx::query(
                        "INSERT INTO icebox_dependencies \
                         (envelope_originator_id, envelope_sequence_id, \
                          dependency_originator_id, dependency_sequence_id) \
                         SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::bigint[], $4::bigint[]) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&dep_envelope_originators)
                    .bind(&dep_envelope_sequences)
                    .bind(&dep_originators)
                    .bind(&dep_sequences)
                    .execute(&mut *c)
                    .await?
                    .rows_affected();
                }

                Ok(total as usize)
            })
            .await
        }

        /// `refresh_state.originator_id` is `INTEGER` and `icebox.originator_id`
        /// is `BIGINT`; Postgres compares the two widths directly, so unlike the
        /// sync path there is no cast.
        async fn prune_icebox(&self) -> Result<usize, crate::ConnectionError> {
            use crate::encrypted_store::refresh_state::EntityKind;

            let kinds: Vec<i32> = vec![
                EntityKind::ApplicationMessage as i32,
                EntityKind::CommitMessage as i32,
            ];
            let mut c = self.conn().await?;
            let deleted = sqlx::query(
                "DELETE FROM icebox WHERE EXISTS ( \
                     SELECT 1 FROM refresh_state rs \
                     WHERE rs.entity_id = icebox.group_id \
                       AND rs.originator_id = icebox.originator_id \
                       AND rs.sequence_id >= icebox.sequence_id \
                       AND rs.entity_kind = ANY($1::int4[]))",
            )
            .bind(&kinds)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(deleted as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    use xmtp_common::Generate;
    use xmtp_proto::types::Cursor;

    use crate::Store;
    use crate::group::{ConversationType, GroupMembershipState, StoredGroup};
    use crate::with_connection;

    use super::*;

    async fn create_test_group(conn: &impl crate::DbQuery) -> GroupId {
        let group_id = GroupId::generate();
        let group = StoredGroup {
            id: group_id,
            created_at_ns: 0,
            membership_state: GroupMembershipState::Allowed,
            installations_last_checked: 0,
            added_by_inbox_id: "test".to_string(),
            sequence_id: None,
            rotated_at_ns: 0,
            conversation_type: ConversationType::Group,
            dm_id: None,
            last_message_ns: None,
            message_disappear_from_ns: None,
            message_disappear_in_ns: None,
            paused_for_version: None,
            maybe_forked: false,
            fork_details: "{}".to_string(),
            originator_id: None,
            should_publish_commit_log: false,
            commit_log_public_key: None,
            is_commit_log_forked: None,
            has_pending_leave_request: None,
        };
        group.store(conn).await.unwrap();
        group_id
    }

    fn iced(group_id: GroupId) -> Vec<OrphanedEnvelope> {
        vec![
            OrphanedEnvelope::builder()
                .cursor(Cursor::new(41, 1u32))
                .depending_on(Cursor::new(40, 1u32))
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap(),
            OrphanedEnvelope::builder()
                .cursor(Cursor::new(40, 1u32))
                .depending_on(Cursor::new(39, 2u32))
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap(),
            OrphanedEnvelope::builder()
                .cursor(Cursor::new(39, 2u32))
                .depending_on(Cursor::new(38, 2u32))
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap(),
        ]
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn icebox_dependency_chain() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            let orphans = iced(group_id);

            // Store envelopes and dependencies
            conn.ice(orphans.clone()).await?;

            let dep_chain = conn.past_dependents(&[Cursor::new(41, 1u32)]).await?;
            assert_eq!(dep_chain.len(), 3);

            assert_eq!(orphans[0].depends_on[&1], 40);
            assert_eq!(orphans[1].depends_on[&2], 39);
            assert_eq!(orphans[2].depends_on[&2], 38);

            let mut dep_chain = conn.future_dependents(&[Cursor::new(39, 2u32)]).await?;
            dep_chain.sort_by_key(|d| d.cursor.sequence_id);
            assert_eq!(dep_chain.len(), 2);
            assert_eq!(dep_chain[0].cursor.sequence_id, 40);
            assert_eq!(dep_chain[0].cursor.originator_id, 1);
            assert_eq!(dep_chain[0].depends_on[&2], 39);

            assert_eq!(dep_chain[1].cursor.sequence_id, 41);
            assert_eq!(dep_chain[1].cursor.originator_id, 1);
            assert_eq!(dep_chain[1].depends_on[&1], 40);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_icebox_wrong_originator() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            // Break the chain by changing the originator
            let mut orphans = iced(group_id);
            // Change envelope (39, 2) to (39, 1), breaking the chain
            orphans[2] = OrphanedEnvelope::builder()
                .cursor(Cursor::new(39, 1u32))
                .depending_on(Cursor::new(38, 1u32))
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap();

            conn.ice(orphans).await?;

            let mut dep_chain = conn.past_dependents(&[Cursor::new(41, 1u32)]).await?;
            dep_chain.sort_by_key(|d| d.cursor.sequence_id);
            // The last iced message should not be there due to the wrong originator_id.
            // past_dependents returns starting envelope + dependencies
            // Should only return (41, 1) and (40, 1) because (40, 1) depends on (39, 2) which doesn't exist
            assert_eq!(dep_chain.len(), 2);
            assert_eq!(dep_chain[0].depends_on[&2], 39);
            assert_eq!(dep_chain[1].depends_on[&1], 40);

            // With the changed originator, envelope (39, 1) has no dependents
            // (40, 1) depends on (39, 2), not (39, 1)
            let dep_chain = conn.future_dependents(&[Cursor::new(39, 1u32)]).await?;
            assert_eq!(dep_chain.len(), 0);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_icebox_wrong_sequence() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            // Break the chain by changing the sequence_id to a non-conflicting value
            let mut orphans = iced(group_id);
            // Change envelope (39, 2) to (100, 2), breaking the chain
            orphans[2] = OrphanedEnvelope::builder()
                .cursor(Cursor::new(100, 2u32))
                .depending_on(Cursor::new(38, 2u32))
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap();

            conn.ice(orphans).await?;

            let mut dep_chain = conn.past_dependents(&[Cursor::new(41, 1u32)]).await?;
            dep_chain.sort_by_key(|d| d.cursor.sequence_id);

            // The last iced message should not be there due to the wrong sequence_id.
            // past_dependents returns starting envelope + dependencies
            // Should only return (41, 1) and (40, 1) because (40, 1) depends on (39, 2) which doesn't exist
            assert_eq!(dep_chain.len(), 2);
            assert_eq!(dep_chain[0].depends_on[&2], 39);
            assert_eq!(dep_chain[1].depends_on[&1], 40);
            // With the changed sequence_id, envelope (100, 2) has no dependents
            // Nothing depends on (100, 2) in the dependency chain
            let dep_chain = conn.future_dependents(&[Cursor::new(100, 2u32)]).await?;
            assert_eq!(dep_chain.len(), 0);
        })
        .await
    }

    // commit + two dependant application messages
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_icebox_multiple_dependencies() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            // Test that two envelopes can depend on the same envelope
            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(1, 100u32))
                    .depending_on(Cursor::new(10, 0u32))
                    .payload(vec![1; 5])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(2, 100u32))
                    .depending_on(Cursor::new(10, 0u32))
                    .payload(vec![1; 5])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];

            let result = conn.ice(orphans);
            assert!(result.await.is_ok());

            let mut got = conn.future_dependents(&[Cursor::new(10, 0u32)]).await?;
            got.sort_by_key(|d| d.cursor.sequence_id);
            assert_eq!(got.len(), 2);
            assert_eq!(got[0].cursor.sequence_id, 1);
            assert_eq!(got[0].cursor.originator_id, 100);
            assert_eq!(got[1].cursor.sequence_id, 2);
            assert_eq!(got[1].cursor.originator_id, 100);

            // Verify both envelopes have the dependency on commit
            for envelope in &got {
                assert_eq!(envelope.depends_on[&0], 10);
            }
        })
        .await
    }

    // chained commits & app messages
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_icebox_chain() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            // Test a chain where envelope 3 depends on 2, and both 1 and 2 depend on 3
            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(1, 100u32))
                    .depending_on(Cursor::new(3, 0u32))
                    .payload(vec![1])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(2, 100u32))
                    .depending_on(Cursor::new(3, 0u32))
                    .payload(vec![1])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(3, 0u32))
                    .depending_on(Cursor::new(2, 0u32))
                    .payload(vec![1])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];

            let result = conn.ice(orphans);
            assert!(result.await.is_ok());

            let mut got = conn.future_dependents(&[Cursor::new(2, 0u32)]).await?;
            got.sort_by_key(|i| i.cursor.sequence_id);
            assert_eq!(got.len(), 3);

            assert_eq!(got[0].cursor.sequence_id, 1);
            assert_eq!(got[0].cursor.originator_id, 100);
            assert_eq!(got[1].cursor.sequence_id, 2);
            assert_eq!(got[1].cursor.originator_id, 100);
            assert_eq!(got[2].cursor.sequence_id, 3);
            assert_eq!(got[2].cursor.originator_id, 0);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_future_dependents_multiple_cursors() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            let orphans = iced(group_id);

            // Store envelopes and dependencies
            conn.ice(orphans).await?;

            // Test query with multiple cursors
            let cursors = vec![Cursor::new(39, 2u32), Cursor::new(40, 1u32)];

            let mut result = conn.future_dependents(&cursors).await?;
            result.sort_by_key(|d| d.cursor.sequence_id);

            // Verify we get the union of dependants
            // (39, 2) is depended on by (40, 1) and (41, 1)
            // (40, 1) is depended on by (41, 1)
            // So we should get (40, 1) and (41, 1), deduplicated
            assert_eq!(result.len(), 2);
            assert_eq!(result[0].cursor.sequence_id, 40);
            assert_eq!(result[0].cursor.originator_id, 1);
            assert_eq!(result[1].cursor.sequence_id, 41);
            assert_eq!(result[1].cursor.originator_id, 1);

            // Verify dependencies are correct
            assert_eq!(result[0].depends_on[&2], 39);
            assert_eq!(result[1].depends_on[&1], 40);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_future_dependents_empty() {
        with_connection(async |conn| {
            // Test with empty cursor list
            let result = conn.future_dependents(&[]).await?;
            assert_eq!(result.len(), 0);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_querying_dependencies_in_middle_works() {
        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;
            let orphans = iced(group_id);

            conn.ice(orphans.clone()).await?;

            let mut result = conn.past_dependents(&[Cursor::new(40, 1u32)]).await?;
            assert_eq!(result.len(), 2);
            result.sort_by_key(|d| d.cursor.originator_id);
            assert_eq!(result[0].cursor, Cursor::new(40, 1u32));
            assert_eq!(result[0].depends_on, Cursor::new(39, 2u32).into());
            assert_eq!(result[1].cursor, Cursor::new(39, 2u32));
            assert_eq!(result[1].depends_on, Cursor::new(38, 2u32).into());

            let result = conn.future_dependents(&[Cursor::new(40, 1u32)]).await?;
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].cursor, Cursor::new(41, 1u32));
            assert_eq!(result[0].depends_on, Cursor::new(40, 1u32).into());
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_prune_icebox() {
        use crate::StoreOrIgnore;
        use crate::encrypted_store::refresh_state::{EntityKind, RefreshState};

        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;

            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(10, 1u32))
                    .depending_on(Cursor::new(9, 1u32))
                    .payload(vec![1, 2, 3])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(20, 1u32))
                    .depending_on(Cursor::new(19, 1u32))
                    .payload(vec![4, 5, 6])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(30, 1u32))
                    .depending_on(Cursor::new(29, 1u32))
                    .payload(vec![7, 8, 9])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(10, 10u32))
                    .depending_on(Cursor::new(1, 1u32))
                    .payload(vec![1, 2, 3])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];
            conn.ice(orphans).await?;

            RefreshState {
                entity_id: group_id.to_vec(),
                entity_kind: EntityKind::ApplicationMessage,
                sequence_id: 20,
                originator_id: 1,
            }
            .store_or_ignore(conn)
            .await?;

            let deleted = conn.prune_icebox().await?;
            assert_eq!(
                deleted, 2,
                "Should delete entries with sequence_id 10 and 20"
            );

            // Verify entry 30 remains
            let mut remaining: Vec<Icebox> =
                conn.raw_query(|conn| dsl::icebox.filter(dsl::group_id.eq(&group_id)).load(conn))?;
            remaining.sort_by_key(|e| e.originator_id);

            assert_eq!(remaining.len(), 2, "Should have 2 entries remaining");
            assert_eq!(remaining[0].sequence_id, 30);
            assert_eq!(remaining[0].originator_id, 1);
            assert_eq!(remaining[1].sequence_id, 10);
            assert_eq!(remaining[1].originator_id, 10);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_prune_icebox_no_cleanup_when_cursor_lower() {
        use crate::StoreOrIgnore;
        use crate::encrypted_store::refresh_state::{EntityKind, RefreshState};

        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;

            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(50, 1u32))
                    .depending_on(Cursor::new(49, 1u32))
                    .payload(vec![1, 2, 3])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(60, 1u32))
                    .depending_on(Cursor::new(59, 1u32))
                    .payload(vec![4, 5, 6])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];
            conn.ice(orphans).await?;

            RefreshState {
                entity_id: group_id.to_vec(),
                entity_kind: EntityKind::ApplicationMessage,
                sequence_id: 40,
                originator_id: 1,
            }
            .store_or_ignore(conn)
            .await?;

            let deleted = conn.prune_icebox().await?;
            assert_eq!(deleted, 0, "Should not delete any entries");

            let remaining: Vec<Icebox> =
                conn.raw_query(|conn| dsl::icebox.filter(dsl::group_id.eq(&group_id)).load(conn))?;
            assert_eq!(remaining.len(), 2);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_prune_icebox_only_relevant_entity_kinds() {
        use crate::StoreOrIgnore;
        use crate::encrypted_store::refresh_state::{EntityKind, RefreshState};

        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;

            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(10, 1u32))
                    .depending_on(Cursor::new(9, 1u32))
                    .payload(vec![1, 2, 3])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];
            conn.ice(orphans).await?;

            RefreshState {
                entity_id: group_id.to_vec(),
                entity_kind: EntityKind::Welcome,
                sequence_id: 100,
                originator_id: 1,
            }
            .store_or_ignore(conn)
            .await?;

            let deleted = conn.prune_icebox().await?;
            assert_eq!(deleted, 0, "Should not delete due to wrong entity_kind");

            let remaining: Vec<Icebox> =
                conn.raw_query(|conn| dsl::icebox.filter(dsl::group_id.eq(&group_id)).load(conn))?;
            assert_eq!(remaining.len(), 1);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_prune_icebox_dependencies_cascade_deleted() {
        use crate::StoreOrIgnore;
        use crate::encrypted_store::refresh_state::{EntityKind, RefreshState};

        with_connection(async |conn| {
            let group_id = create_test_group(conn).await;

            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor::new(10, 1u32))
                    .depending_on(Cursor::new(9, 1u32))
                    .payload(vec![1, 2, 3])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];
            conn.ice(orphans).await?;

            use crate::schema::icebox_dependencies::dsl as dep_dsl;
            let deps: Vec<IceboxDependency> = conn.raw_query(|conn| {
                icebox_dependencies::table
                    .filter(dep_dsl::envelope_originator_id.eq(1))
                    .filter(dep_dsl::envelope_sequence_id.eq(10))
                    .load(conn)
            })?;
            assert_eq!(deps.len(), 1);

            RefreshState {
                entity_id: group_id.to_vec(),
                entity_kind: EntityKind::ApplicationMessage,
                sequence_id: 10,
                originator_id: 1,
            }
            .store_or_ignore(conn)
            .await?;

            let deleted = conn.prune_icebox().await?;
            assert_eq!(deleted, 1, "Should delete the icebox entry");

            let remaining: Vec<Icebox> =
                conn.raw_query(|conn| dsl::icebox.filter(dsl::group_id.eq(&group_id)).load(conn))?;
            assert_eq!(remaining.len(), 0);

            let deps: Vec<IceboxDependency> = conn.raw_query(|conn| {
                icebox_dependencies::table
                    .filter(dep_dsl::envelope_originator_id.eq(1))
                    .filter(dep_dsl::envelope_sequence_id.eq(10))
                    .load(conn)
            })?;
            assert_eq!(deps.len(), 0, "Dependencies should be cascade deleted");
        })
        .await
    }
}
