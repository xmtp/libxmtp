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

/// `peer` joins `group_id` from its Welcome and holds `message` there.
async fn assert_peer_received(
    peer: &crate::utils::FullXmtpClient,
    group_id: &xmtp_proto::types::GroupId,
    message: &[u8],
) {
    let joined = peer.wait_for_welcomes().await.unwrap();
    assert!(joined.iter().any(|group| &group.group_id == group_id));
    let group = peer.group(group_id).unwrap();
    group.sync().await.unwrap();
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert!(
        messages
            .iter()
            .any(|stored| stored.decrypted_message_bytes == message)
    );
}

/// A DM whose creation stopped between storing the group and adding the peer
/// is completed when find-or-create returns it, so the peer joins it.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_creation_is_repaired_on_reuse() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;

    let reused = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    assert_eq!(reused.group_id, orphan.group_id);
    assert!(
        reused
            .members()
            .await?
            .iter()
            .any(|member| member.inbox_id == bo.inbox_id())
    );

    let message = b"sent after the reuse";
    reused
        .send_message(message, SendMessageOpts::default())
        .await?;
    assert_peer_received(&bo, &orphan.group_id, message).await;
}

/// A send on a DM whose creation stopped before the peer was added, reached
/// without find-or-create, adds the peer before it queues the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_creation_is_repaired_on_send() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;

    let message = b"sent on the interrupted DM";
    alix.group(&orphan.group_id)?
        .send_message(message, SendMessageOpts::default())
        .await?;
    assert_peer_received(&bo, &orphan.group_id, message).await;
}

/// Two repairs of one interrupted DM can run at once. Both succeed, and the
/// peer joins and reads the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_repairs_of_an_interrupted_dm_add_the_peer_once() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;

    let message = b"sent during a second repair";
    let (sent, found) = futures::join!(
        orphan.send_message(message, SendMessageOpts::default()),
        alix.find_or_create_dm(bo.inbox_id(), None),
    );
    sent?;
    assert_eq!(found?.group_id, orphan.group_id);
    assert_peer_received(&bo, &orphan.group_id, message).await;
}

/// When the peer of an interrupted DM cannot be added, find-or-create and
/// send return the add's failure instead of the DM or the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_whose_peer_cannot_be_added_fails() {
    use crate::groups::MlsGroup;

    tester!(alix, disable_workers);
    // An inbox with no identity log has no installation to add.
    let peer = hex::encode(xmtp_cryptography::rand::rand_array::<32>());
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, peer.clone())?;

    assert!(alix.find_or_create_dm(peer.as_str(), None).await.is_err());
    let message = b"never queued";
    let sent = alix
        .group(&orphan.group_id)?
        .send_message(message, SendMessageOpts::default())
        .await;
    assert!(sent.is_err());
    let stored = orphan.find_messages(&MsgQueryArgs::default())?;
    assert!(
        !stored
            .iter()
            .any(|stored| stored.decrypted_message_bytes == message)
    );
}

