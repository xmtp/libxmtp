use super::ConnectionExt;
use crate::schema::message_deletions::dsl;
use crate::{DbConnection, impl_store, impl_store_or_ignore, schema::message_deletions};
use crate::{group_message::DeliveryStatus, schema::group_messages};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use xmtp_proto::types::GroupId;
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
#[diesel(table_name = message_deletions)]
#[diesel(primary_key(id))]
/// Represents a deletion record for a message in a group conversation
pub struct StoredMessageDeletion {
    /// The ID of the DeleteMessage in the group_messages table
    pub id: Vec<u8>,
    /// The group this deletion belongs to
    pub group_id: GroupId,
    /// The ID of the original message being deleted
    pub deleted_message_id: Vec<u8>,
    /// The inbox_id of who sent the delete message
    pub deleted_by_inbox_id: String,
    /// Whether the deleter was a super admin at deletion time
    pub is_super_admin_deletion: bool,
    /// Timestamp when the deletion was processed
    pub deleted_at_ns: i64,
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

    /// Get deletion record for a specific deleted message
    fn get_deletion_by_deleted_message_id(
        &self,
        deleted_message_id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError>;

    /// Get deletion candidates whose DeleteMessage has not failed.
    fn get_deletions_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError>;

    /// Get all deletions in a group
    fn get_group_deletions(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError>;

    /// Reports whether a deletion row is stored, not whether it takes effect.
    /// Client code must also validate the group, target type, and sender.
    fn is_message_deleted(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError>;
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

    fn get_deletion_by_deleted_message_id(
        &self,
        deleted_message_id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        (**self).get_deletion_by_deleted_message_id(deleted_message_id)
    }

    fn get_deletions_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError> {
        (**self).get_deletions_for_messages(message_ids)
    }

    fn get_group_deletions(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError> {
        (**self).get_group_deletions(group_id)
    }

    fn is_message_deleted(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        (**self).is_message_deleted(message_id)
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

    fn get_deletion_by_deleted_message_id(
        &self,
        deleted_message_id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::message_deletions
                .filter(dsl::deleted_message_id.eq(deleted_message_id))
                .first(conn)
                .optional()
        })
    }

    fn get_deletions_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError> {
        if message_ids.is_empty() {
            return Ok(vec![]);
        }

        self.raw_query(|conn| {
            dsl::message_deletions
                .inner_join(group_messages::table.on(group_messages::id.eq(dsl::id)))
                .filter(dsl::deleted_message_id.eq_any(message_ids))
                .filter(group_messages::delivery_status.ne(DeliveryStatus::Failed))
                .select(message_deletions::all_columns)
                .load(conn)
        })
    }

    fn get_group_deletions(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::message_deletions
                .filter(dsl::group_id.eq(group_id))
                .load(conn)
        })
    }

    fn is_message_deleted(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::dsl::select(diesel::dsl::exists(
                dsl::message_deletions.filter(dsl::deleted_message_id.eq(message_id)),
            ))
            .get_result::<bool>(conn)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encrypted_store::group::{ConversationType, GroupMembershipState, StoredGroup};
    use crate::encrypted_store::group_message::{
        ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
    };
    use crate::{Store, with_connection};

    fn create_test_group(conn: &DbConnection<impl ConnectionExt>, group_id: GroupId) {
        StoredGroup {
            id: group_id,
            created_at_ns: 0,
            membership_state: GroupMembershipState::Allowed,
            installations_last_checked: 0,
            added_by_inbox_id: "test".to_string(),
            sequence_id: Some(0),
            rotated_at_ns: 0,
            conversation_type: ConversationType::Group,
            dm_id: None,
            last_message_ns: None,
            message_disappear_from_ns: None,
            message_disappear_in_ns: None,
            paused_for_version: None,
            maybe_forked: false,
            fork_details: "[]".to_string(),

            should_publish_commit_log: false,
            commit_log_public_key: None,
            is_commit_log_forked: None,
            has_pending_leave_request: None,
            push_override: None,
        }
        .store(conn)
        .unwrap();
    }

    fn create_test_message(
        conn: &DbConnection<impl ConnectionExt>,
        id: Vec<u8>,
        group_id: GroupId,
    ) {
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
        }
        .store(conn)
        .unwrap();
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_deletions_for_messages() {
        with_connection(|conn| {
            use crate::schema::group_messages;
            let group_id = GroupId::ONE;
            create_test_group(conn, group_id);
            for id in 1..=6 {
                create_test_message(conn, vec![id], group_id);
                assert!(!conn.is_message_deleted(&[id])?);
            }
            // Target 4 failed, but its published deletion still takes effect.
            conn.raw_query(|db| {
                diesel::update(group_messages::table.find(vec![4]))
                    .set(group_messages::delivery_status.eq(DeliveryStatus::Failed))
                    .execute(db)
            })?;
            for (carrier, target, status) in [
                (11, 1, DeliveryStatus::Published),
                (12, 2, DeliveryStatus::Unpublished),
                (13, 3, DeliveryStatus::Failed),
                (14, 4, DeliveryStatus::Published),
                (15, 5, DeliveryStatus::Published), // Unrequested target.
                (16, 1, DeliveryStatus::Failed),    // Mixed valid and failed deletions.
            ] {
                create_test_message(conn, vec![carrier], group_id);
                conn.raw_query(|db| {
                    diesel::update(group_messages::table.find(vec![carrier]))
                        .set(group_messages::delivery_status.eq(status))
                        .execute(db)
                })?;
                let deletion = StoredMessageDeletion {
                    id: vec![carrier],
                    group_id,
                    deleted_message_id: vec![target],
                    deleted_by_inbox_id: "sender".to_string(),
                    is_super_admin_deletion: false,
                    deleted_at_ns: 2000,
                };
                deletion.store(conn)?;
                // These are raw presence lookups, even for a Failed carrier.
                assert!(conn.is_message_deleted(&[target])?);
                assert_eq!(conn.get_message_deletion(&[carrier])?, Some(deletion));
                if carrier != 16 {
                    let by_target = conn.get_deletion_by_deleted_message_id(&[target])?.unwrap();
                    assert_eq!(by_target.deleted_message_id, vec![target]);
                    assert_eq!(by_target.id, vec![carrier]);
                }
            }
            let mut ids: Vec<_> = conn
                .get_deletions_for_messages((1..=4).map(|id| vec![id]).collect())?
                .into_iter()
                .map(|deletion| deletion.id)
                .collect();
            ids.sort();
            assert_eq!(ids, vec![vec![11], vec![12], vec![14]]);
            assert!(conn.get_deletions_for_messages(vec![])?.is_empty());
            assert!(conn.get_deletions_for_messages(vec![vec![6]])?.is_empty());
            assert!(!conn.is_message_deleted(&[6])?);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_group_deletions() {
        with_connection(|conn| {
            let group1 = GroupId::ONE;
            let group2 = GroupId::TWO;
            let msg1 = vec![7, 8, 9];
            let msg2 = vec![10, 11, 12];
            let del1 = vec![13, 14, 15];
            let del2 = vec![16, 17, 18];

            create_test_group(conn, group1);
            create_test_group(conn, group2);
            create_test_message(conn, msg1.clone(), group1);
            create_test_message(conn, msg2.clone(), group2);
            create_test_message(conn, del1.clone(), group1);
            create_test_message(conn, del2.clone(), group2);

            StoredMessageDeletion {
                id: del1.clone(),
                group_id: group1,
                deleted_message_id: msg1.clone(),
                deleted_by_inbox_id: "sender".to_string(),
                is_super_admin_deletion: false,
                deleted_at_ns: 2000,
            }
            .store(conn)?;

            StoredMessageDeletion {
                id: del2.clone(),
                group_id: group2,
                deleted_message_id: msg2.clone(),
                deleted_by_inbox_id: "sender".to_string(),
                is_super_admin_deletion: false,
                deleted_at_ns: 3000,
            }
            .store(conn)?;

            // Get deletions for group1
            let group1_deletions = conn.get_group_deletions(&group1)?;
            assert_eq!(group1_deletions.len(), 1);
            assert_eq!(group1_deletions[0].deleted_message_id, msg1);

            // Get deletions for group2
            let group2_deletions = conn.get_group_deletions(&group2)?;
            assert_eq!(group2_deletions.len(), 1);
            assert_eq!(group2_deletions[0].deleted_message_id, msg2);
        })
    }
}
