use super::ConnectionExt;
use super::schema::conversation_list::dsl::conversation_list;
use crate::consent_record::ConsentState;
use crate::group::{ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
use crate::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
use crate::{DbConnection, StorageError};
use diesel::dsl::sql;
use diesel::{
    BoolExpressionMethods, ExpressionMethods, JoinOnDsl, QueryDsl, Queryable, RunQueryDsl, Table,
};
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
    /// concatenation of dm participant inbox_ids in alphanumeric order
    pub dm_id: Option<String>,
    /// The last time the leaf node encryption key was rotated
    pub rotated_at_ns: i64,
    /// Enum, [`ConversationType`] signifies the group conversation type which extends to who can access it.
    pub conversation_type: ConversationType,
    /// Whether the commit log for this conversation is forked
    pub is_commit_log_forked: Option<bool>,
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
    /// The Content Type of the message
    pub content_type: Option<ContentType>,
    /// The content type version major
    pub version_major: Option<i32>,
    /// The content type version minor
    pub version_minor: Option<i32>,
    /// The ID of the authority defining the content type
    pub authority_id: Option<String>,
}

pub trait QueryConversationList {
    fn fetch_conversation_list<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<ConversationListItem>, StorageError>;
}

impl<T> QueryConversationList for &T
where
    T: QueryConversationList,
{
    fn fetch_conversation_list<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        (**self).fetch_conversation_list(args)
    }
}

impl<C: ConnectionExt> QueryConversationList for DbConnection<C> {
    fn fetch_conversation_list<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        use crate::schema::consent_records::dsl as consent_dsl;
        use crate::schema::conversation_list::dsl as conversation_list_dsl;

        args.as_ref().validate()?;

        let GroupQueryArgs {
            allowed_states,
            created_after_ns,
            created_before_ns,
            limit,
            conversation_type,
            consent_states,
            include_sync_groups,
            include_duplicate_dms,
            last_activity_after_ns,
            last_activity_before_ns,
            order_by,
            ..
        } = args.as_ref();

        let order_expression = match order_by.clone().unwrap_or_default() {
            GroupQueryOrderBy::CreatedAt => {
                diesel::dsl::sql::<diesel::sql_types::BigInt>("created_at_ns DESC")
            }
            GroupQueryOrderBy::LastActivity => diesel::dsl::sql::<diesel::sql_types::BigInt>(
                "COALESCE(sent_at_ns, created_at_ns) DESC",
            ),
        };

        let mut query = conversation_list
            .select(conversation_list::all_columns())
            .filter(
                conversation_list_dsl::conversation_type.ne_all(ConversationType::virtual_types()),
            )
            .order(order_expression)
            .into_boxed();

