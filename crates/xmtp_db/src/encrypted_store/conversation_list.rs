use super::ConnectionExt;
use crate::consent_record::ConsentState;
use crate::group::{ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
use crate::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
use crate::{DbConnection, StorageError};
use diesel::sql_types::{BigInt, Integer};
use diesel::{QueryableByName, RunQueryDsl, sql_query};
use serde::{Deserialize, Serialize};

/// Content types eligible for the latest conversation message.
pub const CONVERSATION_LIST_CONTENT_TYPES: &[ContentType] = &[
    ContentType::Unknown,
    ContentType::Text,
    ContentType::Reaction,
    ContentType::Reply,
    ContentType::Attachment,
    ContentType::RemoteAttachment,
    ContentType::TransactionReference,
    ContentType::WalletSendCalls,
];

#[derive(QueryableByName, Debug, Clone, Deserialize, Serialize)]
/// A group and its latest app-visible message, when one exists.
pub struct ConversationListItem {
    /// Group ID.
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub id: xmtp_proto::types::GroupId,
    /// Time of the Welcome.
    #[diesel(sql_type = BigInt)]
    pub created_at_ns: i64,
    /// Current membership state.
    #[diesel(sql_type = Integer)]
    pub membership_state: GroupMembershipState,
    /// Last installation check.
    #[diesel(sql_type = BigInt)]
    pub installations_last_checked: i64,
    /// Inbox that added this installation.
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub added_by_inbox_id: String,
    /// Welcome sequence ID.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub welcome_sequence_id: Option<i64>,
    /// Canonical DM identity, if this is a DM.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub dm_id: Option<String>,
    /// Last leaf-key rotation.
    #[diesel(sql_type = BigInt)]
    pub rotated_at_ns: i64,
    /// Conversation kind.
    #[diesel(sql_type = Integer)]
    pub conversation_type: ConversationType,
    /// Whether the remote commit log is forked.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Bool>)]
    pub is_commit_log_forked: Option<bool>,
    /// Message ID, absent when no eligible live message exists.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Binary>)]
    pub message_id: Option<Vec<u8>>,
    /// Decrypted message content.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Binary>)]
    pub decrypted_message_bytes: Option<Vec<u8>>,
    /// Message send time.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub sent_at_ns: Option<i64>,
    /// Message kind.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub kind: Option<GroupMessageKind>,
    /// Sender installation.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Binary>)]
    pub sender_installation_id: Option<Vec<u8>>,
    /// Sender inbox.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub sender_inbox_id: Option<String>,
    /// Delivery state.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub delivery_status: Option<DeliveryStatus>,
    /// Content type.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub content_type: Option<ContentType>,
    /// Content type major version.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub version_major: Option<i32>,
    /// Content type minor version.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub version_minor: Option<i32>,
    /// Content type authority.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub authority_id: Option<String>,
    /// Message sequence ID.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub sequence_id: Option<i64>,
    /// Backend retention deadline, separate from disappearing-message expiry.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub expiry_ns: Option<i64>,
    /// Disappearing-message deadline.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub expire_at_ns: Option<i64>,
}

/// Build one row per group with the latest message still visible at the supplied
/// time. The expiry predicate must remain inside the ranked set.
fn conversation_list_cte() -> String {
    let content_types = CONVERSATION_LIST_CONTENT_TYPES
        .iter()
        .map(|value| (*value as i32).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "WITH ranked_messages AS (
            SELECT gm.group_id, gm.id AS message_id,
                   gm.decrypted_message_bytes, gm.sent_at_ns, gm.kind,
                   gm.sender_installation_id, gm.sender_inbox_id,
                   gm.delivery_status, gm.content_type, gm.version_major,
                   gm.version_minor, gm.authority_id, gm.sequence_id,
                   gm.expiry_ns, gm.expire_at_ns,
                   ROW_NUMBER() OVER (
                       PARTITION BY gm.group_id
                       ORDER BY gm.sent_at_ns DESC, gm.id DESC
                   ) AS row_num
            FROM group_messages gm
            WHERE gm.kind = {application_kind}
              AND gm.content_type IN ({content_types})
              AND (gm.expire_at_ns IS NULL OR gm.expire_at_ns > ?)
        ), conversation_list AS (
            SELECT g.id, g.created_at_ns, g.membership_state,
                   g.installations_last_checked, g.added_by_inbox_id,
                   g.sequence_id AS welcome_sequence_id, g.dm_id,
                   g.rotated_at_ns, g.conversation_type,
                   g.is_commit_log_forked, rm.message_id,
                   rm.decrypted_message_bytes, rm.sent_at_ns, rm.kind,
                   rm.sender_installation_id, rm.sender_inbox_id,
                   rm.delivery_status, rm.content_type, rm.version_major,
                   rm.version_minor, rm.authority_id, rm.sequence_id,
                   rm.expiry_ns, rm.expire_at_ns
            FROM groups g
            LEFT JOIN ranked_messages rm
              ON g.id = rm.group_id AND rm.row_num = 1
        )",
        application_kind = GroupMessageKind::Application as i32,
    )
}

