//! Basic query and lookup tests.

use super::super::*;
use crate::{Store, group::tests::generate_group, prelude::*, test_utils::with_connection};
use xmtp_common::{assert_err, assert_ok};

use super::helpers::*;

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

#[xmtp_common::test(unwrap_try = true)]
fn it_cannot_insert_message_without_group() {
    use diesel::result::DatabaseErrorKind::ForeignKeyViolation;
    with_connection(|conn| {
        let message = generate_message(None, None, None, None, None, None);
        let result = message.store(&conn);
        assert_err!(
            result,
            crate::StorageError::DieselResult(diesel::result::Error::DatabaseError(
                ForeignKeyViolation,
                _
            ))
        );
        assert!(conn.get_group_message(message.id).unwrap().is_none());
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
            .raw_query(|raw_conn| {
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

#[xmtp_common::test(unwrap_try = true)]
fn it_orders_messages_by_sent() {
    use diesel::connection::SimpleConnection;

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

        // A forward client clock must not hide the newer canonical message.
        let mut skewed = generate_message(None, Some(&group.id), Some(2_000_000), None, None, None);
        skewed.delivery_status = DeliveryStatus::Unpublished;
        skewed.store(conn)?;
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(2_000_000)
        );
        assert_eq!(
            conn.set_delivery_status_to_published(&skewed.id, 800_000, Cursor(50), None)?,
            1
        );
        assert_eq!(conn.get_group_message(&skewed.id)??.sent_at_ns, 800_000);
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(1_000_000)
        );
        let published_cursor = conn.current_delivery_cursor()?;
        conn.set_delivery_status_to_published(&skewed.id, 800_000, Cursor(50), None)?;
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(1_000_000)
        );
        assert_eq!(conn.current_delivery_cursor()?, published_cursor);

        let mut delayed =
            generate_message(None, Some(&group.id), Some(1_100_000), None, None, None);
        delayed.delivery_status = DeliveryStatus::Unpublished;
        delayed.store(conn)?;
        conn.set_delivery_status_to_published(&delayed.id, 1_200_000, Cursor(60), None)?;
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(1_200_000)
        );

        // Failure of the activity update must also roll back publication and delivery.
        let mut rejected =
            generate_message(None, Some(&group.id), Some(3_000_000), None, None, None);
        rejected.delivery_status = DeliveryStatus::Unpublished;
        rejected.store(conn)?;
        let before_failure = conn.current_delivery_cursor()?;
        conn.raw_query(|conn| {
            conn.batch_execute(
                "CREATE TEMP TRIGGER fail_publication_activity BEFORE UPDATE OF last_message_ns ON groups BEGIN SELECT RAISE(ABORT, 'injected activity failure'); END;",
            )
        })?;
        assert!(
            conn.set_delivery_status_to_published(&rejected.id, 900_000, Cursor(70), None)
                .is_err()
        );
        let unchanged = conn.get_group_message(&rejected.id)??;
        assert_eq!(unchanged.sent_at_ns, 3_000_000);
        assert_eq!(unchanged.delivery_status, DeliveryStatus::Unpublished);
        assert_eq!(unchanged.sequence_id, 0);
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(3_000_000)
        );
        assert_eq!(conn.current_delivery_cursor()?, before_failure);
        conn.raw_query(|conn| conn.batch_execute("DROP TRIGGER fail_publication_activity"))?;
        conn.set_delivery_status_to_published(&rejected.id, 900_000, Cursor(70), None)?;
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(1_200_000)
        );

        // Cached activity can come from imported metadata without a retained message.
        conn.raw_query(|conn| {
            diesel::update(groups_dsl::groups.find(group.id))
                .set(groups_dsl::last_message_ns.eq(4_000_000))
                .execute(conn)
        })?;
        conn.set_delivery_status_to_published(&skewed.id, 800_000, Cursor(50), None)?;
        assert_eq!(
            conn.find_group(&group.id)??.last_message_ns,
            Some(4_000_000)
        );
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
