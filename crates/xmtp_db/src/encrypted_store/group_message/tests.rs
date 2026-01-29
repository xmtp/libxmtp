#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use super::*;
use crate::{Store, group::tests::generate_group, prelude::*, test_utils::with_connection};
use xmtp_common::{assert_err, assert_ok, rand_time, rand_vec};

pub(crate) fn generate_message(
    kind: Option<GroupMessageKind>,
    group_id: Option<&[u8]>,
    sent_at_ns: Option<i64>,
    content_type: Option<ContentType>,
    expire_at_ns: Option<i64>,
    sender_inbox_id: Option<String>,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id: rand_vec::<24>(),
        group_id: group_id.map(<[u8]>::to_vec).unwrap_or(rand_vec::<24>()),
        decrypted_message_bytes: rand_vec::<24>(),
        sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
        sender_installation_id: rand_vec::<24>(),
        sender_inbox_id: sender_inbox_id.unwrap_or("0x0".to_string()),
        kind: kind.unwrap_or(GroupMessageKind::Application),
        delivery_status: DeliveryStatus::Published,
        content_type: content_type.unwrap_or(ContentType::Unknown),
        version_major: 0,
        version_minor: 0,
        authority_id: "unknown".to_string(),
        reference_id: None,
        sequence_id: 0,
        originator_id: 0,
        expire_at_ns,
        inserted_at_ns: 0, // Will be set by database
        should_push: true,
    }
}

#[xmtp_common::test]
fn it_does_not_error_on_empty_messages() {
    with_connection(|conn| {
        let id = vec![0x0];
        assert_eq!(conn.get_group_message(id).unwrap(), None);
    })
}

#[xmtp_common::test]
fn test_exclude_content_types_filter() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create messages with different content types
        let messages = vec![
            generate_message(
                None,
                Some(&group.id),
                Some(1_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(3_000),
                Some(ContentType::Reaction),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(4_000),
                Some(ContentType::ReadReceipt),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(5_000),
                Some(ContentType::Attachment),
                None,
                None,
            ),
        ];
        assert_ok!(messages.store(conn));

        // Test excluding reactions and read receipts
        let exclude_args = MsgQueryArgs {
            exclude_content_types: Some(vec![ContentType::Reaction, ContentType::ReadReceipt]),
            ..Default::default()
        };

        let filtered_messages = conn.get_group_messages(&group.id, &exclude_args).unwrap();
        assert_eq!(filtered_messages.len(), 3); // 2 Text + 1 Attachment
        assert!(
            filtered_messages
                .iter()
                .all(|m| m.content_type != ContentType::Reaction
                    && m.content_type != ContentType::ReadReceipt)
        );

        let count = conn.count_group_messages(&group.id, &exclude_args).unwrap();
        assert_eq!(count, 3);
    })
}

#[xmtp_common::test]
fn it_gets_messages() {
    with_connection(|conn| {
        let group = generate_group(None);
        let message = generate_message(None, Some(&group.id), None, None, None, None);
        group.store(conn).unwrap();
        let id = message.id.clone();

        message.store(conn).unwrap();

        let stored_message = conn.get_group_message(id).unwrap().unwrap();
        assert_eq!(
            stored_message.decrypted_message_bytes,
            message.decrypted_message_bytes
        );
    })
}

#[xmtp_common::test]
fn it_cannot_insert_message_without_group() {
    use diesel::result::DatabaseErrorKind::ForeignKeyViolation;
    with_connection(|conn| {
        let message = generate_message(None, None, None, None, None, None);
        let result = message.store(&conn);
        assert_err!(
            result,
            crate::StorageError::Connection(crate::ConnectionError::Database(
                diesel::result::Error::DatabaseError(ForeignKeyViolation, _)
            ))
        );
    })
}

#[xmtp_common::test]
fn it_gets_many_messages() {
    use crate::encrypted_store::schema::group_messages::dsl;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        for idx in 0..50 {
            let msg = generate_message(None, Some(&group.id), Some(idx), None, None, None);
            assert_ok!(msg.store(conn));
        }

        let count: i64 = conn
            .raw_query_read(|raw_conn| {
                dsl::group_messages
                    .select(diesel::dsl::count_star())
                    .first(raw_conn)
            })
            .unwrap();
        assert_eq!(count, 50);

        let messages = conn
            .get_group_messages(&group.id, &MsgQueryArgs::default())
            .unwrap();

        assert_eq!(messages.len(), 50);
        messages.iter().fold(0, |acc, msg| {
            assert!(msg.sent_at_ns >= acc);
            msg.sent_at_ns
        });
    })
}

#[xmtp_common::test]
fn it_gets_messages_by_time() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let messages = vec![
            generate_message(None, Some(&group.id), Some(1_000), None, None, None),
            generate_message(None, Some(&group.id), Some(100_000), None, None, None),
            generate_message(None, Some(&group.id), Some(10_000), None, None, None),
            generate_message(None, Some(&group.id), Some(1_000_000), None, None, None),
        ];
        assert_ok!(messages.store(conn));
        let message = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_after_ns: Some(1_000),
                    sent_before_ns: Some(100_000),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(message.len(), 1);
        assert_eq!(message.first().unwrap().sent_at_ns, 10_000);

        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_before_ns: Some(100_000),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 2);

        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_after_ns: Some(10_000),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 2);
    })
}

#[xmtp_common::test]
fn it_deletes_middle_message_by_expiration_time() {
    with_connection(|conn| {
        let mut group = generate_group(None);

        let disappear_from_ns = Some(1_000_500_000); // After Message 1
        let disappear_in_ns = Some(500_000); // Before Message 3
        group.message_disappear_from_ns = disappear_from_ns;
        group.message_disappear_in_ns = disappear_in_ns;

        group.store(conn).unwrap();

        let messages = vec![
            generate_message(None, Some(&group.id), Some(1_000_000_000), None, None, None),
            generate_message(
                None,
                Some(&group.id),
                Some(1_001_000_000),
                None,
                Some(1_001_000_000),
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000_000_000_000_000_000),
                None,
                None,
                None,
            ),
        ];
        assert_ok!(messages.store(conn));

        let deleted_messages = conn.delete_expired_messages().unwrap();
        assert_eq!(deleted_messages.len(), 1); // Ensure exactly 1 message is deleted
        assert_eq!(deleted_messages[0].id, messages[1].id); // Verify the correct message (middle one with expiration) was deleted

        let remaining_messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    ..Default::default()
                },
            )
            .unwrap();

        // Verify the count and content of the remaining messages
        assert_eq!(remaining_messages.len(), 2);
        assert!(
            remaining_messages
                .iter()
                .any(|msg| msg.sent_at_ns == 1_000_000_000)
        ); // Message 1
        assert!(
            remaining_messages
                .iter()
                .any(|msg| msg.sent_at_ns == 2_000_000_000_000_000_000)
        ); // Message 3
    })
}

#[xmtp_common::test]
fn it_gets_messages_by_kind() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // just a bunch of random messages so we have something to filter through
        for i in 0..30 {
            match i % 2 {
                0 => {
                    let msg = generate_message(
                        Some(GroupMessageKind::Application),
                        Some(&group.id),
                        None,
                        Some(ContentType::Text),
                        None,
                        None,
                    );
                    msg.store(conn).unwrap();
                }
                _ => {
                    let msg = generate_message(
                        Some(GroupMessageKind::MembershipChange),
                        Some(&group.id),
                        None,
                        Some(ContentType::GroupMembershipChange),
                        None,
                        None,
                    );
                    msg.store(conn).unwrap();
                }
            }
        }

        let application_messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    kind: Some(GroupMessageKind::Application),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(application_messages.len(), 15);

        let membership_changes = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    kind: Some(GroupMessageKind::MembershipChange),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(membership_changes.len(), 15);
    })
}

