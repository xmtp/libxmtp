//! Message deletion tests: rejected deletions.

use super::*;

// verifies: CTYPE-018, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn old_cached_text_cannot_be_deleted_locally() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    for (id, sender) in [
        (vec![0xC7, 0x01], alix.inbox_id()),
        (vec![0xC7, 0x02], "another-inbox"),
    ] {
        let target = cached_text_row(group.group_id, id, sender, "custom.example")?;
        target.store(&db)?;
        let loaded = db.get_group_message(&target.id)?.expect("stored target");
        assert_eq!(loaded.content_type, ContentType::Text);
        assert_eq!(loaded.authority_id, "xmtp.org");
        assert!(matches!(
            group.delete_message(target.id),
            Err(GroupError::DeleteMessage(
                DeleteMessageError::NonDeletableMessage
            ))
        ));
    }

    let standard = cached_text_row(
        group.group_id,
        vec![0xC7, 0x03],
        alix.inbox_id(),
        "xmtp.org",
    )?;
    standard.store(&db)?;
    assert!(!group.delete_message(standard.id)?.is_empty());
}

// verifies: CTYPE-018, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn received_deletion_rejects_old_cached_text_target() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    bo_group.sync().await?;
    let db = bo.context.db();

    for (id, sender, raw_authority) in [
        (vec![0x11; 32], alix.inbox_id(), "custom.example"),
        (vec![0x12; 32], bo.inbox_id(), "custom.example"),
        (vec![0x13; 32], alix.inbox_id(), "xmtp.org"),
    ] {
        cached_text_row(bo_group.group_id, id.clone(), sender, raw_authority)?.store(&db)?;
        let deletion = DeleteMessageCodec::encode(DeleteMessage {
            message_id: hex::encode(id),
        })?;
        alix_group
            .send_message(
                &xmtp_content_types::encoded_content_to_bytes(deletion),
                SendMessageOpts::default(),
            )
            .await?;
    }
    bo_group.sync().await?;
    // A wire type that may not be deleted keeps its content and no mark.
    for id in [vec![0x11; 32], vec![0x12; 32]] {
        assert_eq!(mark_of(&db, &id), None);
        assert!(
            !db.get_group_message(&id)?
                .unwrap()
                .decrypted_message_bytes
                .is_empty()
        );
    }
    assert_eq!(mark_of(&db, &[0x13; 32]).as_deref(), Some(alix.inbox_id()));
    assert!(
        db.get_group_message([0x13; 32])?
            .unwrap()
            .decrypted_message_bytes
            .is_empty()
    );
}

/// Test that non-authorized members cannot delete messages
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_authorization_failure() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Alix's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bola tries to delete Alix's message (should fail - not authorized)
    let result = bo_group.delete_message(message_id.clone());
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(DeleteMessageError::NotAuthorized))
    ));
}

/// Test that transcript messages cannot be deleted
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_transcript_messages() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Sync to get the membership change message
    alix_group.sync().await?;

    // Find the membership change message
    let messages = alix_group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::MembershipChange),
        ..Default::default()
    })?;
    assert!(!messages.is_empty());

    let membership_message_id = messages[0].id.clone();

    // Try to delete the membership change message (should fail)
    let result = alix_group.delete_message(membership_message_id);
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::NonDeletableMessage
        ))
    ));
}

/// Test deleting a message that doesn't exist
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_nonexistent_message() {
    let alix = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;

    // Try to delete a message that doesn't exist
    let fake_message_id = vec![1, 2, 3, 4, 5];
    let result = alix_group.delete_message(fake_message_id);
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageNotFound(_)
        ))
    ));
}

/// Test deleting an already deleted message
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_already_deleted_message() {
    let alix = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;

    // Send a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Delete the message
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // Try to delete again (should fail)
    let result = alix_group.delete_message(message_id);
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));
}

// verifies: PROC-056
#[xmtp_common::test(unwrap_try = true)]
async fn unauthorized_pending_delete_does_not_block_super_admin_delete() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_mls_core::context::CoreContext;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let caro_group = caro.wait_for_welcomes().await?.remove(0);
    bo_group.sync().await?;
    caro_group.sync().await?;

    let original = TextCodec::encode("message for moderation".into())?;
    let target_id = bo_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(original),
            SendMessageOpts::default(),
        )
        .await?;
    caro_group.sync().await?;
    assert!(
        alix.context.db().get_group_message(&target_id)?.is_none(),
        "the super admin has not synced the target"
    );

    // Caro is an ordinary member. Publish a real MLS application message with
    // the target ID, then deliver its decoded row before the target.
    let delete = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&target_id),
    })?;
    let malicious_delete_id = caro_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(delete),
            SendMessageOpts::default(),
        )
        .await?;
    let malicious_delete = caro
        .context
        .db()
        .get_group_message(&malicious_delete_id)?
        .expect("the attacker's deletion message is stored");
    xmtp_mls_core::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, buffer| {
            let storage = tx.storage();
            alix_group.store_external_application_message(
                &storage,
                &malicious_delete,
                false,
                buffer,
            )?;
            Ok::<_, crate::groups::mls_sync::GroupMessageProcessingError>(
                xmtp_db::TransactionOutcome::Continue(()),
            )
        },
    )?;
    // The record is kept, but it marks nothing.
    assert!(
        alix.context
            .db()
            .get_message_deletion(&malicious_delete_id)?
            .is_some()
    );

    // The ordered sync now stores the target and processes the invalid delete
    // again. It marks nothing, and the local delete gate does not refuse.
    alix_group.sync().await?;
    let visible = alix_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("the target message is visible");
    assert!(matches!(visible.content, MessageBody::Text(_)));
    assert_eq!(mark_of(&alix.context.db(), &target_id), None);
    let deletion_id = alix_group.delete_message(target_id.clone())?;
    assert!(!deletion_id.is_empty());
    alix_group.publish_messages().await?;
    let visible = alix_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("the target has a deletion placeholder");
    assert!(matches!(
        visible.content,
        MessageBody::DeletedMessage { .. }
    ));
}

