use super::ConnectionExt;
use crate::schema::message_edits::dsl;
use crate::{DbConnection, impl_store, impl_store_or_ignore, schema::message_edits};
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
#[diesel(table_name = message_edits)]
#[diesel(primary_key(id))]
/// Represents an edit record for a message in a group conversation
pub struct StoredMessageEdit {
    /// The ID of the EditMessage in the group_messages table
    pub id: Vec<u8>,
    /// The group this edit belongs to
    pub group_id: Vec<u8>,
    /// The ID of the original message being edited
    pub original_message_id: Vec<u8>,
    /// The inbox_id of who sent the edit message
    pub edited_by_inbox_id: String,
    /// The edited content (serialized EncodedContent)
    pub edited_content: Vec<u8>,
    /// Timestamp when the edit was processed
    pub edited_at_ns: i64,
}

impl_store!(StoredMessageEdit, message_edits);
impl_store_or_ignore!(StoredMessageEdit, message_edits);

/// Trait for querying message edits
pub trait QueryMessageEdit {
    /// Get an edit record by the EditMessage ID
    fn get_message_edit(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError>;

    /// Get edit records for a specific original message
    fn get_edits_by_original_message_id(
        &self,
        original_message_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError>;

    /// Get the latest edit for a specific original message
    fn get_latest_edit_by_original_message_id(
        &self,
        original_message_id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError>;

    /// Get all edits for a list of message IDs
    fn get_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError>;

    /// Get the latest edit for each message in a list of message IDs
    fn get_latest_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError>;

    /// Get all edits in a group
    fn get_group_edits(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError>;

    /// Check if a message has been edited
    fn is_message_edited(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError>;
}

impl<T> QueryMessageEdit for &T
where
    T: QueryMessageEdit,
{
    fn get_message_edit(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_message_edit(id)
    }

    fn get_edits_by_original_message_id(
        &self,
        original_message_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_edits_by_original_message_id(original_message_id)
    }

    fn get_latest_edit_by_original_message_id(
        &self,
        original_message_id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_latest_edit_by_original_message_id(original_message_id)
    }

    fn get_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_edits_for_messages(message_ids)
    }

    fn get_latest_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_latest_edits_for_messages(message_ids)
    }

    fn get_group_edits(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_group_edits(group_id)
    }

    fn is_message_edited(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        (**self).is_message_edited(message_id)
    }
}

impl<C: ConnectionExt> QueryMessageEdit for DbConnection<C> {
    fn get_message_edit(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::id.eq(id))
                .first(conn)
                .optional()
        })
    }