#[xmtp_common::test]
fn it_orders_messages_by_sent() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        assert_eq!(group.last_message_ns, None);

        let messages = vec![
            generate_message(None, Some(&group.id), Some(10_000), None, None, None),
            generate_message(None, Some(&group.id), Some(1_000), None, None, None),
            generate_message(None, Some(&group.id), Some(100_000), None, None, None),
            generate_message(None, Some(&group.id), Some(1_000_000), None, None, None),
        ];

        assert_ok!(messages.store(conn));

        let group = conn.find_group(&group.id).unwrap().unwrap();
        assert_eq!(group.last_message_ns.unwrap(), 1_000_000);

        let messages_asc = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    direction: Some(SortDirection::Ascending),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(messages_asc.len(), 4);
        assert_eq!(messages_asc[0].sent_at_ns, 1_000);
        assert_eq!(messages_asc[1].sent_at_ns, 10_000);
        assert_eq!(messages_asc[2].sent_at_ns, 100_000);
        assert_eq!(messages_asc[3].sent_at_ns, 1_000_000);

        let messages_desc = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    direction: Some(SortDirection::Descending),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(messages_desc.len(), 4);
        assert_eq!(messages_desc[0].sent_at_ns, 1_000_000);
        assert_eq!(messages_desc[1].sent_at_ns, 100_000);
        assert_eq!(messages_desc[2].sent_at_ns, 10_000);
        assert_eq!(messages_desc[3].sent_at_ns, 1_000);
    })
}

#[xmtp_common::test]
fn it_gets_messages_by_content_type() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let messages = vec![
            generate_message(
                None,
                Some(&group.id),
                Some(1_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000),
                Some(ContentType::GroupMembershipChange),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(3_000),
                Some(ContentType::GroupUpdated),
                None,
                None,
            ),
        ];
        assert_ok!(messages.store(conn));

        // Query for text messages
        let text_messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(text_messages.len(), 1);
        assert_eq!(text_messages[0].content_type, ContentType::Text);

        assert_eq!(text_messages[0].sent_at_ns, 1_000);

        // Query for membership change messages
        let membership_messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::GroupMembershipChange]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(membership_messages.len(), 1);
        assert_eq!(
            membership_messages[0].content_type,
            ContentType::GroupMembershipChange
        );
        assert_eq!(membership_messages[0].sent_at_ns, 2_000);

        // Query for group updated messages
        let updated_messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::GroupUpdated]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated_messages.len(), 1);
        assert_eq!(updated_messages[0].content_type, ContentType::GroupUpdated);
        assert_eq!(updated_messages[0].sent_at_ns, 3_000);
    })
}

#[xmtp_common::test]
fn it_dedupes_group_updated_messages_from_dm_by_default() {
    with_connection(|conn| {
        // Create a DM group
        let mut group = generate_group(None);
        group.conversation_type = ConversationType::Dm;
        group.store(conn).unwrap();

        // Insert one GroupUpdated message and two normal messages
        let group_updated_msg = generate_message(
            Some(GroupMessageKind::Application),
            Some(&group.id),
            Some(5_000),
            Some(ContentType::GroupUpdated),
            None,
            None,
        );

        let group_updated_msg_2 = generate_message(
            Some(GroupMessageKind::Application),
            Some(&group.id),
            Some(7_000),
            Some(ContentType::GroupUpdated),
            None,
            None,
        );

        let earlier_msg = generate_message(
            Some(GroupMessageKind::Application),
            Some(&group.id),
            Some(1_000),
            Some(ContentType::Text),
            None,
            None,
        );

        let later_msg = generate_message(
            Some(GroupMessageKind::Application),
            Some(&group.id),
            Some(10_000),
            Some(ContentType::Text),
            None,
            None,
        );

        assert_ok!(
            vec![
                group_updated_msg.clone(),
                group_updated_msg_2.clone(),
                earlier_msg.clone(),
                later_msg.clone()
            ]
            .store(conn)
        );

        // Default query: GroupUpdated messages are deduplicated for DMs
        let messages_default = conn
            .get_group_messages(&group.id, &MsgQueryArgs::default())
            .unwrap();

        assert_eq!(messages_default.len(), 4);
        // One group updated message for each person joining.
        assert_eq!(
            messages_default
                .iter()
                .filter(|m| m.content_type == ContentType::GroupUpdated)
                .count(),
            2
        );

        // Explicitly request GroupUpdated messages - should get them
        let messages_with_group_updated = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::GroupUpdated]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages_with_group_updated.len(), 2);
        assert_eq!(
            messages_with_group_updated[0].content_type,
            ContentType::GroupUpdated
        );
        assert_eq!(messages_with_group_updated[0].sent_at_ns, 5_000);
    })
}

pub(crate) fn generate_message_with_reference<C: ConnectionExt>(
    conn: &DbConnection<C>,
    group_id: &[u8],
    sent_at_ns: i64,
    content_type: ContentType,
    reference_id: Option<Vec<u8>>,
) -> StoredGroupMessage {
    let message = StoredGroupMessage {
        id: rand_vec::<24>(),
        group_id: group_id.to_vec(),
        decrypted_message_bytes: rand_vec::<24>(),
        sent_at_ns,
        sender_installation_id: rand_vec::<24>(),
        sender_inbox_id: "0x0".to_string(),
        kind: GroupMessageKind::Application,
        delivery_status: DeliveryStatus::Published,
        content_type,
        version_major: 0,
        version_minor: 0,
        authority_id: "unknown".to_string(),
        reference_id,
        sequence_id: 0,
        originator_id: 0,
        expire_at_ns: None,
        inserted_at_ns: 0, // Will be set by database
        should_push: true,
    };
    message.store(conn).unwrap();
    message
}

#[xmtp_common::test]
fn test_inbound_relations_with_results() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create main messages
        let msg1 = generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);
        let msg2 = generate_message_with_reference(conn, &group.id, 2000, ContentType::Text, None);
        let msg3 = generate_message_with_reference(conn, &group.id, 3000, ContentType::Text, None);

        // Create reactions referencing the main messages
        let _reaction1 = generate_message_with_reference(
            conn,
            &group.id,
            4000,
            ContentType::Reaction,
            Some(msg1.id.clone()),
        );
        let _reaction2 = generate_message_with_reference(
            conn,
            &group.id,
            5000,
            ContentType::Reaction,
            Some(msg1.id.clone()),
        );
        let _reaction3 = generate_message_with_reference(
            conn,
            &group.id,
            6000,
            ContentType::Reaction,
            Some(msg2.id.clone()),
        );

        // Get the main messages (exclude reactions)
        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages.len(), 3);

        // Get inbound relations for these messages
        let message_ids: Vec<&[u8]> = messages.iter().map(|m| m.id.as_ref()).collect();
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &message_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(inbound_relations.len(), 2); // msg1 and msg2 have reactions

        // Check msg1 has 2 reactions
        let msg1_reactions = inbound_relations.get(&msg1.id).unwrap();
        assert_eq!(msg1_reactions.len(), 2);

        // Check msg2 has 1 reaction
        let msg2_reactions = inbound_relations.get(&msg2.id).unwrap();
        assert_eq!(msg2_reactions.len(), 1);

        // msg3 should not be in inbound_relations
        assert!(!inbound_relations.contains_key(&msg3.id));
    })
}

