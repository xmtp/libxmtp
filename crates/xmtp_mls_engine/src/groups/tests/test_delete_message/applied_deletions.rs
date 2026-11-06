//! Message deletion tests: applied deletions.

use super::*;

/// Test basic message deletion by the original sender
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_by_sender() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Hello, world!".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group to receive the message
    let bo_groups = bo.wait_for_welcomes().await?;
    assert_eq!(bo_groups.len(), 1);
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify the message exists for both
    let alix_messages = alix_group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix_messages.len(), 2); // 1 text message + 1 membership change

    let bo_messages = bo_group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(bo_messages.len(), 2);

    // Alix deletes the message
    let deletion_id = alix_group.delete_message(message_id.clone())?;
    assert!(!deletion_id.is_empty());

    // Publish and sync
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the message is marked as deleted in the database
    let alix_conn = alix.context.db();
    assert_eq!(
        mark_of(&alix_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );

    let bo_conn = bo.context.db();
    assert_eq!(
        mark_of(&bo_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );

    // Verify deletion record exists
    for conn in [&alix_conn, &bo_conn] {
        let deletion = conn.get_message_deletion(&deletion_id)?.unwrap();
        assert_eq!(deletion.deleted_message_id, message_id);
        assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());
        // The creator is a super admin; its own message still reads as Sender.
        assert!(deletion.is_super_admin_deletion);
        assert!(deletion.authenticated);
    }
}

// verifies: CTYPE-024, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn compressed_delete_message_is_applied() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);

    let text = TextCodec::encode("delete me".into())?;
    let message_id = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(text),
            SendMessageOpts::default(),
        )
        .await?;
    bo_group.sync().await?;
    assert_eq!(mark_of(&bo.context.db(), &message_id), None);

    let delete = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&message_id),
    })?;
    let delete = compress(delete, Compression::Deflate)?;
    alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(delete),
            SendMessageOpts::default(),
        )
        .await?;
    bo_group.sync().await?;

    assert_eq!(
        mark_of(&bo.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );
}