    fn get_edits_by_original_message_id(
        &self,
        original_message_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::original_message_id.eq(original_message_id))
                .order(dsl::edited_at_ns.asc())
                .load(conn)
        })
    }

    fn get_latest_edit_by_original_message_id(
        &self,
        original_message_id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::original_message_id.eq(original_message_id))
                .order(dsl::edited_at_ns.desc())
                .first(conn)
                .optional()
        })
    }

    fn get_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        if message_ids.is_empty() {
            return Ok(vec![]);
        }

        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::original_message_id.eq_any(message_ids))
                .load(conn)
        })
    }

    fn get_latest_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        if message_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = message_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            "SELECT id, group_id, original_message_id, edited_by_inbox_id, edited_content, edited_at_ns
             FROM (
                 SELECT *, ROW_NUMBER() OVER (PARTITION BY original_message_id ORDER BY edited_at_ns DESC) as rn
                 FROM message_edits
                 WHERE original_message_id IN ({})
             ) WHERE rn = 1",
            placeholders
        );

        self.raw_query_read(|conn| {
            use diesel::sql_types::Binary;

            let mut q = diesel::sql_query(query).into_boxed();

            for id in &message_ids {
                q = q.bind::<Binary, _>(id);
            }

            q.load::<StoredMessageEdit>(conn)
        })
    }

    fn get_group_edits(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::group_id.eq(group_id))
                .load(conn)
        })
    }

    fn is_message_edited(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            diesel::dsl::select(diesel::dsl::exists(
                dsl::message_edits.filter(dsl::original_message_id.eq(message_id)),
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
            inserted_at_ns: 0,
            should_push: false,
        }
        .store(conn)
        .unwrap();
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_store_and_get_edit() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let message_id = vec![4, 5, 6];
            let edit_message_id = vec![7, 8, 9];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, message_id.clone(), group_id.clone());
            create_test_message(conn, edit_message_id.clone(), group_id.clone());

            let edit = StoredMessageEdit {
                id: edit_message_id.clone(),
                group_id: group_id.clone(),
                original_message_id: message_id.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"edited content".to_vec(),
                edited_at_ns: 2000,
            };

            edit.store(conn)?;

            // Test get by ID
            let retrieved = conn.get_message_edit(&edit_message_id)?;
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().original_message_id, message_id);

            // Test get by original_message_id
            let by_original_id = conn.get_edits_by_original_message_id(&message_id)?;
            assert_eq!(by_original_id.len(), 1);
            assert_eq!(by_original_id[0].id, edit_message_id);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_is_message_edited() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let message_id = vec![4, 5, 6];
            let edit_message_id = vec![7, 8, 9];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, message_id.clone(), group_id.clone());
            create_test_message(conn, edit_message_id.clone(), group_id.clone());

            // Initially not edited
            assert!(!conn.is_message_edited(&message_id)?);

            // Store edit
            StoredMessageEdit {
                id: edit_message_id.clone(),
                group_id: group_id.clone(),
                original_message_id: message_id.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"edited content".to_vec(),
                edited_at_ns: 2000,
            }
            .store(conn)?;

            // Now it's edited
            assert!(conn.is_message_edited(&message_id)?);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_latest_edit() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let message_id = vec![4, 5, 6];
            let edit1_id = vec![7, 8, 9];
            let edit2_id = vec![10, 11, 12];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, message_id.clone(), group_id.clone());
            create_test_message(conn, edit1_id.clone(), group_id.clone());
            create_test_message(conn, edit2_id.clone(), group_id.clone());

            // Store first edit
            StoredMessageEdit {
                id: edit1_id.clone(),
                group_id: group_id.clone(),
                original_message_id: message_id.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"first edit".to_vec(),
                edited_at_ns: 2000,
            }
            .store(conn)?;

            // Store second edit (later timestamp)
            StoredMessageEdit {
                id: edit2_id.clone(),
                group_id: group_id.clone(),
                original_message_id: message_id.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"second edit".to_vec(),
                edited_at_ns: 3000,
            }
            .store(conn)?;

            // Get latest edit should return the second one
            let latest = conn.get_latest_edit_by_original_message_id(&message_id)?;
            assert!(latest.is_some());
            let latest = latest.unwrap();
            assert_eq!(latest.id, edit2_id);
            assert_eq!(latest.edited_content, b"second edit".to_vec());
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_edits_for_messages() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let msg1 = vec![4, 5, 6];
            let msg2 = vec![7, 8, 9];
            let msg3 = vec![10, 11, 12];
            let edit1 = vec![13, 14, 15];
            let edit2 = vec![16, 17, 18];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, msg1.clone(), group_id.clone());
            create_test_message(conn, msg2.clone(), group_id.clone());
            create_test_message(conn, msg3.clone(), group_id.clone());
            create_test_message(conn, edit1.clone(), group_id.clone());
            create_test_message(conn, edit2.clone(), group_id.clone());

            // Edit msg1 and msg2
            StoredMessageEdit {
                id: edit1.clone(),
                group_id: group_id.clone(),
                original_message_id: msg1.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"edit 1".to_vec(),
                edited_at_ns: 2000,
            }
            .store(conn)?;

            StoredMessageEdit {
                id: edit2.clone(),
                group_id: group_id.clone(),
                original_message_id: msg2.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"edit 2".to_vec(),
                edited_at_ns: 3000,
            }
            .store(conn)?;

            // Query for all three messages
            let edits =
                conn.get_edits_for_messages(vec![msg1.clone(), msg2.clone(), msg3.clone()])?;
            assert_eq!(edits.len(), 2);

            // msg3 should not be edited
            assert!(!conn.is_message_edited(&msg3)?);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_group_edits() {
        with_connection(|conn| {
            let group1 = vec![1, 2, 3];
            let group2 = vec![4, 5, 6];
            let msg1 = vec![7, 8, 9];
            let msg2 = vec![10, 11, 12];
            let edit1 = vec![13, 14, 15];
            let edit2 = vec![16, 17, 18];

            create_test_group(conn, group1.clone());
            create_test_group(conn, group2.clone());
            create_test_message(conn, msg1.clone(), group1.clone());
            create_test_message(conn, msg2.clone(), group2.clone());
            create_test_message(conn, edit1.clone(), group1.clone());
            create_test_message(conn, edit2.clone(), group2.clone());

            StoredMessageEdit {
                id: edit1.clone(),
                group_id: group1.clone(),
                original_message_id: msg1.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"edit 1".to_vec(),
                edited_at_ns: 2000,
            }
            .store(conn)?;

            StoredMessageEdit {
                id: edit2.clone(),
                group_id: group2.clone(),
                original_message_id: msg2.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"edit 2".to_vec(),
                edited_at_ns: 3000,
            }
            .store(conn)?;

            // Get edits for group1
            let group1_edits = conn.get_group_edits(&group1)?;
            assert_eq!(group1_edits.len(), 1);
            assert_eq!(group1_edits[0].original_message_id, msg1);

            // Get edits for group2
            let group2_edits = conn.get_group_edits(&group2)?;
            assert_eq!(group2_edits.len(), 1);
            assert_eq!(group2_edits[0].original_message_id, msg2);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_get_latest_edits_for_messages() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let msg1 = vec![4, 5, 6];
            let msg2 = vec![7, 8, 9];
            let msg3 = vec![10, 11, 12]; // not edited
            let edit1_v1 = vec![13, 14, 15];
            let edit1_v2 = vec![16, 17, 18];
            let edit2_v1 = vec![19, 20, 21];

            create_test_group(conn, group_id.clone());
            create_test_message(conn, msg1.clone(), group_id.clone());
            create_test_message(conn, msg2.clone(), group_id.clone());
            create_test_message(conn, msg3.clone(), group_id.clone());
            create_test_message(conn, edit1_v1.clone(), group_id.clone());
            create_test_message(conn, edit1_v2.clone(), group_id.clone());
            create_test_message(conn, edit2_v1.clone(), group_id.clone());

            // msg1 has two edits - first edit (older)
            StoredMessageEdit {
                id: edit1_v1.clone(),
                group_id: group_id.clone(),
                original_message_id: msg1.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"msg1 first edit".to_vec(),
                edited_at_ns: 2000,
            }
            .store(conn)?;

            // msg1 has two edits - second edit (newer, should be returned)
            StoredMessageEdit {
                id: edit1_v2.clone(),
                group_id: group_id.clone(),
                original_message_id: msg1.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"msg1 second edit".to_vec(),
                edited_at_ns: 4000,
            }
            .store(conn)?;

            // msg2 has one edit
            StoredMessageEdit {
                id: edit2_v1.clone(),
                group_id: group_id.clone(),
                original_message_id: msg2.clone(),
                edited_by_inbox_id: "sender".to_string(),
                edited_content: b"msg2 only edit".to_vec(),
                edited_at_ns: 3000,
            }
            .store(conn)?;

            // Query for all three messages - should get only 2 results (latest per message)
            let latest_edits =
                conn.get_latest_edits_for_messages(vec![msg1.clone(), msg2.clone(), msg3.clone()])?;
            assert_eq!(latest_edits.len(), 2);

            // Find the edit for msg1 - should be the second edit (newer timestamp)
            let msg1_edit = latest_edits
                .iter()
                .find(|e| e.original_message_id == msg1)
                .expect("msg1 should have an edit");
            assert_eq!(msg1_edit.id, edit1_v2);
            assert_eq!(msg1_edit.edited_content, b"msg1 second edit".to_vec());
            assert_eq!(msg1_edit.edited_at_ns, 4000);

            // Find the edit for msg2
            let msg2_edit = latest_edits
                .iter()
                .find(|e| e.original_message_id == msg2)
                .expect("msg2 should have an edit");
            assert_eq!(msg2_edit.id, edit2_v1);
            assert_eq!(msg2_edit.edited_content, b"msg2 only edit".to_vec());

            // Verify msg3 is not in the results (it was never edited)
            assert!(!latest_edits.iter().any(|e| e.original_message_id == msg3));

            // Test empty input
            let empty_result = conn.get_latest_edits_for_messages(vec![])?;
            assert!(empty_result.is_empty());
        })
    }
}