        if !include_duplicate_dms {
            // Fast DM deduplication using EXISTS - avoids expensive window functions
            // For each group, ensure no other group exists with same dm_id and newer last_message_ns
            query = query.filter(sql::<diesel::sql_types::Bool>(
                "NOT EXISTS (
                    SELECT 1 FROM groups g2
                    WHERE COALESCE(g2.dm_id, g2.id) = COALESCE(conversation_list.dm_id, conversation_list.id)
                    AND (COALESCE(g2.last_message_ns, 0) > COALESCE((
                        SELECT g1.last_message_ns FROM groups g1 WHERE g1.id = conversation_list.id
                    ), 0)
                    OR (COALESCE(g2.last_message_ns, 0) = COALESCE((
                        SELECT g1.last_message_ns FROM groups g1 WHERE g1.id = conversation_list.id
                    ), 0) AND g2.id > conversation_list.id))
                )",
            ));
        }

        if let Some(limit) = limit {
            query = query.limit(*limit);
        }

        if let Some(allowed_states) = allowed_states {
            query = query.filter(conversation_list_dsl::membership_state.eq_any(allowed_states));
        }

        // last_activity_after_ns takes precedence over created_after_ns
        if let Some(last_activity_after_ns) = last_activity_after_ns {
            // "Activity after" means groups that were either created,
            // or have sent a message after the specified time.
            query = query.filter(
                diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "COALESCE(sent_at_ns, created_at_ns)",
                )
                .gt(last_activity_after_ns),
            );
        }

        if let Some(created_after_ns) = created_after_ns {
            query = query.filter(conversation_list_dsl::created_at_ns.gt(created_after_ns));
        }

        if let Some(last_activity_before_ns) = last_activity_before_ns {
            query = query.filter(
                diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "COALESCE(sent_at_ns, created_at_ns)",
                )
                .lt(last_activity_before_ns),
            );
        }

        if let Some(created_before_ns) = created_before_ns {
            query = query.filter(conversation_list_dsl::created_at_ns.lt(created_before_ns));
        }

        if let Some(conversation_type) = conversation_type {
            query = query.filter(conversation_list_dsl::conversation_type.eq(conversation_type));
        }

        let effective_consent_states = match &consent_states {
            Some(states) if !states.is_empty() => states.clone(),
            _ => vec![ConsentState::Allowed, ConsentState::Unknown],
        };

        let includes_unknown = effective_consent_states.contains(&ConsentState::Unknown);
        let includes_all = effective_consent_states.len() == 3;

        let filtered_states: Vec<_> = effective_consent_states
            .iter()
            .filter(|state| **state != ConsentState::Unknown)
            .cloned()
            .collect();

        let mut conversations = if includes_all {
            // No filtering at all
            self.raw_query_read(|conn| query.load::<ConversationListItem>(conn))?
        } else if includes_unknown {
            // LEFT JOIN: include Unknown + NULL + filtered states
            let left_joined_query = query
                .left_join(
                    consent_dsl::consent_records.on(sql::<diesel::sql_types::Text>(
                        "lower(hex(conversation_list.id))",
                    )
                    .eq(consent_dsl::entity)),
                )
                .filter(
                    consent_dsl::state
                        .is_null()
                        .or(consent_dsl::state.eq(ConsentState::Unknown))
                        .or(consent_dsl::state.eq_any(filtered_states.clone())),
                )
                .select(conversation_list::all_columns());

            self.raw_query_read(|conn| left_joined_query.load::<ConversationListItem>(conn))?
        } else {
            // INNER JOIN: strict match only to specific states (no Unknown or NULL)
            let inner_joined_query = query
                .inner_join(
                    consent_dsl::consent_records.on(sql::<diesel::sql_types::Text>(
                        "lower(hex(conversation_list.id))",
                    )
                    .eq(consent_dsl::entity)),
                )
                .filter(consent_dsl::state.eq_any(filtered_states.clone()))
                .select(conversation_list::all_columns());

            self.raw_query_read(|conn| inner_joined_query.load::<ConversationListItem>(conn))?
        };

        // Were sync groups explicitly asked for? Was the include_sync_groups flag set to true?
        // Then query for those separately
        if matches!(conversation_type, Some(ConversationType::Sync)) || *include_sync_groups {
            let query = conversation_list_dsl::conversation_list
                .filter(conversation_list_dsl::conversation_type.eq(ConversationType::Sync));
            let mut sync_groups = self.raw_query_read(|conn| query.load(conn))?;
            conversations.append(&mut sync_groups);
        }

        Ok(conversations)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::Store;
    use crate::consent_record::{ConsentState, ConsentType};
    use crate::group::tests::{
        generate_consent_record, generate_dm, generate_group, generate_group_with_created_at,
    };
    use crate::group::{GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
    use crate::group_message::ContentType;
    use crate::prelude::*;
    use crate::test_utils::with_connection;

    #[xmtp_common::test]
    async fn test_single_group_multiple_messages() {
        with_connection(|conn| {
            // Create a group
            let group = generate_group(None);
            group.store(conn).unwrap();

            // Insert multiple messages into the group
            for i in 1..5 {
                let message = crate::encrypted_store::group_message::tests::generate_message(
                    None,
                    Some(&group.id),
                    Some(i * 1000),
                    Some(ContentType::Text),
                    None,
                    None,
                );

                message.store(conn).unwrap();
            }

            // Fetch the conversation list
            let conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
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

    #[xmtp_common::test]
    async fn test_three_groups_specific_ordering() {
        with_connection(|conn| {
            // Create three groups
            let group_a = generate_group_with_created_at(None, 5000); // Created after last message
            let group_b = generate_group_with_created_at(None, 2000); // Created before last message
            let group_c = generate_group_with_created_at(None, 1000); // Created before last message with no messages

            group_a.store(conn).unwrap();
            group_b.store(conn).unwrap();
            group_c.store(conn).unwrap();
            // Add a message to group_b
            let message = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group_b.id),
                Some(3000), // Last message timestamp
                None,
                None,
                None,
            );
            message.store(conn).unwrap();

            // Fetch the conversation list
            let conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();

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

    #[xmtp_common::test]
    async fn test_group_with_newer_message_update() {
        with_connection(|conn| {
            // Create a group
            let group = generate_group(None);
            group.store(conn).unwrap();

            // Add an initial message
            let first_message = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group.id),
                Some(1000),
                Some(ContentType::Text),
                None,
                None,
            );
            first_message.store(conn).unwrap();

            // Fetch the conversation list and check last message
            let mut conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(conversation_list.len(), 1, "Should return one group");
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                1000,
                "Last message should match the first message"
            );

            // Add a newer message
            let second_message = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group.id),
                Some(2000),
                Some(ContentType::Text),
                None,
                None,
            );
            second_message.store(conn).unwrap();

            // Fetch the conversation list again and validate the last message is updated
            conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                2000,
                "Last message should now match the second (newest) message"
            );
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_find_conversations_by_consent_state() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).unwrap();
            let test_group_4 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_4.store(conn).unwrap();
            //todo: add test for unknown state

            let test_group_1_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_1.id.clone()),
            );
            test_group_1_consent.store(conn).unwrap();
            let test_group_2_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(test_group_2.id.clone()),
            );
            test_group_2_consent.store(conn).unwrap();
            let test_group_3_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_3.id.clone()),
            );
            test_group_3_consent.store(conn).unwrap();

            let all_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![
                        ConsentState::Allowed,
                        ConsentState::Unknown,
                        ConsentState::Denied,
                    ]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(all_results.len(), 4);

            let default_results = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(default_results.len(), 3);

            let allowed_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(allowed_results.len(), 2);

            let allowed_unknown_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(allowed_unknown_results.len(), 3);

            let denied_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Denied]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(denied_results.len(), 1);
            assert_eq!(denied_results[0].id, test_group_2.id);

            let unknown_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Unknown]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(unknown_results.len(), 1);
            assert_eq!(unknown_results[0].id, test_group_4.id);

            let empty_array_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(empty_array_results.len(), 3);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_find_conversations_default_excludes_denied() {
        with_connection(|conn| {
            // Create three groups: one allowed, one denied, one unknown (no consent)
            let allowed_group = generate_group(Some(GroupMembershipState::Allowed));
            allowed_group.store(conn).unwrap();

            let denied_group = generate_group(Some(GroupMembershipState::Allowed));
            denied_group.store(conn).unwrap();

            let unknown_group = generate_group(Some(GroupMembershipState::Allowed));
            unknown_group.store(conn).unwrap();

            // Create consent records for allowed and denied; leave unknown_group without one
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id.clone()),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id.clone()),
            );
            denied_consent.store(conn).unwrap();

            // Query using default args (no consent_states specified)
            let default_results = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();

            // Expect to include only: allowed_group and unknown_group (2 total)
            assert_eq!(default_results.len(), 2);
            let returned_ids: Vec<_> = default_results.iter().map(|g| &g.id).collect();
            assert!(returned_ids.contains(&&allowed_group.id));
            assert!(returned_ids.contains(&&unknown_group.id));
            assert!(!returned_ids.contains(&&denied_group.id));
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_last_activity_after_ns_filter() {
        with_connection(|conn| {
            // Create groups with specific creation times
            let group1 = generate_group_with_created_at(None, 1000);
            let group2 = generate_group_with_created_at(None, 2000);
            let group3 = generate_group_with_created_at(None, 3000);

            group1.store(conn).unwrap();
            group2.store(conn).unwrap();
            group3.store(conn).unwrap();

            // Add a message to group1 at timestamp 5000
            let message1 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group1.id),
                Some(5000),
                Some(ContentType::Text),
                None,
                None,
            );
            message1.store(conn).unwrap();

            // Add a message to group2 at timestamp 4000
            let message2 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group2.id),
                Some(4000),
                Some(ContentType::Text),
                None,
                None,
            );
            message2.store(conn).unwrap();

            // group3 has no messages, so its activity time is its created_at_ns (3000)

            // Test: last_activity_after_ns = 3500 should return group1 (activity at 5000) and group2 (activity at 4000)
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(3500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                results.len(),
                2,
                "Should return groups with activity after 3500"
            );

            let returned_ids: Vec<_> = results.iter().map(|g| &g.id).collect();
            assert!(
                returned_ids.contains(&&group1.id),
                "Should include group1 (message at 5000)"
            );
            assert!(
                returned_ids.contains(&&group2.id),
                "Should include group2 (message at 4000)"
            );
            assert!(
                !returned_ids.contains(&&group3.id),
                "Should not include group3 (created at 3000)"
            );

            // Test: last_activity_after_ns = 4500 should only return group1
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(4500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 1, "Should return only group1");
            assert_eq!(results[0].id, group1.id, "Should be group1");

            // Test: last_activity_after_ns = 2500 should return all groups
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(2500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 3, "Should return all groups");
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_last_activity_before_ns_filter() {
        with_connection(|conn| {
            // Create groups with specific creation times
            let group1 = generate_group_with_created_at(None, 1000);
            let group2 = generate_group_with_created_at(None, 2000);
            let group3 = generate_group_with_created_at(None, 3000);

            group1.store(conn).unwrap();
            group2.store(conn).unwrap();
            group3.store(conn).unwrap();

            // Add a message to group1 at timestamp 5000
            let message1 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group1.id),
                Some(5000),
                Some(ContentType::Text),
                None,
                None,
            );
            message1.store(conn).unwrap();

            // Add a message to group2 at timestamp 4000
            let message2 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group2.id),
                Some(4000),
                Some(ContentType::Text),
                None,
                None,
            );
            message2.store(conn).unwrap();

            // group3 has no messages, so its activity time is its created_at_ns (3000)

            // Test: last_activity_before_ns = 4500 should return group2 (activity at 4000) and group3 (created at 3000)
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_before_ns: Some(4500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                results.len(),
                2,
                "Should return groups with activity before 4500"
            );

            let returned_ids: Vec<_> = results.iter().map(|g| &g.id).collect();
            assert!(
                !returned_ids.contains(&&group1.id),
                "Should not include group1 (message at 5000)"
            );
            assert!(
                returned_ids.contains(&&group2.id),
                "Should include group2 (message at 4000)"
            );
            assert!(
                returned_ids.contains(&&group3.id),
                "Should include group3 (created at 3000)"
            );

            // Test: last_activity_before_ns = 3500 should only return group3
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_before_ns: Some(3500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 1, "Should return only group3");
            assert_eq!(results[0].id, group3.id, "Should be group3");

            // Test: last_activity_before_ns = 5500 should return all groups
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_before_ns: Some(5500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 3, "Should return all groups");
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_activity_filters_combined_with_limit() {
        with_connection(|conn| {
            // Create multiple groups with different activity times
            let mut groups = Vec::new();
            for i in 0..5 {
                let group = generate_group_with_created_at(None, (i + 1) * 1000);
                group.store(conn).unwrap();

                // Add a message to each group at different times
                let message = crate::encrypted_store::group_message::tests::generate_message(
                    None,
                    Some(&group.id),
                    Some((100 - i) * 1000), // Messages at 100_000, 99_000, 98_000, etc.
                    Some(ContentType::Text),
                    None,
                    None,
                );
                message.store(conn).unwrap();
                groups.push(group);
            }

            // Test: last_activity_after_ns = 7500 with limit = 2
            // Should return groups with messages at 97_000, 98_000, 99_000, 100_000, but only 2 due to limit
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(96_000),
                    limit: Some(2),
                    order_by: Some(GroupQueryOrderBy::LastActivity),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 2, "Should return 2 groups due to limit");

            // Results should be ordered by activity (latest first)
            assert_eq!(
                results[0].sent_at_ns.unwrap(),
                100_000,
                "First should be most recent"
            );
            assert_eq!(
                results[1].sent_at_ns.unwrap(),
                99_000,
                "Second should be second most recent"
            );
        })
        .await
    }
}
