//! Reference-chain and inbound/outbound relation tests.

use super::super::*;
use crate::{Store, group::tests::generate_group, test_utils::with_connection};

use super::helpers::*;

// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
fn expired_relation_rows_are_not_enriched_or_counted() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();
        let mut expired_parent = generate_message(
            None,
            Some(&group.id),
            None,
            Some(ContentType::Text),
            Some(1),
            None,
        );
        expired_parent.idempotency_key = "expired parent".into();
        expired_parent.store(conn).unwrap();
        let visible_parent = generate_message(
            None,
            Some(&group.id),
            None,
            Some(ContentType::Text),
            None,
            None,
        );
        visible_parent.store(conn).unwrap();

        let mut expired_reply = generate_message(
            None,
            Some(&group.id),
            None,
            Some(ContentType::Reply),
            Some(1),
            None,
        );
        expired_reply.reference_id = Some(visible_parent.id.clone());
        expired_reply.store(conn).unwrap();
        let mut visible_reply = generate_message(
            None,
            Some(&group.id),
            None,
            Some(ContentType::Reply),
            None,
            None,
        );
        visible_reply.reference_id = Some(visible_parent.id.clone());
        visible_reply.store(conn).unwrap();

        let parents = conn
            .get_outbound_relations(
                &group.id,
                &[expired_parent.id.as_slice(), visible_parent.id.as_slice()],
            )
            .unwrap();
        assert!(!parents.contains_key(&expired_parent.id));
        assert!(parents.contains_key(&visible_parent.id));

        let query = || {
            RelationQuery::builder()
                .content_types(Some(vec![ContentType::Reply]))
                .build()
                .unwrap()
        };
        let relations = conn
            .get_inbound_relations(&group.id, &[visible_parent.id.as_slice()], query())
            .unwrap();
        let replies = relations.get(&visible_parent.id).unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].id, visible_reply.id);

        let counts = conn
            .get_inbound_relation_counts(&group.id, &[visible_parent.id.as_slice()], query())
            .unwrap();
        assert_eq!(counts.get(&visible_parent.id), Some(&1));
    })
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

/// Store a reaction to `parent` in `group` at the given position.
fn store_reaction<C: ConnectionExt>(
    conn: &DbConnection<C>,
    group_id: &GroupId,
    parent: &[u8],
    sent_at_ns: i64,
    sequence_id: i64,
    id: Vec<u8>,
) -> StoredGroupMessage {
    let mut reaction = generate_message(
        None,
        Some(group_id),
        Some(sent_at_ns),
        Some(ContentType::Reaction),
        None,
        None,
    );
    reaction.reference_id = Some(parent.to_vec());
    reaction.sequence_id = sequence_id;
    reaction.id = id;
    reaction.store(conn).unwrap();
    reaction
}

/// The limit holds for each parent, so a parent with many reactions does not
/// starve another. Equal send times break by sequence id: a removal published
/// after its addition sorts last even when its message id is lower.
// verifies: PROC-053
#[xmtp_common::test(unwrap_try = true)]
fn inbound_relations_limit_applies_to_each_parent() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();
        let parents: Vec<_> = (0..3)
            .map(|_| generate_message_with_reference(conn, &group.id, 1, ContentType::Text, None))
            .collect();
        for parent in &parents[..2] {
            for index in 0..10 {
                store_reaction(
                    conn,
                    &group.id,
                    &parent.id,
                    1_000 + index,
                    index + 1,
                    xmtp_common::rand_vec::<32>(),
                );
            }
        }
        let addition = store_reaction(conn, &group.id, &parents[2].id, 2_000, 10, vec![0xff; 32]);
        let removal = store_reaction(conn, &group.id, &parents[2].id, 2_000, 11, vec![0x01; 32]);

        let window = |limit| {
            conn.get_inbound_relations(
                &group.id,
                &parents
                    .iter()
                    .map(|parent| parent.id.as_slice())
                    .collect::<Vec<_>>(),
                RelationQuery::builder()
                    .content_types(Some(vec![ContentType::Reaction]))
                    .limit(Some(limit))
                    .build()
                    .unwrap(),
            )
            .unwrap()
        };
        let latest_three = window(3);
        for parent in &parents[..2] {
            let sent: Vec<_> = latest_three[&parent.id]
                .iter()
                .map(|reaction| reaction.sent_at_ns)
                .collect();
            assert_eq!(sent, [1_007, 1_008, 1_009]);
        }
        assert_eq!(
            latest_three[&parents[2].id]
                .iter()
                .map(|reaction| &reaction.id)
                .collect::<Vec<_>>(),
            [&addition.id, &removal.id]
        );
        let latest_one = window(1);
        assert_eq!(
            latest_one[&parents[2].id]
                .iter()
                .map(|reaction| &reaction.id)
                .collect::<Vec<_>>(),
            [&removal.id]
        );
    })
}

/// A stitched DM's reactions can be stored in any of its groups, so the
/// window, the count and the cursor all range over every group of the DM.
// verifies: PROC-053, PROC-054, DMS-009
#[xmtp_common::test(unwrap_try = true)]
fn inbound_relations_window_spans_stitched_dm_groups() {
    use crate::group::tests::generate_dm;
    use xmtp_common::Generate;
    with_connection(|conn| {
        let first = generate_dm(None);
        first.store(conn).unwrap();
        let mut second = first.clone();
        second.id = GroupId::generate();
        second.store(conn).unwrap();
        let parent = generate_message_with_reference(conn, &first.id, 1, ContentType::Text, None);
        let mut reactions = Vec::new();
        for (group_id, sent_at_ns) in [
            (first.id, 100),
            (first.id, 101),
            (first.id, 102),
            (second.id, 200),
            (second.id, 201),
            (second.id, 202),
            (second.id, 203),
        ] {
            reactions.push(store_reaction(
                conn,
                &group_id,
                &parent.id,
                sent_at_ns,
                0,
                xmtp_common::rand_vec::<32>(),
            ));
        }
        let reaction_only = || RelationQuery {
            content_types: Some(vec![ContentType::Reaction]),
            ..Default::default()
        };
        let sent = |query: RelationQuery| -> Vec<i64> {
            conn.get_inbound_relations(&first.id, &[parent.id.as_slice()], query)
                .unwrap()
                .remove(&parent.id)
                .unwrap_or_default()
                .into_iter()
                .map(|reaction| reaction.sent_at_ns)
                .collect()
        };

        assert_eq!(
            sent(RelationQuery {
                limit: Some(5),
                ..reaction_only()
            }),
            [102, 200, 201, 202, 203]
        );
        let counts = conn
            .get_inbound_relation_counts(&first.id, &[parent.id.as_slice()], reaction_only())
            .unwrap();
        assert_eq!(counts[&parent.id], 7);

        let before = conn
            .get_inbound_relation_cursor(
                &first.id,
                &parent.id,
                &reactions[4].id,
                &[ContentType::Reaction],
            )
            .unwrap();
        assert!(before.is_some(), "a sibling group's reaction is a cursor");
        assert_eq!(
            sent(RelationQuery {
                before,
                ..reaction_only()
            }),
            [100, 101, 102, 200]
        );
    })
}
