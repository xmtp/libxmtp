use super::ConnectionExt;
use crate::schema::message_deletions::dsl;
use crate::{DbConnection, impl_store, impl_store_or_ignore, schema::message_deletions};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

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
    pub group_id: Vec<u8>,
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

    /// Get all deletions for a list of message IDs
    fn get_deletions_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError>;

    /// Get all deletions in a group
    fn get_group_deletions(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError>;

    /// Check if a message has been deleted
    fn is_message_deleted(
        &self,
        message_id: &[u8],
    ) -> Result<bool, crate::ConnectionError>;
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
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError> {
        (**self).get_group_deletions(group_id)
    }

    fn is_message_deleted(
        &self,
        message_id: &[u8],
    ) -> Result<bool, crate::ConnectionError> {
        (**self).is_message_deleted(message_id)
    }
}

impl<C: ConnectionExt> QueryMessageDeletion for DbConnection<C> {
    fn get_message_deletion(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageDeletion>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
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
        self.raw_query_read(|conn| {
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

        self.raw_query_read(|conn| {
            dsl::message_deletions
                .filter(dsl::deleted_message_id.eq_any(message_ids))
                .load(conn)
        })
    }

    fn get_group_deletions(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageDeletion>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_deletions
                .filter(dsl::group_id.eq(group_id))
                .load(conn)
        })
    }

    fn is_message_deleted(
        &self,
        message_id: &[u8],
    ) -> Result<bool, crate::ConnectionError> {
        self.raw_query_read(|conn| {
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
    use crate::{Store, with_connection};
    use crate::encrypted_store::group_message::{StoredGroupMessage, GroupMessageKind, DeliveryStatus, ContentType};
    use crate::encrypted_store::group::{StoredGroup, ConversationType, GroupMembershipState};

    fn create_test_group(conn: &DbConnection<impl ConnectionExt>, group_id: Vec<u8>) {
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
            originator_id: None,
            should_publish_commit_log: false,
            commit_log_public_key: None,
            is_commit_log_forked: None,
            has_pending_leave_request: None,
        }
        .store(conn)
        .unwrap();
    }

    fn create_test_message(
        conn: &DbConnection<impl ConnectionExt>,
        id: Vec<u8>,
        group_id: Vec<u8>,
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
            expire_at_ns: None,
            sequence_id: 1,
            originator_id: 1,
        }
        .store(conn)
        .unwrap();
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_store_and_get_deletion() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let message_id = vec![4, 5, 6];
            let delete_message_id = vec![7, 8, 9];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, message_id.clone(), group_id.clone());
            create_test_message(conn, delete_message_id.clone(), group_id.clone());

            let deletion = StoredMessageDeletion {
                id: delete_message_id.clone(),
                group_id: group_id.clone(),
                deleted_message_id: message_id.clone(),
                deleted_by_inbox_id: "sender".to_string(),
                is_super_admin_deletion: false,
                deleted_at_ns: 2000,
            };

            deletion.store(conn)?;

            // Test get by ID
            let retrieved = conn.get_message_deletion(&delete_message_id)?;
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().deleted_message_id, message_id);

            // Test get by deleted_message_id
            let by_deleted_id = conn.get_deletion_by_deleted_message_id(&message_id)?;
            assert!(by_deleted_id.is_some());
            assert_eq!(by_deleted_id.unwrap().id, delete_message_id);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_is_message_deleted() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let message_id = vec![4, 5, 6];
            let delete_message_id = vec![7, 8, 9];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, message_id.clone(), group_id.clone());
            create_test_message(conn, delete_message_id.clone(), group_id.clone());

            // Initially not deleted
            assert!(!conn.is_message_deleted(&message_id)?);

            // Store deletion
            StoredMessageDeletion {
                id: delete_message_id.clone(),
                group_id: group_id.clone(),
                deleted_message_id: message_id.clone(),
                deleted_by_inbox_id: "sender".to_string(),
                is_super_admin_deletion: false,
                deleted_at_ns: 2000,
            }
            .store(conn)?;

            // Now it's deleted
            assert!(conn.is_message_deleted(&message_id)?);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_deletions_for_messages() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let msg1 = vec![4, 5, 6];
            let msg2 = vec![7, 8, 9];
            let msg3 = vec![10, 11, 12];
            let del1 = vec![13, 14, 15];
            let del2 = vec![16, 17, 18];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, msg1.clone(), group_id.clone());
            create_test_message(conn, msg2.clone(), group_id.clone());
            create_test_message(conn, msg3.clone(), group_id.clone());
            create_test_message(conn, del1.clone(), group_id.clone());
            create_test_message(conn, del2.clone(), group_id.clone());

            // Delete msg1 and msg2
            StoredMessageDeletion {
                id: del1.clone(),
                group_id: group_id.clone(),
                deleted_message_id: msg1.clone(),
                deleted_by_inbox_id: "sender".to_string(),
                is_super_admin_deletion: false,
                deleted_at_ns: 2000,
            }
            .store(conn)?;

            StoredMessageDeletion {
                id: del2.clone(),
                group_id: group_id.clone(),
                deleted_message_id: msg2.clone(),
                deleted_by_inbox_id: "admin".to_string(),
                is_super_admin_deletion: true,
                deleted_at_ns: 3000,
            }
            .store(conn)?;

            // Query for all three messages
            let deletions = conn.get_deletions_for_messages(vec![msg1.clone(), msg2.clone(), msg3.clone()])?;
            assert_eq!(deletions.len(), 2);

            // msg3 should not be deleted
            assert!(!conn.is_message_deleted(&msg3)?);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_group_deletions() {
        with_connection(|conn| {
            let group1 = vec![1, 2, 3];
            let group2 = vec![4, 5, 6];
            let msg1 = vec![7, 8, 9];
            let msg2 = vec![10, 11, 12];
            let del1 = vec![13, 14, 15];
            let del2 = vec![16, 17, 18];

            create_test_group(conn, group1.clone());
            create_test_group(conn, group2.clone());
            create_test_message(conn, msg1.clone(), group1.clone());
            create_test_message(conn, msg2.clone(), group2.clone());
            create_test_message(conn, del1.clone(), group1.clone());
            create_test_message(conn, del2.clone(), group2.clone());

            StoredMessageDeletion {
                id: del1.clone(),
                group_id: group1.clone(),
                deleted_message_id: msg1.clone(),
                deleted_by_inbox_id: "sender".to_string(),
                is_super_admin_deletion: false,
                deleted_at_ns: 2000,
            }
            .store(conn)?;

            StoredMessageDeletion {
                id: del2.clone(),
                group_id: group2.clone(),
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
        .await
    }
}