#[xmtp_common::test]
fn test_relations_when_no_references_exist() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create messages without any references
        let _msg1 = generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);
        let _msg2 = generate_message_with_reference(conn, &group.id, 2000, ContentType::Text, None);

        // Get the messages
        let messages = conn
            .get_group_messages(&group.id, &MsgQueryArgs::default())
            .unwrap();
        assert_eq!(messages.len(), 2);

        let message_ids: Vec<&[u8]> = messages.iter().map(|m| m.id.as_ref()).collect();

        // Test inbound relations when no references exist
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &message_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(
            inbound_relations.len(),
            0,
            "No inbound relations should exist"
        );

        // Test outbound relations when messages have no references
        // Since neither msg1 nor msg2 have reference_id set, we pass empty vec
        let reference_ids: Vec<&[u8]> = messages
            .iter()
            .filter_map(|m| m.reference_id.as_deref())
            .collect();

        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        assert_eq!(
            outbound_relations.len(),
            0,
            "No outbound relations should exist"
        );
    })
}

#[xmtp_common::test]
fn test_inbound_relations_no_main_query_results() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Ensure we get an empty map when no IDs are passed
        let empty_ids: Vec<&[u8]> = vec![];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &empty_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(inbound_relations.len(), 0);
    })
}

#[xmtp_common::test]
fn test_inbound_relations_with_limit() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create a main message
        let msg1 = generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);

        // Create many reactions to it
        for i in 0..10 {
            let _reaction = generate_message_with_reference(
                conn,
                &group.id,
                2000 + i * 100,
                ContentType::Reaction,
                Some(msg1.id.clone()),
            );
        }

        // Get the main message (exclude reactions)
        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 1);

        // Query with limit on inbound relations
        let msg1_ids: Vec<&[u8]> = vec![msg1.id.as_ref()];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &msg1_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .limit(Some(3))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let msg1_reactions = inbound_relations.get(&msg1.id).unwrap();
        assert!(msg1_reactions.len() <= 3); // Limited to 3
    })
}

#[xmtp_common::test]
fn test_relations_with_content_type_filters() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create main messages
        let text_msg =
            generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);
        let attachment_msg =
            generate_message_with_reference(conn, &group.id, 2000, ContentType::Attachment, None);

        // Create various types of references to text_msg
        let _reaction = generate_message_with_reference(
            conn,
            &group.id,
            3000,
            ContentType::Reaction,
            Some(text_msg.id.clone()),
        );
        let _reply_to_text = generate_message_with_reference(
            conn,
            &group.id,
            4000,
            ContentType::Reply,
            Some(text_msg.id.clone()),
        );
        let _read_receipt = generate_message_with_reference(
            conn,
            &group.id,
            5000,
            ContentType::ReadReceipt,
            Some(text_msg.id.clone()),
        );

        // Create a reply to attachment_msg
        let _reply_to_attachment = generate_message_with_reference(
            conn,
            &group.id,
            6000,
            ContentType::Reply,
            Some(attachment_msg.id.clone()),
        );

        // Test inbound filter: only reactions
        let msg_ids: Vec<&[u8]> = vec![text_msg.id.as_ref(), attachment_msg.id.as_ref()];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &msg_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let text_msg_relations = inbound_relations.get(&text_msg.id).unwrap();
        assert_eq!(text_msg_relations.len(), 1);
        assert_eq!(text_msg_relations[0].content_type, ContentType::Reaction);

        // Test inbound filter: reactions and replies
        let msg_ids2: Vec<&[u8]> = vec![text_msg.id.as_ref(), attachment_msg.id.as_ref()];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &msg_ids2,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction, ContentType::Reply]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        let text_msg_relations = inbound_relations.get(&text_msg.id).unwrap();
        assert_eq!(text_msg_relations.len(), 2, "Should get reaction and reply");

        // Test outbound filter: only text messages
        // First get the reply messages
        let replies = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Reply]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(replies.len(), 2, "Should get both replies");

        // Get the reference_ids from the replies
        let reference_ids: Vec<&[u8]> = vec![text_msg.id.as_ref()];

        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        assert_eq!(outbound_relations.len(), 1, "Should only get text message");
        assert!(outbound_relations.contains_key(&text_msg.id));
        assert!(!outbound_relations.contains_key(&attachment_msg.id));
    })
}

#[xmtp_common::test]
fn test_outbound_relations_with_results() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create messages that will be referenced
        let original_msg1 =
            generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);
        let original_msg2 =
            generate_message_with_reference(conn, &group.id, 2000, ContentType::Text, None);

        // Create messages that reference the original messages
        let _reply1 = generate_message_with_reference(
            conn,
            &group.id,
            3000,
            ContentType::Reply,
            Some(original_msg1.id.clone()),
        );
        let _reply2 = generate_message_with_reference(
            conn,
            &group.id,
            4000,
            ContentType::Reply,
            Some(original_msg2.id.clone()),
        );
        let _standalone =
            generate_message_with_reference(conn, &group.id, 5000, ContentType::Text, None);

        // Query for replies
        let replies = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Reply]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(replies.len(), 2); // Only the replies

        // Get the reference_ids from the replies
        let reference_ids: Vec<&[u8]> = replies
            .iter()
            .filter_map(|m| m.reference_id.as_deref())
            .collect();

        // Get outbound relations
        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        assert_eq!(outbound_relations.len(), 2); // The original messages

        // Check that we have the original messages in outbound relations
        assert!(outbound_relations.contains_key(&original_msg1.id));
        assert!(outbound_relations.contains_key(&original_msg2.id));
    })
}

#[xmtp_common::test]
fn test_outbound_relations_no_main_query_results() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create an original message
        let original =
            generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);

        // Create a reply to it
        let _reply = generate_message_with_reference(
            conn,
            &group.id,
            2000,
            ContentType::Reply,
            Some(original.id.clone()),
        );

        // Query with time filter that excludes all messages
        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_before_ns: Some(500), // Before any messages
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages.len(), 0);

        // When no messages match the query, we have no reference_ids to look up
        let reference_ids: Vec<&[u8]> = messages
            .iter()
            .filter_map(|m| m.reference_id.as_deref())
            .collect();

        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        assert_eq!(outbound_relations.len(), 0);
    })
}

#[xmtp_common::test]
fn test_outbound_relations_with_limit() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create multiple original messages
        let mut original_ids = Vec::new();
        for i in 0..5 {
            let original = generate_message_with_reference(
                conn,
                &group.id,
                1000 + i * 100,
                ContentType::Text,
                None,
            );
            original_ids.push(original.id.clone());
        }

        // Create replies to all of them
        for (i, original_id) in original_ids.iter().enumerate() {
            let _reply = generate_message_with_reference(
                conn,
                &group.id,
                2000 + i as i64 * 100,
                ContentType::Reply,
                Some(original_id.clone()),
            );
        }

        // Query for replies
        let replies = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Reply]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(replies.len(), 5); // All replies

        // Get only first 2 reference_ids to simulate limit
        let reference_ids: Vec<&[u8]> = replies
            .iter()
            .filter_map(|m| m.reference_id.as_deref())
            .take(2)
            .collect();

        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        assert_eq!(outbound_relations.len(), 2); // Limited to 2
    })
}

