use super::ConnectionExt;
use crate::group_message::StoredGroupMessage;
use crate::schema::message_deletions::dsl;
use crate::{DbConnection, impl_store, impl_store_or_ignore, schema::message_deletions};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use xmtp_proto::types::GroupId;

mod convert;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Selectable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = message_deletions)]
#[diesel(primary_key(id))]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
/// One deletion record: "inbox X deleted target T in group G at envelope time
/// t". Kept for the life of the group, also after its deletion message goes.
pub struct StoredMessageDeletion {
    /// The ID of the DeleteMessage in the group_messages table
    pub id: Vec<u8>,
    /// The group this deletion belongs to
    pub group_id: GroupId,
    /// The ID of the original message being deleted
    pub deleted_message_id: Vec<u8>,
    /// The inbox_id of who sent the delete message
    pub deleted_by_inbox_id: String,
    /// Whether the deleter was a super admin at that point in the topic
    pub is_super_admin_deletion: bool,
    /// The deletion message's envelope time
    pub deleted_at_ns: i64,
    /// Processed from the group's topic or this installation's own echo,
    /// rather than read from an archive.
    #[serde(default)]
    pub authenticated: bool,
}

impl StoredMessageDeletion {
    /// The record of `deletion`, a `DeleteMessage` row naming `target_id`.
    pub fn of(
        deletion: &StoredGroupMessage,
        target_id: &[u8],
        is_super_admin_deletion: bool,
        authenticated: bool,
    ) -> Self {
        Self {
            id: deletion.id.clone(),
            group_id: deletion.group_id,
            deleted_message_id: target_id.to_vec(),
            deleted_by_inbox_id: deletion.sender_inbox_id.clone(),
            is_super_admin_deletion,
            deleted_at_ns: deletion.sent_at_ns,
            authenticated,
        }
    }
}

impl_store!(StoredMessageDeletion, message_deletions);
impl_store_or_ignore!(StoredMessageDeletion, message_deletions);

/// Trait for querying message deletions
pub trait QueryMessageDeletion {
    /// Get a deletion record by the DeleteMessage ID
    fn get_message_deletion(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError>;

    /// Store one record per deletion message. An authenticated record replaces
    /// an imported one; nothing replaces an authenticated record. Returns the
    /// stored record.
    fn record_deletion(
        &self,
        record: &StoredMessageDeletion,
    ) -> Result<StoredMessageDeletion, crate::ConnectionError>;

    /// The record with the lowest `(deleted_at_ns, id)` for `target` whose
    /// deleter is the target's sender or was a super admin, in its group;
    /// with `authenticated_only`, among records from the topic.
    fn first_deletion_of(
        &self,
        target: &StoredGroupMessage,
        authenticated_only: bool,
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError>;
}

impl<T> QueryMessageDeletion for &T
where
    T: QueryMessageDeletion,
{
    fn get_message_deletion(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        (**self).get_message_deletion(id)
    }

    fn record_deletion(
        &self,
        record: &StoredMessageDeletion,
    ) -> Result<StoredMessageDeletion, crate::ConnectionError> {
        (**self).record_deletion(record)
    }

    fn first_deletion_of(
        &self,
        target: &StoredGroupMessage,
        authenticated_only: bool,
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        (**self).first_deletion_of(target, authenticated_only)
    }
}

impl<C: ConnectionExt> QueryMessageDeletion for DbConnection<C> {
    fn get_message_deletion(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::message_deletions
                .filter(dsl::id.eq(id))
                .first(conn)
                .optional()
        })
    }

