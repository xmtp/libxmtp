//! Sending, receiving, and commit basics.

use super::*;
use xmtp_events::{ClientEvent, EventFilter, EventKind, MessageStatus};

// verifies: EVENT-001, EVENT-018, EVENT-019, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn group_membership_and_received_message_emit_once_after_storage() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    let membership = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMembershipChanged]),
        Some(10),
    );
    let metadata = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(10),
    );
    group.add_members(&[bo.inbox_id()]).await?;
    assert!(matches!(
        membership.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMembershipChanged(change)), ..
        }] if change.group_id == group.group_id.as_slice()
            && change.added_inbox_ids == [bo.inbox_id()]
            && change.removed_inbox_ids.is_empty()
    ));
    assert!(matches!(
        metadata.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMetadataChanged(change)), ..
        }] if change.group_id == group.group_id.as_slice()
            && change.changed == ["GROUP_MEMBERSHIP"]
    ));
    group.sync().await?;
    assert!(membership.drain().is_empty());
    assert!(metadata.drain().is_empty());

    let bo_group = receive_group_invite(&bo).await;
    let received = bo
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::MessageReceived]), Some(10));
    let mut typed_filter = EventFilter::new([EventKind::MessageReceived]);
    typed_filter.content_types = Some(vec![xmtp_events::ContentTypeId {
        authority_id: "xmtp.org".into(),
        type_id: "text".into(),
        version_major: 1,
    }]);
    let typed = bo.context.events().subscribe(typed_filter, Some(10));
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;
    bo_group.sync().await?;
    let stored = bo_group.find_messages(&MsgQueryArgs::default())?;
    let latest = stored.last().unwrap();
    assert!(matches!(
        received.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::MessageReceived(message)), ..
        }] if message.group_id == group.group_id.as_slice()
            && message.message_id == latest.id
            && message.sender_inbox_id == alix.inbox_id()
            && message.content_type.is_none()
    ));
    bo_group.sync().await?;
    assert!(received.drain().is_empty());
    assert!(typed.drain().is_empty());
}

// verifies: EVENT-001, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn own_message_emits_one_status_transition_when_published() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    let statuses = alix.context.events().subscribe(
        EventFilter::new([EventKind::MessageStatusChanged]),
        Some(10),
    );
    let remote_statuses = bo.context.events().subscribe(
        EventFilter::new([EventKind::MessageStatusChanged]),
        Some(10),
    );
    let id = group.prepare_message_for_later_publish(b"hello", true, None)?;
    assert!(statuses.drain().is_empty());
    group.publish_stored_message(&id).await?;
    group.sync().await?;
    bo_group.sync().await?;
    assert!(matches!(
        statuses.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::MessageStatusChanged(change)), ..
        }] if change.group_id == group.group_id.as_slice()
            && change.message_id == id
            && change.previous == MessageStatus::Unpublished
            && change.current == MessageStatus::Published
    ));
    group.sync().await?;
    assert!(statuses.drain().is_empty());
    assert!(remote_statuses.drain().is_empty());
}

// verifies: EVENT-001, EVENT-019, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn metadata_commit_emits_changed_field_once() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    let metadata = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(10),
    );
    let remote_metadata = bo.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(10),
    );
    group.update_group_name("New name".to_string()).await?;
    bo_group.sync().await?;
    assert!(matches!(
        metadata.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMetadataChanged(change)), ..
        }] if change.group_id == group.group_id.as_slice()
            && change.changed == ["group_name"]
    ));
    group.sync().await?;
    assert!(metadata.drain().is_empty());
    assert!(matches!(
        remote_metadata.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMetadataChanged(change)), ..
        }] if change.group_id == group.group_id.as_slice()
            && change.changed == ["group_name"]
    ));
    bo_group.sync().await?;
    assert!(remote_metadata.drain().is_empty());
}

