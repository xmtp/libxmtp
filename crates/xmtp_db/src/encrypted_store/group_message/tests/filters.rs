//! Deletion, filtering, sorting, pagination, and expiry tests.

use super::super::*;
use crate::{Store, group::tests::generate_group, test_utils::with_connection};
use xmtp_common::assert_ok;

use super::helpers::*;

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
            .get_group_message_by_timestamp(group.id, 1000)
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

#[xmtp_common::test(unwrap_try = true)]
fn test_min_expire_at_ns() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn)?;

        // No disappearing messages yet -> None
        assert_eq!(conn.min_expire_at_ns()?, None);

        // Two published Application messages with expiries 5000 and 3000,
        // plus one with no expiry (must be ignored).
        generate_message(
            None,
            Some(&group.id),
            Some(1_000),
            Some(ContentType::Text),
            Some(5_000),
            None,
        )
        .store(conn)?;
        generate_message(
            None,
            Some(&group.id),
            Some(1_000),
            Some(ContentType::Text),
            Some(3_000),
            None,
        )
        .store(conn)?;
        generate_message(
            None,
            Some(&group.id),
            Some(1_000),
            Some(ContentType::Text),
            None,
            None,
        )
        .store(conn)?;

        // Soonest expiry wins; the NULL-expiry row is excluded.
        assert_eq!(conn.min_expire_at_ns()?, Some(3_000));
    })
}