#[xmtp_common::test]
fn test_both_inbound_and_outbound_relations() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create an original message
        let original =
            generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);

        // Create a reply that references the original
        let reply = generate_message_with_reference(
            conn,
            &group.id,
            2000,
            ContentType::Reply,
            Some(original.id.clone()),
        );

        // Create reactions to the reply
        let _reaction1 = generate_message_with_reference(
            conn,
            &group.id,
            3000,
            ContentType::Reaction,
            Some(reply.id.clone()),
        );
        let _reaction2 = generate_message_with_reference(
            conn,
            &group.id,
            4000,
            ContentType::Reaction,
            Some(reply.id.clone()),
        );

        // Query for the reply
        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Reply]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages.len(), 1); // The reply
        assert_eq!(messages[0].id, reply.id);

        // Get inbound relations (reactions to the reply)
        let reply_ids: Vec<&[u8]> = vec![reply.id.as_ref()];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &reply_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(inbound_relations.len(), 1);
        let reply_reactions = inbound_relations.get(&reply.id).unwrap();
        assert_eq!(reply_reactions.len(), 2);

        // Get outbound relations (original message referenced by reply)
        let reference_ids: Vec<&[u8]> = messages
            .iter()
            .filter_map(|m| m.reference_id.as_deref())
            .collect();

        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        // Check outbound relation (original message)
        assert_eq!(outbound_relations.len(), 1);
        assert!(outbound_relations.contains_key(&original.id));
    })
}

#[xmtp_common::test]
fn test_relation_filters_none_behavior() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create a complex message graph
        let msg1 = generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);
        let _msg2 = generate_message_with_reference(conn, &group.id, 2000, ContentType::Text, None);

        // Create a reply to msg1
        let reply = generate_message_with_reference(
            conn,
            &group.id,
            3000,
            ContentType::Reply,
            Some(msg1.id.clone()),
        );

        // Create reactions
        let _reaction1 = generate_message_with_reference(
            conn,
            &group.id,
            4000,
            ContentType::Reaction,
            Some(msg1.id.clone()),
        );
        let _reaction2 = generate_message_with_reference(
            conn,
            &group.id,
            5000,
            ContentType::Reaction,
            Some(reply.id.clone()),
        );

        // Test 1: Get messages without fetching any relations (exclude reactions)
        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text, ContentType::Reply]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages.len(), 3, "Should get msg1, msg2, and reply");

        // Test 2: Get inbound relations with reactions filter
        let message_ids: Vec<&[u8]> = vec![msg1.id.as_ref(), reply.id.as_ref()];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &message_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(
            inbound_relations.len(),
            2,
            "Should fetch inbound relations for msg1 and reply"
        );
        assert_eq!(inbound_relations.get(&msg1.id).unwrap().len(), 1);
        assert_eq!(inbound_relations.get(&reply.id).unwrap().len(), 1);

        // Test 3: Get outbound relations for reply message
        let replies = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Reply]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(replies.len(), 1, "Should get only the reply");

        let reference_ids: Vec<&[u8]> = replies
            .iter()
            .filter_map(|m| m.reference_id.as_deref())
            .collect();

        let outbound_relations = conn
            .get_outbound_relations(&group.id, &reference_ids)
            .unwrap();

        assert_eq!(
            outbound_relations.len(),
            1,
            "Should fetch outbound relations"
        );
        assert!(outbound_relations.contains_key(&msg1.id));
    })
}

#[xmtp_common::test]
fn test_complex_relation_chain() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create a chain of messages referencing each other
        let msg1 = generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);

        let reply_to_msg1 = generate_message_with_reference(
            conn,
            &group.id,
            2000,
            ContentType::Reply,
            Some(msg1.id.clone()),
        );

        let _reaction_to_msg1 = generate_message_with_reference(
            conn,
            &group.id,
            3000,
            ContentType::Reaction,
            Some(msg1.id.clone()),
        );

        let _reaction_to_reply = generate_message_with_reference(
            conn,
            &group.id,
            4000,
            ContentType::Reaction,
            Some(reply_to_msg1.id.clone()),
        );

        // Query for the original message
        let messages = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text]),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, msg1.id);

        // Get all inbound relations for msg1
        let msg1_ids: Vec<&[u8]> = vec![msg1.id.as_ref()];
        let inbound_relations = conn
            .get_inbound_relations(
                &group.id,
                &msg1_ids,
                RelationQuery::builder()
                    // Get all inbound
                    .build()
                    .unwrap(),
            )
            .unwrap();

        // Should have reply and reaction as inbound
        let msg1_relations = inbound_relations.get(&msg1.id).unwrap();
        assert_eq!(msg1_relations.len(), 2);

        // Verify the content types of inbound relations
        let content_types: Vec<ContentType> =
            msg1_relations.iter().map(|m| m.content_type).collect();
        assert!(content_types.contains(&ContentType::Reply));
        assert!(content_types.contains(&ContentType::Reaction));
    })
}

#[xmtp_common::test]
fn test_inbound_relation_counts() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create main messages
        let msg1 = generate_message_with_reference(conn, &group.id, 1000, ContentType::Text, None);
        let msg2 = generate_message_with_reference(conn, &group.id, 2000, ContentType::Text, None);
        let msg3 = generate_message_with_reference(conn, &group.id, 3000, ContentType::Text, None);

        // Create multiple reactions to msg1
        for i in 0..5 {
            let _reaction = generate_message_with_reference(
                conn,
                &group.id,
                4000 + i * 100,
                ContentType::Reaction,
                Some(msg1.id.clone()),
            );
        }

        // Create replies to msg2
        for i in 0..3 {
            let _reply = generate_message_with_reference(
                conn,
                &group.id,
                5000 + i * 100,
                ContentType::Reply,
                Some(msg2.id.clone()),
            );
        }

        // Create one reaction to msg2
        let _reaction_to_msg2 = generate_message_with_reference(
            conn,
            &group.id,
            6000,
            ContentType::Reaction,
            Some(msg2.id.clone()),
        );

        // Test getting all relation counts
        let message_ids: Vec<&[u8]> = vec![msg1.id.as_ref(), msg2.id.as_ref(), msg3.id.as_ref()];
        let counts = conn
            .get_inbound_relation_counts(
                &group.id,
                &message_ids,
                RelationQuery::builder().build().unwrap(),
            )
            .unwrap();

        assert_eq!(counts.get(&msg1.id).unwrap(), &5); // 5 reactions
        assert_eq!(counts.get(&msg2.id).unwrap(), &4); // 3 replies + 1 reaction
        assert!(!counts.contains_key(&msg3.id)); // No relations

        // Test getting only reaction counts
        let reaction_counts = conn
            .get_inbound_relation_counts(
                &group.id,
                &message_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(reaction_counts.get(&msg1.id).unwrap(), &5); // 5 reactions
        assert_eq!(reaction_counts.get(&msg2.id).unwrap(), &1); // 1 reaction only
        assert!(!reaction_counts.contains_key(&msg3.id)); // No reactions

        // Test getting only reply counts
        let reply_counts = conn
            .get_inbound_relation_counts(
                &group.id,
                &message_ids,
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reply]))
                    .build()
                    .unwrap(),
            )
            .unwrap();

        assert!(!reply_counts.contains_key(&msg1.id)); // No replies
        assert_eq!(reply_counts.get(&msg2.id).unwrap(), &3); // 3 replies
        assert!(!reply_counts.contains_key(&msg3.id)); // No replies
    })
}

#[xmtp_common::test]
fn test_get_latest_message_times_by_sender_single_sender() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create messages from a single sender with different timestamps
        let sender_id = "0x123".to_string();
        let messages = vec![
            generate_message(
                None,
                Some(&group.id),
                Some(1000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(5000), // Latest message
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(3000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
        ];

        assert_ok!(messages.store(conn));

        // Test getting latest message times
        let latest_times = conn
            .get_latest_message_times_by_sender(&group.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times.len(), 1);
        assert_eq!(latest_times.get(&sender_id).unwrap(), &5000);
    })
}