// verifies: EVENT-001, EVENT-006, EVENT-012, EVENT-019
#[xmtp_common::test(unwrap_try = true)]
async fn removal_precedes_membership_change_for_removed_installation() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    let pending_id = bo_group.send_message_optimistic(b"pending", Default::default())?;
    let events = bo.context.events().subscribe(
        EventFilter::new([
            EventKind::ConversationRemoved,
            EventKind::ConversationMembershipChanged,
            EventKind::ConversationMetadataChanged,
            EventKind::MessageStatusChanged,
        ]),
        Some(10),
    );
    group.remove_members(&[bo.inbox_id()]).await?;
    let _ = bo_group.sync().await;
    let drained = events.drain();
    assert!(
        matches!(
            drained.as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ConversationRemoved(removed)), ..
            }, xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ConversationMembershipChanged(membership)), ..
            }, xmtp_events::EventEnvelope {
                client: Some(ClientEvent::ConversationMetadataChanged(metadata)), ..
            }, xmtp_events::EventEnvelope {
                client: Some(ClientEvent::MessageStatusChanged(status)), ..
            }] if removed.group_id == group.group_id.as_slice()
                && removed.cause == xmtp_events::RemovalCause::Removed
                && membership.removed_inbox_ids == [bo.inbox_id()]
                && metadata.changed == ["GROUP_MEMBERSHIP"]
                && status.message_id == pending_id
                && status.previous == MessageStatus::Unpublished
                && status.current == MessageStatus::Failed
        ),
        "{drained:?}"
    );
}

// verifies: EVENT-006, EVENT-019, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn revoked_installation_emits_removal_without_inbox_membership_change() {
    tester!(alix);
    tester!(bo);
    tester!(bo2, from: bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    let bo2_group = receive_group_invite(&bo2).await;
    let events = bo2.context.events().subscribe(
        EventFilter::new([
            EventKind::ConversationRemoved,
            EventKind::ConversationMembershipChanged,
            EventKind::ConversationMetadataChanged,
        ]),
        Some(10),
    );

    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![bo2.installation_id.to_vec()])
        .await?;
    xmtp_id::associations::test_utils::add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;
    group.add_members(&[] as &[&str]).await?;
    let _ = bo2_group.sync().await;
    bo_group.sync().await?;

    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationRemoved(removed)), ..
        }, xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMetadataChanged(metadata)), ..
        }] if removed.group_id == group.group_id.as_slice()
            && removed.cause == xmtp_events::RemovalCause::Removed
            && metadata.changed == ["GROUP_MEMBERSHIP"]
    ));
    assert!(!bo2_group.is_active()?);
    assert!(bo_group.is_active()?);
    assert_eq!(group.members().await?.len(), 2);
}

// verifies: EVENT-021
#[xmtp_common::test(unwrap_try = true)]
async fn received_reference_filter_selects_replies_and_reactions_to_own_messages() {
    use xmtp_content_types::{
        ContentCodec, encoded_content_to_bytes, reaction::ReactionCodec, reply::ReplyCodec,
        text::TextCodec,
    };
    use xmtp_proto::xmtp::mls::message_contents::content_types::{
        ReactionAction, ReactionSchema, ReactionV2,
    };

    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = receive_group_invite(&bo).await;
    let original = encoded_content_to_bytes(TextCodec::encode("original".into())?);
    let own_id = group
        .send_message(&original, SendMessageOpts::default())
        .await?;
    bo_group.sync().await?;

    let mut filter = EventFilter::new([EventKind::MessageReceived]);
    filter.references_own_messages = true;
    let selected = alix.context.events().subscribe(filter, Some(10));
    let reply = |target: &[u8]| -> Result<Vec<u8>, xmtp_content_types::CodecError> {
        Ok(encoded_content_to_bytes(ReplyCodec::encode(
            xmtp_content_types::reply::Reply {
                reference: hex::encode(target),
                reference_inbox_id: None,
                content: TextCodec::encode("reply".into())?,
            },
        )?))
    };
    let reply_id = bo_group
        .send_message(&reply(&own_id)?, SendMessageOpts::default())
        .await?;
    let reaction = encoded_content_to_bytes(ReactionCodec::encode(ReactionV2 {
        reference: hex::encode(&own_id),
        reference_inbox_id: alix.inbox_id().into(),
        action: ReactionAction::Added.into(),
        content: "👍".into(),
        schema: ReactionSchema::Unicode.into(),
    })?);
    let reaction_id = bo_group
        .send_message(&reaction, SendMessageOpts::default())
        .await?;
    let bo_text = encoded_content_to_bytes(TextCodec::encode("bo text".into())?);
    let bo_id = bo_group
        .send_message(&bo_text, SendMessageOpts::default())
        .await?;
    bo_group
        .send_message(&reply(&bo_id)?, SendMessageOpts::default())
        .await?;
    bo_group
        .send_message(&reply(&[0x77; 32])?, SendMessageOpts::default())
        .await?;

    group.sync().await?;
    let ids: Vec<_> = selected
        .drain()
        .into_iter()
        .filter_map(|event| match event.client {
            Some(ClientEvent::MessageReceived(message)) => Some(message.message_id),
            _ => None,
        })
        .collect();
    assert_eq!(ids, [reply_id, reaction_id]);
}