// verifies: PROC-056
#[xmtp_common::test(unwrap_try = true)]
async fn cross_group_pending_delete_does_not_block_super_admin_delete() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_mls_core::context::CoreContext;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let target_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let side_group = alix
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    caro.sync_all_welcomes_and_groups(None).await?;
    let bo_target = bo.group(&target_group.group_id)?;
    let caro_target = caro.group(&target_group.group_id)?;
    let caro_side = caro.group(&side_group.group_id)?;

    let content = TextCodec::encode("message for moderation".into())?;
    let target_id = bo_target
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(content),
            SendMessageOpts::default(),
        )
        .await?;
    caro_target.sync().await?;
    assert!(alix.context.db().get_group_message(&target_id)?.is_none());

    let deletion = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&target_id),
    })?;
    let side_deletion_id = caro_side
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(deletion),
            SendMessageOpts::default(),
        )
        .await?;
    // Each topic remains ordered. The target topic simply is not synced yet.
    side_group.sync().await?;
    assert!(alix.context.db().get_group_message(&target_id)?.is_none());
    let pending = alix
        .context
        .db()
        .get_message_deletion(&side_deletion_id)?
        .expect("cross-group record stored before target");
    assert_eq!(pending.group_id, side_group.group_id);
    assert_eq!(pending.deleted_message_id, target_id);

    target_group.sync().await?;
    let visible = target_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("target remains visible in its true group");
    assert!(matches!(visible.content, MessageBody::Text(_)));
    let deletion_id = target_group.delete_message(target_id.clone())?;
    assert!(!deletion_id.is_empty());
    target_group.publish_messages().await?;
    let visible = target_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("the target has a deletion placeholder");
    assert!(matches!(
        visible.content,
        MessageBody::DeletedMessage { .. }
    ));
}

/// Test that cross-group deletion attempts are rejected
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_message_from_different_group() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    // Create two separate groups
    let group1 = alix.create_group(None, None)?;
    group1.add_members(&[bo.inbox_id()]).await?;

    let group2 = alix.create_group(None, None)?;
    group2.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message in group1
    let text_content = TextCodec::encode("Message in group 1".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let group1_message_id = group1
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs both groups
    let bo_groups = bo.wait_for_welcomes().await?;
    assert_eq!(bo_groups.len(), 2);
    bo_groups[0].sync().await?;
    bo_groups[1].sync().await?;

    // Attempt to delete group1's message from group2 (should fail)
    let result = group2.delete_message(group1_message_id.clone());
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(DeleteMessageError::NotAuthorized))
    ));

    // Verify the message in group1 is NOT deleted
    let alix_conn = alix.context.db();
    assert_eq!(mark_of(&alix_conn, &group1_message_id), None);

    // Verify we can still delete it from the correct group
    group1.delete_message(group1_message_id.clone())?;
    group1.publish_messages().await?;
    assert_eq!(
        mark_of(&alix_conn, &group1_message_id).as_deref(),
        Some(alix.inbox_id())
    );
}

/// Test that we cannot delete a delete message
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_delete_message() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix deletes the message
    let delete_message_id = alix_group.delete_message(original_message_id.clone())?;

    // Publish the deletion
    alix_group.publish_messages().await?;

    // Verify the original message is deleted
    let alix_conn = alix.context.db();
    assert_eq!(
        mark_of(&alix_conn, &original_message_id).as_deref(),
        Some(alix.inbox_id())
    );

    // Verify the delete message exists in the database
    let delete_msg = alix_conn.get_group_message(&delete_message_id)?;
    assert!(delete_msg.is_some());
    let delete_msg = delete_msg.unwrap();
    assert_eq!(delete_msg.content_type, ContentType::DeleteMessage);

    // Try to delete the delete message - should fail
    let result = alix_group.delete_message(delete_message_id.clone());
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::NonDeletableMessage
        ))
    ));

    // Verify the delete message is NOT deleted
    assert_eq!(mark_of(&alix_conn, &delete_message_id), None);
}