#[xmtp_common::test]
fn test_get_latest_message_times_by_sender_multiple_senders() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let sender1_id = "0x111".to_string();
        let sender2_id = "0x222".to_string();
        let sender3_id = "0x333".to_string();

        // Create messages from multiple senders
        let messages = vec![
            // Sender 1 messages
            generate_message(
                None,
                Some(&group.id),
                Some(1000),
                Some(ContentType::Text),
                None,
                Some(sender1_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(5000), // Latest for sender1
                Some(ContentType::Text),
                None,
                Some(sender1_id.clone()),
            ),
            // Sender 2 messages
            generate_message(
                None,
                Some(&group.id),
                Some(2000),
                Some(ContentType::Text),
                None,
                Some(sender2_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(8000), // Latest for sender2
                Some(ContentType::Text),
                None,
                Some(sender2_id.clone()),
            ),
            // Sender 3 messages
            generate_message(
                None,
                Some(&group.id),
                Some(3000), // Only message for sender3
                Some(ContentType::Text),
                None,
                Some(sender3_id.clone()),
            ),
        ];

        assert_ok!(messages.store(conn));

        // Test getting latest message times
        let latest_times = conn
            .get_latest_message_times_by_sender(&group.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times.len(), 3);
        assert_eq!(latest_times.get(&sender1_id).unwrap(), &5000);
        assert_eq!(latest_times.get(&sender2_id).unwrap(), &8000);
        assert_eq!(latest_times.get(&sender3_id).unwrap(), &3000);
    })
}

#[xmtp_common::test]
fn test_get_latest_message_times_by_sender_empty_results() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Test with no messages
        let latest_times = conn
            .get_latest_message_times_by_sender(&group.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times.len(), 0);

        // Add some messages but filter by content type that doesn't match
        let sender_id = "0x123".to_string();
        let message = generate_message(
            None,
            Some(&group.id),
            Some(1000),
            Some(ContentType::Text),
            None,
            Some(sender_id),
        );

        assert_ok!(message.store(conn));

        // Filter by content type that doesn't match
        let latest_times = conn
            .get_latest_message_times_by_sender(&group.id, &[ContentType::Attachment])
            .unwrap();

        assert_eq!(latest_times.len(), 0);
    })
}

#[xmtp_common::test]
fn test_get_latest_message_times_by_sender_dm_group() {
    with_connection(|conn| {
        // Create multiple DM groups that share the same dm_id
        let shared_dm_id = "dm_123".to_string();

        let mut group1 = generate_group(None);
        group1.conversation_type = ConversationType::Dm;
        group1.dm_id = Some(shared_dm_id.clone());
        group1.store(conn).unwrap();

        let mut group2 = generate_group(None);
        group2.conversation_type = ConversationType::Dm;
        group2.dm_id = Some(shared_dm_id.clone());
        group2.store(conn).unwrap();

        let mut group3 = generate_group(None);
        group3.conversation_type = ConversationType::Dm;
        group3.dm_id = Some(shared_dm_id.clone());
        group3.store(conn).unwrap();

        let sender_id = "0x123".to_string();

        // Create messages across different groups that share the same dm_id
        let messages = vec![
            // Messages in group1
            generate_message(
                None,
                Some(&group1.id),
                Some(1000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            generate_message(
                None,
                Some(&group1.id),
                Some(3000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            // Messages in group2
            generate_message(
                None,
                Some(&group2.id),
                Some(2000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            generate_message(
                None,
                Some(&group2.id),
                Some(6000), // Latest message across all groups
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            // Messages in group3
            generate_message(
                None,
                Some(&group3.id),
                Some(4000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
            generate_message(
                None,
                Some(&group3.id),
                Some(5000),
                Some(ContentType::Text),
                None,
                Some(sender_id.clone()),
            ),
        ];

        assert_ok!(messages.store(conn));

        // Test getting latest message times for any of the groups with the shared dm_id
        // The query should find messages from all groups that share the same dm_id
        let latest_times = conn
            .get_latest_message_times_by_sender(&group1.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times.len(), 1);
        assert_eq!(
            latest_times.get(&sender_id).unwrap(),
            &6000 // Should be the latest message across all groups with the same dm_id
        );

        // Test that querying any of the groups returns the same result
        let latest_times_group2 = conn
            .get_latest_message_times_by_sender(&group2.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times_group2.len(), 1);
        assert_eq!(latest_times_group2.get(&sender_id).unwrap(), &6000);

        let latest_times_group3 = conn
            .get_latest_message_times_by_sender(&group3.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times_group3.len(), 1);
        assert_eq!(latest_times_group3.get(&sender_id).unwrap(), &6000);
    })
}

#[xmtp_common::test]
fn test_count_group_messages() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Setup test data with various message types
        let messages = vec![
            generate_message(
                None,
                Some(&group.id),
                Some(1_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(3_000),
                Some(ContentType::Reaction),
                None,
                None,
            ),
            generate_message(
                Some(GroupMessageKind::MembershipChange),
                Some(&group.id),
                Some(4_000),
                None,
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(5_000),
                Some(ContentType::GroupUpdated),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(10_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(15_000),
                Some(ContentType::Reaction),
                None,
                None,
            ),
        ];

        // Add messages with different delivery statuses
        let mut msg_published = generate_message(
            None,
            Some(&group.id),
            Some(20_000),
            Some(ContentType::Text),
            None,
            None,
        );
        msg_published.delivery_status = DeliveryStatus::Published;
        let mut msg_unpublished = generate_message(
            None,
            Some(&group.id),
            Some(21_000),
            Some(ContentType::Text),
            None,
            None,
        );
        msg_unpublished.delivery_status = DeliveryStatus::Unpublished;
        let mut msg_failed = generate_message(
            None,
            Some(&group.id),
            Some(22_000),
            Some(ContentType::Text),
            None,
            None,
        );
        msg_failed.delivery_status = DeliveryStatus::Failed;

        let all_messages = [messages, vec![msg_published, msg_unpublished, msg_failed]].concat();
        assert_ok!(all_messages.store(conn));

        // Test basic counts
        assert_eq!(
            conn.count_group_messages(&group.id, &MsgQueryArgs::default())
                .unwrap(),
            10
        );

        // Test count by content type
        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text]),
                    ..Default::default()
                }
            )
            .unwrap(),
            6
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Reaction]),
                    ..Default::default()
                }
            )
            .unwrap(),
            2
        );

        // Test count by kind
        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    kind: Some(GroupMessageKind::Application),
                    ..Default::default()
                }
            )
            .unwrap(),
            9
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    kind: Some(GroupMessageKind::MembershipChange),
                    ..Default::default()
                }
            )
            .unwrap(),
            1
        );

        // Test time filters
        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_after_ns: Some(5_000),
                    ..Default::default()
                }
            )
            .unwrap(),
            5 // Messages at 10_000, 15_000, 20_000, 21_000, 22_000
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_before_ns: Some(10_000),
                    ..Default::default()
                }
            )
            .unwrap(),
            5 // Messages at 1_000, 2_000, 3_000, 4_000, 5_000 (before is exclusive)
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_after_ns: Some(3_000),
                    sent_before_ns: Some(12_000),
                    ..Default::default()
                }
            )
            .unwrap(),
            3 // Messages at 4_000, 5_000, 10_000
        );

        // Test delivery status filters (note: generate_message defaults to Published)
        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    delivery_status: Some(DeliveryStatus::Published),
                    ..Default::default()
                }
            )
            .unwrap(),
            8 // 7 default Published + 1 explicitly set to Published
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    delivery_status: Some(DeliveryStatus::Unpublished),
                    ..Default::default()
                }
            )
            .unwrap(),
            1
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    delivery_status: Some(DeliveryStatus::Failed),
                    ..Default::default()
                }
            )
            .unwrap(),
            1
        );
    })
}

