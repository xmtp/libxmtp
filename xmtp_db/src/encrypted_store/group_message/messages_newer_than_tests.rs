#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use super::*;
use crate::{Store, group::tests::generate_group, test_utils::with_connection};
use xmtp_common::{assert_ok, rand_vec};

// Helper function to create a message with specific sequence_id and originator_id
fn generate_message_with_cursor(
    group_id: &[u8],
    originator_id: i64,
    sequence_id: i64,
    sent_at_ns: i64,
) -> StoredGroupMessage {
    StoredGroupMessage {
        id: rand_vec::<24>(),
        group_id: group_id.to_vec(),
        decrypted_message_bytes: rand_vec::<24>(),
        sent_at_ns,
        sender_installation_id: rand_vec::<24>(),
        sender_inbox_id: "0x0".to_string(),
        kind: GroupMessageKind::Application,
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 0,
        version_minor: 0,
        authority_id: "unknown".to_string(),
        reference_id: None,
        inserted_at_ns: sent_at_ns,
        sequence_id,
        originator_id,
        expire_at_ns: None,
    }
}

#[xmtp_common::test]
fn test_messages_newer_than_basic() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create messages with different originator_ids and sequence_ids
        let messages = vec![
            generate_message_with_cursor(&group.id, 1, 10, 1000),
            generate_message_with_cursor(&group.id, 1, 20, 2000),
            generate_message_with_cursor(&group.id, 2, 15, 3000),
            generate_message_with_cursor(&group.id, 2, 25, 4000),
        ];
        assert_ok!(messages.store(conn));

        // Set cursor to originator 1: seq 10, originator 2: seq 15
        let mut cursor = GlobalCursor::default();
        cursor.insert(1, 10);
        cursor.insert(2, 15);

        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group.id.clone(), cursor);

        // Should return messages newer than cursor
        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 2);
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 1 && c.sequence_id == 20)
        );
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 2 && c.sequence_id == 25)
        );
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_new_originator() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Create messages from originator 1 and 2
        let messages = vec![
            generate_message_with_cursor(&group.id, 1, 10, 1000),
            generate_message_with_cursor(&group.id, 2, 5, 2000),
            generate_message_with_cursor(&group.id, 2, 10, 3000),
        ];
        assert_ok!(messages.store(conn));

        // Cursor only knows about originator 1
        let mut cursor = GlobalCursor::default();
        cursor.insert(1, 10);

        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group.id.clone(), cursor);

        // Should return all messages from originator 2 (new originator)
        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 2);
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 2 && c.sequence_id == 5)
        );
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 2 && c.sequence_id == 10)
        );
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_multiple_groups() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group1 = generate_group(None);
        let group2 = generate_group(None);
        group1.store(conn).unwrap();
        group2.store(conn).unwrap();

        // Create messages in both groups
        let messages = vec![
            generate_message_with_cursor(&group1.id, 1, 10, 1000),
            generate_message_with_cursor(&group1.id, 1, 20, 2000),
            generate_message_with_cursor(&group2.id, 1, 5, 3000),
            generate_message_with_cursor(&group2.id, 1, 15, 4000),
        ];
        assert_ok!(messages.store(conn));

        // Set different cursors for each group
        let mut cursor1 = GlobalCursor::default();
        cursor1.insert(1, 10);

        let mut cursor2 = GlobalCursor::default();
        cursor2.insert(1, 5);

        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group1.id.clone(), cursor1);
        cursors_by_group.insert(group2.id.clone(), cursor2);

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 2);
        assert!(newer.iter().any(|c| c.sequence_id == 20)); // from group1
        assert!(newer.iter().any(|c| c.sequence_id == 15)); // from group2
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_batching() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        // Create more than 100 groups to test batching
        let mut groups = Vec::new();
        for _ in 0..150 {
            let group = generate_group(None);
            group.store(conn).unwrap();
            groups.push(group);
        }

        // Create one message per group
        let mut messages = Vec::new();
        for (i, group) in groups.iter().enumerate() {
            let msg = generate_message_with_cursor(&group.id, 1, (i + 1) as i64, 1000 + i as i64);
            messages.push(msg);
        }
        assert_ok!(messages.store(conn));

        // Set cursor to 0 for all groups (all messages are newer)
        let mut cursors_by_group = HashMap::new();
        for group in &groups {
            let cursor = GlobalCursor::default();
            cursors_by_group.insert(group.id.clone(), cursor);
        }

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        // Should get all 150 messages
        assert_eq!(newer.len(), 150);
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_empty_cursor() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let messages = vec![
            generate_message_with_cursor(&group.id, 1, 10, 1000),
            generate_message_with_cursor(&group.id, 2, 5, 2000),
            generate_message_with_cursor(&group.id, 3, 8, 3000),
        ];
        assert_ok!(messages.store(conn));

        // Empty cursor - all messages should be newer
        let cursor = GlobalCursor::default();
        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group.id.clone(), cursor);

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 3);
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_no_new_messages() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        let messages = vec![
            generate_message_with_cursor(&group.id, 1, 10, 1000),
            generate_message_with_cursor(&group.id, 2, 15, 2000),
        ];
        assert_ok!(messages.store(conn));

        // Cursor is already at or past all messages
        let mut cursor = GlobalCursor::default();
        cursor.insert(1, 10);
        cursor.insert(2, 15);

        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group.id.clone(), cursor);

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 0);
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_mixed_originators() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // Messages from 3 originators
        let messages = vec![
            generate_message_with_cursor(&group.id, 1, 5, 1000),
            generate_message_with_cursor(&group.id, 1, 10, 2000),
            generate_message_with_cursor(&group.id, 2, 3, 3000),
            generate_message_with_cursor(&group.id, 2, 7, 4000),
            generate_message_with_cursor(&group.id, 3, 2, 5000),
            generate_message_with_cursor(&group.id, 3, 4, 6000),
        ];
        assert_ok!(messages.store(conn));

        // Cursor knows about originator 1 (seq 5) and originator 2 (seq 3)
        // Does not know about originator 3
        let mut cursor = GlobalCursor::default();
        cursor.insert(1, 5);
        cursor.insert(2, 3);

        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group.id.clone(), cursor);

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 4);
        // From originator 1: seq 10 (newer than 5)
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 1 && c.sequence_id == 10)
        );
        // From originator 2: seq 7 (newer than 3)
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 2 && c.sequence_id == 7)
        );
        // From originator 3: both messages (new originator)
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 3 && c.sequence_id == 2)
        );
        assert!(
            newer
                .iter()
                .any(|c| c.originator_id == 3 && c.sequence_id == 4)
        );
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_empty_groups() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();

        // No messages in group
        let cursor = GlobalCursor::default();
        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group.id.clone(), cursor);

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        assert_eq!(newer.len(), 0);
    })
}

