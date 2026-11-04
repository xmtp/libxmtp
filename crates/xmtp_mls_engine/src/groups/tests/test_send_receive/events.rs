//! Send and receive tests: events.

use super::*;

// verifies: EVENT-001, EVENT-018, EVENT-019, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn group_membership_and_received_message_emit_once_after_storage() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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

// verifies: EVENT-019
#[xmtp_common::test(unwrap_try = true)]
async fn admin_and_permission_commits_name_the_changed_components() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let metadata = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(10),
    );

    group
        .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    assert!(matches!(
        metadata.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMetadataChanged(change)), ..
        }] if change.changed == ["ADMIN_LIST"]
    ));

    group
        .update_permission_policy(
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Deny,
            None,
        )
        .await?;
    assert!(matches!(
        metadata.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ConversationMetadataChanged(change)), ..
        }] if change.changed == ["COMPONENT_REGISTRY"]
    ));
}

// verifies: EVENT-001, EVENT-006, EVENT-012, EVENT-019
#[xmtp_common::test(unwrap_try = true)]
async fn removal_precedes_membership_change_for_removed_installation() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let bo2 = bo.new_installation().await;
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

    revoke(&bo, bo2.context.installation_id().as_slice()).await;
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

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let alix2 = alix.new_installation().await;
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

    let alix = EngineTestPeer::new().await;
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
    let alix = EngineTestPeer::new().await;
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

// verifies: EVENT-019
#[xmtp_common::test(unwrap_try = true)]
async fn application_component_commit_names_each_changed_item() {
    use crate::groups::{
        intents::AppDataUpdateIntentData, tests::test_dictionary_creation::definition,
    };
    use xmtp_mls_group_state::app_data::component_id::ComponentId;
    for (id, expected) in [
        (0x8000, "COMPONENT_REGISTRY"),
        (0x8001, "SUPER_ADMIN_LIST"),
        (0x8002, "ADMIN_LIST"),
        (0x8003, "GROUP_MEMBERSHIP"),
        (0x8004, "group_name"),
        (0x8005, "description"),
        (0x8006, "group_image_url_square"),
        (0x8007, "message_disappear_from_ns"),
        (0x8008, "message_disappear_in_ns"),
        (0x8009, "app_data"),
        (0x800a, "minimum_supported_protocol_version"),
        (0x800b, "_commit_log_signer"),
        (0x800c, "USER_DISPLAY_NAME"),
        (0x800d, "GROUP_IMAGE"),
        (0xbffb, "GROUP_ROOT_KEY"),
        (0xbffc, "ONESHOT_MESSAGE"),
        (0xbffd, "DM_MEMBERS"),
        (0xbffe, "CREATOR_INBOX_ID"),
        (0xbfff, "CONVERSATION_TYPE"),
        (0xc01a, "component:c01a"),
        (0x8abc, "component:8abc"),
    ] {
        assert_eq!(ComponentId::new(id).event_name(), expected);
    }
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentType, metadata_policy::MetadataBasePolicy,
    };
    let alix = EngineTestPeer::new_with(|o| {
        o.configure(|config| {
            config.application_components = vec![definition(
                0xc01a,
                ComponentType::Bytes,
                MetadataBasePolicy::Allow,
                true,
                false,
            )]
        })
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = receive_group_invite(&bo).await;
    let own = alix.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(16),
    );
    let remote = bo.context.events().subscribe(
        EventFilter::new([EventKind::ConversationMetadataChanged]),
        Some(16),
    );
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            0xc01a,
            b"value".to_vec(),
        )))
        .queue(&group)?;
    group.sync_until_intent_resolved(intent.id).await?;
    bo_group.receive().await?;
    for events in [&own, &remote] {
        assert!(
            matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(ClientEvent::ConversationMetadataChanged(change)), .. }] if change.group_id == group.group_id.as_slice() && change.changed == ["component:c01a"])
        );
    }
    group.sync().await?;
    bo_group.receive().await?;
    assert!(own.drain().is_empty());
    assert!(remote.drain().is_empty());
}