#[xmtp_common::test]
fn test_count_group_messages_dm_vs_regular_groups() {
    with_connection(|conn| {
        // Test DM group behavior
        let mut dm_group = generate_group(None);
        dm_group.conversation_type = ConversationType::Dm;
        dm_group.store(conn).unwrap();

        // Test regular group behavior
        let regular_group = generate_group(None);
        regular_group.store(conn).unwrap();

        // Create identical message sets for both groups
        let create_messages = |group_id: &Vec<u8>| {
            vec![
                generate_message(
                    Some(GroupMessageKind::Application),
                    Some(group_id),
                    Some(1_000),
                    Some(ContentType::GroupUpdated),
                    None,
                    None,
                ),
                generate_message(
                    Some(GroupMessageKind::Application),
                    Some(group_id),
                    Some(2_000),
                    Some(ContentType::GroupUpdated),
                    None,
                    None,
                ),
                generate_message(
                    Some(GroupMessageKind::Application),
                    Some(group_id),
                    Some(3_000),
                    Some(ContentType::GroupUpdated),
                    None,
                    None,
                ),
                generate_message(
                    Some(GroupMessageKind::Application),
                    Some(group_id),
                    Some(4_000),
                    Some(ContentType::Text),
                    None,
                    None,
                ),
                generate_message(
                    Some(GroupMessageKind::Application),
                    Some(group_id),
                    Some(5_000),
                    Some(ContentType::Text),
                    None,
                    None,
                ),
            ]
        };

        let dm_messages = create_messages(&dm_group.id);
        let regular_messages = create_messages(&regular_group.id);

        assert_ok!(dm_messages.store(conn));
        assert_ok!(regular_messages.store(conn));

        // DM groups exclude GroupUpdated messages by default (should get 2 Text messages)
        assert_eq!(
            conn.count_group_messages(&dm_group.id, &MsgQueryArgs::default())
                .unwrap(),
            2
        );

        // Regular groups count all messages (should get all 5)
        assert_eq!(
            conn.count_group_messages(&regular_group.id, &MsgQueryArgs::default())
                .unwrap(),
            5
        );

        // When explicitly requesting GroupUpdated messages, both should return 3
        let group_updated_args = MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupUpdated]),
            ..Default::default()
        };
        assert_eq!(
            conn.count_group_messages(&dm_group.id, &group_updated_args)
                .unwrap(),
            3
        );
        assert_eq!(
            conn.count_group_messages(&regular_group.id, &group_updated_args)
                .unwrap(),
            3
        );

        // Text messages should be the same for both
        let text_args = MsgQueryArgs {
            content_types: Some(vec![ContentType::Text]),
            ..Default::default()
        };
        assert_eq!(
            conn.count_group_messages(&dm_group.id, &text_args).unwrap(),
            2
        );
        assert_eq!(
            conn.count_group_messages(&regular_group.id, &text_args)
                .unwrap(),
            2
        );
    })
}

#[xmtp_common::test]
fn test_count_group_messages_empty_groups() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Test count with no messages
        assert_eq!(
            conn.count_group_messages(&group.id, &MsgQueryArgs::default())
                .unwrap(),
            0
        );

        // Test count with filters that would match nothing
        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::Text]),
                    ..Default::default()
                }
            )
            .unwrap(),
            0
        );

        assert_eq!(
            conn.count_group_messages(
                &group.id,
                &MsgQueryArgs {
                    sent_after_ns: Some(1000),
                    ..Default::default()
                }
            )
            .unwrap(),
            0
        );
    })
}

#[xmtp_common::test]
fn test_get_latest_message_times_by_sender_mixed_content_types() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let sender1_id = "0x111".to_string();
        let sender2_id = "0x222".to_string();

        // Create messages with mixed content types from different senders
        let messages = vec![
            // Sender 1: Text messages
            generate_message(
                None,
                Some(&group.id),
                Some(1000),
                Some(ContentType::Text),
                None,
                Some(sender1_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(5000), // Latest text from sender1
                Some(ContentType::Text),
                None,
                Some(sender1_id.clone()),
            ),
            // Sender 1: Attachment messages
            generate_message(
                None,
                Some(&group.id),
                Some(3000),
                Some(ContentType::Attachment),
                None,
                Some(sender1_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(8000), // Latest attachment from sender1
                Some(ContentType::Attachment),
                None,
                Some(sender1_id.clone()),
            ),
            // Sender 2: Only text messages
            generate_message(
                None,
                Some(&group.id),
                Some(2000),
                Some(ContentType::Text),
                None,
                Some(sender2_id.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(6000), // Latest text from sender2
                Some(ContentType::Text),
                None,
                Some(sender2_id.clone()),
            ),
        ];

        assert_ok!(messages.store(conn));

        // Test filtering by text only - should get both senders
        let latest_times_text = conn
            .get_latest_message_times_by_sender(&group.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times_text.len(), 2);
        assert_eq!(latest_times_text.get(&sender1_id).unwrap(), &5000);
        assert_eq!(latest_times_text.get(&sender2_id).unwrap(), &6000);

        // Test filtering by attachment only - should get only sender1
        let latest_times_attachment = conn
            .get_latest_message_times_by_sender(&group.id, &[ContentType::Attachment])
            .unwrap();

        assert_eq!(latest_times_attachment.len(), 1);
        assert_eq!(latest_times_attachment.get(&sender1_id).unwrap(), &8000);

        // Test filtering by both - should get both senders with their latest overall times
        let latest_times_both = conn
            .get_latest_message_times_by_sender(
                &group.id,
                &[ContentType::Text, ContentType::Attachment],
            )
            .unwrap();

        assert_eq!(latest_times_both.len(), 2);
        assert_eq!(latest_times_both.get(&sender1_id).unwrap(), &8000); // Latest overall
        assert_eq!(latest_times_both.get(&sender2_id).unwrap(), &6000); // Latest text
    })
}

#[xmtp_common::test]
fn it_deletes_message_by_id() {
    with_connection(|conn| {
        let group = generate_group(None);
        assert_ok!(group.store(conn));

        // Create a message
        let message = generate_message(None, Some(&group.id), None, None, None, None);
        assert_ok!(message.store(conn));

        // Verify the message exists
        let retrieved_message = conn.get_group_message(&message.id).unwrap();
        assert!(retrieved_message.is_some());
        assert_eq!(retrieved_message.unwrap().id, message.id);

        // Delete the message
        let deleted_count = conn.delete_message_by_id(&message.id).unwrap();
        assert_eq!(deleted_count, 1, "Should delete exactly 1 message");

        // Verify the message no longer exists
        let retrieved_message = conn.get_group_message(&message.id).unwrap();
        assert!(
            retrieved_message.is_none(),
            "Message should not exist after deletion"
        );

        // Test idempotency - deleting again should return 0
        let deleted_count = conn.delete_message_by_id(&message.id).unwrap();
        assert_eq!(
            deleted_count, 0,
            "Deleting non-existent message should return 0"
        );
    })
}

