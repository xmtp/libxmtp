use crate::storage::group::{ConversationType, GroupMembershipState};
use crate::storage::group_message::{DeliveryStatus, GroupMessageKind};
use crate::storage::schema::conversation_list::dsl::conversation_list;
use crate::storage::{DbConnection, StorageError};
use crate::{Fetch, FetchListWithKey};
use diesel::{Identifiable, QueryDsl, Queryable, RunQueryDsl, Table};
use serde::{Deserialize, Serialize};

#[derive(Queryable, Debug, Clone, Deserialize, Serialize)]
#[diesel(table_name = conversation_list)]
#[diesel(primary_key(id))]
/// Combined view of a group and its messages, now named `conversation_list`.
pub struct ConversationListItem {
    /// group_id
    pub id: Vec<u8>,
    /// Based on timestamp of the welcome message
    pub created_at_ns: i64,
    /// Enum, [`GroupMembershipState`] representing access to the group
    pub membership_state: GroupMembershipState,
    /// Track when the latest, most recent installations were checked
    pub installations_last_checked: i64,
    /// The inbox_id of who added the user to the group
    pub added_by_inbox_id: String,
    /// The sequence id of the welcome message
    pub welcome_id: Option<i64>,
    /// The inbox_id of the DM target
    pub dm_inbox_id: Option<String>,
    /// The last time the leaf node encryption key was rotated
    pub rotated_at_ns: i64,
    /// Enum, [`ConversationType`] signifies the group conversation type which extends to who can access it.
    pub conversation_type: ConversationType,
    /// Id of the message. Nullable because not every group has messages.
    pub message_id: Option<Vec<u8>>,
    /// Contents of message after decryption.
    pub decrypted_message_bytes: Option<Vec<u8>>,
    /// Time in nanoseconds the message was sent.
    pub sent_at_ns: Option<i64>,
    /// Group Message Kind Enum: 1 = Application, 2 = MembershipChange
    pub kind: Option<GroupMessageKind>,
    /// The ID of the App Installation this message was sent from.
    pub sender_installation_id: Option<Vec<u8>>,
    /// The Inbox ID of the Sender
    pub sender_inbox_id: Option<String>,
    /// We optimistically store messages before sending.
    pub delivery_status: Option<DeliveryStatus>,
}

impl DbConnection {
    pub fn fetch_conversation_list(&self) -> Result<Vec<ConversationListItem>, StorageError> {
        let query = conversation_list
            .select(conversation_list::all_columns())
            .into_boxed();
        Ok(self.raw_query(|conn| query.load::<ConversationListItem>(conn))?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::storage::group::tests::{generate_group, generate_group_with_created_at};
    use crate::storage::tests::with_connection;
    use crate::Store;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_single_group_multiple_messages() {
        with_connection(|conn| {
            // Create a group
            let group = generate_group(None);
            group.store(conn).unwrap();

            // Insert multiple messages into the group
            for i in 1..5 {
                let message =
                    crate::storage::encrypted_store::group_message::tests::generate_message(
                        None,
                        Some(&group.id),
                        Some(i * 1000), // Increment timestamp for each message
                    );
                message.store(conn).unwrap();
            }

            // Fetch the conversation list
            let conversation_list = conn.fetch_conversation_list().unwrap();
            assert_eq!(conversation_list.len(), 1, "Should return one group");
            assert_eq!(
                conversation_list[0].id, group.id,
                "Returned group ID should match the created group"
            );
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                4000,
                "Last message should be the most recent one"
            );
        })
        .await
    }
    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_three_groups_specific_ordering() {
        with_connection(|conn| {
            // Create three groups
            let group_a = generate_group_with_created_at(None,5000); // Created after last message
            let group_b = generate_group_with_created_at(None,2000); // Created before last message
            let group_c = generate_group_with_created_at(None,1000); // Created before last message with no messages

            group_a.store(conn).unwrap();
            group_b.store(conn).unwrap();
            group_c.store(conn).unwrap();
            // Add a message to group_b
            let message = crate::storage::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group_b.id),
                Some(3000), // Last message timestamp
            );
            message.store(conn).unwrap();

            // Fetch the conversation list
            let conversation_list = conn.fetch_conversation_list().unwrap();

            assert_eq!(conversation_list.len(), 3, "Should return all three groups");
            assert_eq!(
                conversation_list[0].id, group_a.id,
                "Group created after the last message should come first"
            );
            assert_eq!(
                conversation_list[1].id, group_b.id,
                "Group with the last message should come second"
            );
            assert_eq!(
                conversation_list[2].id, group_c.id,
                "Group created before the last message with no messages should come last"
            );
        })
        .await
    }
    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_group_with_newer_message_update() {
        with_connection(|conn| {
            // Create a group
            let group = generate_group(None);
            group.store(conn).unwrap();

            // Add an initial message
            let first_message =
                crate::storage::encrypted_store::group_message::tests::generate_message(
                    None,
                    Some(&group.id),
                    Some(1000),
                );
            first_message.store(conn).unwrap();

            // Fetch the conversation list and check last message
            let mut conversation_list = conn.fetch_conversation_list().unwrap();
            assert_eq!(conversation_list.len(), 1, "Should return one group");
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                1000,
                "Last message should match the first message"
            );

            // Add a newer message
            let second_message =
                crate::storage::encrypted_store::group_message::tests::generate_message(
                    None,
                    Some(&group.id),
                    Some(2000),
                );
            second_message.store(conn).unwrap();

            // Fetch the conversation list again and validate the last message is updated
            conversation_list = conn.fetch_conversation_list().unwrap();
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                2000,
                "Last message should now match the second (newest) message"
            );
        })
        .await
    }
}
