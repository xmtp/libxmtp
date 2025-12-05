use super::{ConnectionExt, db_connection::DbConnection};
use crate::icebox::types::{IceboxOrphans, IceboxWithDep};
use crate::schema::icebox::dsl;
use crate::schema::icebox_dependencies;
use crate::{impl_store, schema::icebox};
use diesel::prelude::*;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use xmtp_proto::types::{
    Cursor, OriginatorId, OrphanedEnvelope, OrphanedEnvelopeBuilder, SequenceId,
};

mod types;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = icebox)]
#[diesel(primary_key(sequence_id, originator_id))]
#[diesel(belongs_to(crate::group::StoredGroup, foreign_key = group_id))]
pub struct Icebox {
    pub sequence_id: i64,
    pub originator_id: i64,
    pub group_id: Vec<u8>,
    pub envelope_payload: Vec<u8>,
}

impl_store!(Icebox, icebox);

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = icebox_dependencies)]
#[diesel(primary_key(
    envelope_sequence_id,
    envelope_originator_id,
    dependency_sequence_id,
    dependency_originator_id
))]
pub struct IceboxDependency {
    pub envelope_sequence_id: i64,
    pub envelope_originator_id: i64,
    pub dependency_sequence_id: i64,
    pub dependency_originator_id: i64,
}

impl_store!(IceboxDependency, icebox_dependencies);