/// Test message deletion by super admin
// verifies: PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_by_super_admin() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bola sends a message
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text_content = TextCodec::encode("Message from Bola".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = bo_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync alix's group to receive the message
    alix_group.sync().await?;

    // Alix (super admin) deletes Bola's message
    let deletion_id = alix_group.delete_message(message_id.clone())?;
    assert!(!deletion_id.is_empty());

    // Publish and sync
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the message is marked as deleted on both installations
    let alix_conn = alix.context.db();
    assert_eq!(
        mark_of(&alix_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );
    assert_eq!(
        mark_of(&bo.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );

    // Verify deletion was done by super admin
    let deletion = alix_conn.get_message_deletion(&deletion_id)?.unwrap();
    assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());
    assert!(deletion.is_super_admin_deletion);
    let MessageBody::DeletedMessage { deleted_by } = &bo_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == message_id)
        .unwrap()
        .content
    else {
        panic!("expected a placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Admin(alix.inbox_id().to_string()));
}

/// A member who joins after a message and its deletion receives both in topic order
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_deletion() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix deletes the message immediately (before bo syncs)
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Bola syncs and should receive both the message and deletion
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // The topic orders the message before its deletion.
    let bo_conn = bo.context.db();
    assert_eq!(
        mark_of(&bo_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );
}

/// Test concurrent deletions - multiple people trying to delete the same message
#[xmtp_common::test(unwrap_try = true)]
async fn test_concurrent_deletions() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    // Alix creates a group with Bo and Caro, making Bo also a super admin
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    // Bo syncs and gets the group
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Caro syncs and gets the group
    let caro_groups = caro.wait_for_welcomes().await?;
    let caro_group = &caro_groups[0];
    caro_group.sync().await?;

    // Make Bo a super admin
    alix_group
        .update_admin_list(
            crate::groups::UpdateAdminListType::AddSuper,
            bo.inbox_id().to_string(),
        )
        .await?;
    alix_group.publish_messages().await?;

    // Sync everyone
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Caro sends a message
    let text_content = TextCodec::encode("Message to be deleted".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = caro_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync Alix and Bo to receive the message
    alix_group.sync().await?;
    bo_group.sync().await?;

    // Alix (super admin) deletes first; once Bo (super admin) has synced
    // its echo, Bo's request finds the message marked.
    let alix_deletion = alix_group.delete_message(message_id.clone());
    alix_group.publish_messages().await?;

    // Bo syncs to get Alix's deletion, then tries to delete
    bo_group.sync().await?;
    let bo_deletion = bo_group.delete_message(message_id.clone());

    assert!(alix_deletion.is_ok());
    assert!(matches!(
        bo_deletion,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));

    alix_group.sync().await?;
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Verify the message is deleted for everyone
    let alix_conn = alix.context.db();
    let bo_conn = bo.context.db();
    let caro_conn = caro.context.db();

    // The first deletion applied marks the message on every installation.
    let mark = mark_of(&alix_conn, &message_id);
    assert_eq!(mark.as_deref(), Some(alix.inbox_id()));
    assert_eq!(mark_of(&bo_conn, &message_id), mark);
    assert_eq!(mark_of(&caro_conn, &message_id), mark);

    // Verify enriched messages show the deleted state
    let caro_messages = caro_group.find_messages_v2(&MsgQueryArgs::default())?;
    let deleted_msg = caro_messages
        .iter()
        .find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Admin(alix.inbox_id().to_string()));
}

/// Test that deletion works correctly when both sender and admin try to delete
#[xmtp_common::test(unwrap_try = true)]
async fn test_sender_and_admin_both_delete() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bo syncs and gets the group
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bo sends a message
    let text_content = TextCodec::encode("Bo's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = bo_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix syncs to receive the message
    alix_group.sync().await?;

    // Bo (sender) deletes their own message first
    bo_group.delete_message(message_id.clone())?;
    bo_group.publish_messages().await?;

    // Alix syncs to see the deletion
    alix_group.sync().await?;

    // Alix (super admin) tries to delete the same message - should fail
    let result = alix_group.delete_message(message_id.clone());
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));

    // Verify the message is deleted and shows as deleted by sender
    let bo_messages = bo_group.find_messages_v2(&MsgQueryArgs::default())?;
    let deleted_msg = bo_messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
    assert_eq!(
        mark_of(&alix.context.db(), &message_id).as_deref(),
        Some(bo.inbox_id())
    );
}

/// A request queues the deletion and changes nothing; the echo marks the
/// target and emits one `message.deleted`. A second request before publish is
/// allowed, and both publish to one mark.
// verifies: PROC-037, PROC-056, EVENT-001, EVENT-007
#[xmtp_common::test(unwrap_try = true)]
async fn request_queues_and_applies_at_echo() {
    use xmtp_mls_core::context::CoreContext;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let text = TextCodec::encode("delete me".into())?;
    let message_id = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(text),
            SendMessageOpts::default(),
        )
        .await?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageDeleted]),
        Some(10),
    );

    let first = alix_group.delete_message(message_id.clone())?;
    assert_ne!(first, message_id);
    assert_eq!(mark_of(&alix.context.db(), &message_id), None);
    assert!(events.drain().is_empty(), "the request emits nothing");
    let readable = alix_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == message_id)
        .unwrap();
    assert!(matches!(readable.content, MessageBody::Text(_)));
    let second = alix_group.delete_message(message_id.clone())?;
    assert_ne!(second, first);

    alix_group.publish_messages().await?;
    assert_eq!(
        mark_of(&alix.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageDeleted(deleted)), ..
        }] if deleted.message_id == message_id
            && deleted.cause == xmtp_events::DeletionCause::DeletedLocally
    ));
    for id in [&first, &second] {
        assert_eq!(
            alix.context
                .db()
                .get_group_message(id)?
                .unwrap()
                .delivery_status,
            DeliveryStatus::Published
        );
    }
    bo_group.sync().await?;
    assert_eq!(
        mark_of(&bo.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );
}