    // implements: PROC-037
    fn record_deletion(
        &self,
        record: &StoredMessageDeletion,
    ) -> Result<StoredMessageDeletion, crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::insert_or_ignore_into(dsl::message_deletions)
                .values(record)
                .execute(conn)?;
            if record.authenticated {
                diesel::update(
                    dsl::message_deletions
                        .find(&record.id)
                        .filter(dsl::authenticated.eq(false)),
                )
                .set((
                    dsl::group_id.eq(record.group_id),
                    dsl::deleted_message_id.eq(&record.deleted_message_id),
                    dsl::deleted_by_inbox_id.eq(&record.deleted_by_inbox_id),
                    dsl::is_super_admin_deletion.eq(record.is_super_admin_deletion),
                    dsl::deleted_at_ns.eq(record.deleted_at_ns),
                    dsl::authenticated.eq(true),
                ))
                .execute(conn)?;
            }
            dsl::message_deletions.find(&record.id).first(conn)
        })
    }

    // implements: PROC-037
    fn first_deletion_of(
        &self,
        target: &StoredGroupMessage,
        authenticated_only: bool,
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        // The SQL mirror of may_delete's deleter-side checks (same group;
        // sender or super admin), applied before LIMIT 1. The other checks
        // depend only on the target, so if the first candidate fails, all fail.
        self.raw_query(|conn| {
            let mut query = dsl::message_deletions.into_boxed();
            if authenticated_only {
                query = query.filter(dsl::authenticated.eq(true));
            }
            query
                .filter(dsl::group_id.eq(target.group_id))
                .filter(dsl::deleted_message_id.eq(&target.id))
                .filter(
                    dsl::deleted_by_inbox_id
                        .eq(&target.sender_inbox_id)
                        .or(dsl::is_super_admin_deletion.eq(true)),
                )
                .order((dsl::deleted_at_ns.asc(), dsl::id.asc()))
                .first(conn)
                .optional()
        })
    }
}