fn enum_values<T>(values: &[T], to_i32: impl Fn(&T) -> i32) -> String {
    values
        .iter()
        .map(|value| to_i32(value).to_string())
        .collect::<Vec<_>>()
        .join(", ")
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
        self.fetch_conversation_list_at(args.as_ref(), xmtp_common::time::now_ns())
    }
}

impl<C: ConnectionExt> DbConnection<C> {
    // implements: CONS-030
    // implements: META-051
    fn fetch_conversation_list_at(
        &self,
        args: &GroupQueryArgs,
        current_time_ns: i64,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        args.validate()?;
        if matches!(&args.consent_states, Some(states) if states.is_empty()) {
            return Ok(Vec::new());
        }

        let effective_consent_states = args
            .consent_states
            .clone()
            .unwrap_or_else(|| vec![ConsentState::Allowed, ConsentState::Unknown]);
        let includes_unknown = effective_consent_states.contains(&ConsentState::Unknown);
        let includes_all = effective_consent_states.len() == 3;
        let filtered_states: Vec<_> = effective_consent_states
            .iter()
            .filter(|state| **state != ConsentState::Unknown)
            .copied()
            .collect();

        let mut query = sql_query(format!(
            "{} SELECT c.* FROM conversation_list c",
            conversation_list_cte()
        ))
        .into_boxed::<diesel::sqlite::Sqlite>()
        .bind::<BigInt, _>(current_time_ns);

        if !includes_all {
            query = query.sql(
                " LEFT JOIN consent_records consent
                  ON consent.entity = lower(hex(c.id))",
            );
        }
        query = query.sql(format!(
            " WHERE c.conversation_type NOT IN ({}, {})",
            ConversationType::Sync as i32,
            ConversationType::Oneshot as i32
        ));

        if !args.include_duplicate_dms {
            query = query.sql(format!(
                " AND NOT EXISTS (
                    SELECT 1 FROM groups g2
                    WHERE COALESCE(g2.dm_id, g2.id) = COALESCE(c.dm_id, c.id)
                    AND (g2.membership_state != {restored}, COALESCE(g2.last_message_ns, 0), g2.id)
                      > (c.membership_state != {restored}, COALESCE((
                           SELECT g1.last_message_ns FROM groups g1 WHERE g1.id = c.id
                         ), 0), c.id)
                )",
                restored = GroupMembershipState::Restored as i32,
            ));
        }

        if let Some(states) = &args.allowed_states {
            if states.is_empty() {
                query = query.sql(" AND 0");
            } else {
                query = query.sql(format!(
                    " AND c.membership_state IN ({})",
                    enum_values(states, |state| *state as i32)
                ));
            }
        }
        if let Some(after) = args.last_activity_after_ns {
            query = query
                .sql(" AND COALESCE(c.sent_at_ns, c.created_at_ns) > ?")
                .bind::<BigInt, _>(after);
        }
        if let Some(after) = args.created_after_ns {
            query = query
                .sql(" AND c.created_at_ns > ?")
                .bind::<BigInt, _>(after);
        }
        if let Some(before) = args.last_activity_before_ns {
            query = query
                .sql(" AND COALESCE(c.sent_at_ns, c.created_at_ns) < ?")
                .bind::<BigInt, _>(before);
        }
        if let Some(before) = args.created_before_ns {
            query = query
                .sql(" AND c.created_at_ns < ?")
                .bind::<BigInt, _>(before);
        }
        if let Some(conversation_type) = args.conversation_type {
            query = query
                .sql(" AND c.conversation_type = ?")
                .bind::<Integer, _>(conversation_type as i32);
        }