#[xmtp_common::test]
fn test_messages_newer_than_per_group_cursors() {
    use std::collections::HashMap;
    use xmtp_proto::types::GlobalCursor;

    with_connection(|conn| {
        let group1 = generate_group(None);
        let group2 = generate_group(None);
        group1.store(conn).unwrap();
        group2.store(conn).unwrap();

        // Create messages in both groups from the same originator
        let messages = vec![
            // Group 1 messages from originator 1
            generate_message_with_cursor(&group1.id, 1, 50, 1000),
            generate_message_with_cursor(&group1.id, 1, 150, 2000), // newer than cursor (100)
            // Group 2 messages from originator 1
            generate_message_with_cursor(&group2.id, 1, 200, 3000), // older than cursor (300)
            generate_message_with_cursor(&group2.id, 1, 400, 4000), // newer than cursor (300)
        ];
        assert_ok!(messages.store(conn));

        // Group 1 has cursor {originator_1: 100}
        let mut cursor1 = GlobalCursor::default();
        cursor1.insert(1, 100);

        // Group 2 has cursor {originator_1: 300}
        let mut cursor2 = GlobalCursor::default();
        cursor2.insert(1, 300);

        let mut cursors_by_group = HashMap::new();
        cursors_by_group.insert(group1.id.clone(), cursor1);
        cursors_by_group.insert(group2.id.clone(), cursor2);

        let newer = conn.messages_newer_than(&cursors_by_group).unwrap();

        // Should only get messages newer than each group's specific cursor
        assert_eq!(newer.len(), 2);

        // From group 1: sequence_id 150 (> 100)
        assert!(newer.iter().any(|c| c.sequence_id == 150));

        // From group 2: sequence_id 400 (> 300)
        assert!(newer.iter().any(|c| c.sequence_id == 400));

        // Should NOT include group 2's message with sequence_id 200 (< 300)
        assert!(!newer.iter().any(|c| c.sequence_id == 200));

        // Should NOT include group 1's message with sequence_id 50 (< 100)
        assert!(!newer.iter().any(|c| c.sequence_id == 50));
    })
}