// verifies: EVENT-001, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn own_inbox_other_installation_message_is_received() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    tester!(alix2, from: alix);
    group.update_installations().await?;
    let other_group = alix2
        .sync_welcomes()
        .await?
        .into_iter()
        .find(|joined| joined.group_id == group.group_id)
        .expect("second installation joined group");
    let received = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::MessageReceived]), Some(10));
    let sent = other_group
        .send_message(b"from another installation", SendMessageOpts::default())
        .await?;
    group.sync().await?;
    assert!(matches!(
        received.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::MessageReceived(message)), ..
        }] if message.message_id == sent
            && message.sender_inbox_id == alix.inbox_id()
    ));
}

// verifies: EVENT-012
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_same_group_commits_emit_in_stored_order() {
    use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    tester!(alix);
    let group = alix.create_group(None, None)?;
    let metadata = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(10),
    );
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let name_group = group.clone();
    let name_barrier = barrier.clone();
    let name = xmtp_common::spawn(None, async move {
        name_barrier.wait().await;
        name_group
            .update_group_name("name from concurrent call".into())
            .await
    });
    let description_group = group.clone();
    let description = xmtp_common::spawn(None, async move {
        barrier.wait().await;
        description_group
            .update_group_description("description from concurrent call".into())
            .await
    });
    name.join().await??;
    description.join().await??;

    let emitted: Vec<_> = metadata
        .drain()
        .into_iter()
        .filter_map(|envelope| match envelope.client {
            Some(ClientEvent::ConversationMetadataChanged(change)) => Some(change.changed),
            _ => None,
        })
        .collect();
    let mut transcripts = group.find_messages(&MsgQueryArgs {
        content_types: Some(vec![xmtp_db::group_message::ContentType::GroupUpdated]),
        ..Default::default()
    })?;
    transcripts.sort_by_key(|message| message.sequence_id);
    let stored: Vec<_> = transcripts
        .into_iter()
        .filter_map(|message| {
            let encoded = EncodedContent::decode(message.decrypted_message_bytes.as_slice())
                .expect("stored group update decodes");
            let update = GroupUpdatedCodec::decode(encoded).expect("group update decodes");
            let fields: Vec<_> = update
                .metadata_field_changes
                .into_iter()
                .map(|field| field.field_name)
                .collect();
            (!fields.is_empty()).then_some(fields)
        })
        .collect();
    assert_eq!(emitted.len(), 2, "both commits must emit");
    assert_eq!(emitted, stored, "handoff order must match commit order");
}

// verifies: EVENT-001, EVENT-019
#[xmtp_common::test(unwrap_try = true)]
async fn no_op_metadata_commit_emits_no_change() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    group.update_group_name("same name".into()).await?;
    let metadata = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(10),
    );
    let before = alix
        .context
        .api()
        .query_group_messages(group.group_id)
        .await?
        .len();
    group.update_group_name("same name".into()).await?;
    let after = alix
        .context
        .api()
        .query_group_messages(group.group_id)
        .await?
        .len();
    assert!(after > before, "the no-op update must commit on the wire");
    assert!(metadata.drain().is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;
    let messages = alix
        .context
        .api()
        .query_group_messages(group.group_id)
        .await?;

    group.sync().await?;
    let decrypted_messages = group.find_messages(&MsgQueryArgs::default())?;

    tracing::info!("The messages: {decrypted_messages:?}");

    // The AppDataUpdate proposal, the key-update commit, and the application message.
    assert_eq!(messages.len(), 3);
    let stored = decrypted_messages.last().unwrap();
    let envelope = messages.last().unwrap();
    assert!(envelope.envelope_hash.is_some());
    assert!(envelope.expiry_ns.is_some());
    assert_eq!(stored.envelope_hash, envelope.envelope_hash);
    assert_eq!(
        stored.expiry_ns,
        envelope.expiry_ns.map(|expiry| expiry as i64)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receive_self_message() {
    tester!(alix);
    let group = alix.create_group(None, None).expect("create group");
    let msg = b"hello";

    group
        .send_message(msg, SendMessageOpts::default())
        .await
        .expect("send message");

    group.receive().await?;
    // Check for messages
    let messages = group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receive_message_from_other() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None).expect("create group");
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    let alix_message = b"hello from alix";
    alix_group
        .send_message(alix_message, SendMessageOpts::default())
        .await
        .expect("send message");

    let bo_group = receive_group_invite(&bo).await;
    let message = get_latest_message(&bo_group).await;
    assert_eq!(message.decrypted_message_bytes, alix_message);

    let bo_message = b"hello from bo";
    bo_group
        .send_message(bo_message, SendMessageOpts::default())
        .await
        .expect("send message");

    let message = get_latest_message(&alix_group).await;
    assert_eq!(message.decrypted_message_bytes, bo_message);
}

#[xmtp_common::test]
async fn test_key_update() {
    tester!(client);
    tester!(bola_client);

    let group = client.create_group(None, None).expect("create group");
    group.add_members(&[bola_client.inbox_id()]).await.unwrap();

    group.key_update().await.unwrap();

    let messages = client
        .context
        .api()
        .query_group_messages(group.group_id)
        .await
        .unwrap();
    // Adding Bola emits an Add proposal, a membership AppDataUpdate proposal, and a commit.
    // The explicit key update emits the fourth envelope.
    assert_eq!(messages.len(), 4);

    let pending_commit_is_none = group
        .load_mls_group_with_lock(client.context.mls_storage(), |mls_group| {
            Ok(mls_group.pending_commit().is_none())
        })
        .unwrap();

    assert!(pending_commit_is_none);

    group
        .send_message(b"hello", SendMessageOpts::default())
        .await
        .expect("send message");

    bola_client.sync_welcomes().await.unwrap();
    let bola_groups = bola_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bola_messages.len(), 2);
}