/// Mark and empty, once, each target that the upgrade to deletion marks
/// queued and that the old read-time check hid: a record in its group whose
/// deleter is its sender or a super admin, and a deletable wire type. The
/// earliest such record names the deleter. Then empty the queue.
// implements: PROC-037
pub(crate) fn backfill_deletion_marks(
    conn: &mut diesel::SqliteConnection,
) -> Result<(), diesel::result::Error> {
    use crate::schema::{deletion_backfill, group_messages};
    let queued: bool = diesel::select(diesel::dsl::exists(
        deletion_backfill::table.select(deletion_backfill::target_id),
    ))
    .get_result(conn)?;
    if !queued {
        return Ok(());
    }
    conn.immediate_transaction(|conn| {
        let targets: Vec<Vec<u8>> = deletion_backfill::table
            .select(deletion_backfill::target_id)
            .load(conn)?;
        for id in targets {
            // Only the columns the old check read: other columns of an old
            // row need not load.
            let Some((group_id, kind, sender, bytes, marked)) = group_messages::table
                .find(&id)
                .select((
                    group_messages::group_id,
                    group_messages::kind,
                    group_messages::sender_inbox_id,
                    group_messages::decrypted_message_bytes,
                    group_messages::deleted_by_inbox_id,
                ))
                .first::<(Vec<u8>, i32, String, Vec<u8>, Option<String>)>(conn)
                .optional()?
            else {
                continue;
            };
            let application = kind == crate::group_message::GroupMessageKind::Application as i32;
            if marked.is_some()
                || !application
                || !crate::group_message::content_is_deletable(&bytes)
            {
                continue;
            }
            let deleter = dsl::message_deletions
                .filter(dsl::group_id.eq(group_id))
                .filter(dsl::deleted_message_id.eq(&id))
                .filter(
                    dsl::deleted_by_inbox_id
                        .eq(&sender)
                        .or(dsl::is_super_admin_deletion.eq(true)),
                )
                .order((dsl::deleted_at_ns.asc(), dsl::id.asc()))
                .select(dsl::deleted_by_inbox_id)
                .first::<String>(conn)
                .optional()?;
            if let Some(deleter) = deleter {
                diesel::update(
                    group_messages::table
                        .find(&id)
                        .filter(group_messages::deleted_by_inbox_id.is_null()),
                )
                .set((
                    group_messages::deleted_by_inbox_id.eq(deleter),
                    group_messages::decrypted_message_bytes.eq(Vec::<u8>::new()),
                ))
                .execute(conn)?;
            }
        }
        diesel::delete(deletion_backfill::table).execute(conn)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encrypted_store::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
    use crate::with_connection;

    fn test_message(id: Vec<u8>, group_id: GroupId) -> StoredGroupMessage {
        StoredGroupMessage {
            id,
            group_id,
            decrypted_message_bytes: vec![],
            sent_at_ns: 1000,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: "sender".to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::Text,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expiry_unknown: false,
            expire_at_ns: None,
            sequence_id: 1,
            envelope_hash: None,
            expiry_ns: None,

            inserted_at_ns: 0,
            should_push: false,
            idempotency_key: 1000.to_string(),
            deleted_by_inbox_id: None,
        }
    }

    fn record(id: u8, group_id: GroupId, by: &str, admin: bool, at: i64) -> StoredMessageDeletion {
        StoredMessageDeletion {
            id: vec![id, 0xde],
            group_id,
            deleted_message_id: vec![0xaa],
            deleted_by_inbox_id: by.to_string(),
            is_super_admin_deletion: admin,
            deleted_at_ns: at,
            authenticated: true,
        }
    }

    #[derive(QueryableByName)]
    struct QueryPlan {
        #[diesel(sql_type = diesel::sql_types::Text)]
        detail: String,
    }

    /// 500 earlier unauthorized records name the target; the case's record
    /// comes last. A LIMIT before the authority filter would miss it.
    // verifies: PROC-037
    #[rstest::rstest]
    #[case::authorized_by_sender("sender", false, GroupId::ONE, true)]
    #[case::authorized_by_bit("admin", true, GroupId::ONE, true)]
    #[case::unauthorized("other", false, GroupId::ONE, false)]
    #[case::other_group("sender", true, GroupId::TWO, false)]
    #[xmtp_common::test(unwrap_try = true)]
    fn deletion_lookup_is_bounded(
        #[case] by: &str,
        #[case] admin: bool,
        #[case] group_id: GroupId,
        #[case] found: bool,
    ) {
        with_connection(|conn| {
            let target = test_message(vec![0xaa], GroupId::ONE);
            for n in 0..500u32 {
                let mut early = record(0, GroupId::ONE, "other", false, i64::from(n));
                early.id = n.to_be_bytes().to_vec();
                conn.record_deletion(&early).unwrap();
            }
            let case = record(9, group_id, by, admin, 10_000);
            conn.record_deletion(&case).unwrap();
            assert_eq!(
                conn.first_deletion_of(&target, false).unwrap(),
                found.then_some(case)
            );
            let plan = conn
                .raw_query(|raw| {
                    diesel::sql_query(
                        "EXPLAIN QUERY PLAN SELECT * FROM message_deletions \
                     WHERE group_id = x'01' AND deleted_message_id = x'aa' \
                     AND (deleted_by_inbox_id = 'sender' OR is_super_admin_deletion = 1) \
                     ORDER BY deleted_at_ns, id LIMIT 1",
                    )
                    .load::<QueryPlan>(raw)
                })
                .unwrap();
            assert!(
                plan.iter()
                    .any(|row| row.detail.contains("message_deletions_target")),
                "{:?}",
                plan.iter().map(|row| &row.detail).collect::<Vec<_>>()
            );
        })
    }

    /// An authenticated record replaces an imported one; nothing replaces an
    /// authenticated record.
    // verifies: PROC-037
    #[rstest::rstest]
    #[case::imported_then_authenticated(false, true, true)]
    #[case::authenticated_then_imported(true, false, false)]
    #[case::authenticated_then_authenticated(true, true, false)]
    #[xmtp_common::test(unwrap_try = true)]
    fn record_deletion_keeps_authenticated(
        #[case] first_authenticated: bool,
        #[case] second_authenticated: bool,
        #[case] replaced: bool,
    ) {
        with_connection(|conn| {
            let first = StoredMessageDeletion {
                authenticated: first_authenticated,
                ..record(1, GroupId::ONE, "first", true, 10)
            };
            let second = StoredMessageDeletion {
                authenticated: second_authenticated,
                ..record(1, GroupId::ONE, "second", false, 20)
            };
            assert_eq!(conn.record_deletion(&first).unwrap(), first);
            let stored = conn.record_deletion(&second).unwrap();
            assert_eq!(stored, if replaced { second } else { first });
            assert_eq!(conn.get_message_deletion(&stored.id).unwrap(), Some(stored));
        })
    }
}