pub trait QueryIcebox {
    /// Returns the envelopes (if they exist) plus all their dependencies, and
    /// dependencies of dependencies, along with each envelope's own dependencies.
    /// This could be useful for resolving issues where a commit that could have been
    /// processed, was accidentally committed to the icebox.
    /// Generally, if an envelope has a dependency on something in the icebox already
    /// it means its dependency could not be processed, so it must also be iceboxed.
    fn past_dependants(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError>;

    /// Returns envelopes that depend on any of the specified cursors,
    /// along with each envelope's own dependencies.
    /// Does not return the cursors themselves, if they exist in the chain.
    fn future_dependants(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError>;

    /// cache the orphans until its parent(s) may be found.
    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<usize, crate::ConnectionError>;
}

impl<T> QueryIcebox for &T
where
    T: QueryIcebox,
{
    fn past_dependants(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        (**self).past_dependants(cursors)
    }

    fn future_dependants(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        (**self).future_dependants(cursors)
    }

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<usize, crate::ConnectionError> {
        (**self).ice(orphans)
    }
}

impl<C: ConnectionExt> DbConnection<C> {
    fn do_icebox_query(
        &self,
        query_str: String,
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            diesel::sql_query(query_str)
                .load_iter::<IceboxWithDep, _>(conn)?
                .process_results(|iter| {
                    // since we're using load_iter
                    // to optimize, we load a *const [u8] into `IceboxWithDep` for group_id and
                    // envelope_payload, cloning it only once in `fold_with`.
                    // as long as we are in the scope of `load_iter` (attached to the lifetime of
                    // `conn` or `&mut SqliteConnection` within `raw_query_read`) the lifetime of group_id and
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
                                    .cursor(Cursor {
                                        sequence_id: row.sequence_id as SequenceId,
                                        originator_id: row.originator_id as OriginatorId,
                                    })
                                    .payload(payload)
                                    .group_id(group_id);
                                builder
                            },
                            |mut acc, _key, row| {
                                acc.depending_on(Cursor {
                                    originator_id: row.dependency_originator_id as OriginatorId,
                                    sequence_id: row.dependency_sequence_id as SequenceId,
                                });
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

impl<C: ConnectionExt> QueryIcebox for DbConnection<C> {
    fn past_dependants(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        if cursors.is_empty() {
            return Ok(Vec::new());
        }

        let values_clause = cursors
            .iter()
            .map(|c| format!("({}, {})", c.sequence_id, c.originator_id))
            .join(", ");

        let query_str = format!(
            r#"
            WITH RECURSIVE
            start_cursors(sequence_id, originator_id) AS (
                VALUES {}
            ),
            dependency_chain AS (
                -- Base case: Start with the specified envelopes if they exist
                SELECT i.sequence_id, i.originator_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN start_cursors sc ON i.sequence_id = sc.sequence_id
                                      AND i.originator_id = sc.originator_id

                UNION

                -- OR start with their immediate dependencies if they don't
                SELECT i.sequence_id, i.originator_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.sequence_id = d.dependency_sequence_id
                                           AND i.originator_id = d.dependency_originator_id
                JOIN start_cursors sc ON d.envelope_sequence_id = sc.sequence_id
                                      AND d.envelope_originator_id = sc.originator_id

                UNION ALL

                -- Recursive case: Continue traversing the dependency chain
                SELECT i.sequence_id, i.originator_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.sequence_id = d.dependency_sequence_id
                                           AND i.originator_id = d.dependency_originator_id
                JOIN dependency_chain dc ON d.envelope_sequence_id = dc.sequence_id
                                         AND d.envelope_originator_id = dc.originator_id
            )
            SELECT
                dc.sequence_id,
                dc.originator_id,
                dc.group_id,
                dc.envelope_payload,
                d.dependency_originator_id,
                d.dependency_sequence_id
            FROM (SELECT DISTINCT * FROM dependency_chain) dc
            INNER JOIN icebox_dependencies d
                ON dc.sequence_id = d.envelope_sequence_id
                AND dc.originator_id = d.envelope_originator_id
            ORDER BY dc.sequence_id DESC, dc.originator_id DESC
            "#,
            values_clause
        );

        self.do_icebox_query(query_str)
    }

    fn future_dependants(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<OrphanedEnvelope>, crate::ConnectionError> {
        if cursors.is_empty() {
            return Ok(Vec::new());
        }

        // Build the VALUES clause with actual values (safe since they're i64)
        let values_clause = cursors
            .iter()
            .map(|c| format!("({}, {})", c.sequence_id, c.originator_id))
            .join(", ");

        let query_str = format!(
            r#"
            WITH RECURSIVE
            start_cursors(sequence_id, originator_id) AS (
                VALUES {}
            ),
            dependency_chain AS (
                -- Base case: Find all immediate dependents from any starting cursor
                SELECT i.sequence_id, i.originator_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.sequence_id = d.envelope_sequence_id
                                           AND i.originator_id = d.envelope_originator_id
                JOIN start_cursors sc ON d.dependency_sequence_id = sc.sequence_id
                                      AND d.dependency_originator_id = sc.originator_id

                UNION ALL

                -- Recursive case: Continue traversing the dependent chain
                SELECT i.sequence_id, i.originator_id, i.group_id, i.envelope_payload
                FROM icebox i
                JOIN icebox_dependencies d ON i.sequence_id = d.envelope_sequence_id
                                           AND i.originator_id = d.envelope_originator_id
                JOIN dependency_chain dc ON d.dependency_sequence_id = dc.sequence_id
                                         AND d.dependency_originator_id = dc.originator_id
            )
            SELECT
                dc.sequence_id,
                dc.originator_id,
                dc.group_id,
                dc.envelope_payload,
                d.dependency_originator_id,
                d.dependency_sequence_id
            FROM dependency_chain dc
            INNER JOIN icebox_dependencies d
                ON dc.sequence_id = d.envelope_sequence_id
                AND dc.originator_id = d.envelope_originator_id
            "#,
            values_clause
        );

        self.do_icebox_query(query_str)
    }

    fn ice(&self, orphans: Vec<OrphanedEnvelope>) -> Result<usize, crate::ConnectionError> {
        self.raw_query_write(|conn| {
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
}

#[cfg(test)]
mod tests {
    use xmtp_proto::types::Cursor;

    use crate::Store;
    use crate::group::{ConversationType, GroupMembershipState, StoredGroup};
    use crate::with_connection;

    use super::*;

    fn create_test_group(conn: &impl crate::DbQuery) -> Vec<u8> {
        let group_id = vec![1u8; 1];
        let group = StoredGroup {
            id: group_id.clone(),
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
        group.store(conn).unwrap();
        group_id
    }

    fn iced(group_id: Vec<u8>) -> Vec<OrphanedEnvelope> {
        vec![
            OrphanedEnvelope::builder()
                .cursor(Cursor {
                    sequence_id: 41,
                    originator_id: 1,
                })
                .depending_on(Cursor {
                    sequence_id: 40,
                    originator_id: 1,
                })
                .payload(vec![1, 2, 3])
                .group_id(group_id.clone())
                .build()
                .unwrap(),
            OrphanedEnvelope::builder()
                .cursor(Cursor {
                    sequence_id: 40,
                    originator_id: 1,
                })
                .depending_on(Cursor {
                    sequence_id: 39,
                    originator_id: 2,
                })
                .payload(vec![1, 2, 3])
                .group_id(group_id.clone())
                .build()
                .unwrap(),
            OrphanedEnvelope::builder()
                .cursor(Cursor {
                    sequence_id: 39,
                    originator_id: 2,
                })
                .depending_on(Cursor {
                    sequence_id: 38,
                    originator_id: 2,
                })
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap(),
        ]
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn icebox_dependency_chain() {
        with_connection(|conn| {
            let group_id = create_test_group(conn);
            let orphans = iced(group_id);

            // Store envelopes and dependencies
            conn.ice(orphans.clone())?;

            // past_dependants returns the starting envelope + all dependencies
            let dep_chain = conn.past_dependants(&[Cursor {
                sequence_id: 41,
                originator_id: 1,
            }])?;
            assert_eq!(dep_chain.len(), 3);

            assert_eq!(orphans[0].depends_on[&1], 40);
            assert_eq!(orphans[1].depends_on[&2], 39);
            assert_eq!(orphans[2].depends_on[&2], 38);

            // future_dependants returns only envelopes that depend on (39, 2)
            let mut dep_chain = conn.future_dependants(&[Cursor {
                sequence_id: 39,
                originator_id: 2,
            }])?;
            dep_chain.sort_by_key(|d| d.cursor.sequence_id);
            assert_eq!(dep_chain.len(), 2);
            assert_eq!(dep_chain[0].cursor.sequence_id, 40);
            assert_eq!(dep_chain[0].cursor.originator_id, 1);
            assert_eq!(dep_chain[0].depends_on[&2], 39);

            assert_eq!(dep_chain[1].cursor.sequence_id, 41);
            assert_eq!(dep_chain[1].cursor.originator_id, 1);
            assert_eq!(dep_chain[1].depends_on[&1], 40);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_icebox_wrong_originator() {
        with_connection(|conn| {
            let group_id = create_test_group(conn);
            // Break the chain by changing the originator
            let mut orphans = iced(group_id.clone());
            // Change envelope (39, 2) to (39, 1), breaking the chain
            orphans[2] = OrphanedEnvelope::builder()
                .cursor(Cursor {
                    sequence_id: 39,
                    originator_id: 1, // Changed from 2 to 1
                })
                .depending_on(Cursor {
                    sequence_id: 38,
                    originator_id: 1, // Changed from 2 to 1
                })
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap();

            conn.ice(orphans)?;

            let mut dep_chain = conn.past_dependants(&[Cursor {
                sequence_id: 41,
                originator_id: 1,
            }])?;
            dep_chain.sort_by_key(|d| d.cursor.sequence_id);
            // The last iced message should not be there due to the wrong originator_id.
            // past_dependants returns starting envelope + dependencies
            // Should only return (41, 1) and (40, 1) because (40, 1) depends on (39, 2) which doesn't exist
            assert_eq!(dep_chain.len(), 2);
            assert_eq!(dep_chain[0].depends_on[&2], 39);
            assert_eq!(dep_chain[1].depends_on[&1], 40);

            // With the changed originator, envelope (39, 1) has no dependents
            // (40, 1) depends on (39, 2), not (39, 1)
            let dep_chain = conn.future_dependants(&[Cursor {
                sequence_id: 39,
                originator_id: 1,
            }])?;
            assert_eq!(dep_chain.len(), 0);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_icebox_wrong_sequence() {
        with_connection(|conn| {
            let group_id = create_test_group(conn);
            // Break the chain by changing the sequence_id to a non-conflicting value
            let mut orphans = iced(group_id.clone());
            // Change envelope (39, 2) to (100, 2), breaking the chain
            orphans[2] = OrphanedEnvelope::builder()
                .cursor(Cursor {
                    sequence_id: 100, // Changed from 39 to 100
                    originator_id: 2,
                })
                .depending_on(Cursor {
                    sequence_id: 38,
                    originator_id: 2,
                })
                .payload(vec![1, 2, 3])
                .group_id(group_id)
                .build()
                .unwrap();

            conn.ice(orphans)?;

            let mut dep_chain = conn.past_dependants(&[Cursor {
                sequence_id: 41,
                originator_id: 1,
            }])?;
            dep_chain.sort_by_key(|d| d.cursor.sequence_id);

            // The last iced message should not be there due to the wrong sequence_id.
            // past_dependants returns starting envelope + dependencies
            // Should only return (41, 1) and (40, 1) because (40, 1) depends on (39, 2) which doesn't exist
            assert_eq!(dep_chain.len(), 2);
            assert_eq!(dep_chain[0].depends_on[&2], 39);
            assert_eq!(dep_chain[1].depends_on[&1], 40);
            // With the changed sequence_id, envelope (100, 2) has no dependents
            // Nothing depends on (100, 2) in the dependency chain
            let dep_chain = conn.future_dependants(&[Cursor {
                sequence_id: 100,
                originator_id: 2,
            }])?;
            assert_eq!(dep_chain.len(), 0);
        })
    }

    // commit + two dependant application messages
    #[xmtp_common::test(unwrap_try = true)]
    fn test_icebox_multiple_dependencies() {
        with_connection(|conn| {
            let group_id = create_test_group(conn);
            // Test that two envelopes can depend on the same envelope
            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor {
                        sequence_id: 1,
                        originator_id: 100,
                    })
                    .depending_on(Cursor {
                        sequence_id: 10,
                        originator_id: 0,
                    })
                    .payload(vec![1; 5])
                    .group_id(group_id.clone())
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor {
                        sequence_id: 2,
                        originator_id: 100,
                    })
                    .depending_on(Cursor {
                        sequence_id: 10,
                        originator_id: 0,
                    })
                    .payload(vec![1; 5])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];

            let result = conn.ice(orphans);
            assert!(result.is_ok());

            let mut got = conn.future_dependants(&[Cursor {
                sequence_id: 10,
                originator_id: 0,
            }])?;
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
    }

    // chained commits & app messages
    #[xmtp_common::test(unwrap_try = true)]
    fn test_icebox_chain() {
        with_connection(|conn| {
            let group_id = create_test_group(conn);
            // Test a chain where envelope 3 depends on 2, and both 1 and 2 depend on 3
            let orphans = vec![
                OrphanedEnvelope::builder()
                    .cursor(Cursor {
                        sequence_id: 1,
                        originator_id: 100,
                    })
                    .depending_on(Cursor {
                        sequence_id: 3,
                        originator_id: 0,
                    })
                    .payload(vec![1])
                    .group_id(group_id.clone())
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor {
                        sequence_id: 2,
                        originator_id: 100,
                    })
                    .depending_on(Cursor {
                        sequence_id: 3,
                        originator_id: 0,
                    })
                    .payload(vec![1])
                    .group_id(group_id.clone())
                    .build()
                    .unwrap(),
                OrphanedEnvelope::builder()
                    .cursor(Cursor {
                        sequence_id: 3,
                        originator_id: 0,
                    })
                    .depending_on(Cursor {
                        sequence_id: 2,
                        originator_id: 0,
                    })
                    .payload(vec![1])
                    .group_id(group_id)
                    .build()
                    .unwrap(),
            ];

            let result = conn.ice(orphans);
            assert!(result.is_ok());

            let mut got = conn.future_dependants(&[Cursor {
                sequence_id: 2,
                originator_id: 0,
            }])?;
            got.sort_by_key(|i| i.cursor.sequence_id);
            assert_eq!(got.len(), 3);

            assert_eq!(got[0].cursor.sequence_id, 1);
            assert_eq!(got[0].cursor.originator_id, 100);
            assert_eq!(got[1].cursor.sequence_id, 2);
            assert_eq!(got[1].cursor.originator_id, 100);
            assert_eq!(got[2].cursor.sequence_id, 3);
            assert_eq!(got[2].cursor.originator_id, 0);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_future_dependants_multiple_cursors() {
        with_connection(|conn| {
            let group_id = create_test_group(conn);
            let orphans = iced(group_id);

            // Store envelopes and dependencies
            conn.ice(orphans)?;

            // Test query with multiple cursors
            let cursors = vec![
                Cursor {
                    sequence_id: 39,
                    originator_id: 2,
                },
                Cursor {
                    sequence_id: 40,
                    originator_id: 1,
                },
            ];

            let mut result = conn.future_dependants(&cursors)?;
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
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_future_dependants_empty() {
        with_connection(|conn| {
            // Test with empty cursor list
            let result = conn.future_dependants(&[])?;
            assert_eq!(result.len(), 0);
        })
    }
}
