use super::*;

// verifies: CONS-048
#[xmtp_common::test(unwrap_try = true)]
async fn expired_messages_do_not_set_listing_or_activity_time() {
    use crate::test::mock::generate_stored_msg;
    use xmtp_db::Store;
    use xmtp_proto::types::Cursor;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let expired_only = alix.create_group(None, None)?;
    let db = alix.context.db();
    let now = now_ns();
    for (cursor, group_id, sent_at_ns, expire_at_ns) in [
        (100, group.group_id, 200, now + xmtp_common::NS_IN_DAY),
        (200, group.group_id, 300, now - 1),
        (300, expired_only.group_id, 400, now - 1),
    ] {
        let mut message = generate_stored_msg(Cursor(cursor), group_id);
        message.sent_at_ns = sent_at_ns;
        message.expire_at_ns = Some(expire_at_ns);
        message.store(&db)?;
    }

    let listed = alix.list_conversations(GroupQueryArgs::default())?;
    let listing_time = |group_id| {
        listed
            .iter()
            .find(|item| item.group.group_id == group_id)
            .map(|item| item.listing_time_ns)
            .expect("group remains listed")
    };
    assert_eq!(group.last_activity_ns(None)?, 200);
    assert_eq!(listing_time(group.group_id), 200);
    assert_eq!(
        expired_only.last_activity_ns(None)?,
        expired_only.created_at_ns
    );
    assert_eq!(
        listing_time(expired_only.group_id),
        expired_only.created_at_ns
    );
}

// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn expired_message_is_absent_from_history_and_direct_lookup() {
    use crate::test::mock::generate_stored_msg;
    use xmtp_db::Store;
    use xmtp_proto::types::Cursor;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    let make_message = |cursor,
                        expiry|
     -> Result<
        xmtp_db::group_message::StoredGroupMessage,
        xmtp_content_types::CodecError,
    > {
        let mut message = generate_stored_msg(Cursor(cursor), group.group_id);
        message.decrypted_message_bytes =
            TextCodec::encode(format!("message {cursor}"))?.encode_to_vec();
        message.expire_at_ns = expiry;
        Ok(message)
    };
    let expired = make_message(100, Some(now_ns() - 1))?;
    let future = make_message(200, Some(i64::MAX))?;
    let persistent = make_message(300, None)?;
    for message in [&expired, &future, &persistent] {
        message.store(&db)?;
    }

    assert!(db.get_group_message(&expired.id)?.is_some());
    let history = group.find_messages(&MsgQueryArgs::default())?;
    assert!(!history.iter().any(|message| message.id == expired.id));
    assert!(history.iter().any(|message| message.id == future.id));
    assert!(history.iter().any(|message| message.id == persistent.id));
    assert!(matches!(
        alix.message(expired.id.clone()),
        Err(crate::client::ClientError::Storage(
            xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(_))
        ))
    ));
    assert!(alix.message_with_group(&expired.id).await?.is_none());
    assert!(matches!(
        alix.message_v2(expired.id.clone()),
        Err(crate::client::ClientError::Storage(
            xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(_))
        ))
    ));
    for id in [&future.id, &persistent.id] {
        assert_eq!(alix.message(id.clone())?.id, *id);
        assert_eq!(alix.message_v2(id.clone())?.metadata.id, *id);
        assert!(alix.message_with_group(id).await?.is_some());
    }
}

// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn expired_reply_parent_is_not_enriched() {
    use crate::messages::decoded_message::MessageBody;
    use crate::test::mock::generate_stored_msg;
    use xmtp_content_types::reply::{Reply as EncodedReply, ReplyCodec};
    use xmtp_db::{Store, group_message::ContentType};
    use xmtp_proto::types::Cursor;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    let mut parent = generate_stored_msg(Cursor(100), group.group_id);
    parent.decrypted_message_bytes = TextCodec::encode("parent".into())?.encode_to_vec();
    parent.expire_at_ns = Some(now_ns() - 1);
    parent.store(&db)?;
    let mut reply = generate_stored_msg(Cursor(200), group.group_id);
    reply.decrypted_message_bytes = ReplyCodec::encode(EncodedReply {
        reference: hex::encode(&parent.id),
        reference_inbox_id: None,
        content: TextCodec::encode("reply".into())?,
    })?
    .encode_to_vec();
    reply.content_type = ContentType::Reply;
    reply.reference_id = Some(parent.id.clone());
    reply.store(&db)?;

    assert!(db.get_group_message(&parent.id)?.is_some());
    let enriched = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
    let actual = enriched
        .iter()
        .find(|item| item.stored.id == reply.id)
        .expect("visible reply");
    let MessageBody::Reply(body) = &actual.decoded.content else {
        panic!("expected reply")
    };
    assert!(body.in_reply_to.is_none());
    assert!(actual.parent_stored.is_none());

    let mut visible_parent = parent.clone();
    visible_parent.id = xmtp_common::rand_vec::<32>();
    visible_parent.sequence_id = 300;
    visible_parent.expire_at_ns = Some(i64::MAX);
    visible_parent.store(&db)?;
    let mut visible_reply = reply.clone();
    visible_reply.id = xmtp_common::rand_vec::<32>();
    visible_reply.sequence_id = 400;
    visible_reply.reference_id = Some(visible_parent.id.clone());
    visible_reply.decrypted_message_bytes = ReplyCodec::encode(EncodedReply {
        reference: hex::encode(&visible_parent.id),
        reference_inbox_id: None,
        content: TextCodec::encode("visible reply".into())?,
    })?
    .encode_to_vec();
    visible_reply.store(&db)?;
    let enriched = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
    let actual = enriched
        .iter()
        .find(|item| item.stored.id == visible_reply.id)
        .expect("visible reply with visible parent");
    let MessageBody::Reply(body) = &actual.decoded.content else {
        panic!("expected reply")
    };
    assert!(body.in_reply_to.is_some());
    assert_eq!(
        actual.parent_stored.as_ref().map(|item| &item.id),
        Some(&visible_parent.id)
    );
}

#[xmtp_common::test]
async fn test_group_member_recovery() {
    tester!(amal);
    tester!(bola_a);
    tester!(bola_b, from: bola_a);

    let group = amal.create_group(None, None).unwrap();

    // Add both of Bola's installations to the group
    group
        .add_members(&[bola_a.inbox_id(), bola_b.inbox_id()])
        .await
        .unwrap();

    let conn = amal.context.store().conn();
    conn.raw_query(|conn| diesel::delete(identity_updates::table).execute(conn))
        .unwrap();

    let members = group.members().await.unwrap();
    // The three installations should count as two members
    assert_eq!(members.len(), 2);
}