        if !includes_all {
            if includes_unknown {
                query = query.sql(" AND (consent.state IS NULL OR consent.state = 0");
                if !filtered_states.is_empty() {
                    query = query.sql(format!(
                        " OR consent.state IN ({})",
                        enum_values(&filtered_states, |state| *state as i32)
                    ));
                }
                query = query.sql(")");
            } else {
                query = query.sql(format!(
                    " AND consent.state IN ({})",
                    enum_values(&filtered_states, |state| *state as i32)
                ));
            }
        }

        query = match args.order_by.clone().unwrap_or_default() {
            GroupQueryOrderBy::CreatedAt => query.sql(" ORDER BY c.created_at_ns DESC"),
            GroupQueryOrderBy::LastActivity => {
                query.sql(" ORDER BY COALESCE(c.sent_at_ns, c.created_at_ns) DESC")
            }
        };
        if let Some(limit) = args.limit {
            query = query.sql(" LIMIT ?").bind::<BigInt, _>(limit);
        }

        let mut conversations = self.raw_query(|conn| query.load::<ConversationListItem>(conn))?;

        // Sync groups bypass the regular filters and limit, as before.
        if matches!(args.conversation_type, Some(ConversationType::Sync))
            || args.include_sync_groups
        {
            let sync = sql_query(format!(
                "{} SELECT c.* FROM conversation_list c WHERE c.conversation_type = ?",
                conversation_list_cte()
            ))
            .bind::<BigInt, _>(current_time_ns)
            .bind::<Integer, _>(ConversationType::Sync as i32);
            let mut sync_groups = self.raw_query(|conn| sync.load::<ConversationListItem>(conn))?;
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
    use crate::group::{ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
    use crate::group_message::ContentType;
    use crate::group_message::tests::generate_message;
    use crate::prelude::*;
    use crate::test_utils::with_connection;

    // verifies: META-051
    #[xmtp_common::test(unwrap_try = true)]
    fn latest_conversation_preview_uses_live_rows_before_ranking() {
        let now = 1_000_000;
        with_connection(|conn| -> Result<(), crate::StorageError> {
            let mixed = generate_group_with_created_at(None, 100);
            let persistent = generate_group_with_created_at(None, 150);
            let empty = generate_group_with_created_at(None, 175);
            let all_expired = generate_group_with_created_at(None, 250);
            for group in [&mixed, &persistent, &empty, &all_expired] {
                group.store(conn)?;
            }

            let mut older_live = generate_message(
                None,
                Some(&mixed.id),
                Some(300),
                Some(ContentType::Text),
                Some(now + 1),
                None,
            );
            older_live.expiry_ns = Some(now + 100);
            older_live.store(conn)?;
            let newer_expired = generate_message(
                None,
                Some(&mixed.id),
                Some(500),
                Some(ContentType::Text),
                Some(now - 1),
                None,
            );
            newer_expired.store(conn)?;
            let equality_expired = generate_message(
                None,
                Some(&all_expired.id),
                Some(450),
                Some(ContentType::Text),
                Some(now),
                None,
            );
            equality_expired.store(conn)?;
            let no_deadline = generate_message(
                None,
                Some(&persistent.id),
                Some(350),
                Some(ContentType::Text),
                None,
                None,
            );
            no_deadline.store(conn)?;

            assert!(conn.get_group_message(&newer_expired.id)?.is_some());
            assert!(conn.get_group_message(&equality_expired.id)?.is_some());
            let args = GroupQueryArgs {
                order_by: Some(GroupQueryOrderBy::LastActivity),
                ..Default::default()
            };
            let listed = conn.fetch_conversation_list_at(&args, now)?;
            assert_eq!(listed.len(), 4);
            assert_eq!(listed[0].id, persistent.id);
            assert_eq!(
                listed[0].message_id.as_deref(),
                Some(no_deadline.id.as_slice())
            );
            assert_eq!(listed[1].id, mixed.id);
            assert_eq!(
                listed[1].message_id.as_deref(),
                Some(older_live.id.as_slice())
            );
            assert_eq!(listed[1].expire_at_ns, older_live.expire_at_ns);
            assert_eq!(listed[1].expiry_ns, older_live.expiry_ns);
            assert_eq!(listed[2].id, all_expired.id);
            assert!(listed[2].message_id.is_none());
            assert_eq!(listed[3].id, empty.id);
            assert!(listed[3].message_id.is_none());

            let after = conn.fetch_conversation_list_at(
                &GroupQueryArgs {
                    last_activity_after_ns: Some(325),
                    ..args.clone()
                },
                now,
            )?;
            assert_eq!(after.len(), 1);
            assert_eq!(after[0].id, persistent.id);

            let bounded = conn.fetch_conversation_list_at(
                &GroupQueryArgs {
                    limit: Some(2),
                    ..args
                },
                now,
            )?;
            assert_eq!(bounded.len(), 2);
            assert_eq!(bounded[0].id, persistent.id);
            assert_eq!(bounded[1].id, mixed.id);
            Ok(())
        })?;
    }

    #[xmtp_common::test]
    fn test_single_group_multiple_messages() {
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
    }

    #[xmtp_common::test]
    fn test_three_groups_specific_ordering() {
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
    }

    #[xmtp_common::test]
    fn test_group_with_newer_message_update() {
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
    }

    // verifies: CONS-030
    #[xmtp_common::test]
    fn test_find_conversations_by_consent_state() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).unwrap();
            let test_group_4 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_4.store(conn).unwrap();

            let test_group_1_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_1.id),
            );
            test_group_1_consent.store(conn).unwrap();
            let test_group_2_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(test_group_2.id),
            );
            test_group_2_consent.store(conn).unwrap();
            let test_group_3_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_3.id),
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
            assert!(empty_array_results.is_empty());

            let mut sync_group = generate_group(Some(GroupMembershipState::Allowed));
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn).unwrap();
            let with_sync = conn
                .fetch_conversation_list(GroupQueryArgs {
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            assert!(with_sync.iter().any(|group| group.id == sync_group.id));
            let empty_with_sync = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            assert!(empty_with_sync.is_empty());
        })
    }

    /// A `Restored` archive placeholder for a DM must not hide the row the
    /// client can act in, even when the placeholder holds the newer message
    /// and the higher id.
    #[xmtp_common::test]
    fn test_dm_list_prefers_joined_over_restored() {
        use crate::group::StoredGroup;
        use xmtp_common::{Generate, time::now_ns};
        use xmtp_proto::types::GroupId;

        with_connection(|conn| {
            let dm_id = "dm:alice:bob";
            let (low_id, high_id) = {
                let a = GroupId::generate();
                let b = GroupId::generate();
                if a.as_ref() < b.as_ref() {
                    (a, b)
                } else {
                    (b, a)
                }
            };
            let now = now_ns();

            let restored = StoredGroup::builder()
                .id(high_id)
                .created_at_ns(now)
                .last_message_ns(now)
                .membership_state(GroupMembershipState::Restored)
                .added_by_inbox_id("alice")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            restored.store(conn).unwrap();

            let active = StoredGroup::builder()
                .id(low_id)
                .created_at_ns(now)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("alice")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            active.store(conn).unwrap();

            let listed = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            let dm_rows: Vec<_> = listed
                .iter()
                .filter(|conversation| conversation.dm_id.as_deref() == Some(dm_id))
                .collect();
            assert_eq!(dm_rows.len(), 1);
            assert_eq!(dm_rows[0].id, active.id);
        })
    }

    #[xmtp_common::test]
    fn test_find_conversations_default_excludes_denied() {
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
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
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
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_unknown_content_type_is_present() {
        with_connection(|conn| {
            let dm = generate_dm(None);
            dm.store(conn)?;

            let m = generate_message(
                None,
                Some(&dm.id),
                Some(5000),
                Some(ContentType::Unknown),
                None,
                None,
            );
            m.store(conn)?;

            let conv = conn.fetch_conversation_list(GroupQueryArgs {
                ..Default::default()
            })?;

            // Message id should be present
            assert!(conv[0].message_id.is_some());
        })
    }

    #[xmtp_common::test]
    fn test_last_activity_after_ns_filter() {
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
    }

    #[xmtp_common::test]
    fn test_last_activity_before_ns_filter() {
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
    }

    #[xmtp_common::test]
    fn test_activity_filters_combined_with_limit() {
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
    }
}
