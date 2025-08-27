#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use super::*;
use crate::{
    EncryptedMessageStore, Store, group::tests::generate_group, prelude::*,
    test_utils::with_connection,
};
use xmtp_common::{assert_err, assert_ok, rand_time, rand_vec};
use xmtp_content_types::should_push;

pub(crate) fn generate_message(
    kind: Option<GroupMessageKind>,
    group_id: Option<&[u8]>,
    sent_at_ns: Option<i64>,
    content_type: Option<ContentType>,
    expire_at_ns: Option<i64>,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id: rand_vec::<24>(),
        group_id: group_id.map(<[u8]>::to_vec).unwrap_or(rand_vec::<24>()),
        decrypted_message_bytes: rand_vec::<24>(),
        sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
        sender_installation_id: rand_vec::<24>(),
        sender_inbox_id: "0x0".to_string(),
        kind: kind.unwrap_or(GroupMessageKind::Application),
        delivery_status: DeliveryStatus::Published,
        content_type: content_type.unwrap_or(ContentType::Unknown),
        version_major: 0,
        version_minor: 0,
        authority_id: "unknown".to_string(),
        reference_id: None,
        sequence_id: None,
        originator_id: None,
        expire_at_ns,
    }
}

#[xmtp_common::test]
async fn it_does_not_error_on_empty_messages() {
    with_connection(|conn| {
        let id = vec![0x0];
        assert_eq!(conn.get_group_message(id).unwrap(), None);
    })
    .await
}

#[xmtp_common::test]
async fn it_gets_messages() {
    with_connection(|conn| {
        let group = generate_group(None);
        let message = generate_message(None, Some(&group.id), None, None, None);
        group.store(conn).unwrap();
        let id = message.id.clone();

        message.store(conn).unwrap();

        let stored_message = conn.get_group_message(id).unwrap().unwrap();
        assert_eq!(
            stored_message.decrypted_message_bytes,
            message.decrypted_message_bytes
        );
    })
    .await
}

#[xmtp_common::test]
async fn it_cannot_insert_message_without_group() {
    use diesel::result::DatabaseErrorKind::ForeignKeyViolation;
    let store = EncryptedMessageStore::new_test().await;
    let conn = DbConnection::new(store.conn());
    let message = generate_message(None, None, None, None, None);
    let result = message.store(&conn);
    assert_err!(
        result,
        crate::StorageError::Connection(crate::ConnectionError::Database(
            diesel::result::Error::DatabaseError(ForeignKeyViolation, _)
        ))
    );
}

#[xmtp_common::test]
async fn it_gets_many_messages() {
    use crate::encrypted_store::schema::group_messages::dsl;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        for idx in 0..50 {
            let msg = generate_message(None, Some(&group.id), Some(idx), None, None);
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
    .await
}

#[xmtp_common::test]
async fn it_gets_messages_by_time() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let messages = vec![
            generate_message(None, Some(&group.id), Some(1_000), None, None),
            generate_message(None, Some(&group.id), Some(100_000), None, None),
            generate_message(None, Some(&group.id), Some(10_000), None, None),
            generate_message(None, Some(&group.id), Some(1_000_000), None, None),
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
    .await
}

#[xmtp_common::test]
async fn it_deletes_middle_message_by_expiration_time() {
    with_connection(|conn| {
        let mut group = generate_group(None);

        let disappear_from_ns = Some(1_000_500_000); // After Message 1
        let disappear_in_ns = Some(500_000); // Before Message 3
        group.message_disappear_from_ns = disappear_from_ns;
        group.message_disappear_in_ns = disappear_in_ns;

        group.store(conn).unwrap();

        let messages = vec![
            generate_message(None, Some(&group.id), Some(1_000_000_000), None, None),
            generate_message(
                None,
                Some(&group.id),
                Some(1_001_000_000),
                None,
                Some(1_001_000_000),
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000_000_000_000_000_000),
                None,
                None,
            ),
        ];
        assert_ok!(messages.store(conn));

        let result = conn.delete_expired_messages().unwrap();
        assert_eq!(result, 1); // Ensure exactly 1 message is deleted

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
    .await
}

#[xmtp_common::test]
async fn it_gets_messages_by_kind() {
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
    .await
}

#[xmtp_common::test]
async fn it_orders_messages_by_sent() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        assert_eq!(group.last_message_ns, None);

        let messages = vec![
            generate_message(None, Some(&group.id), Some(10_000), None, None),
            generate_message(None, Some(&group.id), Some(1_000), None, None),
            generate_message(None, Some(&group.id), Some(100_000), None, None),
            generate_message(None, Some(&group.id), Some(1_000_000), None, None),
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
    .await
}

#[xmtp_common::test]
async fn it_gets_messages_by_content_type() {
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
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(2_000),
                Some(ContentType::GroupMembershipChange),
                None,
            ),
            generate_message(
                None,
                Some(&group.id),
                Some(3_000),
                Some(ContentType::GroupUpdated),
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
        assert!(should_push(text_messages[0].content_type.to_string()));

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
        assert!(!should_push(updated_messages[0].content_type.to_string()));
        assert_eq!(updated_messages[0].sent_at_ns, 3_000);
    })
    .await
}