#[xmtp_common::test]
async fn test_find_groups() {
    tester!(client);
    let group_1 = client.create_group(None, None).unwrap();
    let group_2 = client.create_group(None, None).unwrap();

    let groups = client.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(groups.len(), 2);
    assert!(groups.iter().any(|g| g.group_id == group_1.group_id));
    assert!(groups.iter().any(|g| g.group_id == group_2.group_id));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_double_dms() {
    tester!(alice);
    tester!(bob);

    let alice_dm = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    alice_dm
        .send_message(b"Welcome 1", SendMessageOpts::default())
        .await?;

    let bob_dm = bob
        .create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
        .await?;

    tester!(alice2, from: alice);
    let alice_dm2 = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    alice_dm2
        .send_message(b"Welcome 2", SendMessageOpts::default())
        .await?;

    alice_dm.update_installations().await?;
    alice.sync_welcomes().await?;
    bob.sync_welcomes().await?;

    alice_dm
        .send_message(b"Welcome from 1", SendMessageOpts::default())
        .await?;

    // This message will set bob's dm as the primary DM for all clients
    bob_dm
        .send_message(b"Bob says hi 1", SendMessageOpts::default())
        .await?;
    // Alice will sync, pulling in Bob's DM message, which will cause
    // a database trigger to update `last_message_ns`, putting bob's DM to the top.
    alice_dm.sync().await?;

    alice2.sync_welcomes().await?;
    let mut groups = alice2.find_groups(GroupQueryArgs::default())?;

    assert_eq!(groups.len(), 1);
    let group = groups.pop()?;

    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs::default())?;

    assert_eq!(messages.len(), 6);

    // Reload alice's DM. This will load the DM that Bob just created and sent a message on.
    let new_alice_dm = alice.stitched_group(&alice_dm.group_id)?;

    // The group_id should not be what we asked for because it was stitched
    assert_ne!(alice_dm.group_id, new_alice_dm.group_id);
    // They should be the same, due the the message that Bob sent above.
    assert_eq!(new_alice_dm.group_id, bob_dm.group_id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn message_with_group_resolves_stitched_dm() {
    tester!(alice, disable_workers);
    tester!(bob, disable_workers);

    let alice_dm = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await?;
    let old_message_id = alice_dm
        .send_message(b"from alice", SendMessageOpts::default())
        .await?;
    let bob_dm = bob
        .create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
        .await?;
    alice.sync_welcomes().await?;
    bob_dm
        .send_message(b"from bob", SendMessageOpts::default())
        .await?;
    alice_dm.sync().await?;

    let (message, group) = alice.message_with_group(&old_message_id).await?.unwrap();
    assert_eq!(message.id, old_message_id);
    assert_eq!(message.group_id, alice_dm.group_id);
    assert_ne!(group.group_id, message.group_id);
    assert_eq!(group.group_id, bob_dm.group_id);
    assert!(
        alice
            .message_with_group(b"unknown message")
            .await?
            .is_none()
    );
}

#[rstest::rstest]
#[xmtp_common::test]
async fn test_add_remove_then_add_again() {
    let amal = Tester::new().await;
    let bola = Tester::new().await;

    // Create a group and invite bola
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 2);

    // Now remove bola
    amal_group.remove_members(&[bola.inbox_id()]).await.unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 1);

    // See if Bola can see that they were added to the group
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(Default::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    assert!(!bola_group.is_active().unwrap());

    // Bola should have one readable message (them being added to the group)
    let mut bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();

    assert_eq!(bola_messages.len(), 2);

    // Add Bola back to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();

    // Send a message from Amal, now that Bola is back in the group
    amal_group
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync Bola's state to get the latest
    if let Err(err) = bola_group.sync().await {
        panic!("Error syncing group: {:?}", err);
    }
    // Find Bola's updated list of messages
    bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    // Bola should have been able to decrypt the last message
    assert_eq!(bola_messages.len(), 4);
    assert_eq!(
        bola_messages.get(3).unwrap().decrypted_message_bytes,
        vec![1, 2, 3]
    )
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn test_list_conversations_pagination() {
    use prost::Message;
    use xmtp_mls_common::group::GroupMetadataOptions;

    let alix = Tester::builder().build().await;
    let bo = Tester::builder().build().await;

    // Create 15 groups with small delays to ensure different created_at_ns values
    let mut all_group_ids = Vec::new();
    for i in 0..15 {
        let group = alix
            .create_group_with_members(
                &[bo.inbox_id().to_string()],
                None,
                Some(GroupMetadataOptions {
                    name: Some(format!("Group {}", i + 1)),
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
        all_group_ids.push(group.group_id);
        group
            .send_message(
                TextCodec::encode("hello".to_string())
                    .unwrap()
                    .encode_to_vec()
                    .as_slice(),
                SendMessageOpts::default(),
            )
            .await
            .unwrap();
        // Small delay to ensure different timestamps
        xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let mut before = None;
    let mut all_conversation_ids = Vec::new();
    loop {
        let results = alix
            .list_conversations(GroupQueryArgs {
                limit: Some(5),
                last_activity_before_ns: before.map(|(time, _)| time),
                before_id: before.map(|(_, id)| id),
                ..Default::default()
            })
            .unwrap();

        if results.is_empty() {
            break;
        }
        assert_eq!(results.len(), 5);

        all_conversation_ids.extend(results.iter().map(|item| item.group.group_id));

        let last = results.last().unwrap();
        before = Some((last.listing_time_ns, last.group.group_id));
    }

    assert_eq!(
        all_conversation_ids.len(),
        15,
        "Should have 15 total conversations"
    );
    all_conversation_ids.dedup();

    // Check that we got all 15 unique groups
    assert_eq!(
        all_conversation_ids.len(),
        15,
        "Should have 15 total conversations after deduping"
    );
}

/// Store a conversation row with no MLS state; a listing reads only rows.
fn store_listed_group(
    db: &impl xmtp_db::ConnectionExt,
    id: xmtp_proto::types::GroupId,
    created_at_ns: i64,
) -> Result<(), xmtp_db::StorageError> {
    use xmtp_db::{Store, group::GroupMembershipState, group::StoredGroup};
    StoredGroup::builder()
        .id(id)
        .created_at_ns(created_at_ns)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("peer")
        .build()
        .expect("group row")
        .store(db)
}

/// A peer can fill the default listing with Unknown-consent groups. A page
/// holds at most the app's limit, or 100, and carries each group's listing
/// time instead of its peer-sized latest message.
// verifies: CONS-046
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_is_bounded_and_carries_listing_time() {
    use crate::client::ClientError;
    use xmtp_common::Generate;
    use xmtp_db::group_message::GroupMessageKind;
    use xmtp_proto::types::GroupId;

    const GROUP_COUNT: usize = 8;
    const TEXT_BYTES: usize = 128 * 1024;
    const SEEDED_ROWS: usize = 120;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    let message = TextCodec::encode("x".repeat(TEXT_BYTES))?.encode_to_vec();
    for _ in 0..GROUP_COUNT {
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        group
            .send_message(&message, SendMessageOpts::default())
            .await?;
    }

    bo.sync_all_welcomes_and_groups(None).await?;
    let db = bo.context.db();
    for consent_states in [None, Some(vec![ConsentState::Unknown])] {
        let listed = bo.list_conversations(GroupQueryArgs {
            consent_states,
            ..Default::default()
        })?;
        assert_eq!(listed.len(), GROUP_COUNT);
        for item in &listed {
            let sent_at_ns = db
                .get_group_messages(
                    &item.group.group_id,
                    &MsgQueryArgs {
                        kind: Some(GroupMessageKind::Application),
                        ..Default::default()
                    },
                )?
                .iter()
                .map(|message| message.sent_at_ns)
                .max();
            assert_eq!(Some(item.listing_time_ns), sent_at_ns);
        }
    }

    for limit in [0, 1001] {
        let result = bo.list_conversations(GroupQueryArgs {
            limit: Some(limit),
            ..Default::default()
        });
        assert!(
            matches!(result, Err(ClientError::InvalidListOptions(_))),
            "limit {limit} was accepted"
        );
    }

    for _ in GROUP_COUNT..SEEDED_ROWS {
        store_listed_group(&db, GroupId::generate(), now_ns())?;
    }
    assert_eq!(bo.list_conversations(GroupQueryArgs::default())?.len(), 100);
    assert_eq!(
        bo.list_conversations(GroupQueryArgs {
            limit: Some(1000),
            ..Default::default()
        })?
        .len(),
        SEEDED_ROWS
    );
}

/// Pages resume after the last row of the previous page, even when many
/// groups share its listing time, so every group is returned once in each
/// order. Each page is the next slice of the full descending
/// (listing time, group id) order.
// verifies: CONS-047, CONS-049
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_pages_resume_after_ties() {
    use crate::client::ClientError;
    use crate::test::mock::generate_stored_msg;
    use xmtp_db::{Store, group::GroupQueryOrderBy};
    use xmtp_proto::types::{Cursor, GroupId};

    const GROUPS: u8 = 120;
    const PAGE: i64 = 50;
    // Group `index` has id [index; 16]. Times fall as the index grows, and a
    // range of groups shares the time of its first group. Creation-time ties
    // fill rows 31 to 70 and activity-time ties rows 41 to 80, so the first
    // page ends inside a tie block in both orders.
    const CREATED_TIES: std::ops::Range<u8> = 30..70;
    const ACTIVITY_TIES: std::ops::Range<u8> = 40..80;
    // Includes group 70, the last row of the first activity page.
    const EXPIRED_NEWER: std::ops::Range<u8> = 65..75;
    let time = |base: i64, ties: &std::ops::Range<u8>, index: u8| {
        let rank = if ties.contains(&index) {
            ties.start
        } else {
            index
        };
        base - i64::from(rank) * 1_000
    };

    tester!(alix, disable_workers);
    let db = alix.context.db();
    let now = now_ns();
    for index in 0..GROUPS {
        let id = GroupId::from([index; 16]);
        store_listed_group(&db, id, time(2_000_000, &CREATED_TIES, index))?;
        let mut live = generate_stored_msg(Cursor(u64::from(index) + 1), id);
        live.sent_at_ns = time(9_000_000, &ACTIVITY_TIES, index);
        live.store(&db)?;
        if EXPIRED_NEWER.contains(&index) {
            let mut expired = generate_stored_msg(Cursor(u64::from(index) + 1_000), id);
            expired.sent_at_ns = 10_000_000;
            expired.expire_at_ns = Some(now - 1);
            expired.store(&db)?;
        }
    }

    for order_by in [
        GroupQueryOrderBy::CreatedAt,
        GroupQueryOrderBy::LastActivity,
    ] {
        let list = |limit, before: Option<(i64, GroupId)>| {
            let time = before.map(|(time, _)| time);
            let (created_before_ns, last_activity_before_ns) = match order_by {
                GroupQueryOrderBy::CreatedAt => (time, None),
                GroupQueryOrderBy::LastActivity => (None, time),
            };
            alix.list_conversations(GroupQueryArgs {
                limit: Some(limit),
                created_before_ns,
                last_activity_before_ns,
                before_id: before.map(|(_, id)| id),
                order_by: Some(order_by.clone()),
                ..Default::default()
            })
        };
        let full = list(1000, None)?;
        assert_eq!(full.len(), usize::from(GROUPS));
        let keys: Vec<_> = full
            .iter()
            .map(|item| (item.listing_time_ns, item.group.group_id))
            .collect();
        assert!(
            keys.windows(2).all(|pair| pair[0] > pair[1]),
            "{order_by:?} rows are not in descending (listing time, id) order"
        );

        let mut paged = Vec::new();
        let mut before = None;
        loop {
            let page = list(PAGE, before)?;
            let Some(last) = page.last() else { break };
            // The app's cursor is the creation time or the last-activity time.
            let time = match order_by {
                GroupQueryOrderBy::CreatedAt => last.group.created_at_ns,
                GroupQueryOrderBy::LastActivity => last.group.last_activity_ns(None)?,
            };
            before = Some((time, last.group.group_id));
            paged.extend(page.iter().map(|item| item.group.group_id));
            assert!(
                paged.len() <= usize::from(GROUPS),
                "{order_by:?} pages repeat groups"
            );
        }
        let full_ids: Vec<_> = full.iter().map(|item| item.group.group_id).collect();
        assert_eq!(paged, full_ids, "{order_by:?} pages skip or repeat a group");

        let result = alix.list_conversations(GroupQueryArgs {
            before_id: Some(full_ids[0]),
            order_by: Some(order_by.clone()),
            ..Default::default()
        });
        assert!(
            matches!(result, Err(ClientError::InvalidListOptions(_))),
            "{order_by:?} accepted before_id without a before time"
        );
    }
}

#[xmtp_common::test]
async fn test_delete_message() {
    tester!(alix);
    tester!(bo);

    // Create a group with both users
    let group = alix
        .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
        .await
        .unwrap();

    // Send a message
    let message_id = group
        .send_message(
            TextCodec::encode("test message".to_string())
                .unwrap()
                .encode_to_vec()
                .as_slice(),
            SendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Verify the message exists
    let message = alix.message(message_id.clone()).unwrap();
    assert_eq!(message.id, message_id);

    // Delete the message
    let deleted_count = alix.delete_message(message_id.clone()).unwrap();
    assert_eq!(deleted_count, 1, "Should delete exactly 1 message");

    // Verify the message no longer exists
    let result = alix.message(message_id.clone());
    assert!(result.is_err(), "Message should not exist after deletion");

    // Test idempotency - deleting again should not error and return 0
    let deleted_count = alix.delete_message(message_id).unwrap();
    assert_eq!(
        deleted_count, 0,
        "Deleting non-existent message should return 0"
    );
}

/// A new DM inherits the newest decision about its pair with that decision's
/// time, so a block made in a later DM stays a block in the next one. A block
/// stored at an older inherited time would lose to the older allow.
// verifies: DMS-010
#[xmtp_common::test(unwrap_try = true)]
async fn newer_dm_inherits_newest_decision_with_its_time() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let record = |group_id| {
        bo.db()
            .get_consent_record(hex::encode(group_id), ConsentType::ConversationId)
    };

    let first = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_welcomes().await?;
    let bo_first = bo.group(&first.group_id)?;
    bo_first.update_consent_state(ConsentState::Allowed)?;

    let second = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_welcomes().await?;
    let bo_second = bo.group(&second.group_id)?;
    assert_eq!(bo_second.consent_state()?, ConsentState::Allowed);
    bo_second.update_consent_state(ConsentState::Denied)?;

    let third = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_welcomes().await?;
    let oldest_decision = record(first.group_id)?.expect("older Allowed decision");
    let newest_decision = record(second.group_id)?.expect("newer Denied decision");
    let inherited = record(third.group_id)?.expect("inherited consent");
    assert_eq!(oldest_decision.state, ConsentState::Allowed);
    assert_eq!(newest_decision.state, ConsentState::Denied);
    assert!(oldest_decision.consented_at_ns < newest_decision.consented_at_ns);
    assert_eq!(
        (inherited.state, inherited.consented_at_ns),
        (ConsentState::Denied, newest_decision.consented_at_ns)
    );
}

/// A DM this inbox created, joined from a Welcome another installation of
/// this inbox sent, inherits the pair's newest decision instead of the
/// allowed default for this inbox's own conversations.
// verifies: CONS-024
#[xmtp_common::test(unwrap_try = true)]
async fn own_inbox_dm_join_keeps_inherited_denial() {
    tester!(alix, disable_workers);
    tester!(bo1, disable_workers);
    tester!(bo2, from: bo1, disable_workers);
    let record = |group_id| {
        bo2.db()
            .get_consent_record(hex::encode(group_id), ConsentType::ConversationId)
    };

    let first = alix
        .create_dm_by_inbox_id(bo1.inbox_id().to_string(), None)
        .await?;
    bo2.sync_welcomes().await?;
    bo2.group(&first.group_id)?
        .update_consent_state(ConsentState::Denied)?;
    let denial = record(first.group_id)?.expect("the denial");

    let second = bo1
        .create_dm_by_inbox_id(alix.inbox_id().to_string(), None)
        .await?;
    second.update_installations().await?;
    bo2.sync_welcomes().await?;
    let bo2_second = bo2.group(&second.group_id)?;
    assert_eq!(bo2_second.added_by_inbox_id()?, bo1.inbox_id());
    let inherited = record(second.group_id)?.expect("inherited consent");
    assert_eq!(
        (inherited.state, inherited.consented_at_ns),
        (ConsentState::Denied, denial.consented_at_ns)
    );
}