#[xmtp_common::test]
fn test_exclude_sender_inbox_ids_filter() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let sender1 = "inbox_id_1".to_string();
        let sender2 = "inbox_id_2".to_string();
        let sender3 = "inbox_id_3".to_string();

        // Create messages from different senders
        let messages = vec![
            generate_message(
                None,
                Some(&group.id),
                Some(1_000),
                Some(ContentType::Text),
                None,
                Some(sender1.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000),
                Some(ContentType::Text),
                None,
                Some(sender2.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(3_000),
                Some(ContentType::Text),
                None,
                Some(sender3.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(4_000),
                Some(ContentType::Text),
                None,
                Some(sender1.clone()),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(5_000),
                Some(ContentType::Text),
                None,
                Some(sender2.clone()),
            ),
        ];
        assert_ok!(messages.store(conn));

        // Test excluding sender1
        let exclude_sender1_args = MsgQueryArgs {
            exclude_sender_inbox_ids: Some(vec![sender1.clone()]),
            ..Default::default()
        };

        let filtered_messages = conn
            .get_group_messages(&group.id, &exclude_sender1_args)
            .unwrap();
        assert_eq!(filtered_messages.len(), 3); // sender2 (2) + sender3 (1)
        assert!(
            filtered_messages
                .iter()
                .all(|m| m.sender_inbox_id != sender1)
        );

        let count = conn
            .count_group_messages(&group.id, &exclude_sender1_args)
            .unwrap();
        assert_eq!(count, 3);

        // Test excluding multiple senders
        let exclude_multiple_args = MsgQueryArgs {
            exclude_sender_inbox_ids: Some(vec![sender1.clone(), sender2.clone()]),
            ..Default::default()
        };

        let filtered_messages = conn
            .get_group_messages(&group.id, &exclude_multiple_args)
            .unwrap();
        assert_eq!(filtered_messages.len(), 1); // Only sender3
        assert_eq!(filtered_messages[0].sender_inbox_id, sender3);

        let count = conn
            .count_group_messages(&group.id, &exclude_multiple_args)
            .unwrap();
        assert_eq!(count, 1);

        // Test excluding all senders
        let exclude_all_args = MsgQueryArgs {
            exclude_sender_inbox_ids: Some(vec![sender1.clone(), sender2.clone(), sender3.clone()]),
            ..Default::default()
        };

        let filtered_messages = conn
            .get_group_messages(&group.id, &exclude_all_args)
            .unwrap();
        assert_eq!(filtered_messages.len(), 0);

        let count = conn
            .count_group_messages(&group.id, &exclude_all_args)
            .unwrap();
        assert_eq!(count, 0);

        // Test excluding non-existent sender (should return all messages)
        let exclude_nonexistent_args = MsgQueryArgs {
            exclude_sender_inbox_ids: Some(vec!["nonexistent_sender".to_string()]),
            ..Default::default()
        };

        let filtered_messages = conn
            .get_group_messages(&group.id, &exclude_nonexistent_args)
            .unwrap();
        assert_eq!(filtered_messages.len(), 5); // All messages

        // Test combining with other filters
        let combined_args = MsgQueryArgs {
            exclude_sender_inbox_ids: Some(vec![sender1.clone()]),
            sent_after_ns: Some(2_000),
            ..Default::default()
        };

        let filtered_messages = conn.get_group_messages(&group.id, &combined_args).unwrap();
        assert_eq!(filtered_messages.len(), 2); // sender2 at 5000 and sender3 at 3000
        assert!(
            filtered_messages
                .iter()
                .all(|m| m.sender_inbox_id != sender1 && m.sent_at_ns > 2_000)
        );
    })
}

#[xmtp_common::test]
fn test_sort_by_sent_at() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Insert messages with different sent_at_ns in non-chronological order
        let messages = vec![
            generate_message(None, Some(&group.id), Some(3000), None, None, None),
            generate_message(None, Some(&group.id), Some(1000), None, None, None),
            generate_message(None, Some(&group.id), Some(2000), None, None, None),
        ];
        assert_ok!(messages.store(conn));

        // Test ascending by sent_at (default)
        let asc_args = MsgQueryArgs {
            sort_by: Some(SortBy::SentAt),
            direction: Some(SortDirection::Ascending),
            ..Default::default()
        };
        let asc_messages = conn.get_group_messages(&group.id, &asc_args).unwrap();
        assert_eq!(asc_messages.len(), 3);
        assert_eq!(asc_messages[0].sent_at_ns, 1000);
        assert_eq!(asc_messages[1].sent_at_ns, 2000);
        assert_eq!(asc_messages[2].sent_at_ns, 3000);

        // Test descending by sent_at
        let desc_args = MsgQueryArgs {
            sort_by: Some(SortBy::SentAt),
            direction: Some(SortDirection::Descending),
            ..Default::default()
        };
        let desc_messages = conn.get_group_messages(&group.id, &desc_args).unwrap();
        assert_eq!(desc_messages.len(), 3);
        assert_eq!(desc_messages[0].sent_at_ns, 3000);
        assert_eq!(desc_messages[1].sent_at_ns, 2000);
        assert_eq!(desc_messages[2].sent_at_ns, 1000);
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test]
fn test_sort_by_inserted_at() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Insert messages one at a time with small delays
        // SQLite evaluates strftime at insert time, but rapid inserts can get same microsecond timestamp
        // Insert with sent_at_ns that differ from insertion order
        let msg1 = generate_message(None, Some(&group.id), Some(3000), None, None, None);
        msg1.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg2 = generate_message(None, Some(&group.id), Some(1000), None, None, None);
        msg2.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg3 = generate_message(None, Some(&group.id), Some(2000), None, None, None);
        msg3.store(conn).unwrap();

        // Test ascending by inserted_at (insertion order)
        let asc_args = MsgQueryArgs {
            sort_by: Some(SortBy::InsertedAt),
            direction: Some(SortDirection::Ascending),
            ..Default::default()
        };
        let asc_messages = conn.get_group_messages(&group.id, &asc_args).unwrap();
        assert_eq!(asc_messages.len(), 3);
        // Should be in insertion order: 3000, 1000, 2000
        assert_eq!(asc_messages[0].sent_at_ns, 3000);
        assert_eq!(asc_messages[1].sent_at_ns, 1000);
        assert_eq!(asc_messages[2].sent_at_ns, 2000);

        // Verify inserted_at_ns are sequential
        let inserted1 = asc_messages[0].inserted_at_ns;
        let inserted2 = asc_messages[1].inserted_at_ns;
        let inserted3 = asc_messages[2].inserted_at_ns;
        assert!(inserted2 > inserted1);
        assert!(inserted3 > inserted2);
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test]
fn test_inserted_after_filter() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Insert messages one at a time with small delays
        // SQLite evaluates strftime at insert time, but rapid inserts can get same microsecond timestamp
        let msg1 = generate_message(None, Some(&group.id), Some(1000), None, None, None);
        msg1.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg2 = generate_message(None, Some(&group.id), Some(2000), None, None, None);
        msg2.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg3 = generate_message(None, Some(&group.id), Some(3000), None, None, None);
        msg3.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg4 = generate_message(None, Some(&group.id), Some(4000), None, None, None);
        msg4.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg5 = generate_message(None, Some(&group.id), Some(5000), None, None, None);
        msg5.store(conn).unwrap();

        // Get all messages to get their inserted_at_ns
        let all_messages = conn
            .get_group_messages(&group.id, &MsgQueryArgs::default())
            .unwrap();
        assert_eq!(all_messages.len(), 5);

        // Use inserted_after to get messages after the 2nd one
        let second_inserted_at = all_messages[1].inserted_at_ns;
        println!("Filtering for inserted_at_ns > {}", second_inserted_at);
        let after_args = MsgQueryArgs {
            inserted_after_ns: Some(second_inserted_at),
            ..Default::default()
        };
        let after_messages = conn.get_group_messages(&group.id, &after_args).unwrap();

        // Should get messages 3, 4, 5
        assert_eq!(after_messages.len(), 3);
        assert_eq!(after_messages[0].sent_at_ns, 3000);
        assert_eq!(after_messages[1].sent_at_ns, 4000);
        assert_eq!(after_messages[2].sent_at_ns, 5000);

        // Verify all after_messages have inserted_at_ns within the last 5 minutes
        let five_minutes_ago_ns = xmtp_common::time::now_ns() - (5 * 60 * 1_000_000_000);
        for msg in &after_messages {
            assert!(
                msg.inserted_at_ns >= five_minutes_ago_ns,
                "Message inserted_at_ns {} is older than 5 minutes ago {}",
                msg.inserted_at_ns,
                five_minutes_ago_ns
            );
        }
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test]
fn test_inserted_before_filter() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Insert messages one at a time with small delays
        // SQLite evaluates strftime at insert time, but rapid inserts can get same microsecond timestamp
        let msg1 = generate_message(None, Some(&group.id), Some(1000), None, None, None);
        msg1.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg2 = generate_message(None, Some(&group.id), Some(2000), None, None, None);
        msg2.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg3 = generate_message(None, Some(&group.id), Some(3000), None, None, None);
        msg3.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg4 = generate_message(None, Some(&group.id), Some(4000), None, None, None);
        msg4.store(conn).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        let msg5 = generate_message(None, Some(&group.id), Some(5000), None, None, None);
        msg5.store(conn).unwrap();

        // Get all messages to get their inserted_at_ns
        let all_messages = conn
            .get_group_messages(&group.id, &MsgQueryArgs::default())
            .unwrap();
        assert_eq!(all_messages.len(), 5);

        // Use inserted_before to get messages before the 4th one
        let fourth_inserted_at = all_messages[3].inserted_at_ns;
        let before_args = MsgQueryArgs {
            inserted_before_ns: Some(fourth_inserted_at),
            ..Default::default()
        };
        let before_messages = conn.get_group_messages(&group.id, &before_args).unwrap();

        // Should get messages 1, 2, 3
        assert_eq!(before_messages.len(), 3);
        assert_eq!(before_messages[0].sent_at_ns, 1000);
        assert_eq!(before_messages[1].sent_at_ns, 2000);
        assert_eq!(before_messages[2].sent_at_ns, 3000);
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test]
fn test_inserted_at_based_pagination() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Insert 10 messages one at a time with small delays
        // SQLite evaluates strftime at insert time, but rapid inserts can get same microsecond timestamp
        for i in 1..=10 {
            let msg = generate_message(None, Some(&group.id), Some(i * 1000), None, None, None);
            msg.store(conn).unwrap();
            if i < 10 {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }

        // Page 1: Get first 3 messages
        let page1_args = MsgQueryArgs {
            limit: Some(3),
            ..Default::default()
        };
        let page1 = conn.get_group_messages(&group.id, &page1_args).unwrap();
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[2].sent_at_ns, 3000);
        let last_inserted_page1 = page1[2].inserted_at_ns;

        // Page 2: Get next 3 messages after page 1
        let page2_args = MsgQueryArgs {
            inserted_after_ns: Some(last_inserted_page1),
            limit: Some(3),
            ..Default::default()
        };
        let page2 = conn.get_group_messages(&group.id, &page2_args).unwrap();
        assert_eq!(page2.len(), 3);
        assert_eq!(page2[0].sent_at_ns, 4000);
        assert_eq!(page2[2].sent_at_ns, 6000);
        let last_inserted_page2 = page2[2].inserted_at_ns;

        // Page 3: Get next 3 messages after page 2
        let page3_args = MsgQueryArgs {
            inserted_after_ns: Some(last_inserted_page2),
            limit: Some(3),
            ..Default::default()
        };
        let page3 = conn.get_group_messages(&group.id, &page3_args).unwrap();
        assert_eq!(page3.len(), 3);
        assert_eq!(page3[0].sent_at_ns, 7000);
        assert_eq!(page3[2].sent_at_ns, 9000);

        // Verify no duplicates across pages
        let all_page_ids: Vec<_> = page1
            .iter()
            .chain(page2.iter())
            .chain(page3.iter())
            .map(|m| &m.id)
            .collect();
        let unique_ids: std::collections::HashSet<_> = all_page_ids.iter().collect();
        assert_eq!(all_page_ids.len(), unique_ids.len());
    })
}