#[xmtp_common::test]
async fn test_post_commit() {
    tester!(client);
    tester!(client_2);
    let group = client.create_group(None, None).expect("create group");

    group.add_members(&[client_2.inbox_id()]).await.unwrap();

    // Check if the welcome was actually sent
    let welcome_messages = client
        .context
        .api()
        .query_welcome_messages(client_2.installation_public_key())
        .await
        .unwrap();

    assert_eq!(welcome_messages.len(), 1);
}

#[xmtp_common::test]
async fn test_staged_welcome() {
    // Create Clients
    tester!(amal);
    tester!(bola);

    // Amal creates a group
    let amal_group = amal.create_group(None, None).unwrap();

    // Amal adds Bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
    // and then store that value on the group and insert into the database
    let bola_groups = bola.sync_welcomes().await.unwrap();

    // Bola gets the group id. This will be needed to fetch the group from
    // the database.
    let bola_group = bola_groups.first().unwrap();
    let bola_group_id = bola_group.group_id;

    // Bola fetches group from the database
    let bola_fetched_group = bola.group(&bola_group_id).unwrap();

    // Check Bola's group for the added_by_inbox_id of the inviter
    let added_by_inbox = bola_fetched_group.added_by_inbox_id().unwrap();

    // Verify the welcome host_credential is equal to Amal's
    assert_eq!(
        amal.inbox_id(),
        added_by_inbox,
        "The Inviter and added_by_address do not match!"
    );
}

#[xmtp_common::test]
async fn test_can_read_group_creator_inbox_id() {
    tester!(amal);
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(mutable_metadata.super_admin_list.len(), 1);
    assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

    let protected_metadata: GroupMetadata = amal_group.metadata().await.unwrap();
    assert_eq!(
        protected_metadata.conversation_type,
        ConversationType::Group
    );

    assert_eq!(protected_metadata.creator_inbox_id, amal.inbox_id());
}

#[xmtp_common::test]
async fn test_optimistic_send() {
    tester!(amal);
    tester!(bola);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.sync().await.unwrap();
    // Add bola to the group
    amal_group
        .add_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();
    let bola_group = receive_group_invite(&bola).await;

    let ids = vec![
        amal_group
            .send_message_optimistic(b"test one", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test two", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test three", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test four", SendMessageOpts::default())
            .unwrap(),
    ];

    let messages = amal_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .collect::<Vec<StoredGroupMessage>>();

    let text = messages
        .iter()
        .map(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).to_string())
        .collect::<Vec<String>>();
    assert_eq!(
        ids,
        messages
            .iter()
            .cloned()
            .map(|m| m.id)
            .collect::<Vec<Vec<u8>>>()
    );
    assert_eq!(
        text,
        vec![
            "test one".to_string(),
            "test two".to_string(),
            "test three".to_string(),
            "test four".to_string(),
        ]
    );

    let delivery = messages
        .iter()
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
        ]
    );

    amal_group.publish_messages().await.unwrap();
    bola_group.sync().await.unwrap();

    let messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let delivery = messages
        .iter()
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
        ]
    );
}
