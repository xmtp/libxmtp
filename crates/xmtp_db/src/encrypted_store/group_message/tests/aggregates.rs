//! Latest-message-time and counting tests.

use super::super::*;
use crate::{Store, group::tests::generate_group, test_utils::with_connection};
use xmtp_common::assert_ok;

use super::helpers::*;

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
            .get_latest_message_times_by_sender(group.id, &[ContentType::Text])
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
            .get_latest_message_times_by_sender(group.id, &[ContentType::Text])
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
            .get_latest_message_times_by_sender(group.id, &[ContentType::Text])
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
            .get_latest_message_times_by_sender(group.id, &[ContentType::Attachment])
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
            .get_latest_message_times_by_sender(group1.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times.len(), 1);
        assert_eq!(
            latest_times.get(&sender_id).unwrap(),
            &6000 // Should be the latest message across all groups with the same dm_id
        );

        // Test that querying any of the groups returns the same result
        let latest_times_group2 = conn
            .get_latest_message_times_by_sender(group2.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times_group2.len(), 1);
        assert_eq!(latest_times_group2.get(&sender_id).unwrap(), &6000);

        let latest_times_group3 = conn
            .get_latest_message_times_by_sender(group3.id, &[ContentType::Text])
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
        let create_messages = |group_id: &GroupId| {
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
            .get_latest_message_times_by_sender(group.id, &[ContentType::Text])
            .unwrap();

        assert_eq!(latest_times_text.len(), 2);
        assert_eq!(latest_times_text.get(&sender1_id).unwrap(), &5000);
        assert_eq!(latest_times_text.get(&sender2_id).unwrap(), &6000);

        // Test filtering by attachment only - should get only sender1
        let latest_times_attachment = conn
            .get_latest_message_times_by_sender(group.id, &[ContentType::Attachment])
            .unwrap();

        assert_eq!(latest_times_attachment.len(), 1);
        assert_eq!(latest_times_attachment.get(&sender1_id).unwrap(), &8000);

        // Test filtering by both - should get both senders with their latest overall times
        let latest_times_both = conn
            .get_latest_message_times_by_sender(
                group.id,
                &[ContentType::Text, ContentType::Attachment],
            )
            .unwrap();

        assert_eq!(latest_times_both.len(), 2);
        assert_eq!(latest_times_both.get(&sender1_id).unwrap(), &8000); // Latest overall
        assert_eq!(latest_times_both.get(&sender2_id).unwrap(), &6000); // Latest text
    })
}