#[xmtp_common::test]
async fn it_places_group_updated_message_correctly_based_on_sort_order() {
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
        );

        let earlier_msg = generate_message(
            Some(GroupMessageKind::Application),
            Some(&group.id),
            Some(1_000),
            Some(ContentType::Text),
            None,
        );

        let later_msg = generate_message(
            Some(GroupMessageKind::Application),
            Some(&group.id),
            Some(10_000),
            Some(ContentType::Text),
            None,
        );

        assert_ok!(
            vec![
                group_updated_msg.clone(),
                earlier_msg.clone(),
                later_msg.clone()
            ]
            .store(conn)
        );

        // Ascending order: GroupUpdated should be at position 0
        let messages_asc = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    direction: Some(SortDirection::Ascending),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages_asc.len(), 3);
        assert_eq!(messages_asc[0].content_type, ContentType::GroupUpdated);
        assert_eq!(messages_asc[1].sent_at_ns, 1_000);
        assert_eq!(messages_asc[2].sent_at_ns, 10_000);

        // Descending order: GroupUpdated should be at the end
        let messages_desc = conn
            .get_group_messages(
                &group.id,
                &MsgQueryArgs {
                    direction: Some(SortDirection::Descending),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(messages_desc.len(), 3);
        assert_eq!(messages_desc[0].sent_at_ns, 10_000);
        assert_eq!(messages_desc[1].sent_at_ns, 1_000);
        assert_eq!(messages_desc[2].content_type, ContentType::GroupUpdated);
    })
    .await
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
        sequence_id: None,
        originator_id: None,
        expire_at_ns: None,
    };
    message.store(conn).unwrap();
    message
}

#[xmtp_common::test]
async fn test_inbound_relations_with_results() {
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
    .await
}

#[xmtp_common::test]
async fn test_relations_when_no_references_exist() {
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
    .await
}

#[xmtp_common::test]
async fn test_inbound_relations_no_main_query_results() {
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
    .await
}

#[xmtp_common::test]
async fn test_inbound_relations_with_limit() {
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
    .await
}

#[xmtp_common::test]
async fn test_relations_with_content_type_filters() {
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
    .await
}

#[xmtp_common::test]
async fn test_outbound_relations_with_results() {
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
    .await
}

#[xmtp_common::test]
async fn test_outbound_relations_no_main_query_results() {
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
    .await
}

#[xmtp_common::test]
async fn test_outbound_relations_with_limit() {
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
    .await
}

#[xmtp_common::test]
async fn test_both_inbound_and_outbound_relations() {
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
    .await
}

#[xmtp_common::test]
async fn test_relation_filters_none_behavior() {
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
    .await
}

#[xmtp_common::test]
async fn test_complex_relation_chain() {
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
    .await
}

#[xmtp_common::test]
async fn test_inbound_relation_counts() {
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
    .await
}