/// An installation its own inbox revoked cannot complete an interrupted DM:
/// the add resolves as a no-op, so find-or-create fails instead of returning
/// a DM without the peer.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn interrupted_dm_repair_that_adds_no_peer_fails() {
    use crate::groups::MlsGroup;
    use xmtp_id::associations::test_utils::add_wallet_signature;

    tester!(alix, disable_workers);
    tester!(alix2, from: alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;
    let mut revoke = alix2
        .identity_updates()
        .revoke_installations(vec![alix.installation_id.to_vec()])
        .await?;
    add_wallet_signature(&mut revoke, &alix2.builder.owner).await;
    alix2
        .identity_updates()
        .apply_signature_request(revoke)
        .await?;

    let result = alix.find_or_create_dm(bo.inbox_id(), None).await;
    assert!(result.is_err(), "returned a DM without its peer");
    assert!(bo.sync_welcomes().await?.is_empty());
    assert!(bo.db().find_group(&orphan.group_id)?.is_none());
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

/// Alice's DM with bob, plus `count` more groups for the same pair that bob
/// created and alice joined. Returns alice's DM and bob's sibling groups.
async fn dm_with_peer_created_siblings(
    alice: &crate::utils::FullXmtpClient,
    bob: &crate::utils::FullXmtpClient,
    count: usize,
) -> (crate::utils::TestMlsGroup, Vec<crate::utils::TestMlsGroup>) {
    let target = alice
        .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
        .await
        .unwrap();
    bob.sync_welcomes().await.unwrap();
    let mut siblings = Vec::with_capacity(count);
    for _ in 0..count {
        siblings.push(
            bob.create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
                .await
                .unwrap(),
        );
    }
    alice.sync_welcomes().await.unwrap();
    assert_eq!(alice.db().other_dms(&target.group_id).unwrap().len(), count);
    (target, siblings)
}

/// Sends one message in each group and returns `(group id, message id)` pairs.
async fn send_in_each(
    groups: &[crate::utils::TestMlsGroup],
) -> Vec<(xmtp_proto::types::GroupId, Vec<u8>)> {
    let mut sent = Vec::with_capacity(groups.len());
    for group in groups {
        let id = group
            .send_message(b"sibling message", SendMessageOpts::default())
            .await
            .unwrap();
        sent.push((group.group_id, id));
    }
    sent
}

/// A paused sibling of a stitched DM does not stop the DM's sync: the named
/// group and every other sibling sync, and the paused sibling stays held.
// verifies: DMS-019
#[xmtp_common::test(unwrap_try = true)]
async fn duplicate_dm_sync_reaches_target_past_failing_sibling() {
    use xmtp_db::group::QueryGroupVersion;

    tester!(alice, disable_workers);
    tester!(bob, disable_workers);
    let (target, siblings_for_bob) = dm_with_peer_created_siblings(&alice, &bob, 3).await;
    let paused = alice.db().other_dms(&target.group_id)?.first()?.id;
    alice.db().set_group_paused(&paused, "999.0.0")?;

    let target_message = bob
        .group(&target.group_id)?
        .send_message(b"target message", SendMessageOpts::default())
        .await?;
    let sibling_messages = send_in_each(&siblings_for_bob).await;

    target.sync().await?;
    let stored = |id: &[u8]| alice.db().get_group_message(id).map(|m| m.is_some());
    assert!(stored(&target_message)?);
    let mut held = None;
    for (group_id, message) in sibling_messages {
        assert_eq!(stored(&message)?, group_id != paused, "{group_id}");
        if group_id == paused {
            held = Some(message);
        }
    }

    // The paused sibling's envelope was held, not skipped: once the group
    // resumes, its sync processes it.
    alice.db().unpause_group(&paused)?;
    alice.group(&paused)?.sync().await?;
    assert!(stored(&held?)?);
}

/// When the named group of a stitched DM fails, its siblings still sync and
/// the call returns the named group's own error.
// verifies: DMS-019
#[xmtp_common::test(unwrap_try = true)]
async fn duplicate_dm_sync_reaches_siblings_past_failing_named_group() {
    use crate::groups::GroupError;
    use xmtp_db::group::{GroupMembershipState, QueryGroupVersion};

    tester!(alice, disable_workers);
    tester!(bob, disable_workers);
    let (target, siblings_for_bob) = dm_with_peer_created_siblings(&alice, &bob, 2).await;
    let assert_siblings_stored = |sent: Vec<(xmtp_proto::types::GroupId, Vec<u8>)>| {
        for (group_id, message) in sent {
            assert!(
                alice.db().get_group_message(&message).unwrap().is_some(),
                "{group_id}"
            );
        }
    };

    alice.db().set_group_paused(&target.group_id, "999.0.0")?;
    let sent = send_in_each(&siblings_for_bob).await;
    let error = target.sync().await.unwrap_err();
    assert!(
        matches!(
            &error,
            GroupError::Sync(summary)
                if matches!(summary.other.as_deref(), Some(GroupError::GroupPausedUntilUpdate(_)))
        ),
        "{error:?}"
    );
    assert_siblings_stored(sent);

    alice.db().unpause_group(&target.group_id)?;
    alice
        .db()
        .update_group_membership(target.group_id, GroupMembershipState::Restored)?;
    let sent = send_in_each(&siblings_for_bob).await;
    let error = target.sync().await.unwrap_err();
    assert!(matches!(error, GroupError::GroupInactive), "{error:?}");
    assert_siblings_stored(sent);
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
    // Sync groups would bypass the page's limit and cursor.
    for args in [
        GroupQueryArgs {
            include_sync_groups: true,
            ..Default::default()
        },
        GroupQueryArgs {
            conversation_type: Some(xmtp_proto::types::ConversationType::Sync),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            bo.list_conversations(args),
            Err(ClientError::InvalidListOptions(_))
        ));
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

/// A DM's consent is its peer's inbox record, so one decision covers every
/// group of the pair, through any group id or the inbox, and every reader
/// sees it: listing, the conversation list, push, and commit-log selection.
/// A group with the peer keeps its own record.
// verifies: CONS-052, CONS-060, CONS-059
#[xmtp_common::test(unwrap_try = true)]
async fn dm_block_covers_every_group_with_the_peer() {
    use xmtp_db::consent_record::{ConsentKey, ConsentState, ConsentType};
    use xmtp_db::group::GroupQueryArgs;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let peer = ConsentKey::inbox(bo.inbox_id());

    let g1 = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    let g2 = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    assert_ne!(g1.group_id, g2.group_id);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let commit_log_ids = || -> Result<Vec<_>, xmtp_db::ConnectionError> {
        Ok(alix
            .db()
            .get_conversation_ids_for_remote_log_publish()?
            .into_iter()
            .map(|row| row.id)
            .collect())
    };
    let selected = commit_log_ids()?;
    assert!(selected.contains(&g1.group_id) && selected.contains(&g2.group_id));
    // Both read allowed from creating, through the peer's record.
    assert_eq!(g1.consent_state()?, ConsentState::Allowed);
    assert_eq!(g2.consent_state()?, ConsentState::Allowed);
    assert!(
        alix.db()
            .consent_row(&ConsentKey::group(g1.group_id.as_ref()))?
            .is_none()
    );

    let generation = || {
        alix.db().raw_query(|c| {
            xmtp_db::diesel::sql_query(
                "SELECT generation AS value FROM group_set_generation WHERE id = 1",
            )
            .get_result::<Generation>(c)
        })
    };
    #[derive(xmtp_db::diesel::QueryableByName)]
    struct Generation {
        #[diesel(sql_type = xmtp_db::diesel::sql_types::BigInt)]
        value: i64,
    }
    let before = generation()?.value;
    // Deny through one group id; every group of the pair is denied.
    g2.update_consent_state(ConsentState::Denied)?;
    assert!(
        generation()?.value > before,
        "inbox consent moves the group set"
    );
    for group in [&g1, &g2] {
        assert_eq!(group.consent_state()?, ConsentState::Denied);
    }
    assert_eq!(
        alix.get_consent_state(ConsentType::InboxId, bo.inbox_id().to_string())
            .await?,
        ConsentState::Denied
    );

    // The default listing (Allowed + Unknown) excludes every group of the
    // pair, and keeps a group with the peer, which has its own record.
    let listed = alix.find_groups(GroupQueryArgs::default())?;
    assert!(listed.iter().all(|g| g.dm_id.is_none()));
    assert!(listed.iter().any(|g| g.group_id == group.group_id));
    let all = alix.find_groups(GroupQueryArgs {
        consent_states: Some(vec![ConsentState::Denied]),
        ..Default::default()
    })?;
    assert!(
        all.iter().any(|g| g.group_id == g1.group_id)
            || all.iter().any(|g| g.group_id == g2.group_id)
    );
    let list = alix.list_conversations(GroupQueryArgs::default())?;
    assert!(list.iter().all(|c| c.group.dm_id.is_none()));
    // Push and commit-log publication read the same record.
    assert!(!g1.notifications_enabled()?);
    let selected = commit_log_ids()?;
    assert!(!selected.contains(&g1.group_id) && !selected.contains(&g2.group_id));

    // A group id the client does not hold is its own key, which holds nothing.
    let g3 = xmtp_proto::types::GroupId::from([0x5b; 32]);
    assert_eq!(
        xmtp_db::consent_record::conversation_consent(&alix.db(), alix.inbox_id(), &g3)?,
        ConsentState::Unknown
    );
    // Allow through the inbox directly, then both DMs are allowed again.
    alix.set_consent_states(&[crate::client::ConsentAct {
        entity_type: ConsentType::InboxId,
        entity: bo.inbox_id().to_string(),
        state: ConsentState::Allowed,
    }])
    .await?;
    for group in [&g1, &g2] {
        assert_eq!(group.consent_state()?, ConsentState::Allowed);
    }
    assert_eq!(
        alix.db().consent_row(&peer)?.unwrap().state,
        ConsentState::Allowed
    );
}

/// An act naming a conversation the client does not hold fails and stores
/// nothing, also for the other acts of its call. An act on an inbox the
/// client has never seen is stored and covers a DM that arrives later. A read
/// of the unheld id still answers unknown.
// verifies: CONS-052
#[xmtp_common::test(unwrap_try = true)]
async fn act_on_unheld_conversation_is_refused() {
    use crate::client::ConsentAct;
    use xmtp_db::consent_record::{ConsentActError, ConsentKey, ConsentState, ConsentType};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let unheld = hex::encode([0x6c; 32]);
    let refused = ConsentAct::new(
        ConsentType::ConversationId,
        ConsentState::Denied,
        unheld.clone(),
    );
    let inbox = ConsentAct::new(
        ConsentType::InboxId,
        ConsentState::Denied,
        bo.inbox_id().to_string(),
    );
    let before = alix.db().consent_records()?.len();
    for acts in [vec![refused.clone()], vec![inbox.clone(), refused]] {
        let result = alix.set_consent_states(&acts).await;
        assert!(
            matches!(
                result,
                Err(crate::client::ClientError::ConsentAct(ConsentActError::ConversationNotHeld(ref id))) if *id == unheld
            ),
            "{result:?}"
        );
        assert_eq!(
            alix.db().consent_records()?.len(),
            before,
            "nothing is stored"
        );
    }
    assert_eq!(
        alix.get_consent_state(ConsentType::ConversationId, unheld)
            .await?,
        ConsentState::Unknown
    );
    // An inbox the client has never seen can be set, and a DM that arrives
    // later reads it.
    alix.set_consent_states(&[inbox]).await?;
    assert!(
        alix.db()
            .consent_row(&ConsentKey::inbox(bo.inbox_id()))?
            .is_some()
    );
    let dm = bo.find_or_create_dm(alix.inbox_id(), None).await?;
    dm.update_installations().await?;
    alix.sync_welcomes().await?;
    assert_eq!(
        alix.group(&dm.group_id)?.consent_state()?,
        ConsentState::Denied
    );
}