#[xmtp_common::test]
fn test_inserted_at_populated_in_all_queries() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let msg = generate_message(None, Some(&group.id), Some(1000), None, None, None);
        msg.store(conn).unwrap();

        // Test get_group_message
        let fetched = conn.get_group_message(&msg.id).unwrap().unwrap();
        assert!(fetched.inserted_at_ns > 0);

        // Test get_group_message_by_timestamp
        let by_timestamp = conn
            .get_group_message_by_timestamp(&group.id, 1000)
            .unwrap()
            .unwrap();
        assert!(by_timestamp.inserted_at_ns > 0);

        // Test group_messages_paged
        let paged_messages = conn
            .group_messages_paged(&MsgQueryArgs::default(), 0)
            .unwrap();
        assert_eq!(paged_messages.len(), 1);
        assert!(paged_messages[0].inserted_at_ns > 0);
    })
}

#[xmtp_common::test]
fn test_expired_messages_excluded_from_queries() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let now = xmtp_common::time::now_ns();
        let past = now - 1_000_000_000; // 1 second ago
        let future = now + 1_000_000_000_000; // 1000 seconds from now

        // Create messages with different expiration states
        let messages = vec![
            // Message with no expiration (should be included)
            generate_message(
                None,
                Some(&group.id),
                Some(1_000),
                Some(ContentType::Text),
                None,
                None,
            ),
            // Message expired in the past (should be excluded)
            generate_message(
                None,
                Some(&group.id),
                Some(2_000),
                Some(ContentType::Text),
                Some(past),
                None,
            ),
            // Message expiring in the future (should be included)
            generate_message(
                None,
                Some(&group.id),
                Some(3_000),
                Some(ContentType::Text),
                Some(future),
                None,
            ),
        ];
        assert_ok!(messages.store(conn));

        // Query should only return non-expired messages
        let results = conn
            .get_group_messages(&group.id, &MsgQueryArgs::default())
            .unwrap();

        assert_eq!(
            results.len(),
            2,
            "Should only return 2 non-expired messages"
        );

        // Verify we got the right messages (no expiration and future expiration)
        let sent_times: Vec<i64> = results.iter().map(|m| m.sent_at_ns).collect();
        assert!(
            sent_times.contains(&1_000),
            "Should include message with no expiration"
        );
        assert!(
            sent_times.contains(&3_000),
            "Should include message with future expiration"
        );
        assert!(
            !sent_times.contains(&2_000),
            "Should exclude expired message"
        );
    })
}

#[test]
fn test_content_type_is_deletable() {
    // User content should be deletable
    assert!(ContentType::Text.is_deletable());
    assert!(ContentType::Reply.is_deletable());
    assert!(ContentType::Attachment.is_deletable());
    assert!(ContentType::RemoteAttachment.is_deletable());
    assert!(ContentType::TransactionReference.is_deletable());
    assert!(ContentType::WalletSendCalls.is_deletable());

    // System messages should NOT be deletable
    assert!(!ContentType::GroupMembershipChange.is_deletable());
    assert!(!ContentType::GroupUpdated.is_deletable());
    assert!(!ContentType::LeaveRequest.is_deletable());

    // Metadata should NOT be deletable
    assert!(!ContentType::Reaction.is_deletable());
    assert!(!ContentType::ReadReceipt.is_deletable());

    // Delete messages should NOT be deletable (prevents recursive deletion)
    assert!(!ContentType::DeleteMessage.is_deletable());

    // Unknown content types should NOT be deletable for safety
    // (we don't know if they're system messages that shouldn't be deleted)
    assert!(!ContentType::Unknown.is_deletable());
}

#[test]
fn test_group_message_kind_is_deletable() {
    // Application messages should be deletable
    assert!(GroupMessageKind::Application.is_deletable());

    // Membership changes are transcript messages - should NOT be deletable
    assert!(!GroupMessageKind::MembershipChange.is_deletable());
}
